use crate::collection::collection::{
    CollectionInfo, CollectionItem, PostmanCollection, PostmanRequestDetails, PostmanRequestNode,
    PostmanUrl, PostmanVariable,
};
use crate::collection::env::Environment;
use crate::collection_adapter::create_tab_from_request;
use crate::http_client::send_request;
use crate::message::{Message, ResizeKind};
use crate::session::{SavedSession, SavedTabEntry};
use crate::ui::menu::menu::DropdownMenuState;
use crate::ui::menu::menu_message::MenuMessage;
use crate::ui::save_request_model::types::SaveRequestModalState;
use crate::ui::tab::types::{KeyValuePair, ResponseView};
use crate::ui::tab::{Tab, TabMessage};
use crate::ui::toast::toast::{ToastManager, ToastStatus};
use crate::updater::{UpdateInfo, check_for_update, perform_update};
use crate::utils::{
    contains_request_node_by_id, format_json_or_fallback, insert_nested, insert_nested_request,
    remove_nested, remove_nested_request, rename_nested_folder, update_node,
};
use crate::workspace::{CollectionSource, SavedWorkspace, WorkspaceManifest};
use crate::{APP_NAME, APP_VERSION};
use iced::Task;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq)]
pub enum CollectionSubTab {
    Variables,
    Documentation,
}

#[derive(Debug, Clone)]
pub enum WorkspaceContent {
    HttpRequest,
    CollectionRoot {
        collection_id: usize,
        collection_name: String,
        active_sub_tab: CollectionSubTab,
    },
}

pub struct TabState {
    pub tab: Tab,
    pub content: WorkspaceContent,
    pub is_editing_name: bool,
}

pub struct ResizeDrag {
    pub kind: ResizeKind,
    pub start_cursor: iced::Point,
    pub start_size: f32,
}

pub const SIDEBAR_WIDTH_RANGE: (f32, f32) = (180.0, 520.0);
pub const REQUEST_PANE_HEIGHT_RANGE: (f32, f32) = (120.0, 700.0);
pub const CONSOLE_PANEL_HEIGHT_RANGE: (f32, f32) = (120.0, 500.0);

pub enum ContextMenu {
    Collection(usize),
    Folder {
        col_id: usize,
        path: Vec<String>,
    },
    Request {
        col_id: usize,
        folder_path: Vec<String>,
        req_id: usize,
    },
}

pub struct Rustrest {
    pub collections: Vec<PostmanCollection>,
    pub environments: Vec<Environment>,
    pub active_env_index: Option<usize>,
    pub editing_env_index: Option<usize>,
    pub editing_env_name: bool,
    pub tabs: Vec<TabState>,
    pub active_tab_index: usize,
    pub next_tab_id: usize,
    pub next_request_id: usize,

    pub workspaces: Vec<SavedWorkspace>,
    pub active_workspace_id: usize,
    pub next_workspace_id: usize,
    pub editing_workspace_id: Option<usize>,

    // Rename state management tracks
    pub editing_collection_id: Option<usize>,
    pub editing_folder_collection_id: Option<usize>,
    pub editing_folder_path: Vec<String>,
    pub active_context_menu: Option<ContextMenu>,
    pub context_menu_position: iced::Point,
    pub cursor_position: iced::Point,
    pub last_tab_name_click: Option<(usize, std::time::Instant)>,
    pub tab_rename_input_hovered: bool,

    pub next_collection_id_counter: usize,
    pub next_request_id_counter: usize,

    pub toast_manager: ToastManager,
    pub menu_state: DropdownMenuState,
    pub save_request_model: Option<SaveRequestModalState>,

    pub available_update: Option<UpdateInfo>,
    pub update_toast_id: Option<usize>,

    // panel resizing
    pub sidebar_width: f32,
    pub request_pane_height: f32,
    pub resize_drag: Option<ResizeDrag>,

    // console panel (global, bottom bar)
    pub console_logs: Vec<String>,
    pub console_collapsed: bool,
    pub console_panel_height: f32,
}

impl Rustrest {
    pub fn build_session_snapshot(&self) -> SavedSession {
        let tabs = self
            .tabs
            .iter()
            .filter_map(|t| match &t.content {
                WorkspaceContent::HttpRequest => {
                    let req_id = t.tab.request_id.unwrap_or(0);
                    let node = t.tab.to_postman_request_node(req_id, &t.tab.name);
                    Some(SavedTabEntry::HttpRequest {
                        request_id: t.tab.request_id,
                        collection_id: t.tab.collection_id,
                        node,
                    })
                }
                WorkspaceContent::CollectionRoot { collection_id, .. } => {
                    Some(SavedTabEntry::CollectionRoot {
                        collection_id: *collection_id,
                    })
                }
            })
            .collect();

        SavedSession {
            tabs,
            active_tab_index: self.active_tab_index,
            next_tab_id: self.next_tab_id,
            next_request_id: self.next_request_id,
        }
    }

    /// builds a `SavedWorkspace` record for the currently active workspace
    /// from the live app state. Collections that were never saved to a file
    /// or git folder have no known location to reload from, so they're
    /// dropped from the snapshot
    pub fn snapshot_active_workspace(&self) -> (SavedWorkspace, usize) {
        let mut collection_sources = Vec::new();
        let mut dropped = 0;
        for c in &self.collections {
            if let Some(dir) = &c.storage_dir {
                collection_sources.push(CollectionSource::Dir(dir.clone()));
            } else if let Some(path) = &c.file_path {
                collection_sources.push(CollectionSource::File(path.clone()));
            } else {
                dropped += 1;
            }
        }

        let name = self
            .workspaces
            .iter()
            .find(|w| w.id == self.active_workspace_id)
            .map(|w| w.name.clone())
            .unwrap_or_else(|| "Workspace".to_string());

        let ws = SavedWorkspace {
            id: self.active_workspace_id,
            name,
            collection_sources,
            environments: self.environments.clone(),
            active_env_index: self.active_env_index,
            session: self.build_session_snapshot(),
        };
        (ws, dropped)
    }

    /// snapshots the active workspace and writes it back into `self.workspaces`
    pub fn commit_active_workspace_snapshot(&mut self) -> usize {
        let (ws, dropped) = self.snapshot_active_workspace();
        if let Some(existing) = self.workspaces.iter_mut().find(|w| w.id == ws.id) {
            *existing = ws;
        } else {
            self.workspaces.push(ws);
        }
        dropped
    }

    pub fn build_workspace_manifest(&self) -> WorkspaceManifest {
        WorkspaceManifest {
            workspaces: self.workspaces.clone(),
            active_workspace_id: self.active_workspace_id,
            next_workspace_id: self.next_workspace_id,
        }
    }

