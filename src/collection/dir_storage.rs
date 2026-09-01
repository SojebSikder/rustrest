use crate::collection::collection::{
    CollectionInfo, CollectionItem, PostmanCollection, PostmanEvent, PostmanFolder,
    PostmanProtocolProfileBehavior, PostmanRequestNode, PostmanVariable,
};
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

/// writes a `PostmanCollection` out as a directory tree of individual JSON
/// files. Safe to call repeatedly on the same `root` (overwrites in place);
/// note this does NOT delete files that are no longer referenced (e.g. after a rename)
pub fn save_collection_to_dir(collection: &PostmanCollection, root: &Path) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| format!("Failed to create {root:?}: {e}"))?;

    let order = write_items(&collection.item, root)?;

    let meta = CollectionMeta {
        info: collection.info.clone(),
        variable: collection.variable.clone(),
        order,
    };
    write_json(&root.join(COLLECTION_META_FILE), &meta)?;

    Ok(())
}

/// same as `save_collection_to_dir`, but first wipes `root` so stale files
/// from renamed/deleted items don't linger. Prefer this for the normal
/// "Save" action; it keeps the working tree exactly matching in-memory state,
/// which is good for clean git diffs
pub fn save_collection_to_dir_clean(
    collection: &PostmanCollection,
    root: &Path,
) -> Result<(), String> {
    if root.exists() {
        fs::remove_dir_all(root).map_err(|e| format!("Failed to clear {root:?}: {e}"))?;
    }
    save_collection_to_dir(collection, root)
}

fn write_items(items: &[CollectionItem], dir: &Path) -> Result<Vec<String>, String> {
    let mut used_names = std::collections::HashSet::new();
    let mut order = Vec::with_capacity(items.len());

    for item in items {
        match item {
            CollectionItem::Request(node) => {
                let base = sanitize_name(&node.name);
                let file_name = format!("{}.json", dedupe_name(&base, &mut used_names));
                write_json(&dir.join(&file_name), node)?;
                order.push(file_name);
            }
            CollectionItem::Folder(folder) => {
                let dir_name = dedupe_name(&sanitize_name(&folder.name), &mut used_names);
                let folder_dir = dir.join(&dir_name);
                fs::create_dir_all(&folder_dir)
                    .map_err(|e| format!("Failed to create {folder_dir:?}: {e}"))?;

                let child_order = write_items(&folder.item, &folder_dir)?;

                let meta = FolderMeta {
                    description: folder.description.clone(),
                    protocol_profile_behavior: folder.protocol_profile_behavior.clone(),
                    event: folder.event.clone(),
                    order: child_order,
                };
                write_json(&folder_dir.join(FOLDER_META_FILE), &meta)?;

                order.push(dir_name);
            }
        }
    }

    Ok(order)
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize {path:?}: {e}"))?;
    fs::write(path, json).map_err(|e| format!("Failed to write {path:?}: {e}"))
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
pub fn looks_like_collection_dir(path: &Path) -> bool {
    path.join(COLLECTION_META_FILE).is_file()
}
