//! syncs a `PostmanCollection` to/from a directory-backed collection tree on
//! a remote host, using the same on-disk format (`_collection.json` /
//! `_folder.json`) as `rustrest_core::collection::dir_storage`, but driven
//! over the remote-agent's file RPCs instead of `std::fs`.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

use rustrest_core::collection::dir_format::{
    COLLECTION_META_FILE, CollectionMeta, FOLDER_META_FILE, FolderMeta, dedupe_name, sanitize_name,
};
use rustrest_core::collection::model::{
    CollectionItem, PostmanCollection, PostmanFolder, PostmanRequestNode,
};

use crate::error::RemoteError;
use crate::session::RemoteSession;

/// joins a directory path and an entry name using `/` (collections are only
/// synced to POSIX remotes for now, matching the remote file browser).
fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// a planned (or applied) set of changes to a remote directory-backed
/// collection's working tree, expressed as paths relative to the collection
/// root. Mirrors `rustrest_core::collection::dir_storage::DirSyncPlan`.
#[derive(Debug, Clone, Default)]
pub struct RemoteDirSyncPlan {
    pub writes: Vec<String>,
    pub removals: Vec<String>,
}

impl RemoteDirSyncPlan {
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.removals.is_empty()
    }
}

/// computes what `sync_collection_to_remote_dir` would change, without
/// touching the remote host.
pub async fn plan_remote_dir_sync(
    session: &RemoteSession,
    collection: &PostmanCollection,
    root: &str,
) -> Result<RemoteDirSyncPlan, RemoteError> {
    let mut plan = RemoteDirSyncPlan::default();
    sync_collection_inner(session, collection, root, false, &mut plan).await?;
    Ok(plan)
}

/// writes a `PostmanCollection` out as a directory tree of individual JSON
/// files on the remote host, only touching files that actually changed and
/// removing stale files left over from renames/deletes. Dotfiles/directories
/// are never touched.
pub async fn sync_collection_to_remote_dir(
    session: &RemoteSession,
    collection: &PostmanCollection,
    root: &str,
) -> Result<(), RemoteError> {
    let mut plan = RemoteDirSyncPlan::default();
    sync_collection_inner(session, collection, root, true, &mut plan).await
}

async fn sync_collection_inner(
    session: &RemoteSession,
    collection: &PostmanCollection,
    root: &str,
    apply: bool,
    plan: &mut RemoteDirSyncPlan,
) -> Result<(), RemoteError> {
    if apply {
        session.create_dir(root).await?;
    }

    let order = sync_items(session, &collection.item, root, "", apply, plan).await?;

    let meta = CollectionMeta {
        info: collection.info.clone(),
        variable: collection.variable.clone(),
        order: order.clone(),
    };
    sync_json_file(
        session,
        &join(root, COLLECTION_META_FILE),
        COLLECTION_META_FILE,
        &meta,
        apply,
        plan,
    )
    .await?;

    let mut expected: HashSet<String> = order.into_iter().collect();
    expected.insert(COLLECTION_META_FILE.to_string());
    remove_stale_entries(session, root, "", &expected, apply, plan).await?;

    Ok(())
}

/// mirrors the collection/folder tree into `dir` on the remote host, only
/// writing files whose content actually changed. `rel` tracks `dir`'s path
/// relative to the sync root so plan entries can be reported as root-relative paths.
fn sync_items<'a>(
    session: &'a RemoteSession,
    items: &'a [CollectionItem],
    dir: &'a str,
    rel: &'a str,
    apply: bool,
    plan: &'a mut RemoteDirSyncPlan,
) -> Pin<Box<dyn Future<Output = Result<Vec<String>, RemoteError>> + Send + 'a>> {
    Box::pin(async move {
        let mut used_names = HashSet::new();
        let mut order = Vec::with_capacity(items.len());

        for item in items {
            match item {
                CollectionItem::Request(node) => {
                    let base = sanitize_name(&node.name);
                    let file_name = format!("{}.json", dedupe_name(&base, &mut used_names));
                    sync_json_file(
                        session,
                        &join(dir, &file_name),
                        &join(rel, &file_name),
                        node,
                        apply,
                        plan,
                    )
                    .await?;
                    order.push(file_name);
                }
                CollectionItem::Folder(folder) => {
                    let dir_name = dedupe_name(&sanitize_name(&folder.name), &mut used_names);
                    let folder_dir = join(dir, &dir_name);
                    let folder_rel = join(rel, &dir_name);

                    if apply {
                        session.create_dir(&folder_dir).await?;
                    }

                    let child_order =
                        sync_items(session, &folder.item, &folder_dir, &folder_rel, apply, plan)
                            .await?;

                    let meta = FolderMeta {
                        description: folder.description.clone(),
                        protocol_profile_behavior: folder.protocol_profile_behavior.clone(),
                        event: folder.event.clone(),
                        order: child_order.clone(),
                    };
                    sync_json_file(
                        session,
                        &join(&folder_dir, FOLDER_META_FILE),
                        &join(&folder_rel, FOLDER_META_FILE),
                        &meta,
                        apply,
                        plan,
                    )
                    .await?;

                    let mut expected: HashSet<String> = child_order.into_iter().collect();
                    expected.insert(FOLDER_META_FILE.to_string());
                    remove_stale_entries(session, &folder_dir, &folder_rel, &expected, apply, plan)
                        .await?;

                    order.push(dir_name);
                }
            }
        }

        Ok(order)
    })
}

