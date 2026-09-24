//! Executes a `CollectionOperation` a `RightPanel` plugin proposed (see
//! `plugins::propose_collection_op`) against the real collection tree,
//! reusing the same primitives the sidebar's own create/rename/delete/move
//! actions use (`crate::utils`, `rustrest_core::collection::tree_ops`).

use super::Rustrest;
use crate::collection::collection::{
    CollectionInfo, CollectionItem, PostmanCollection, PostmanRequestDetails, PostmanRequestNode,
    PostmanUrl,
};
use crate::message::SidebarDragItem;
use crate::utils::{
    find_request_mut, insert_nested_named, insert_nested_request, move_sidebar_item, remove_nested,
    remove_nested_request, rename_nested_folder,
};
use rustrest_plugin_host::CollectionOperation;

fn find_collection_mut(
    app: &mut Rustrest,
    collection_id: usize,
) -> Result<&mut PostmanCollection, String> {
    app.collections
        .iter_mut()
        .find(|c| c.id == collection_id)
        .ok_or_else(|| format!("no collection with id {collection_id}"))
}

/// makes sure `path` (and every ancestor of it) is expanded in the sidebar,
/// same bookkeeping `add_folder_pressed`/`add_request_pressed` do after
/// inserting a new item, so a plugin-created item is visible without the
/// user needing to expand anything.
fn reveal_in_sidebar(app: &mut Rustrest, collection_id: usize, path: &[String]) {
    app.sidebar.collapsed_collections.remove(&collection_id);
    for i in 0..=path.len() {
        app.sidebar
            .collapsed_folders
            .remove(&(collection_id, path[..i].to_vec()));
    }
}

