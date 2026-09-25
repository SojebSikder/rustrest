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
/// `items` itself, this is the "already inside the target parent" case used by
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
/// segment names (used by rename, where the folder itself, not its children - is
/// the target).
pub fn find_folder_mut<'a>(
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

/// read-only counterpart of `find_folder_mut`.
pub fn find_folder<'a>(items: &'a [CollectionItem], path: &[String]) -> Option<&'a PostmanFolder> {
    let (head, rest) = path.split_first()?;
    items.iter().find_map(|item| match item {
        CollectionItem::Folder(folder) if folder.name == *head => {
            if rest.is_empty() {
                Some(folder)
            } else {
                find_folder(&folder.item, rest)
            }
        }
        _ => None,
    })
}

/// read-only counterpart of `find_request_mut`.
pub fn find_request(
    items: &[CollectionItem],
    target_id: usize,
) -> Option<&crate::collection::model::PostmanRequestNode> {
    items.iter().find_map(|item| match item {
        CollectionItem::Request(node) if node.id == target_id => Some(node),
        CollectionItem::Folder(folder) => find_request(&folder.item, target_id),
        _ => None,
    })
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
            unsaved: true,
        }));
    }
}

/// inserts a nested folder with a caller-supplied name into the collection
/// at the specified path (like `insert_nested`, but for callers - e.g. a
/// plugin-proposed `CollectionOperation` - that already know the folder's
/// final name instead of relying on the sidebar's "New Folder" + rename-in-place flow).
pub fn insert_nested_named(items: &mut Vec<CollectionItem>, path: &[String], name: &str) {
    if let Some(target) = find_folder_items_mut(items, path) {
        target.push(CollectionItem::Folder(PostmanFolder {
            name: name.to_string(),
            description: None,
            item: Vec::new(),
            protocol_profile_behavior: None,
            event: None,
            unsaved: true,
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

/// removes and returns the request with `req_id` living directly under `path`,
/// for relocating it elsewhere in the tree (drag-and-drop moves).
pub fn take_request(
    items: &mut Vec<CollectionItem>,
    path: &[String],
    req_id: usize,
) -> Option<CollectionItem> {
    let target = find_folder_items_mut(items, path)?;
    let idx = target
        .iter()
        .position(|item| matches!(item, CollectionItem::Request(req) if req.id == req_id))?;
    Some(target.remove(idx))
}

/// removes and returns the folder (with all its descendants) named by `path`'s
/// last segment, for relocating it elsewhere in the tree (drag-and-drop moves).
pub fn take_folder(items: &mut Vec<CollectionItem>, path: &[String]) -> Option<CollectionItem> {
    let (last, parent_path) = path.split_last()?;
    let target = find_folder_items_mut(items, parent_path)?;
    let idx = target
        .iter()
        .position(|item| matches!(item, CollectionItem::Folder(folder) if folder.name == *last))?;
    Some(target.remove(idx))
}

/// inserts `item` into the children of the folder at `path` (or the root, if
/// `path` is empty), either right before an existing request with the id
/// `before_request_id`, or at the end when that request isn't found there.
pub fn insert_item_at(
    items: &mut Vec<CollectionItem>,
    path: &[String],
    item: CollectionItem,
    before_request_id: Option<usize>,
) -> bool {
    let Some(target) = find_folder_items_mut(items, path) else {
        return false;
    };
    let insert_idx = before_request_id
        .and_then(|id| {
            target
                .iter()
                .position(|i| matches!(i, CollectionItem::Request(req) if req.id == id))
        })
        .unwrap_or(target.len());
    target.insert(insert_idx, item);
    true
}

pub fn rename_nested_folder(
    items: &mut Vec<CollectionItem>,
    path: &[String],
    new_val: &str,
) -> bool {
    match find_folder_mut(items, path) {
        Some(folder) => {
            folder.name = new_val.to_string();
            folder.unsaved = true;
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

/// gives every request under `items` a fresh id from `next_id` and flags every
/// node as unsaved, so a cloned subtree doesn't alias the ids of its original.
fn reassign_ids_as_unsaved(items: &mut [CollectionItem], next_id: &mut usize) {
    for item in items {
        match item {
            CollectionItem::Request(node) => {
                node.id = *next_id;
                *next_id += 1;
                node.unsaved = true;
            }
            CollectionItem::Folder(folder) => {
                folder.unsaved = true;
                reassign_ids_as_unsaved(&mut folder.item, next_id);
            }
        }
    }
}

/// "<name> Copy", or "<name> Copy N" when that's already taken by `taken`.
pub fn copy_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let base = format!("{name} Copy");
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or(base)
}

/// clones the request `req_id` living directly under `path` and inserts the
/// copy (renamed "<name> Copy", with a fresh id from `next_id`) right after
/// the original. returns the copy's id.
pub fn duplicate_request(
    items: &mut Vec<CollectionItem>,
    path: &[String],
    req_id: usize,
    next_id: &mut usize,
) -> Option<usize> {
    let target = find_folder_items_mut(items, path)?;
    let idx = target
        .iter()
        .position(|item| matches!(item, CollectionItem::Request(req) if req.id == req_id))?;
    let CollectionItem::Request(original) = &target[idx] else {
        return None;
    };

    let mut copy = original.clone();
    copy.name = format!("{} Copy", original.name);
    copy.id = *next_id;
    *next_id += 1;
    copy.unsaved = true;

    let new_id = copy.id;
    target.insert(idx + 1, CollectionItem::Request(copy));
    Some(new_id)
}

/// clones the folder named by `path`'s last segment (with all its descendants)
/// and inserts the copy right after the original under a sibling-unique
/// "<name> Copy" name, since folders are addressed by name. every request in
/// the copy gets a fresh id from `next_id`. returns the copy's name.
pub fn duplicate_folder(
    items: &mut Vec<CollectionItem>,
    path: &[String],
    next_id: &mut usize,
) -> Option<String> {
    let (last, parent_path) = path.split_last()?;
    let target = find_folder_items_mut(items, parent_path)?;
    let idx = target
        .iter()
        .position(|item| matches!(item, CollectionItem::Folder(folder) if folder.name == *last))?;
    let CollectionItem::Folder(original) = &target[idx] else {
        return None;
    };

    let mut copy = original.clone();
    copy.name = copy_name(&original.name, |candidate| {
        target
            .iter()
            .any(|item| matches!(item, CollectionItem::Folder(f) if f.name == candidate))
    });
    copy.unsaved = true;
    reassign_ids_as_unsaved(&mut copy.item, next_id);
    let new_name = copy.name.clone();
    target.insert(idx + 1, CollectionItem::Folder(copy));
    Some(new_name)
}

/// clones a whole collection's tree with fresh request ids from `next_id`,
/// every node flagged unsaved (used when duplicating a collection).
pub fn clone_items_with_new_ids(
    items: &[CollectionItem],
    next_id: &mut usize,
) -> Vec<CollectionItem> {
    let mut copy = items.to_vec();
    reassign_ids_as_unsaved(&mut copy, next_id);
    copy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::model::{PostmanRequestDetails, PostmanRequestNode};

    fn request(id: usize, name: &str) -> CollectionItem {
        CollectionItem::Request(PostmanRequestNode {
            id,
            name: name.to_string(),
            event: None,
            request: PostmanRequestDetails {
                method: "GET".to_string(),
                url: None,
                header: None,
                body: None,
                auth: None,
                description: None,
            },
            unsaved: false,
            response: None,
            protocol_request: None,
        })
    }

    fn folder(name: &str, item: Vec<CollectionItem>) -> CollectionItem {
        CollectionItem::Folder(PostmanFolder {
            name: name.to_string(),
            protocol_profile_behavior: None,
            item,
            event: None,
            description: None,
            unsaved: false,
        })
    }

    #[test]
    fn duplicates_request_after_original_with_new_id() {
        let mut items = vec![request(1, "a"), request(2, "b")];
        let mut next_id = 10;
        assert_eq!(
            duplicate_request(&mut items, &[], 1, &mut next_id),
            Some(10)
        );
        assert_eq!(next_id, 11);
        let CollectionItem::Request(copy) = &items[1] else {
            panic!("expected request");
        };
        assert_eq!(
            (copy.id, copy.name.as_str(), copy.unsaved),
            (10, "a Copy", true)
        );
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn duplicates_folder_with_unique_name_and_fresh_ids() {
        let mut items = vec![
            folder(
                "f",
                vec![request(1, "a"), folder("sub", vec![request(2, "b")])],
            ),
            folder("f Copy", Vec::new()),
        ];
        let mut next_id = 10;
        let name = duplicate_folder(&mut items, &["f".to_string()], &mut next_id);
        assert_eq!(name.as_deref(), Some("f Copy 2"));
        assert_eq!(next_id, 12);
        let copy = find_folder(&items, &["f Copy 2".to_string()]).unwrap();
        assert!(copy.unsaved);
        assert!(find_request(&copy.item, 10).is_some());
        assert!(find_request(&copy.item, 11).is_some());
        // original is untouched
        assert!(contains_request_node_by_id(&items[..1], 1));
        assert!(contains_request_node_by_id(&items[..1], 2));
    }
}
