//! Sidebar tree interaction state: inline rename editing (collection/folder/
//! request/saved-response name), drag-and-drop, multi-select, and
//! collapse/expand - plus the batch-delete flow driven by a multi-selection.

use super::{Rustrest, WorkspaceContent};
use crate::message::{Message, SidebarDragItem, SidebarItemKey};
use crate::ui::confirm_dialog::ConfirmDialogState;
use crate::ui::sidebar::flatten_visible_sidebar_items;
use crate::utils::{
    contains_request_node_by_id, move_sidebar_item, remove_nested, remove_nested_request,
    rename_nested_folder,
};
use iced::Task;
use std::collections::HashSet;

#[derive(Default)]
pub struct SidebarState {
    pub editing_collection_id: Option<usize>,
    pub editing_folder_collection_id: Option<usize>,
    pub editing_folder_path: Vec<String>,
    pub editing_request_collection_id: Option<usize>,
    pub editing_request_id: Option<usize>,
    /// (collection_id, request_id, index) of the saved response currently being renamed.
    pub editing_saved_response: Option<(usize, usize, usize)>,
    pub sidebar_drag: Option<SidebarDragItem>,
    pub collapsed_collections: HashSet<usize>,
    pub collapsed_folders: HashSet<(usize, Vec<String>)>,
    /// request ids whose saved-responses list is collapsed in the sidebar.
    pub collapsed_saved_responses: HashSet<usize>,
    pub selected_sidebar_items: HashSet<SidebarItemKey>,
    pub sidebar_selection_anchor: Option<SidebarItemKey>,
    /// live keyboard modifier state, updated by a `ModifiersChanged` subscription;
    /// the sidebar view reads this to decide what a row click means.
    pub current_modifiers: iced::keyboard::Modifiers,
}

pub fn show_saved_response_context_menu(
    app: &mut Rustrest,
    collection_id: usize,
    request_id: usize,
    index: usize,
) -> Task<Message> {
    app.overlays.active_context_menu = Some(crate::ui::context_menu::ContextMenu::SavedResponse {
        col_id: collection_id,
        req_id: request_id,
        index,
    });
    app.overlays.context_menu_position = app.cursor_position;
    Task::none()
}

pub fn rename_saved_response_pressed(
    app: &mut Rustrest,
    collection_id: usize,
    request_id: usize,
    index: usize,
) -> Task<Message> {
    app.sidebar.editing_saved_response = Some((collection_id, request_id, index));
    app.overlays.active_context_menu = None;
    Task::none()
}

pub fn saved_response_name_changed(
    app: &mut Rustrest,
    collection_id: usize,
    request_id: usize,
    index: usize,
    new_name: String,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        if let Some(node) = crate::utils::find_request_mut(&mut col.item, request_id) {
            if let Some(example) = node
                .response
                .as_mut()
                .and_then(|responses| responses.get_mut(index))
            {
                example.name = new_name;
                node.unsaved = true;
            }
        }
    }
    app.refresh_open_tab_saved_responses(collection_id, request_id);
    Task::none()
}

pub fn save_saved_response_name_pressed(app: &mut Rustrest) -> Task<Message> {
    app.sidebar.editing_saved_response = None;
    Task::none()
}

pub fn delete_saved_response_pressed(
    app: &mut Rustrest,
    collection_id: usize,
    request_id: usize,
    index: usize,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        if let Some(node) = crate::utils::find_request_mut(&mut col.item, request_id) {
            if let Some(responses) = node.response.as_mut() {
                if index < responses.len() {
                    responses.remove(index);
                    node.unsaved = true;
                }
                if responses.is_empty() {
                    node.response = None;
                }
            }
        }
    }
    if app.sidebar.editing_saved_response == Some((collection_id, request_id, index)) {
        app.sidebar.editing_saved_response = None;
    }
    app.refresh_open_tab_saved_responses(collection_id, request_id);
    Task::none()
}

pub fn rename_collection_pressed(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    app.sidebar.editing_collection_id = Some(col_id);
    Task::none()
}