pub fn apply(app: &mut Rustrest, op: CollectionOperation) -> Result<String, String> {
    match op {
        CollectionOperation::CreateCollection { name } => {
            let col_id = app.next_tab_id;
            app.next_tab_id += 1;
            app.collections.push(PostmanCollection {
                id: col_id,
                info: CollectionInfo {
                    name: name.clone(),
                    postman_id: None,
                    schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                        .to_string(),
                    description: None,
                },
                item: Vec::new(),
                variable: Some(Vec::new()),
                file_path: None,
                storage_dir: None,
                remote_dir: None,
                unsaved: false,
            });
            Ok(format!("Created collection \"{name}\""))
        }

        CollectionOperation::RenameCollection {
            collection_id,
            new_name,
        } => {
            let col = find_collection_mut(app, collection_id)?;
            col.rename(&new_name);
            Ok(format!(
                "Renamed collection #{collection_id} to \"{new_name}\""
            ))
        }

        CollectionOperation::DeleteCollection { collection_id } => {
            let existed = app.collections.iter().any(|c| c.id == collection_id);
            if !existed {
                return Err(format!("no collection with id {collection_id}"));
            }
            app.collections.retain(|c| c.id != collection_id);
            app.tabs.retain(|t| {
                !matches!(
                    t.content,
                    super::WorkspaceContent::CollectionRoot { collection_id: cid, .. } if cid == collection_id
                )
            });
            if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
                app.active_tab_index = app.tabs.len() - 1;
            }
            Ok(format!("Deleted collection #{collection_id}"))
        }

        CollectionOperation::CreateFolder {
            collection_id,
            parent_path,
            name,
        } => {
            let col = find_collection_mut(app, collection_id)?;
            insert_nested_named(&mut col.item, &parent_path, &name);
            let mut new_path = parent_path;
            new_path.push(name.clone());
            reveal_in_sidebar(app, collection_id, &new_path);
            Ok(format!("Created folder \"{name}\""))
        }

        CollectionOperation::RenameFolder {
            collection_id,
            path,
            new_name,
        } => {
            let col = find_collection_mut(app, collection_id)?;
            if !rename_nested_folder(&mut col.item, &path, &new_name) {
                return Err(format!(
                    "no folder at {} in collection #{collection_id}",
                    path.join("/")
                ));
            }
            Ok(format!(
                "Renamed folder {} to \"{new_name}\"",
                path.join("/")
            ))
        }

        CollectionOperation::DeleteFolder {
            collection_id,
            path,
        } => {
            if path.is_empty() {
                return Err("can't delete the collection root".to_string());
            }
            let col = find_collection_mut(app, collection_id)?;
            remove_nested(&mut col.item, &path);
            Ok(format!("Deleted folder {}", path.join("/")))
        }

        CollectionOperation::CreateRequest {
            collection_id,
            parent_path,
            name,
            method,
            url,
        } => {
            let req_id = app.next_request_id;
            app.next_request_id += 1;

            let col = find_collection_mut(app, collection_id)?;
            insert_nested_request(
                &mut col.item,
                &parent_path,
                CollectionItem::Request(PostmanRequestNode {
                    id: req_id,
                    name: name.clone(),
                    request: PostmanRequestDetails {
                        method,
                        url: Some(PostmanUrl::String(url)),
                        header: None,
                        body: None,
                        auth: None,
                        description: None,
                    },
                    event: None,
                    unsaved: true,
                    response: None,
                    protocol_request: None,
                }),
            );
            reveal_in_sidebar(app, collection_id, &parent_path);
            Ok(format!("Created request \"{name}\""))
        }

        CollectionOperation::RenameRequest {
            collection_id,
            request_id,
            new_name,
        } => {
            let col = find_collection_mut(app, collection_id)?;
            let Some(req) = find_request_mut(&mut col.item, request_id) else {
                return Err(format!("no request with id {request_id}"));
            };
            req.name = new_name.clone();
            req.unsaved = true;
            for tab in app.tabs.iter_mut() {
                if tab.tab.request_id == Some(request_id) {
                    tab.tab.name = new_name.clone();
                }
            }
            Ok(format!("Renamed request #{request_id} to \"{new_name}\""))
        }

        CollectionOperation::DeleteRequest {
            collection_id,
            parent_path,
            request_id,
        } => {
            let col = find_collection_mut(app, collection_id)?;
            remove_nested_request(&mut col.item, &parent_path, request_id);
            app.tabs.retain(|t| t.tab.request_id != Some(request_id));
            if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
                app.active_tab_index = app.tabs.len() - 1;
            }
            Ok(format!("Deleted request #{request_id}"))
        }

        CollectionOperation::DuplicateRequest {
            collection_id,
            parent_path,
            request_id,
        } => {
            let new_id = app.next_request_id;
            app.next_request_id += 1;

            let col = find_collection_mut(app, collection_id)?;
            let Some(original) = find_request_mut(&mut col.item, request_id) else {
                return Err(format!("no request with id {request_id}"));
            };
            let mut duplicate = original.clone();
            duplicate.id = new_id;
            duplicate.name = format!("{} Copy", duplicate.name);
            duplicate.unsaved = true;

            insert_nested_request(
                &mut col.item,
                &parent_path,
                CollectionItem::Request(duplicate),
            );
            Ok(format!("Duplicated request #{request_id}"))
        }

        CollectionOperation::MoveRequest {
            collection_id,
            from_path,
            request_id,
            to_path,
        } => {
            if !app.collections.iter().any(|c| c.id == collection_id) {
                return Err(format!("no collection with id {collection_id}"));
            }
            move_sidebar_item(
                &mut app.collections,
                SidebarDragItem::Request {
                    collection_id,
                    parent_path: from_path,
                    request_id,
                },
                collection_id,
                to_path.clone(),
                None,
            );
            reveal_in_sidebar(app, collection_id, &to_path);
            Ok(format!(
                "Moved request #{request_id} to {}",
                if to_path.is_empty() {
                    "the collection root".to_string()
                } else {
                    to_path.join("/")
                }
            ))
        }
    }
}