    /// makes `ws` the live workspace: clears current collections/tabs, reloads
    /// `ws`'s collections from their remembered file/folder locations, adopts
    /// its environments and restores its tabs
    pub fn apply_workspace(&mut self, ws: &SavedWorkspace) -> Vec<String> {
        self.collections.clear();
        self.tabs.clear();
        self.active_tab_index = 0;

        let mut errors = Vec::new();
        for source in &ws.collection_sources {
            match load_collection_from_source(source) {
                Ok(mut collection) => {
                    collection.id = self.next_tab_id;
                    self.next_tab_id += 1;
                    collection.assign_request_ids(&mut self.next_request_id);
                    self.collections.push(collection);
                }
                Err(err) => errors.push(err),
            }
        }

        self.environments = ws.environments.clone();
        self.active_env_index = ws.active_env_index;

        restore_session_into_app(self, &ws.session);

        errors
    }

    // syncs the collection tabs to the collection's current state,
    // pushing any in-memory changes back into the collection tree.
    pub fn sync_collection_tabs(&mut self, col_id: usize) {
        for idx in 0..self.tabs.len() {
            let belongs = match &self.tabs[idx].content {
                WorkspaceContent::HttpRequest => self.tabs[idx].tab.collection_id == Some(col_id),
                WorkspaceContent::CollectionRoot { collection_id, .. } => *collection_id == col_id,
            };
            if belongs {
                self.sync_tab_to_collection(idx);
            }
        }
    }

    pub fn sync_active_tab_to_collection(&mut self) {
        self.sync_tab_to_collection(self.active_tab_index);
    }