pub fn collection_name_changed(
    app: &mut Rustrest,
    col_id: usize,
    new_name: String,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
        col.info.name = new_name.clone();
        col.unsaved = true;

        for t in &mut app.tabs {
            if let WorkspaceContent::CollectionRoot {
                collection_id,
                ref mut collection_name,
                ..
            } = t.content
            {
                if collection_id == col_id {
                    *collection_name = new_name.clone();
                    t.tab.name = new_name.clone();
                }
            }
        }
    }
    Task::none()
}

pub fn save_collection_name_pressed(app: &mut Rustrest) -> Task<Message> {
    app.sidebar.editing_collection_id = None;
    Task::none()
}

pub fn rename_folder_pressed(
    app: &mut Rustrest,
    collection_id: usize,
    folder_path: Vec<String>,
) -> Task<Message> {
    app.sidebar.editing_folder_collection_id = Some(collection_id);
    app.sidebar.editing_folder_path = folder_path;
    Task::none()
}

pub fn folder_name_changed(
    app: &mut Rustrest,
    collection_id: usize,
    folder_path: Vec<String>,
    new_name: String,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        if rename_nested_folder(&mut col.item, &folder_path, &new_name) {
            if let Some(last) = app.sidebar.editing_folder_path.last_mut() {
                *last = new_name;
            }
        }
    }
    Task::none()
}

pub fn save_folder_name_pressed(app: &mut Rustrest) -> Task<Message> {
    app.sidebar.editing_folder_collection_id = None;
    app.sidebar.editing_folder_path.clear();
    Task::none()
}

pub fn rename_request_pressed(
    app: &mut Rustrest,
    collection_id: usize,
    request_id: usize,
) -> Task<Message> {
    app.sidebar.editing_request_collection_id = Some(collection_id);
    app.sidebar.editing_request_id = Some(request_id);
    app.overlays.active_context_menu = None;
    Task::none()
}

pub fn request_name_changed(
    app: &mut Rustrest,
    collection_id: usize,
    request_id: usize,
    new_name: String,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        if let Some(node) = crate::utils::find_request_mut(&mut col.item, request_id) {
            node.name = new_name.clone();
            node.unsaved = true;
        }
    }
    for t in &mut app.tabs {
        if t.tab.collection_id == Some(collection_id)
            && t.tab.request_id == Some(request_id)
            && matches!(t.content, WorkspaceContent::HttpRequest)
        {
            t.tab.name = new_name.clone();
        }
    }
    Task::none()
}

pub fn save_request_name_pressed(app: &mut Rustrest) -> Task<Message> {
    app.sidebar.editing_request_collection_id = None;
    app.sidebar.editing_request_id = None;
    Task::none()
}

pub fn toggle_saved_responses_collapsed(app: &mut Rustrest, request_id: usize) -> Task<Message> {
    if !app.sidebar.collapsed_saved_responses.remove(&request_id) {
        app.sidebar.collapsed_saved_responses.insert(request_id);
    }
    Task::none()
}

pub fn drag_started(app: &mut Rustrest, item: SidebarDragItem) -> Task<Message> {
    let key = SidebarItemKey::from_drag_item(&item);
    if !app.sidebar.selected_sidebar_items.contains(&key) {
        app.sidebar.selected_sidebar_items.clear();
        app.sidebar.sidebar_selection_anchor = None;
    }
    app.sidebar.sidebar_drag = Some(item);
    Task::none()
}

