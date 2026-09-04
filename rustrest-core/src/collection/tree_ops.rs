use crate::collection::model::{CollectionItem, PostmanFolder};

pub fn contains_request_node_by_id(items: &[CollectionItem], target_id: usize) -> bool {
    for item in items {
        match item {
            CollectionItem::Request(node) => {
                if node.id == target_id {
                    return true;
                }
            }
            CollectionItem::Folder(folder) => {
                if contains_request_node_by_id(&folder.item, target_id) {
                    return true;
                }
            }
        }
    }
    false
}

/// Walks `path` through nested folders and returns the `Vec<CollectionItem>` that
/// `path` resolves to (i.e. that folder's children). An empty `path` resolves to
/// `items` itself — this is the "already inside the target parent" case used by
/// insert/remove-request operations, where the request itself is never part of `path`.
fn find_folder_items_mut<'a>(
    items: &'a mut Vec<CollectionItem>,
    path: &[String],
) -> Option<&'a mut Vec<CollectionItem>> {
    match path.split_first() {
        None => Some(items),
        Some((head, rest)) => {
            for item in items.iter_mut() {
                if let CollectionItem::Folder(folder) = item {
                    if folder.name == *head {
                        return find_folder_items_mut(&mut folder.item, rest);
                    }
                }
            }
            None
        }
    }
}

/// Walks `path` through nested folders and returns the folder that `path`'s last
/// segment names (used by rename, where the folder itself — not its children — is
/// the target).
fn find_folder_mut<'a>(
    items: &'a mut Vec<CollectionItem>,
    path: &[String],
) -> Option<&'a mut PostmanFolder> {
    let (head, rest) = path.split_first()?;
    for item in items.iter_mut() {
        if let CollectionItem::Folder(folder) = item {
            if folder.name == *head {
                if rest.is_empty() {
                    return Some(folder);
                }
                return find_folder_mut(&mut folder.item, rest);
            }
        }
    }
    None
}

/// inserts a nested folder into the collection at the specified path
pub fn insert_nested(items: &mut Vec<CollectionItem>, path: &[String]) {
    if let Some(target) = find_folder_items_mut(items, path) {
        target.push(CollectionItem::Folder(PostmanFolder {
            name: "New Folder".to_string(),
            description: None,
            item: Vec::new(),
            protocol_profile_behavior: None,
            event: None,
        }));
    }
}

/// inserts a nested request into the collection at the specified path
pub fn insert_nested_request(
    items: &mut Vec<CollectionItem>,
    path: &[String],
    new_req: CollectionItem,
) {
    if let Some(target) = find_folder_items_mut(items, path) {
        target.push(new_req);
    }
}

/// removes a nested request from the collection at the specified path
pub fn remove_nested_request(items: &mut Vec<CollectionItem>, path: &[String], req_id: usize) {
    if let Some(target) = find_folder_items_mut(items, path) {
        target.retain(|item| !matches!(item, CollectionItem::Request(req) if req.id == req_id));
    }
}

/// removes a nested folder from the collection at the specified path
pub fn remove_nested(items: &mut Vec<CollectionItem>, path: &[String]) {
    let Some((last, parent_path)) = path.split_last() else {
        return;
    };
    if let Some(target) = find_folder_items_mut(items, parent_path) {
        target
            .retain(|item| !matches!(item, CollectionItem::Folder(folder) if folder.name == *last));
    }
}

pub fn rename_nested_folder(
    items: &mut Vec<CollectionItem>,
    path: &[String],
    new_val: &str,
) -> bool {
    match find_folder_mut(items, path) {
        Some(folder) => {
            folder.name = new_val.to_string();
            true
        }
        None => false,
    }
}

/// Finds a request anywhere in the tree by id, for mutation. Used by GUI-side
/// code that needs to sync a request node from live UI state.
pub fn find_request_mut(
    items: &mut Vec<CollectionItem>,
    target_id: usize,
) -> Option<&mut crate::collection::model::PostmanRequestNode> {
    for item in items.iter_mut() {
        match item {
            CollectionItem::Request(node) if node.id == target_id => return Some(node),
            CollectionItem::Folder(folder) => {
                if let Some(found) = find_request_mut(&mut folder.item, target_id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}
