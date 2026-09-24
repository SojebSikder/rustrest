//! Collection tree CRUD: import/export/save to disk, the collection-level
//! variables editor, and adding/deleting folders and requests. `collections`
//! itself stays a top-level field on `Rustrest` (not wrapped in its own
//! state struct) since every other field that used to live alongside it
//! (`next_collection_id_counter`/`next_request_id_counter`) turned out to be
//! dead code, removed as part of this reorganization.

use super::{CollectionSubTab, Rustrest, Tab, TabState, WorkspaceContent};
use crate::collection::collection::{
    CollectionInfo, CollectionItem, PostmanCollection, PostmanRequestDetails, PostmanRequestNode,
    PostmanUrl, PostmanVariable,
};
use crate::collection_adapter::create_tab_from_request;
use crate::message::Message;
use crate::ui::tab::types::KeyValuePair;
use crate::ui::toast::toast::ToastStatus;
use crate::utils::{insert_nested, insert_nested_request, remove_nested, remove_nested_request};
use crate::{APP_NAME, APP_VERSION};
use iced::Task;

fn default_headers() -> Vec<KeyValuePair> {
    vec![
        KeyValuePair::new("Content-Type", "application/json"),
        KeyValuePair::new("User-Agent", &format!("{}/{}", APP_NAME, APP_VERSION)),
        KeyValuePair::new("Accept", "*/*"),
        KeyValuePair::new("Connection", "keep-alive"),
    ]
}

pub fn import_pressed() -> Task<Message> {
    iced::Task::perform(
        async {
            let file_handle = rfd::AsyncFileDialog::new()
                .add_filter("Postman Collection (*.json)", &["json"])
                .pick_file()
                .await;

            if let Some(file) = file_handle {
                let path = file.path().to_path_buf();
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    return Some((path, content));
                }
            }
            None
        },
        |result| match result {
            Some((path, content)) => Message::CollectionLoaded(Some(path), content),
            None => Message::None,
        },
    )
}

pub fn loaded(
    app: &mut Rustrest,
    path: Option<std::path::PathBuf>,
    content: String,
) -> Task<Message> {
    match serde_json::from_str::<PostmanCollection>(&content) {
        Ok(mut collection) => {
            let col_name = collection.info.name.clone();
            collection.id = app.next_tab_id;
            collection.file_path = path;
            app.next_tab_id += 1;
            collection.assign_request_ids(&mut app.next_request_id);
            collection.set_headers(default_headers());
            app.collections.push(collection);

            iced::Task::done(Message::ShowToast(
                format!("Collection '{}' imported successfully", col_name),
                ToastStatus::Success,
            ))
        }
        Err(err) => {
            eprintln!(
                "Failed to parse Postman collection from {:?}: {}",
                path, err
            );
            iced::Task::done(Message::ShowToast(
                format!("Failed to load collection: {}", err),
                ToastStatus::Error,
            ))
        }
    }
}

/// flushes `col_id` to whatever location it's known to live at (a remote
/// SSH host, a git-backed folder, or a plain file); if it's never been
/// saved anywhere, just confirms the in-memory update.
pub fn persist_if_known_location(
    app: &mut Rustrest,
    col_id: usize,
    success_msg: String,
) -> Task<Message> {
    if let Some(collection) = app.collections.iter().find(|c| c.id == col_id) {
        if let Some(remote) = &collection.remote_dir {
            let profile_id = remote.profile_id;
            let root = remote.root.clone();
            return match app.remote.remote_sessions.get(&profile_id).cloned() {
                Some(session) => {
                    let collection = collection.clone();
                    Task::perform(
                        async move {
                            rustrest_remote::collection_sync::sync_collection_to_remote_dir(
                                &session, &collection, &root,
                            )
                            .await
                        },
                        move |result| match result {
                            Ok(()) => Message::ShowToast(success_msg.clone(), ToastStatus::Success),
                            Err(err) => Message::ShowToast(
                                format!("Saved in memory, but failed to reach remote host: {err}"),
                                ToastStatus::Error,
                            ),
                        },
                    )
                }
                None => Task::done(Message::ShowToast(
                    "Saved in memory, but not connected — reconnect to push changes to the remote host".to_string(),
                    ToastStatus::Error,
                )),
            };
        }

        if let Some(ref dir) = collection.storage_dir {
            return match crate::collection::dir_storage::save_collection_to_dir_clean(
                collection, dir,
            ) {
                Ok(()) => Task::done(Message::ShowToast(success_msg, ToastStatus::Success)),
                Err(err) => Task::done(Message::ShowToast(
                    format!("Saved in memory, but failed to write to disk: {}", err),
                    ToastStatus::Error,
                )),
            };
        }

        if let Some(ref path) = collection.file_path {
            if let Ok(json_content) = collection.to_postman_json() {
                let write_path = path.clone();
                return Task::perform(
                    async move { tokio::fs::write(write_path, json_content).await },
                    move |result| match result {
                        Ok(_) => Message::ShowToast(success_msg.clone(), ToastStatus::Success),
                        Err(err) => Message::ShowToast(
                            format!("Saved in memory, but failed to write to disk: {}", err),
                            ToastStatus::Error,
                        ),
                    },
                );
            }
        }
    }

    // no known location yet (never-saved collection)
    // nothing to flush to disk; just confirm the in-memory update.
    Task::done(Message::ShowToast(success_msg, ToastStatus::Success))
}