pub fn dropped(app: &mut Rustrest, target: crate::message::SidebarDropTarget) -> Task<Message> {
    if let Some(drag) = app.sidebar.sidebar_drag.take() {
        let dragged_key = SidebarItemKey::from_drag_item(&drag);
        let is_batch = app.sidebar.selected_sidebar_items.len() > 1
            && app.sidebar.selected_sidebar_items.contains(&dragged_key);

        // when the dragged row is part of a larger selection, move the
        // whole selection together; otherwise just the one dragged item.
        let items_to_move: Vec<SidebarDragItem> = if is_batch {
            let mut keys: Vec<SidebarItemKey> =
                app.sidebar.selected_sidebar_items.iter().cloned().collect();
            let folder_paths: Vec<(usize, Vec<String>)> = keys
                .iter()
                .filter_map(|k| match k {
                    SidebarItemKey::Folder {
                        collection_id,
                        path,
                    } => Some((*collection_id, path.clone())),
                    _ => None,
                })
                .collect();
            // drop whole-collection selections (collections don't move) and
            // any folder/request nested under another selected folder, since
            // moving that ancestor folder already relocates its descendants
            keys.retain(|k| match k {
                SidebarItemKey::Collection(_) => false,
                SidebarItemKey::Folder {
                    collection_id,
                    path,
                } => !folder_paths.iter().any(|(fc, fp)| {
                    fc == collection_id && fp.len() < path.len() && path.starts_with(fp.as_slice())
                }),
                SidebarItemKey::Request {
                    collection_id,
                    parent_path,
                    ..
                } => !folder_paths
                    .iter()
                    .any(|(fc, fp)| fc == collection_id && parent_path.starts_with(fp.as_slice())),
            });
            keys.into_iter().filter_map(|k| k.as_drag_item()).collect()
        } else {
            vec![drag]
        };

        for item in items_to_move {
            let dropped_on_self = matches!(
                (&item, &target),
                (
                    SidebarDragItem::Request { request_id: a, .. },
                    crate::message::SidebarDropTarget::Request { request_id: b, .. }
                ) if a == b
            );
            if dropped_on_self {
                continue;
            }

            let (dest_collection_id, dest_folder_path, before_request_id) = match &target {
                crate::message::SidebarDropTarget::Folder {
                    collection_id,
                    folder_path,
                } => (*collection_id, folder_path.clone(), None),
                crate::message::SidebarDropTarget::CollectionRoot(collection_id) => {
                    (*collection_id, Vec::new(), None)
                }
                crate::message::SidebarDropTarget::Request {
                    collection_id,
                    parent_path,
                    request_id,
                } => (*collection_id, parent_path.clone(), Some(*request_id)),
            };
            move_sidebar_item(
                &mut app.collections,
                item,
                dest_collection_id,
                dest_folder_path,
                before_request_id,
            );
        }

        app.sidebar.selected_sidebar_items.clear();
        app.sidebar.sidebar_selection_anchor = None;
    }
    Task::none()
}

pub fn item_toggle_select(app: &mut Rustrest, key: SidebarItemKey) -> Task<Message> {
    if !app.sidebar.selected_sidebar_items.remove(&key) {
        app.sidebar.selected_sidebar_items.insert(key.clone());
    }
    app.sidebar.sidebar_selection_anchor = Some(key);
    Task::none()
}

pub fn item_range_select(app: &mut Rustrest, key: SidebarItemKey) -> Task<Message> {
    let flat = flatten_visible_sidebar_items(app);
    let anchor = app
        .sidebar
        .sidebar_selection_anchor
        .clone()
        .unwrap_or_else(|| key.clone());

    let start = flat.iter().position(|k| *k == anchor);
    let end = flat.iter().position(|k| *k == key);
    match (start, end) {
        (Some(start), Some(end)) => {
            let (lo, hi) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            app.sidebar.selected_sidebar_items = flat[lo..=hi].iter().cloned().collect();
        }
        _ => {
            app.sidebar.selected_sidebar_items.insert(key);
        }
    }
    Task::none()
}

pub fn clear_selection(app: &mut Rustrest) -> Task<Message> {
    app.sidebar.selected_sidebar_items.clear();
    app.sidebar.sidebar_selection_anchor = None;
    Task::none()
}

pub fn batch_delete_selected_pressed(app: &mut Rustrest) -> Task<Message> {
    let count = app.sidebar.selected_sidebar_items.len();
    if count == 0 {
        return Task::none();
    }
    app.overlays.confirm_dialog = Some(ConfirmDialogState {
        title: "Delete Items".to_string(),
        message: format!(
            "Delete {count} selected item{}? This can't be undone.",
            if count == 1 { "" } else { "s" }
        ),
        confirm_label: "Delete".to_string(),
        on_confirm: Box::new(Message::BatchDeleteConfirmed),
    });
    Task::none()
}

