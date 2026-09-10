use crate::collection::collection::{CollectionItem, PostmanCollection};
use crate::collection_adapter::{RequestNodeTabExt, sync_body_from_tab};
use crate::message::SidebarDragItem;
use crate::ui::tab::Tab;

pub use rustrest_core::collection::tree_ops::{
    contains_request_node_by_id, find_request_mut, insert_item_at, insert_nested,
    insert_nested_request, remove_nested, remove_nested_request, rename_nested_folder,
    take_folder, take_request,
};

/// relocates a dragged request/folder from its source location to `dest_folder_path`
/// within `dest_collection_id` (that folder's children, or the collection root when
/// empty). when `before_request_id` is set, the moved item is inserted immediately
/// before that sibling request; otherwise it's appended at the end.
pub fn move_sidebar_item(
    collections: &mut Vec<PostmanCollection>,
    source: SidebarDragItem,
    dest_collection_id: usize,
    dest_folder_path: Vec<String>,
    before_request_id: Option<usize>,
) {
    // guard against dropping a folder into itself or one of its own descendants
    if let SidebarDragItem::Folder { collection_id, path } = &source {
        if *collection_id == dest_collection_id && dest_folder_path.starts_with(path.as_slice()) {
            return;
        }
    }

    let extracted = match &source {
        SidebarDragItem::Request {
            collection_id,
            parent_path,
            request_id,
        } => collections
            .iter_mut()
            .find(|c| c.id == *collection_id)
            .and_then(|c| take_request(&mut c.item, parent_path, *request_id)),
        SidebarDragItem::Folder { collection_id, path } => collections
            .iter_mut()
            .find(|c| c.id == *collection_id)
            .and_then(|c| take_folder(&mut c.item, path)),
    };

    let Some(mut item) = extracted else {
        return;
    };

    // moving is a structural change, so flag it for saving same as add/rename do
    match &mut item {
        CollectionItem::Request(req) => req.unsaved = true,
        CollectionItem::Folder(folder) => folder.unsaved = true,
    }

    if let Some(dest_col) = collections.iter_mut().find(|c| c.id == dest_collection_id) {
        insert_item_at(&mut dest_col.item, &dest_folder_path, item, before_request_id);
    }
}

pub fn format_json_or_fallback(raw_body: &str) -> String {
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(raw_body) {
        serde_json::to_string_pretty(&json_value).unwrap_or_else(|_| raw_body.to_string())
    } else {
        format!("// Invalid JSON:\n{}", raw_body)
    }
}

// recursively updates node in the collection by its ID, syncing tab state with the request
pub fn update_node(items: &mut Vec<CollectionItem>, target_id: usize, tab: &Tab) -> bool {
    let Some(req) = rustrest_core::collection::tree_ops::find_request_mut(items, target_id) else {
        return false;
    };

    // sync name / method / url / headers / pre-request and test scripts
    req.update_from_tab(tab);

    // sync request body conditionally on the active body type
    sync_body_from_tab(req, tab);

    true
}