pub fn save_pressed(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    app.sync_collection_tabs(col_id);

    let has_known_location = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .map(|c| c.storage_dir.is_some() || c.file_path.is_some() || c.remote_dir.is_some())
        .unwrap_or(false);

    if !has_known_location {
        // never been saved anywhere: pick a file to save to and remember it,
        // so the collection stops showing as unsaved once it's written.
        return first_save_pressed(app, col_id);
    }

    if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
        col.clear_unsaved();
    }
    clear_tab_dirty_for_collection(app, col_id);

    persist_if_known_location(app, col_id, "Collection saved successfully".to_string())
}

/// clears the dirty flag on every open tab (request or collection root)
/// belonging to `col_id`, once its contents have just been persisted.
fn clear_tab_dirty_for_collection(app: &mut Rustrest, col_id: usize) {
    for tab_state in &mut app.tabs {
        let belongs = match &tab_state.content {
            WorkspaceContent::HttpRequest => tab_state.tab.collection_id == Some(col_id),
            WorkspaceContent::CollectionRoot { collection_id, .. } => *collection_id == col_id,
            WorkspaceContent::Terminal { .. } => false,
            WorkspaceContent::RemoteFile { .. } => false,
            WorkspaceContent::Plugin { .. } => false,
            WorkspaceContent::PluginManager | WorkspaceContent::ReleaseNotes(_) => false,
            WorkspaceContent::Folder(_) => false,
            WorkspaceContent::WebSocket(_)
            | WorkspaceContent::GraphQl(_)
            | WorkspaceContent::Grpc(_) => false,
        };
        if belongs {
            tab_state.tab.dirty = false;
        }
    }
}

/// the first save of a collection that has never been persisted anywhere:
/// prompts for a file, writes it, and (unlike a plain "Export As...", which
/// just makes a copy) remembers the chosen path on the collection so it's no
/// longer flagged unsaved.
fn first_save_pressed(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    let Some(collection) = app.collections.iter().find(|c| c.id == col_id) else {
        return iced::Task::none();
    };
    match collection.to_postman_json() {
        Ok(json_content) => {
            let default_name = format!("{}.postman_collection.json", collection.info.name);
            iced::Task::perform(
                async move {
                    let file_handle = rfd::AsyncFileDialog::new()
                        .set_title("Save Collection")
                        .set_file_name(&default_name)
                        .add_filter("Postman Collection (*.json)", &["json"])
                        .save_file()
                        .await?;

                    let path = file_handle.path().to_path_buf();
                    tokio::fs::write(&path, json_content).await.ok()?;
                    Some(path)
                },
                move |result| match result {
                    Some(path) => Message::CollectionFirstSaved(col_id, path),
                    None => Message::None,
                },
            )
        }
        Err(err_msg) => iced::Task::done(Message::ShowToast(
            format!("Save failed: {}", err_msg),
            ToastStatus::Error,
        )),
    }
}