pub fn batch_delete_confirmed(app: &mut Rustrest) -> Task<Message> {
    let items: Vec<SidebarItemKey> = app.sidebar.selected_sidebar_items.drain().collect();
    app.sidebar.sidebar_selection_anchor = None;

    let collections_to_delete: HashSet<usize> = items
        .iter()
        .filter_map(|k| match k {
            SidebarItemKey::Collection(id) => Some(*id),
            _ => None,
        })
        .collect();

    let mut folders_to_delete: Vec<(usize, Vec<String>)> = items
        .iter()
        .filter_map(|k| match k {
            SidebarItemKey::Folder {
                collection_id,
                path,
            } if !collections_to_delete.contains(collection_id) => {
                Some((*collection_id, path.clone()))
            }
            _ => None,
        })
        .collect();
    // drop folders nested inside another selected folder - deleting the
    // ancestor already removes it, so deleting it again would be a no-op
    // at best (and reorders the list at worst)
    let folders_snapshot = folders_to_delete.clone();
    folders_to_delete.retain(|(col_id, path)| {
        !folders_snapshot.iter().any(|(other_col, other_path)| {
            other_col == col_id
                && other_path.len() < path.len()
                && path.starts_with(other_path.as_slice())
        })
    });

    let requests_to_delete: Vec<(usize, Vec<String>, usize)> = items
        .iter()
        .filter_map(|k| match k {
            SidebarItemKey::Request {
                collection_id,
                parent_path,
                request_id,
            } if !collections_to_delete.contains(collection_id)
                && !folders_to_delete.iter().any(|(fc, fp)| {
                    fc == collection_id && parent_path.starts_with(fp.as_slice())
                }) =>
            {
                Some((*collection_id, parent_path.clone(), *request_id))
            }
            _ => None,
        })
        .collect();

    for col_id in &collections_to_delete {
        app.collections.retain(|c| c.id != *col_id);
    }
    app.tabs.retain(|t| {
        if let WorkspaceContent::CollectionRoot { collection_id, .. } = t.content {
            !collections_to_delete.contains(&collection_id)
        } else {
            true
        }
    });

    for (col_id, path) in &folders_to_delete {
        if let Some(col) = app.collections.iter_mut().find(|c| c.id == *col_id) {
            remove_nested(&mut col.item, path);
        }
    }

    for (col_id, parent_path, request_id) in &requests_to_delete {
        if let Some(col) = app.collections.iter_mut().find(|c| c.id == *col_id) {
            remove_nested_request(&mut col.item, parent_path, *request_id);
        }
    }

    // close any request tab whose backing request no longer exists
    // anywhere (covers requests removed directly, or nested under a
    // deleted folder/collection)
    app.tabs.retain(|t| match t.content {
        WorkspaceContent::HttpRequest => match t.tab.request_id {
            Some(req_id) => app
                .collections
                .iter()
                .any(|c| contains_request_node_by_id(&c.item, req_id)),
            None => true,
        },
        _ => true,
    });

    if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
        app.active_tab_index = app.tabs.len() - 1;
    }

    Task::none()
}

pub fn modifiers_changed(
    app: &mut Rustrest,
    modifiers: iced::keyboard::Modifiers,
) -> Task<Message> {
    app.sidebar.current_modifiers = modifiers;
    Task::none()
}

pub fn toggle_collection_collapsed(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    if !app.sidebar.collapsed_collections.remove(&col_id) {
        app.sidebar.collapsed_collections.insert(col_id);
    }
    Task::none()
}

pub fn toggle_folder_collapsed(
    app: &mut Rustrest,
    collection_id: usize,
    folder_path: Vec<String>,
) -> Task<Message> {
    let key = (collection_id, folder_path);
    if !app.sidebar.collapsed_folders.remove(&key) {
        app.sidebar.collapsed_folders.insert(key);
    }
    Task::none()
}