    /// pushes a tab's current in-memory state (name, headers, body, etc.)
    /// back into the collection tree that owns it, so the sidebar and any
    /// exported/saved output stay in sync with what's shown in the tab.
    pub fn sync_tab_to_collection(&mut self, idx: usize) {
        if let Some(tab_state) = self.tabs.get(idx) {
            match &tab_state.content {
                WorkspaceContent::HttpRequest => {
                    if let (Some(req_id), Some(col_id)) =
                        (tab_state.tab.request_id, tab_state.tab.collection_id)
                    {
                        if let Some(col) = self.collections.iter_mut().find(|c| c.id == col_id) {
                            update_node(&mut col.item, req_id, &tab_state.tab);
                        }
                    }
                }
                WorkspaceContent::CollectionRoot { collection_id, .. } => {
                    let col_id = *collection_id;
                    let new_name = tab_state.tab.name.clone();
                    if let Some(col) = self.collections.iter_mut().find(|c| c.id == col_id) {
                        col.info.name = new_name.clone();
                    }
                    // keep any other open CollectionRoot tabs for the same collection in sync
                    for t in &mut self.tabs {
                        if let WorkspaceContent::CollectionRoot {
                            collection_id,
                            collection_name,
                            ..
                        } = &mut t.content
                        {
                            if *collection_id == col_id {
                                *collection_name = new_name.clone();
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn init() -> (Rustrest, Task<Message>) {
    let mut app = Rustrest {
        collections: Vec::new(),
        environments: Vec::new(),
        active_env_index: None,
        tabs: vec![],
        active_tab_index: 0,
        editing_env_index: None,
        next_tab_id: 2,
        next_request_id: 1,
        workspaces: Vec::new(),
        active_workspace_id: 1,
        next_workspace_id: 2,
        editing_workspace_id: None,
        editing_collection_id: None,
        editing_folder_collection_id: None,
        editing_folder_path: Vec::new(),
        active_context_menu: None,
        context_menu_position: iced::Point::ORIGIN,
        cursor_position: iced::Point::ORIGIN,
        last_tab_name_click: None,
        tab_rename_input_hovered: false,
        next_collection_id_counter: 0,
        next_request_id_counter: 0,
        toast_manager: ToastManager::new(),
        menu_state: DropdownMenuState::new(),
        save_request_model: None,
        editing_env_name: false,
        available_update: None,
        update_toast_id: None,
        sidebar_width: 260.0,
        request_pane_height: 320.0,
        resize_drag: None,
        console_logs: Vec::new(),
        console_collapsed: true,
        console_panel_height: 220.0,
    };

    let load_errors = if let Some(manifest) = crate::workspace::load() {
        app.workspaces = manifest.workspaces;
        app.active_workspace_id = manifest.active_workspace_id;
        app.next_workspace_id = manifest.next_workspace_id;

        if app.workspaces.is_empty() {
            let default_ws = default_workspace(1, None);
            app.workspaces.push(default_ws);
            app.active_workspace_id = 1;
            app.next_workspace_id = 2;
        }

        let active = app
            .workspaces
            .iter()
            .find(|w| w.id == app.active_workspace_id)
            .or_else(|| app.workspaces.first())
            .cloned();

        match active {
            Some(active) => {
                app.active_workspace_id = active.id;
                app.apply_workspace(&active)
            }
            None => Vec::new(),
        }
    } else {
        // first run, or upgrading from a pre-workspace install: best-effort
        // migrate any tabs from the legacy session.json into a new default
        // workspace, then persist the manifest so this only runs once.
        let legacy_session = crate::session::load();
        let default_ws = default_workspace(1, legacy_session);
        app.workspaces = vec![default_ws.clone()];
        app.active_workspace_id = 1;
        app.next_workspace_id = 2;
        let errors = app.apply_workspace(&default_ws);
        crate::workspace::save(&app.build_workspace_manifest());
        errors
    };

    if app.tabs.is_empty() {
        app.tabs.push(TabState {
            tab: Tab::new(app.next_tab_id),
            content: WorkspaceContent::HttpRequest,
            is_editing_name: false,
        });
        app.next_tab_id += 1;
    }

    let startup_task = if load_errors.is_empty() {
        Task::none()
    } else {
        Task::batch(
            load_errors
                .into_iter()
                .map(|err| Task::done(Message::ShowToast(err, ToastStatus::Error))),
        )
    };

    (app, startup_task)
}

fn default_workspace(id: usize, legacy_session: Option<SavedSession>) -> SavedWorkspace {
    let mut demo_env = Environment::new("Default");
    if !demo_env.variables.is_empty() {
        demo_env.variables[0].is_active = true;
    }

    SavedWorkspace {
        id,
        name: "Default".to_string(),
        collection_sources: Vec::new(),
        environments: vec![demo_env],
        active_env_index: None,
        session: legacy_session.unwrap_or(SavedSession {
            tabs: Vec::new(),
            active_tab_index: 0,
            next_tab_id: 0,
            next_request_id: 0,
        }),
    }
}

fn load_collection_from_source(source: &CollectionSource) -> Result<PostmanCollection, String> {
    match source {
        CollectionSource::File(path) => {
            let content = std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read {:?}: {e}", path))?;
            let mut collection = serde_json::from_str::<PostmanCollection>(&content)
                .map_err(|e| format!("Failed to parse {:?}: {e}", path))?;
            collection.file_path = Some(path.clone());
            Ok(collection)
        }
        CollectionSource::Dir(dir) => crate::collection::dir_storage::load_collection_from_dir(dir),
    }
}

/// restores tabs from a saved session into `app`
fn restore_session_into_app(app: &mut Rustrest, saved: &SavedSession) {
    for entry in &saved.tabs {
        match entry {
            SavedTabEntry::HttpRequest {
                collection_id,
                request_id,
                node,
            } => {
                let mut tab = create_tab_from_request(app.next_tab_id, node, *collection_id);
                tab.request_id = *request_id;
                app.tabs.push(TabState {
                    tab,
                    content: WorkspaceContent::HttpRequest,
                    is_editing_name: false,
                });
                app.next_tab_id += 1;
            }
            SavedTabEntry::CollectionRoot { collection_id } => {
                let collection_id = *collection_id;
                if let Some(col) = app.collections.iter().find(|c| c.id == collection_id) {
                    let mut root_tab = Tab::new(app.next_tab_id);
                    root_tab.name = col.info.name.clone();
                    app.tabs.push(TabState {
                        tab: root_tab,
                        content: WorkspaceContent::CollectionRoot {
                            collection_id,
                            collection_name: col.info.name.clone(),
                            active_sub_tab: CollectionSubTab::Variables,
                        },
                        is_editing_name: false,
                    });
                    app.next_tab_id += 1;
                }
            }
        }
    }
    app.active_tab_index = saved.active_tab_index.min(app.tabs.len().saturating_sub(1));
    app.next_request_id = saved.next_request_id.max(app.next_request_id);
}

fn persist_collection_if_known_location(
    app: &mut Rustrest,
    col_id: usize,
    success_msg: String,
) -> Task<Message> {
    if let Some(collection) = app.collections.iter().find(|c| c.id == col_id) {
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

    // no known location yet (brand new, never-saved collection) —
    // nothing to flush to disk; just confirm the in-memory update.
    Task::done(Message::ShowToast(success_msg, ToastStatus::Success))
}

fn finalize_tab_rename(app: &mut Rustrest, idx: usize) {
    if let Some(tab_state) = app.tabs.get_mut(idx) {
        tab_state.is_editing_name = false;
        if tab_state.tab.name.trim().is_empty() {
            tab_state.tab.name = match &tab_state.content {
                WorkspaceContent::HttpRequest => "Untitled Request".to_string(),
                WorkspaceContent::CollectionRoot {
                    collection_name, ..
                } => collection_name.clone(),
            };
        }
    }

    app.tab_rename_input_hovered = false;
    app.sync_tab_to_collection(idx);
}

pub fn update(app: &mut Rustrest, message: Message) -> Task<Message> {
    match message {
        Message::None => Task::none(),
        Message::ImportCollectionPressed => {
            iced::Task::perform(
                async {
                    // open the file dialog and await selection
                    let file_handle = rfd::AsyncFileDialog::new()
                        .add_filter("Postman Collection (*.json)", &["json"])
                        .pick_file()
                        .await;

                    // if a file was selected, read its contents
                    if let Some(file) = file_handle {
                        let path = file.path().to_path_buf();
                        if let Ok(content) = tokio::fs::read_to_string(&path).await {
                            return Some((path, content));
                        }
                    }
                    None
                },
                |result| {
                    // map the final result back to a single Message
                    if let Some((path, content)) = result {
                        Message::CollectionLoaded(Some(path), content)
                    } else {
                        Message::None
                    }
                },
            )
        }

        // process file contents once loaded from disk
        Message::CollectionLoaded(path, content) => {
            match serde_json::from_str::<PostmanCollection>(&content) {
                Ok(mut collection) => {
                    let col_name = collection.info.name.clone();
                    collection.id = app.next_tab_id;
                    collection.file_path = path;
                    app.next_tab_id += 1;

                    collection.assign_request_ids(&mut app.next_request_id);

                    // set default headers for the collection
                    let default_headers: Vec<KeyValuePair> = vec![
                        KeyValuePair::new("Content-Type", "application/json"),
                        KeyValuePair::new("User-Agent", &format!("{}/{}", APP_NAME, APP_VERSION)),
                        KeyValuePair::new("Accept", "*/*"),
                        KeyValuePair::new("Connection", "keep-alive"),
                    ];
                    collection.set_headers(default_headers);

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

        // simple inline disk overwrite action
        Message::SaveCollectionPressed(col_id) => {
            app.sync_collection_tabs(col_id);

            let has_known_location = app
                .collections
                .iter()
                .find(|c| c.id == col_id)
                .map(|c| c.storage_dir.is_some() || c.file_path.is_some())
                .unwrap_or(false);

            if !has_known_location {
                // never been saved anywhere, behave like export (first-time save)
                return iced::Task::done(Message::ExportCollectionPressed(col_id));
            }

            persist_collection_if_known_location(
                app,
                col_id,
                "Collection saved successfully".to_string(),
            )
        }

        // git
        // Point an existing (or new) collection at a git-friendly folder on disk.
        Message::InitGitCollectionPressed(col_id) => iced::Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Choose folder for git collection")
                    .pick_folder()
                    .await
                    .map(|h| h.path().to_path_buf())
            },
            move |path| Message::GitCollectionDirChosen(col_id, path),
        ),

        Message::GitCollectionDirChosen(col_id, Some(dir)) => {
            app.sync_collection_tabs(col_id);
            if let Some(collection) = app.collections.iter_mut().find(|c| c.id == col_id) {
                collection.storage_dir = Some(dir.clone());
                match crate::collection::dir_storage::save_collection_to_dir_clean(collection, &dir)
                {
                    Ok(()) => {
                        return Task::done(Message::ShowToast(
                            format!("Collection now stored at {:?}", dir),
                            ToastStatus::Success,
                        ));
                    }
                    Err(e) => {
                        return Task::done(Message::ShowToast(
                            format!("Failed to initialize git folder: {e}"),
                            ToastStatus::Error,
                        ));
                    }
                }
            }
            Task::none()
        }
        Message::GitCollectionDirChosen(_, None) => Task::none(),

        // Import: pick a folder, try to load it as a directory-backed collection.
        Message::ImportGitCollectionPressed => iced::Task::perform(
            async {
                let dir = rfd::AsyncFileDialog::new()
                    .set_title("Open git collection folder")
                    .pick_folder()
                    .await
                    .map(|h| h.path().to_path_buf());

                match dir {
                    Some(path) => {
                        let result =
                            crate::collection::dir_storage::load_collection_from_dir(&path);
                        (Some(path), result)
                    }
                    None => (None, Err("No folder selected".to_string())),
                }
            },
            |(path, result)| Message::GitCollectionLoaded(path, result),
        ),

        Message::GitCollectionLoaded(Some(path), Ok(mut collection)) => {
            let col_name = collection.info.name.clone();
            collection.id = app.next_tab_id;
            app.next_tab_id += 1;
            collection.assign_request_ids(&mut app.next_request_id);
            app.collections.push(collection);

            Task::done(Message::ShowToast(
                format!("Collection '{}' loaded from {:?}", col_name, path),
                ToastStatus::Success,
            ))
        }
        Message::GitCollectionLoaded(_, Err(e)) => Task::done(Message::ShowToast(
            format!("Failed to load git collection: {e}"),
            ToastStatus::Error,
        )),
        Message::GitCollectionLoaded(None, _) => Task::none(),
        // end git
        Message::ExportCollectionPressed(col_id) => {
            app.sync_collection_tabs(col_id);
            // find collection by internal ID
            if let Some(collection) = app.collections.iter().find(|c| c.id == col_id) {
                match collection.to_postman_json() {
                    Ok(json_content) => {
                        let default_name =
                            format!("{}.postman_collection.json", collection.info.name);

                        // open save-file window dialog and write content asynchronously
                        return iced::Task::perform(
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
                        );
                    }
                    Err(err_msg) => {
                        return iced::Task::done(Message::ShowToast(
                            format!("Export failed: {}", err_msg),
                            ToastStatus::Error,
                        ));
                    }
                }
            }
            iced::Task::none()
        }

        Message::SidebarCollectionRootClicked(col_id) => {
            let existing_tab_idx = app.tabs.iter().position(|t| {
                if let WorkspaceContent::CollectionRoot { collection_id, .. } = t.content {
                    collection_id == col_id
                } else {
                    false
                }
            });

            if let Some(idx) = existing_tab_idx {
                app.active_tab_index = idx;
            } else if let Some(col) = app.collections.iter().find(|c| c.id == col_id) {
                let mut root_tab = Tab::new(app.next_tab_id);
                root_tab.name = col.info.name.clone();

                app.tabs.push(TabState {
                    tab: root_tab,
                    content: WorkspaceContent::CollectionRoot {
                        collection_id: col_id,
                        collection_name: col.info.name.clone(),
                        active_sub_tab: CollectionSubTab::Variables,
                    },
                    is_editing_name: false,
                });
                app.next_tab_id += 1;
                app.active_tab_index = app.tabs.len() - 1;
            }
            Task::none()
        }

        Message::SidebarRequestClicked(req_node) => {
            let existing_tab_idx = app.tabs.iter().position(|t| {
                t.tab.request_id == Some(req_node.id)
                    && matches!(t.content, WorkspaceContent::HttpRequest)
            });

            if let Some(idx) = existing_tab_idx {
                app.active_tab_index = idx;
            } else {
                let associated_collection_id = app
                    .collections
                    .iter()
                    .find(|c| contains_request_node_by_id(&c.item, req_node.id))
                    .map(|c| c.id);

                let new_tab =
                    create_tab_from_request(app.next_tab_id, &req_node, associated_collection_id);

                app.tabs.push(TabState {
                    tab: new_tab,
                    content: WorkspaceContent::HttpRequest,
                    is_editing_name: false,
                });
                app.next_tab_id += 1;
                app.active_tab_index = app.tabs.len() - 1;
            }
            Task::none()
        }

        Message::TabSelected(index) => {
            if index < app.tabs.len() {
                app.active_tab_index = index;
            }
            Task::none()
        }

        Message::NewTabPressed => {
            app.tabs.push(TabState {
                tab: Tab::new(app.next_tab_id),
                content: WorkspaceContent::HttpRequest,
                is_editing_name: false,
            });
            app.active_tab_index = app.tabs.len() - 1;
            app.next_tab_id += 1;
            Task::none()
        }

        Message::CloseTabPressed(index) => {
            if app.tabs.len() > 1 {
                if let Some(tab_state) = app.tabs.get(index) {
                    if tab_state.tab.is_loading {
                        tab_state.tab.cancel_token.cancel();
                    }
                }
                app.tabs.remove(index);
                if app.active_tab_index >= app.tabs.len() {
                    app.active_tab_index = app.tabs.len() - 1;
                }
            }
            Task::none()
        }

        Message::ActiveTabMessage(tab_msg) => {
            if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
                if let TabMessage::ResponseViewChanged(view) = tab_msg {
                    tab_state.tab.response_view = view;
                    if let Some(Ok(resp)) = &tab_state.tab.response {
                        let body_text = match view {
                            ResponseView::Json => format_json_or_fallback(&resp.body),
                            ResponseView::Raw => resp.body.clone(),
                        };
                        tab_state.tab.response_body_editor =
                            iced::widget::text_editor::Content::with_text(&body_text);
                    }
                } else {
                    tab_state.tab.update(tab_msg);
                }
            }
            Task::none()
        }

        // script engine used
        Message::SendPressed => {
            if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
                if let WorkspaceContent::CollectionRoot { .. } = tab_state.content {
                    return Task::none();
                }
                let tab = &mut tab_state.tab;
                if tab.is_loading || tab.url.is_empty() {
                    return Task::none();
                }

                app.console_logs.clear();

                // build variable/header maps for the pre-request script
                let mut script_vars: std::collections::HashMap<String, String> = app
                    .active_env_index
                    .and_then(|i| app.environments.get(i))
                    .map(|e| {
                        e.variables
                            .iter()
                            .filter(|v| v.is_active)
                            .map(|v| (v.key.clone(), v.value.clone()))
                            .collect()
                    })
                    .unwrap_or_default();

                if let Some(c_id) = tab.collection_id {
                    if let Some(col) = app.collections.iter().find(|c| c.id == c_id) {
                        for kv in col.get_native_variables() {
                            script_vars.insert(kv.key, kv.value);
                        }
                    }
                }

                let mut script_headers: std::collections::HashMap<String, String> = tab
                    .request_headers
                    .iter()
                    .filter(|h| h.is_active)
                    .map(|h| (h.key.clone(), h.value.clone()))
                    .collect();

                let pre_script_text = tab.pre_request_script.text();
                match crate::script_engine::ScriptRunner::run_pre_request(
                    &pre_script_text,
                    &mut script_vars,
                    &mut script_headers,
                ) {
                    Ok(logs) => app.console_logs.extend(logs),
                    Err(e) => return Task::done(Message::ShowToast(e, ToastStatus::Error)),
                }

                let mut effective_env = app
                    .active_env_index
                    .and_then(|i| app.environments.get(i))
                    .cloned()
                    .unwrap_or_else(|| Environment::new("__script"));

                for (k, v) in &script_vars {
                    if let Some(existing) =
                        effective_env.variables.iter_mut().find(|kv| &kv.key == k)
                    {
                        existing.value = v.clone();
                        existing.is_active = true;
                    } else {
                        let mut kv = KeyValuePair::new(k, v);
                        kv.is_active = true;
                        effective_env.variables.push(kv);
                    }
                }

                let tab_id = tab.id;
                tab.cancel_token = CancellationToken::new();
                tab.is_loading = true;
                tab.response = None;

                let collection_vars = tab
                    .collection_id
                    .and_then(|c_id| app.collections.iter().find(|c| c.id == c_id))
                    .map(|c| c.get_native_variables());

                let (
                    final_url,
                    compiled_body,
                    compiled_form_data,
                    mut filtered_headers,
                    filtered_cookies,
                    compiled_auth,
                ) = tab.compile_request_fields(&Some(effective_env), collection_vars.as_deref());

                // apply any header overrides the script made via pm.setHeader(...)
                for (k, v) in script_headers {
                    if let Some(existing) = filtered_headers.iter_mut().find(|(hk, _)| hk == &k) {
                        existing.1 = v;
                    } else {
                        filtered_headers.push((k, v));
                    }
                }

                let spec = crate::http_client::RequestSpec::new(final_url, tab.method.clone())
                    .body_type(tab.body_type)
                    .raw_body(compiled_body)
                    .form_data(compiled_form_data)
                    .binary_file_path(tab.binary_file_path.clone())
                    .headers(filtered_headers)
                    .cookies(filtered_cookies)
                    .auth_raw(compiled_auth);

                return Task::perform(send_request(spec, tab.cancel_token.clone()), move |res| {
                    Message::ResponseReceived(tab_id, res)
                });
            }
            Task::none()
        }

        // script engine used
        Message::ResponseReceived(tab_id, res) => {
            if let Some(tab_state) = app.tabs.iter_mut().find(|t| t.tab.id == tab_id) {
                let tab = &mut tab_state.tab;
                tab.is_loading = false;

                match &res {
                    Ok(resp) => {
                        let initial_body = match tab.response_view {
                            ResponseView::Json => format_json_or_fallback(&resp.body),
                            ResponseView::Raw => resp.body.clone(),
                        };
                        tab.response_body_editor =
                            iced::widget::text_editor::Content::with_text(&initial_body);
                    }
                    Err(err_msg) => {
                        tab.response_body_editor =
                            iced::widget::text_editor::Content::with_text(err_msg);
                    }
                }

                if let Ok(resp) = &res {
                    let script_text = tab.post_response_script.text();
                    if !script_text.trim().is_empty() {
                        let base_vars: std::collections::HashMap<String, String> = app
                            .active_env_index
                            .and_then(|idx| app.environments.get(idx))
                            .map(|e| {
                                e.variables
                                    .iter()
                                    .filter(|v| v.is_active)
                                    .map(|v| (v.key.clone(), v.value.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();

                        let exec_ctx = crate::script_engine::ScriptExecutionContext {
                            variables: base_vars,
                            // request_headers: std::collections::HashMap::new(),
                            response_status: resp.status,
                            response_body: resp.body.clone(),
                        };

                        match crate::script_engine::ScriptRunner::run_post_response(
                            &script_text,
                            &exec_ctx,
                        ) {
                            Ok((updated_vars, logs)) => {
                                app.console_logs.extend(logs);
                                if let Some(idx) = app.active_env_index {
                                    if let Some(env) = app.environments.get_mut(idx) {
                                        for (k, v) in updated_vars {
                                            if let Some(existing) =
                                                env.variables.iter_mut().find(|kv| kv.key == k)
                                            {
                                                existing.value = v;
                                                existing.is_active = true;
                                            } else {
                                                let mut kv = KeyValuePair::new(&k, &v);
                                                kv.is_active = true;
                                                env.variables.push(kv);
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let toast_task = crate::ui::toast::toast::show_and_schedule(
                                    &mut app.toast_manager,
                                    e,
                                    ToastStatus::Error,
                                    crate::ui::toast::toast::TOAST_DURATION,
                                );
                                tab.response = Some(res);
                                return toast_task;
                            }
                        }
                    }
                }

                tab.response = Some(res);
            }
            Task::none()
        }

        Message::TabNameDoubleClick(idx) => {
            const DOUBLE_CLICK_THRESHOLD: std::time::Duration =
                std::time::Duration::from_millis(400);

            if idx < app.tabs.len() {
                app.active_tab_index = idx;
            }

            let is_double_click = matches!(
                app.last_tab_name_click,
                Some((last_idx, last_time))
                    if last_idx == idx && last_time.elapsed() < DOUBLE_CLICK_THRESHOLD
            );

            if is_double_click {
                app.last_tab_name_click = None;
                if let Some(tab_state) = app.tabs.get_mut(idx) {
                    tab_state.is_editing_name = true;
                }
                // the double click happened right on the name, so the
                // cursor is over the input the moment it appears
                app.tab_rename_input_hovered = true;
            } else {
                app.last_tab_name_click = Some((idx, std::time::Instant::now()));
            }
            Task::none()
        }

        Message::TabNameChanged(idx, new_name) => {
            if let Some(tab_state) = app.tabs.get_mut(idx) {
                tab_state.tab.name = new_name;
            }
            Task::none()
        }

        Message::TabNameSave(idx) => {
            finalize_tab_rename(app, idx);
            Task::none()
        }

        Message::TabRenameBlur => {
            if !app.tab_rename_input_hovered {
                if let Some(idx) = app.tabs.iter().position(|t| t.is_editing_name) {
                    finalize_tab_rename(app, idx);
                }
            }
            Task::none()
        }

        Message::TabRenameInputHover(is_hovered) => {
            app.tab_rename_input_hovered = is_hovered;
            Task::none()
        }

        Message::EnvSelected(selected_name) => {
            if let Some(name) = selected_name {
                app.active_env_index = app.environments.iter().position(|e| e.name == name);
            } else {
                app.active_env_index = None;
            }
            Task::none()
        }

        Message::CreateEnvironmentPressed => {
            let new_count = app.environments.len() + 1;
            let new_env_name = format!("Environment {}", new_count);

            // add a new environment instance
            app.environments.push(Environment {
                name: new_env_name,
                variables: Vec::new(),
            });

            // auto-select the newly created environment
            app.active_env_index = Some(app.environments.len() - 1);

            Task::none()
        }

        Message::DeleteEnvironmentPressed(idx) => {
            if idx < app.environments.len() {
                app.environments.remove(idx);

                // adjust active environment index safely
                if app.environments.is_empty() {
                    app.active_env_index = None;
                } else if let Some(active) = app.active_env_index {
                    if active == idx {
                        // if we deleted the active item, fallback to previous or first item
                        app.active_env_index = Some(idx.saturating_sub(1));
                    } else if active > idx {
                        // shift index back if an item before it was removed
                        app.active_env_index = Some(active - 1);
                    }
                }
            }

            Task::none()
        }

        Message::CollectionSubTabSelected(sub_tab) => {
            if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
                if let WorkspaceContent::CollectionRoot {
                    ref mut active_sub_tab,
                    ..
                } = tab_state.content
                {
                    *active_sub_tab = sub_tab;
                }
            }
            Task::none()
        }

        Message::CollectionVariableChanged {
            collection_id,
            index,
            key,
            value,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                let vars = col.variable.get_or_insert_with(Vec::new);
                if let Some(var) = vars.get_mut(index) {
                    var.key = key;
                    var.value = Some(serde_json::Value::String(value));
                }
            }
            Task::none()
        }

        Message::CollectionVariableToggled {
            collection_id,
            index,
            is_active,
        } => {
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
            }
            Task::none()
        }

        Message::AddCollectionVariablePressed(collection_id) => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                let vars = col.variable.get_or_insert_with(Vec::new);
                vars.push(PostmanVariable {
                    key: String::new(),
                    value: Some(serde_json::Value::String(String::new())),
                    r#type: Some("string".to_string()),
                });
            }
            Task::none()
        }

        Message::DeleteCollectionVariablePressed(collection_id, index) => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                if let Some(ref mut vars) = col.variable {
                    if index < vars.len() {
                        vars.remove(index);
                    }
                }
            }
            Task::none()
        }

        Message::CreateNewCollectionPressed => {
            let col_id = app.next_tab_id;
            app.next_tab_id += 1;

            let new_col = PostmanCollection {
                id: col_id,
                info: CollectionInfo {
                    name: format!("New Collection {}", col_id),
                    postman_id: None,
                    schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                        .to_string(),
                },
                item: Vec::new(),
                variable: Some(Vec::new()),
                file_path: None,
                storage_dir: None,
            };
            app.collections.push(new_col);
            Task::none()
        }

        Message::DeleteCollectionPressed(col_id) => {
            app.collections.retain(|c| c.id != col_id);
            app.tabs.retain(|t| {
                if let WorkspaceContent::CollectionRoot { collection_id, .. } = t.content {
                    collection_id != col_id
                } else {
                    true
                }
            });
            if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
                app.active_tab_index = app.tabs.len() - 1;
            }
            Task::none()
        }

        Message::RenameCollectionPressed(col_id) => {
            app.editing_collection_id = Some(col_id);
            Task::none()
        }

        Message::CollectionNameChanged(col_id, new_name) => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
                col.info.name = new_name.clone();

                // update associated workspace tabs showing this collection's root
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

        Message::SaveCollectionNamePressed(_col_id) => {
            app.editing_collection_id = None;
            Task::none()
        }

        Message::RenameFolderPressed {
            collection_id,
            folder_path,
        } => {
            app.editing_folder_collection_id = Some(collection_id);
            app.editing_folder_path = folder_path;
            Task::none()
        }

        Message::FolderNameChanged {
            collection_id,
            folder_path,
            new_name,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                if rename_nested_folder(&mut col.item, &folder_path, &new_name) {
                    // update our navigation path to track the new name dynamically
                    if let Some(last) = app.editing_folder_path.last_mut() {
                        *last = new_name;
                    }
                }
            }
            Task::none()
        }

        Message::SaveFolderNamePressed { .. } => {
            app.editing_folder_collection_id = None;
            app.editing_folder_path.clear();
            Task::none()
        }

        Message::AddFolderPressed {
            collection_id,
            parent_folder_path,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                insert_nested(&mut col.item, &parent_folder_path);
            }
            Task::none()
        }

        Message::DeleteFolderPressed {
            collection_id,
            folder_path,
        } => {
            if !folder_path.is_empty() {
                if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                    remove_nested(&mut col.item, &folder_path);
                }
            }
            Task::none()
        }

        Message::AddRequestPressed {
            collection_id,
            parent_folder_path,
        } => {
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
                    },
                    event: None,
                };

                insert_nested_request(
                    &mut col.item,
                    &parent_folder_path,
                    CollectionItem::Request(new_request_node),
                );
            }
            Task::none()
        }

        Message::DeleteRequestPressed {
            collection_id,
            parent_folder_path,
            request_id,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                remove_nested_request(&mut col.item, &parent_folder_path, request_id);

                app.tabs.retain(|t| t.tab.request_id != Some(request_id));
                if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
                    app.active_tab_index = app.tabs.len() - 1;
                }
            }
            Task::none()
        }

        // context menu
        Message::ShowCollectionContextMenu(col_id) => {
            app.active_context_menu = Some(ContextMenu::Collection(col_id));
            app.context_menu_position = app.cursor_position;
            Task::none()
        }

        Message::ShowFolderContextMenu {
            collection_id,
            folder_path,
        } => {
            app.active_context_menu = Some(ContextMenu::Folder {
                col_id: collection_id,
                path: folder_path,
            });
            app.context_menu_position = app.cursor_position;
            Task::none()
        }

        Message::ShowRequestContextMenu {
            collection_id,
            folder_path,
            request_id,
        } => {
            app.active_context_menu = Some(ContextMenu::Request {
                col_id: collection_id,
                folder_path,
                req_id: request_id,
            });
            app.context_menu_position = app.cursor_position;
            Task::none()
        }

        Message::CloseContextMenu => {
            app.active_context_menu = None;
            Task::none()
        }

        Message::CursorMoved(position) => {
            if let Some(drag) = &app.resize_drag {
                match drag.kind {
                    ResizeKind::Sidebar => {
                        let delta = position.x - drag.start_cursor.x;
                        app.sidebar_width = (drag.start_size + delta)
                            .clamp(SIDEBAR_WIDTH_RANGE.0, SIDEBAR_WIDTH_RANGE.1);
                    }
                    ResizeKind::RequestPane => {
                        let delta = position.y - drag.start_cursor.y;
                        app.request_pane_height = (drag.start_size + delta)
                            .clamp(REQUEST_PANE_HEIGHT_RANGE.0, REQUEST_PANE_HEIGHT_RANGE.1);
                    }
                    ResizeKind::ConsolePanel => {
                        let delta = drag.start_cursor.y - position.y;
                        app.console_panel_height = (drag.start_size + delta)
                            .clamp(CONSOLE_PANEL_HEIGHT_RANGE.0, CONSOLE_PANEL_HEIGHT_RANGE.1);
                    }
                }
            }
            app.cursor_position = position;
            Task::none()
        }

        Message::ResizeDragStarted(kind) => {
            let start_size = match kind {
                ResizeKind::Sidebar => app.sidebar_width,
                ResizeKind::RequestPane => app.request_pane_height,
                ResizeKind::ConsolePanel => app.console_panel_height,
            };
            app.resize_drag = Some(ResizeDrag {
                kind,
                start_cursor: app.cursor_position,
                start_size,
            });
            Task::none()
        }

        Message::ResizeDragEnded => {
            app.resize_drag = None;
            Task::none()
        }

        Message::ToggleConsolePanel => {
            app.console_collapsed = !app.console_collapsed;
            Task::none()
        }

        Message::ClearConsoleLogs => {
            app.console_logs.clear();
            Task::none()
        }

        Message::ShowToast(msg, status) => crate::ui::toast::toast::show_and_schedule(
            &mut app.toast_manager,
            msg,
            status,
            crate::ui::toast::toast::TOAST_DURATION,
        ),

        // env actions
        Message::EditEnvironmentPressed(idx) => {
            app.editing_env_index = Some(idx);
            app.editing_env_name = false; // reset on open
            Task::none()
        }

        Message::CloseEnvEditorPressed => {
            app.editing_env_index = None;
            app.editing_env_name = false; // reset on close
            Task::none()
        }

        Message::AddEnvVariablePressed(env_idx) => {
            if let Some(env) = app.environments.get_mut(env_idx) {
                env.variables.push(KeyValuePair::new("", ""));
            }
            Task::none()
        }

        Message::DeleteEnvVariablePressed { env_idx, var_idx } => {
            if let Some(env) = app.environments.get_mut(env_idx) {
                if var_idx < env.variables.len() {
                    env.variables.remove(var_idx);
                }
            }
            Task::none()
        }

        Message::EnvVariableKeyChanged {
            env_idx,
            var_idx,
            key,
        } => {
            if let Some(env) = app.environments.get_mut(env_idx) {
                if let Some(var) = env.variables.get_mut(var_idx) {
                    var.key = key;
                }
            }
            Task::none()
        }

        Message::EnvVariableValueChanged {
            env_idx,
            var_idx,
            value,
        } => {
            if let Some(env) = app.environments.get_mut(env_idx) {
                if let Some(var) = env.variables.get_mut(var_idx) {
                    var.value = value;
                }
            }
            Task::none()
        }

        Message::EnvVariableToggled {
            env_idx,
            var_idx,
            is_active,
        } => {
            if let Some(env) = app.environments.get_mut(env_idx) {
                if let Some(var) = env.variables.get_mut(var_idx) {
                    var.is_active = is_active;
                }
            }
            Task::none()
        }

        Message::RenameEnvironmentPressed(idx) => {
            if idx < app.environments.len() {
                app.editing_env_name = true;
            }
            Task::none()
        }

        Message::EnvNameChanged(idx, new_name) => {
            if let Some(env) = app.environments.get_mut(idx) {
                env.name = new_name;
            }
            Task::none()
        }

        Message::SaveEnvNamePressed(idx) => {
            app.editing_env_name = false;
            if let Some(env) = app.environments.get_mut(idx) {
                if env.name.trim().is_empty() {
                    env.name = format!("Environment {}", idx + 1);
                }
            }
            Task::none()
        }

        // workspace actions
        Message::WorkspaceSelected(name) => {
            let target_id = app.workspaces.iter().find(|w| w.name == name).map(|w| w.id);
            let Some(target_id) = target_id else {
                return Task::none();
            };
            if target_id == app.active_workspace_id {
                return Task::none();
            }

            let dropped = app.commit_active_workspace_snapshot();
            crate::workspace::save(&app.build_workspace_manifest());

            let target = app.workspaces.iter().find(|w| w.id == target_id).cloned();
            let Some(target) = target else {
                return Task::none();
            };

            let load_errors = app.apply_workspace(&target);
            app.active_workspace_id = target_id;

            let mut tasks = vec![Task::done(Message::ShowToast(
                format!("Switched to workspace '{}'", target.name),
                ToastStatus::Success,
            ))];
            if dropped > 0 {
                tasks.push(Task::done(Message::ShowToast(
                    format!(
                        "{dropped} unsaved collection(s) weren't carried over — save them to disk first to keep them across workspace switches"
                    ),
                    ToastStatus::Info,
                )));
            }
            tasks.extend(
                load_errors
                    .into_iter()
                    .map(|err| Task::done(Message::ShowToast(err, ToastStatus::Error))),
            );
            Task::batch(tasks)
        }

        Message::CreateWorkspacePressed => {
            app.commit_active_workspace_snapshot();
            crate::workspace::save(&app.build_workspace_manifest());

            let new_id = app.next_workspace_id;
            app.next_workspace_id += 1;

            let mut env = Environment::new("Default");
            if !env.variables.is_empty() {
                env.variables[0].is_active = true;
            }

            let new_ws = SavedWorkspace {
                id: new_id,
                name: format!("Workspace {new_id}"),
                collection_sources: Vec::new(),
                environments: vec![env],
                active_env_index: None,
                session: SavedSession {
                    tabs: Vec::new(),
                    active_tab_index: 0,
                    next_tab_id: 0,
                    next_request_id: 0,
                },
            };
            app.workspaces.push(new_ws.clone());
            app.apply_workspace(&new_ws);
            app.active_workspace_id = new_id;
            crate::workspace::save(&app.build_workspace_manifest());

            Task::done(Message::ShowToast(
                format!("Created workspace '{}'", new_ws.name),
                ToastStatus::Success,
            ))
        }

        Message::DeleteWorkspacePressed(id) => {
            if app.workspaces.len() <= 1 {
                return Task::done(Message::ShowToast(
                    "Can't delete the only workspace".to_string(),
                    ToastStatus::Error,
                ));
            }

            let deleted_name = app
                .workspaces
                .iter()
                .find(|w| w.id == id)
                .map(|w| w.name.clone());
            app.workspaces.retain(|w| w.id != id);

            let mut load_errors = Vec::new();
            if id == app.active_workspace_id {
                if let Some(next) = app.workspaces.first().cloned() {
                    load_errors = app.apply_workspace(&next);
                    app.active_workspace_id = next.id;
                }
            }
            crate::workspace::save(&app.build_workspace_manifest());

            let mut tasks = Vec::new();
            if let Some(name) = deleted_name {
                tasks.push(Task::done(Message::ShowToast(
                    format!("Deleted workspace '{name}'"),
                    ToastStatus::Success,
                )));
            }
            tasks.extend(
                load_errors
                    .into_iter()
                    .map(|err| Task::done(Message::ShowToast(err, ToastStatus::Error))),
            );
            Task::batch(tasks)
        }

        Message::RenameWorkspacePressed(id) => {
            app.editing_workspace_id = Some(id);
            Task::none()
        }

        Message::WorkspaceNameChanged(id, new_name) => {
            if let Some(ws) = app.workspaces.iter_mut().find(|w| w.id == id) {
                ws.name = new_name;
            }
            Task::none()
        }

        Message::SaveWorkspaceNamePressed(id) => {
            app.editing_workspace_id = None;
            if let Some(ws) = app.workspaces.iter_mut().find(|w| w.id == id) {
                if ws.name.trim().is_empty() {
                    ws.name = format!("Workspace {id}");
                }
            }
            crate::workspace::save(&app.build_workspace_manifest());
            Task::none()
        }

        // menu actions
        Message::MenuInteraction(dropdown_msg) => {
            if let Some(menu_action) = app.menu_state.update(dropdown_msg) {
                match menu_action {
                    MenuMessage::FileNew => {
                        return update(app, Message::CreateNewCollectionPressed);
                    }
                    MenuMessage::FileOpen => {
                        return update(app, Message::ImportCollectionPressed);
                    }
                    MenuMessage::FileOpenGitFolder => {
                        return update(app, Message::ImportGitCollectionPressed);
                    }
                    MenuMessage::FileExit => {
                        return update(app, Message::AppExit);
                    }
                    MenuMessage::CheckForUpdate => {
                        return update(app, Message::CheckForUpdate);
                    }
                    MenuMessage::HelpAbout => {
                        let version_info = format!("{} v{}", APP_NAME, APP_VERSION);
                        return update(app, Message::ShowToast(version_info, ToastStatus::Info));
                    }
                }
            }
            Task::none()
        }

        // request model actions
        Message::SaveRequestPressed(tab_idx) => {
            if let Some(tab_state) = app.tabs.get(tab_idx) {
                if matches!(tab_state.content, WorkspaceContent::HttpRequest) {
                    // if this request is already saved into a collection, sync its
                    // current state back in place and flush the collection to disk
                    // if it already has a known save location (git folder / file).
                    if let (Some(_req_id), Some(col_id)) =
                        (tab_state.tab.request_id, tab_state.tab.collection_id)
                    {
                        app.sync_tab_to_collection(tab_idx);
                        return persist_collection_if_known_location(
                            app,
                            col_id,
                            "Request updated".to_string(),
                        );
                    }

                    let default_name = if tab_state.tab.name.trim().is_empty() {
                        "Untitled Request".to_string()
                    } else {
                        tab_state.tab.name.clone()
                    };

                    app.save_request_model = Some(SaveRequestModalState {
                        tab_index: tab_idx,
                        request_name: default_name,
                        selected_collection_id: tab_state
                            .tab
                            .collection_id
                            .or_else(|| app.collections.first().map(|c| c.id)),
                        selected_folder_path: Vec::new(),
                    });
                }
            }
            Task::none()
        }

        Message::SaveRequestModalCollectionSelected(col_id) => {
            if let Some(modal) = app.save_request_model.as_mut() {
                modal.selected_collection_id = Some(col_id);
                modal.selected_folder_path.clear();
            }
            Task::none()
        }

        Message::SaveRequestModalFolderSelected(path) => {
            if let Some(modal) = app.save_request_model.as_mut() {
                modal.selected_folder_path = path;
            }
            Task::none()
        }

        Message::SaveRequestNameChanged(name) => {
            if let Some(modal) = app.save_request_model.as_mut() {
                modal.request_name = name;
            }
            Task::none()
        }

        Message::CloseSaveRequestModal => {
            app.save_request_model = None;
            Task::none()
        }

        Message::SaveRequestConfirmed => {
            let Some(modal) = app.save_request_model.take() else {
                return Task::none();
            };
            let Some(col_id) = modal.selected_collection_id else {
                return Task::none();
            };

            let name = if modal.request_name.trim().is_empty() {
                "Untitled Request".to_string()
            } else {
                modal.request_name.clone()
            };

            let already_linked = app
                .tabs
                .get(modal.tab_index)
                .and_then(|t| t.tab.request_id)
                .is_some();

            if already_linked {
                if let Some(tab_state) = app.tabs.get_mut(modal.tab_index) {
                    tab_state.tab.name = name;
                }
                app.sync_tab_to_collection(modal.tab_index);
                return persist_collection_if_known_location(
                    app,
                    col_id,
                    "Request updated".to_string(),
                );
            }

            let req_id = app.next_request_id;
            app.next_request_id += 1;

            let request_node = if let Some(tab_state) = app.tabs.get_mut(modal.tab_index) {
                tab_state.tab.name = name.clone();
                tab_state.tab.request_id = Some(req_id);
                tab_state.tab.collection_id = Some(col_id);
                Some(tab_state.tab.to_postman_request_node(req_id, &name))
            } else {
                None
            };

            if let Some(request_node) = request_node {
                if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
                    insert_nested_request(
                        &mut col.item,
                        &modal.selected_folder_path,
                        CollectionItem::Request(request_node),
                    );

                    let col_name = col.info.name.clone();
                    return Task::done(Message::ShowToast(
                        format!("Request saved to '{}'", col_name),
                        ToastStatus::Success,
                    ));
                }
            }

            Task::none()
        }