/// completes `first_save_pressed`: remembers the chosen path on the
/// collection and clears every unsaved marker now that it's on disk.
pub fn first_saved(app: &mut Rustrest, col_id: usize, path: std::path::PathBuf) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
        col.file_path = Some(path);
        col.clear_unsaved();
    }
    clear_tab_dirty_for_collection(app, col_id);

    Task::done(Message::ShowToast(
        "Collection saved successfully".to_string(),
        ToastStatus::Success,
    ))
}

pub fn export_pressed(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    app.sync_collection_tabs(col_id);
    let Some(collection) = app.collections.iter().find(|c| c.id == col_id) else {
        return iced::Task::none();
    };
    match collection.to_postman_json() {
        Ok(json_content) => {
            let default_name = format!("{}.postman_collection.json", collection.info.name);
            iced::Task::perform(
                async move {
                    let file_handle = rfd::AsyncFileDialog::new()
                        .set_title("Export Postman Collection")
                        .set_file_name(&default_name)
                        .add_filter("Postman Collection (*.json)", &["json"])
                        .save_file()
                        .await?;

                    let path = file_handle.path().to_path_buf();
                    tokio::fs::write(&path, json_content).await.ok()?;
                    Some(path)
                },
                move |result| match result {
                    Some(path) => Message::ShowToast(
                        format!("Collection exported to {:?}", path),
                        ToastStatus::Success,
                    ),
                    None => Message::None,
                },
            )
        }
        Err(err_msg) => iced::Task::done(Message::ShowToast(
            format!("Export failed: {}", err_msg),
            ToastStatus::Error,
        )),
    }
}

pub fn sub_tab_selected(app: &mut Rustrest, sub_tab: CollectionSubTab) -> Task<Message> {
    let mut collection_id_for_git = None;
    if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
        if let WorkspaceContent::CollectionRoot {
            collection_id,
            ref mut active_sub_tab,
            ref mut docs,
            ..
        } = tab_state.content
        {
            *active_sub_tab = sub_tab.clone();
            if sub_tab == CollectionSubTab::Git {
                collection_id_for_git = Some(collection_id);
            }
            // reload on every visit so edits made from a Docs tab show up
            if sub_tab == CollectionSubTab::Documentation {
                *docs = app
                    .collections
                    .iter()
                    .find(|c| c.id == collection_id)
                    .map(|c| {
                        Box::new(crate::ui::docs_view::DocsState::new(
                            c,
                            rustrest_core::docs::DocsTarget::Collection,
                        ))
                    });
            }
        }
    }
    if let Some(col_id) = collection_id_for_git {
        return iced::Task::done(Message::GitStatusRequested(col_id));
    }
    Task::none()
}

pub fn variable_changed(
    app: &mut Rustrest,
    collection_id: usize,
    index: usize,
    key: String,
    value: String,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        let vars = col.variable.get_or_insert_with(Vec::new);
        if let Some(var) = vars.get_mut(index) {
            var.key = key;
            var.value = Some(serde_json::Value::String(value));
        }
        col.unsaved = true;
    }
    Task::none()
}

pub fn variable_toggled(
    app: &mut Rustrest,
    collection_id: usize,
    index: usize,
    is_active: bool,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        if let Some(ref mut vars) = col.variable {
            if let Some(var) = vars.get_mut(index) {
                var.r#type = Some(if is_active {
                    "string".to_string()
                } else {
                    "disabled".to_string()
                });
            }
        }
        col.unsaved = true;
    }
    Task::none()
}

pub fn add_variable_pressed(app: &mut Rustrest, collection_id: usize) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        let vars = col.variable.get_or_insert_with(Vec::new);
        vars.push(PostmanVariable {
            key: String::new(),
            value: Some(serde_json::Value::String(String::new())),
            r#type: Some("string".to_string()),
        });
        col.unsaved = true;
    }
    Task::none()
}

pub fn delete_variable_pressed(
    app: &mut Rustrest,
    collection_id: usize,
    index: usize,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        if let Some(ref mut vars) = col.variable {
            if index < vars.len() {
                vars.remove(index);
            }
        }
        col.unsaved = true;
    }
    Task::none()
}