/// writes `value` as pretty JSON to `path` on the remote host, but only if
/// its serialized form differs from what's already there. When `apply` is
/// false, only records what *would* be written.
async fn sync_json_file<T: serde::Serialize>(
    session: &RemoteSession,
    path: &str,
    rel_path: &str,
    value: &T,
    apply: bool,
    plan: &mut RemoteDirSyncPlan,
) -> Result<(), RemoteError> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| RemoteError::Remote(format!("Failed to serialize {path}: {e}")))?;

    let needs_write = match session.read_file(path).await {
        Ok(existing) => existing != json.as_bytes(),
        Err(_) => true,
    };

    if needs_write {
        plan.writes.push(rel_path.to_string());
        if apply {
            session.write_file(path, json.into_bytes()).await?;
        }
    }

    Ok(())
}

/// removes entries in `dir` (on the remote host) that aren't in `expected`
/// (a rename/delete left them behind). Dotfiles/directories are always left
/// alone, regardless of `expected`.
async fn remove_stale_entries(
    session: &RemoteSession,
    dir: &str,
    rel: &str,
    expected: &HashSet<String>,
    apply: bool,
    plan: &mut RemoteDirSyncPlan,
) -> Result<(), RemoteError> {
    let entries = match session.list_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        if entry.name.starts_with('.') || expected.contains(&entry.name) {
            continue;
        }

        let path = join(dir, &entry.name);
        plan.removals.push(join(rel, &entry.name));

        if apply {
            session.delete(&path).await?;
        }
    }

    Ok(())
}

/// loads a directory-backed collection tree from the remote host back into a
/// `PostmanCollection`. The returned collection's `remote_dir` is left unset
/// — the caller (which knows the SSH profile id this `session` belongs to)
/// is expected to fill it in.
pub async fn load_collection_from_remote_dir(
    session: &RemoteSession,
    root: &str,
) -> Result<PostmanCollection, RemoteError> {
    let meta_path = join(root, COLLECTION_META_FILE);
    let meta_bytes = session.read_file(&meta_path).await.map_err(|e| {
        RemoteError::Remote(format!(
            "Not a collection folder ({meta_path} missing): {e}"
        ))
    })?;
    let meta: CollectionMeta = serde_json::from_slice(&meta_bytes)
        .map_err(|e| RemoteError::Remote(format!("Failed to parse {meta_path}: {e}")))?;

    let item = read_items(session, root, &meta.order).await?;

    Ok(PostmanCollection {
        id: 0,
        file_path: None,
        storage_dir: None,
        remote_dir: None,
        unsaved: false,
        info: meta.info,
        item,
        variable: meta.variable,
    })
}

fn read_items<'a>(
    session: &'a RemoteSession,
    dir: &'a str,
    order: &'a [String],
) -> Pin<Box<dyn Future<Output = Result<Vec<CollectionItem>, RemoteError>> + Send + 'a>> {
    Box::pin(async move {
        let entries = session.list_dir(dir).await?;
        let dirs: HashSet<&str> = entries
            .iter()
            .filter(|e| e.is_dir)
            .map(|e| e.name.as_str())
            .collect();

        let mut items = Vec::with_capacity(order.len());

        for entry_name in order {
            let path = join(dir, entry_name);

            if dirs.contains(entry_name.as_str()) {
                let folder_meta_path = join(&path, FOLDER_META_FILE);
                let meta_bytes = session.read_file(&folder_meta_path).await.map_err(|e| {
                    RemoteError::Remote(format!("Failed to read {folder_meta_path}: {e}"))
                })?;
                let meta: FolderMeta = serde_json::from_slice(&meta_bytes).map_err(|e| {
                    RemoteError::Remote(format!("Failed to parse {folder_meta_path}: {e}"))
                })?;

                let child_items = read_items(session, &path, &meta.order).await?;

                items.push(CollectionItem::Folder(PostmanFolder {
                    name: entry_name.clone(),
                    protocol_profile_behavior: meta.protocol_profile_behavior,
                    item: child_items,
                    event: meta.event,
                    description: meta.description,
                    unsaved: false,
                }));
            } else {
                let bytes = session
                    .read_file(&path)
                    .await
                    .map_err(|e| RemoteError::Remote(format!("Failed to read {path}: {e}")))?;
                let node: PostmanRequestNode = serde_json::from_slice(&bytes)
                    .map_err(|e| RemoteError::Remote(format!("Failed to parse {path}: {e}")))?;
                items.push(CollectionItem::Request(node));
            }
        }

        Ok(items)
    })
}