        Message::SaveActiveRequestShortcut => {
            if let Some(tab_state) = app.tabs.get(app.active_tab_index) {
                match &tab_state.content {
                    WorkspaceContent::HttpRequest => {
                        update(app, Message::SaveRequestPressed(app.active_tab_index))
                    }
                    WorkspaceContent::CollectionRoot { collection_id, .. } => {
                        update(app, Message::SaveCollectionPressed(*collection_id))
                    }
                }
            } else {
                Task::none()
            }
        } // end save_request_model actions

        // temporary data stores
        Message::AutosaveTick => {
            app.commit_active_workspace_snapshot();
            crate::workspace::save(&app.build_workspace_manifest());
            Task::none()
        }

        // self update
        Message::CheckForUpdate => iced::Task::perform(
            async {
                tokio::task::spawn_blocking(check_for_update)
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
            },
            Message::UpdateCheckResult,
        ),

        Message::UpdateCheckResult(Ok(Some(info))) => {
            let msg = format!("Update available: v{}", info.version);
            app.available_update = Some(info);
            let (id, task) = crate::ui::toast::toast::show_with_action_and_schedule(
                &mut app.toast_manager,
                msg,
                ToastStatus::Info,
                crate::ui::toast::toast::TOAST_DURATION,
                "Update",
            );

            app.update_toast_id = Some(id);
            task
        }