pub fn create_new_pressed(app: &mut Rustrest) -> Task<Message> {
    let col_id = app.next_tab_id;
    app.next_tab_id += 1;

    let col_name = "New Collection".to_string();
    let new_col = PostmanCollection {
        id: col_id,
        info: CollectionInfo {
            name: col_name.clone(),
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
    };
    app.collections.push(new_col);

    // open the newly created collection's root tab
    let mut root_tab = Tab::new(app.next_tab_id);
    root_tab.name = col_name.clone();

    app.tabs.push(TabState {
        tab: root_tab,
        content: WorkspaceContent::CollectionRoot {
            collection_id: col_id,
            collection_name: col_name,
            active_sub_tab: CollectionSubTab::Variables,
            docs: None,
        },
        is_editing_name: false,
    });
    app.next_tab_id += 1;
    app.active_tab_index = app.tabs.len() - 1;
    iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
}

pub fn delete_pressed(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    app.collections.retain(|c| c.id != col_id);
    app.tabs.retain(|t| match &t.content {
        WorkspaceContent::CollectionRoot { collection_id, .. } => *collection_id != col_id,
        WorkspaceContent::Folder(docs) => docs.collection_id != col_id,
        _ => true,
    });
    if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
        app.active_tab_index = app.tabs.len() - 1;
    }
    Task::none()
}

pub fn add_folder_pressed(
    app: &mut Rustrest,
    collection_id: usize,
    parent_folder_path: Vec<String>,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        insert_nested(&mut col.item, &parent_folder_path);

        // make sure the new folder is visible in the sidebar
        app.sidebar.collapsed_collections.remove(&collection_id);
        for i in 0..=parent_folder_path.len() {
            app.sidebar
                .collapsed_folders
                .remove(&(collection_id, parent_folder_path[..i].to_vec()));
        }

        // open the newly created folder for renaming
        let mut new_folder_path = parent_folder_path.clone();
        new_folder_path.push("New Folder".to_string());
        app.sidebar.editing_folder_collection_id = Some(collection_id);
        app.sidebar.editing_folder_path = new_folder_path;
    }
    Task::none()
}

pub fn delete_folder_pressed(
    app: &mut Rustrest,
    collection_id: usize,
    folder_path: Vec<String>,
) -> Task<Message> {
    if !folder_path.is_empty() {
        if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
            remove_nested(&mut col.item, &folder_path);
        }
    }
    Task::none()
}

pub fn add_request_pressed(
    app: &mut Rustrest,
    collection_id: usize,
    parent_folder_path: Vec<String>,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        let req_id = app.next_request_id;
        app.next_request_id += 1;

        let new_request_node = PostmanRequestNode {
            id: req_id,
            name: "Untitled Request".to_string(),
            request: PostmanRequestDetails {
                method: "GET".to_string(),
                url: Some(PostmanUrl::String(String::new())),
                header: None,
                body: None,
                auth: None,
                description: None,
            },
            event: None,
            unsaved: true,
            response: None,
            protocol_request: None,
        };

        let tab_request_node = new_request_node.clone();

        insert_nested_request(
            &mut col.item,
            &parent_folder_path,
            CollectionItem::Request(new_request_node),
        );

        // make sure the new request is visible in the sidebar
        app.sidebar.collapsed_collections.remove(&collection_id);
        for i in 0..=parent_folder_path.len() {
            app.sidebar
                .collapsed_folders
                .remove(&(collection_id, parent_folder_path[..i].to_vec()));
        }

        // open the newly created request in a new tab
        let new_tab =
            create_tab_from_request(app.next_tab_id, &tab_request_node, Some(collection_id));
        app.tabs.push(TabState {
            tab: new_tab,
            content: WorkspaceContent::HttpRequest,
            is_editing_name: false,
        });
        app.next_tab_id += 1;
        app.active_tab_index = app.tabs.len() - 1;
        return iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id());
    }
    Task::none()
}

pub fn delete_request_pressed(
    app: &mut Rustrest,
    collection_id: usize,
    parent_folder_path: Vec<String>,
    request_id: usize,
) -> Task<Message> {
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        remove_nested_request(&mut col.item, &parent_folder_path, request_id);

        app.tabs.retain(|t| t.tab.request_id != Some(request_id));
        if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
            app.active_tab_index = app.tabs.len() - 1;
        }
    }
    Task::none()
}
