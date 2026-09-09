use crate::collection::model::{
    CollectionInfo, CollectionItem, PostmanCollection, PostmanEvent, PostmanFolder,
    PostmanProtocolProfileBehavior, PostmanRequestNode, PostmanVariable,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const COLLECTION_META_FILE: &str = "_collection.json";
const FOLDER_META_FILE: &str = "_folder.json";

/// metadata persisted at the root of a directory-backed collection.
/// mirrors `CollectionInfo` + variables + explicit child ordering.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CollectionMeta {
    info: CollectionInfo,
    variable: Option<Vec<PostmanVariable>>,
    /// ordered list of child entry names (file or directory names,
    /// relative to this directory).
    order: Vec<String>,
}

/// metadata persisted inside every folder directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct FolderMeta {
    description: Option<String>,
    #[serde(rename = "protocolProfileBehavior")]
    protocol_profile_behavior: Option<PostmanProtocolProfileBehavior>,
    event: Option<Vec<PostmanEvent>>,
    order: Vec<String>,
}

/// turns an arbitrary item name into a filesystem-safe slug. Keeps things
/// human-readable (good for diffs) while avoiding characters that are
/// illegal or awkward on common filesystems.
fn sanitize_name(name: &str) -> String {
    let mut out: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();

    if out.is_empty() {
        out = "untitled".to_string();
    }
    out
}

/// ensures a filename is unique within `used`, appending `-2`, `-3`, ... on
/// collision (e.g. two requests both named "Get User").
fn dedupe_name(base: &str, used: &mut std::collections::HashSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

/// a planned (or applied) set of changes to a directory-backed collection's
/// working tree, expressed as paths relative to the collection root.
#[derive(Debug, Clone, Default)]
pub struct DirSyncPlan {
    pub writes: Vec<PathBuf>,
    pub removals: Vec<PathBuf>,
}

impl DirSyncPlan {
    fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.removals.is_empty()
    }
}

/// computes what `sync_collection_to_dir` would change, without touching
/// disk. Used to detect whether in-memory edits differ from what's already
/// written at `root` (e.g. before overwriting a folder someone else edited).
pub fn plan_dir_sync(collection: &PostmanCollection, root: &Path) -> Result<DirSyncPlan, String> {
    let mut plan = DirSyncPlan::new();
    sync_collection_inner(collection, root, false, &mut plan)?;
    Ok(plan)
}

/// writes a `PostmanCollection` out as a directory tree of individual JSON
/// files, only touching files that actually changed (preserving mtimes on
/// everything else) and removing stale files left over from renames/deletes.
/// Dotfiles and dot-directories (`.git`, `.gitignore`, ...) are never
/// touched, so this is safe to point at an existing git repository.
pub fn sync_collection_to_dir(collection: &PostmanCollection, root: &Path) -> Result<(), String> {
    let mut plan = DirSyncPlan::new();
    sync_collection_inner(collection, root, true, &mut plan)
}

/// same as `sync_collection_to_dir`
pub fn save_collection_to_dir_clean(
    collection: &PostmanCollection,
    root: &Path,
) -> Result<(), String> {
    sync_collection_to_dir(collection, root)
}

fn sync_collection_inner(
    collection: &PostmanCollection,
    root: &Path,
    apply: bool,
    plan: &mut DirSyncPlan,
) -> Result<(), String> {
    if apply {
        fs::create_dir_all(root).map_err(|e| format!("Failed to create {root:?}: {e}"))?;
    }

    let order = sync_items(&collection.item, root, Path::new(""), apply, plan)?;

    let meta = CollectionMeta {
        info: collection.info.clone(),
        variable: collection.variable.clone(),
        order: order.clone(),
    };
    sync_json_file(
        &root.join(COLLECTION_META_FILE),
        Path::new(COLLECTION_META_FILE),
        &meta,
        apply,
        plan,
    )?;

    let mut expected: HashSet<String> = order.into_iter().collect();
    expected.insert(COLLECTION_META_FILE.to_string());
    remove_stale_entries(root, Path::new(""), &expected, apply, plan)?;

    Ok(())
}