        Message::ToastActionPressed(id) => {
            if app.update_toast_id == Some(id) {
                app.update_toast_id = None;
                app.toast_manager.dismiss(id);
                return update(app, Message::InstallUpdate);
            }
            Task::none()
        }

        Message::UpdateCheckResult(Ok(None)) => crate::ui::toast::toast::show_and_schedule(
            &mut app.toast_manager,
            "You're up to date.".to_string(),
            ToastStatus::Info,
            crate::ui::toast::toast::TOAST_DURATION,
        ),

        Message::UpdateCheckResult(Err(e)) => crate::ui::toast::toast::show_and_schedule(
            &mut app.toast_manager,
            format!("Update check failed: {e}"),
            ToastStatus::Error,
            crate::ui::toast::toast::TOAST_DURATION,
        ),

        Message::InstallUpdate => {
            let toast_task = crate::ui::toast::toast::show_and_schedule(
                &mut app.toast_manager,
                "Downloading update…".to_string(),
                ToastStatus::Info,
                crate::ui::toast::toast::TOAST_DURATION,
            );
            let update_task = iced::Task::perform(
                async {
                    tokio::task::spawn_blocking(perform_update)
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::UpdateInstallResult,
            );
            iced::Task::batch([toast_task, update_task])
        }

        Message::UpdateInstallResult(Ok(version)) => crate::ui::toast::toast::show_and_schedule(
            &mut app.toast_manager,
            format!("Updated to v{version}. Please restart the app."),
            ToastStatus::Success,
            crate::ui::toast::toast::TOAST_DURATION,
        ),

        Message::UpdateInstallResult(Err(e)) => crate::ui::toast::toast::show_and_schedule(
            &mut app.toast_manager,
            format!("Update failed: {e}"),
            ToastStatus::Error,
            crate::ui::toast::toast::TOAST_DURATION,
        ),
        // end self update
        Message::DismissToast(id) => {
            app.toast_manager.dismiss(id);
            iced::Task::none()
        }
        // exit the application
        Message::AppExit => {
            app.commit_active_workspace_snapshot();
            crate::workspace::save(&app.build_workspace_manifest());
            iced::exit()
        }
    }
}
