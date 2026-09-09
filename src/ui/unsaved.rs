use crate::app::{Rustrest, TabState, WorkspaceContent};
use crate::collection::collection::{CollectionItem, PostmanCollection, PostmanRequestNode};
use iced::Element;
use iced::widget::text;

/// small dot rendered next to a sidebar item or tab name to flag unsaved changes
pub fn unsaved_dot<'a, Message: 'a>() -> Element<'a, Message> {
    text("●")
        .size(8)
        .color(iced::Color::from_rgb(0.85, 0.55, 0.10))
        .into()
}

/// whether an open tab (request or collection root) has unsaved changes.
pub fn tab_is_unsaved(app: &Rustrest, tab_state: &TabState) -> bool {
    match &tab_state.content {
        WorkspaceContent::HttpRequest => tab_state.tab.request_id.is_none() || tab_state.tab.dirty,
        WorkspaceContent::CollectionRoot { collection_id, .. } => app
            .collections
            .iter()
            .find(|c| c.id == *collection_id)
            .map(|c| collection_is_unsaved(app, c))
            .unwrap_or(false),
    }
}

/// whether a saved request has unsaved edits: either its own node was added
/// since the last save, or it has an open tab with pending edits.
pub fn request_is_unsaved(app: &Rustrest, req: &PostmanRequestNode) -> bool {
    req.unsaved
        || app.tabs.iter().any(|t| {
            matches!(t.content, WorkspaceContent::HttpRequest)
                && t.tab.request_id == Some(req.id)
                && t.tab.dirty
        })
}

fn item_has_unsaved_changes(app: &Rustrest, item: &CollectionItem) -> bool {
    match item {
        CollectionItem::Request(req) => request_is_unsaved(app, req),
        CollectionItem::Folder(folder) => {
            folder.unsaved
                || folder
                    .item
                    .iter()
                    .any(|sub| item_has_unsaved_changes(app, sub))
        }
    }
}

/// whether a folder itself, or anything it contains (at any depth), is unsaved.
pub fn folder_is_unsaved(app: &Rustrest, items: &[CollectionItem]) -> bool {
    items.iter().any(|item| item_has_unsaved_changes(app, item))
}

/// whether a collection has unsaved changes: it has never been saved to
/// disk, its own info/variables were edited, or any descendant item is.
pub fn collection_is_unsaved(app: &Rustrest, col: &PostmanCollection) -> bool {
    let never_saved_to_disk = col.storage_dir.is_none() && col.file_path.is_none();

    never_saved_to_disk || col.unsaved || folder_is_unsaved(app, &col.item)
}