/// mirrors the collection/folder tree into `dir`, only writing files whose
/// content actually changed. `rel` tracks `dir`'s path relative to the sync
/// root so plan entries can be reported as root-relative paths.
fn sync_items(
    items: &[CollectionItem],
    dir: &Path,
    rel: &Path,
    apply: bool,
    plan: &mut DirSyncPlan,
) -> Result<Vec<String>, String> {
    let mut used_names = HashSet::new();
    let mut order = Vec::with_capacity(items.len());

    for item in items {
        match item {
            CollectionItem::Request(node) => {
                let base = sanitize_name(&node.name);
                let file_name = format!("{}.json", dedupe_name(&base, &mut used_names));
                sync_json_file(
                    &dir.join(&file_name),
                    &rel.join(&file_name),
                    node,
                    apply,
                    plan,
                )?;
                order.push(file_name);
            }
            CollectionItem::Folder(folder) => {
                let dir_name = dedupe_name(&sanitize_name(&folder.name), &mut used_names);
                let folder_dir = dir.join(&dir_name);
                let folder_rel = rel.join(&dir_name);

                if apply && !folder_dir.exists() {
                    fs::create_dir_all(&folder_dir)
                        .map_err(|e| format!("Failed to create {folder_dir:?}: {e}"))?;
                }

                let child_order = sync_items(&folder.item, &folder_dir, &folder_rel, apply, plan)?;

                let meta = FolderMeta {
                    description: folder.description.clone(),
                    protocol_profile_behavior: folder.protocol_profile_behavior.clone(),
                    event: folder.event.clone(),
                    order: child_order.clone(),
                };
                sync_json_file(
                    &folder_dir.join(FOLDER_META_FILE),
                    &folder_rel.join(FOLDER_META_FILE),
                    &meta,
                    apply,
                    plan,
                )?;

                let mut expected: HashSet<String> = child_order.into_iter().collect();
                expected.insert(FOLDER_META_FILE.to_string());
                remove_stale_entries(&folder_dir, &folder_rel, &expected, apply, plan)?;

                order.push(dir_name);
            }
        }
    }

    Ok(order)
}

/// writes `value` as pretty JSON to `path`, but only if its serialized form
/// differs from what's already on disk (so unchanged files keep their mtime).
/// When `apply` is false, only records what *would* be written.
fn sync_json_file<T: serde::Serialize>(
    path: &Path,
    rel_path: &Path,
    value: &T,
    apply: bool,
    plan: &mut DirSyncPlan,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize {path:?}: {e}"))?;

    let needs_write = match fs::read_to_string(path) {
        Ok(existing) => existing != json,
        Err(_) => true,
    };

    if needs_write {
        plan.writes.push(rel_path.to_path_buf());
        if apply {
            fs::write(path, json).map_err(|e| format!("Failed to write {path:?}: {e}"))?;
        }
    }

    Ok(())
}

/// removes entries in `dir` that aren't in `expected` (a rename/delete left
/// them behind). Dotfiles and dot-directories (`.git`, `.gitignore`, ...) are
/// always left alone, regardless of `expected`.
fn remove_stale_entries(
    dir: &Path,
    rel: &Path,
    expected: &HashSet<String>,
    apply: bool,
    plan: &mut DirSyncPlan,
) -> Result<(), String> {
    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };

    for entry in read_dir {
        let entry = entry.map_err(|e| format!("Failed to read {dir:?}: {e}"))?;
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') || expected.contains(&name) {
            continue;
        }

        let path = entry.path();
        plan.removals.push(rel.join(&name));

        if apply {
            if path.is_dir() {
                fs::remove_dir_all(&path).map_err(|e| format!("Failed to remove {path:?}: {e}"))?;
            } else {
                fs::remove_file(&path).map_err(|e| format!("Failed to remove {path:?}: {e}"))?;
            }
        }
    }

    Ok(())
}

/// loads a directory-backed collection tree back into a `PostmanCollection`
pub fn load_collection_from_dir(root: &Path) -> Result<PostmanCollection, String> {
    let meta_path = root.join(COLLECTION_META_FILE);
    let meta_text = fs::read_to_string(&meta_path)
        .map_err(|e| format!("Not a git collection folder ({meta_path:?} missing): {e}"))?;
    let meta: CollectionMeta = serde_json::from_str(&meta_text)
        .map_err(|e| format!("Failed to parse {meta_path:?}: {e}"))?;

    let item = read_items(root, &meta.order)?;

    Ok(PostmanCollection {
        id: 0,
        file_path: None,
        storage_dir: Some(root.to_path_buf()),
        info: meta.info,
        item,
        variable: meta.variable,
    })
}

fn read_items(dir: &Path, order: &[String]) -> Result<Vec<CollectionItem>, String> {
    let mut items = Vec::with_capacity(order.len());

    for entry_name in order {
        let path = dir.join(entry_name);

        if path.is_dir() {
            let folder_meta_path = path.join(FOLDER_META_FILE);
            let meta_text = fs::read_to_string(&folder_meta_path)
                .map_err(|e| format!("Failed to read {folder_meta_path:?}: {e}"))?;
            let meta: FolderMeta = serde_json::from_str(&meta_text)
                .map_err(|e| format!("Failed to parse {folder_meta_path:?}: {e}"))?;

            let child_items = read_items(&path, &meta.order)?;

            items.push(CollectionItem::Folder(PostmanFolder {
                name: entry_name.clone(),
                protocol_profile_behavior: meta.protocol_profile_behavior,
                item: child_items,
                event: meta.event,
                description: meta.description,
            }));
        } else {
            let text =
                fs::read_to_string(&path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;
            let node: PostmanRequestNode = serde_json::from_str(&text)
                .map_err(|e| format!("Failed to parse {path:?}: {e}"))?;
            items.push(CollectionItem::Request(node));
        }
    }

    Ok(items)
}

/// true if `path` looks like a directory-backed collection root
/// (i.e. has a `_collection.json`), used to decide how to import a
/// folder the user picked.
#[allow(dead_code)]
pub fn looks_like_collection_dir(path: &Path) -> bool {
    path.join(COLLECTION_META_FILE).is_file()
}
