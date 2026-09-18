mod collections;
mod environment;
mod git;
mod layout;
mod overlays;
mod plugin_collection_ops;
mod plugins;
mod remote;
mod settings;
mod sidebar;
mod terminal;
mod workbench;
mod workspace;

use crate::collection::collection::{CollectionInfo, PostmanCollection, RemoteDirRef};
use crate::collection::env::Environment;
use crate::collection_adapter::{
    create_tab_from_request, examples_to_saved_responses, saved_responses_to_examples,
};
use crate::message::{Message, SidebarDragItem, SidebarItemKey};
use crate::session::{SavedSession, SavedTabEntry};
use crate::ui::confirm_dialog::ConfirmDialogState;
use crate::ui::settings::SettingsTab;
use crate::ui::tab::Tab;
use crate::ui::tab::types::{KeyValuePair, ResponseSubTab};
use crate::ui::toast::toast::ToastStatus;
use crate::utils::{contains_request_node_by_id, find_request_mut, update_node};
use crate::workspace::{CollectionSource, SavedWorkspace, WorkspaceManifest};
use crate::{APP_NAME, APP_VERSION};
use iced::Task;
use rustrest_terminal::TerminalManager;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum CollectionSubTab {
    Variables,
    Documentation,
    Git,
}

#[derive(Debug, Clone)]
pub enum WorkspaceContent {
    HttpRequest,
    CollectionRoot {
        collection_id: usize,
        collection_name: String,
        active_sub_tab: CollectionSubTab,
    },
    Terminal {
        terminal_id: u64,
        widget_id: iced::widget::Id,
    },
    RemoteFile {
        profile_id: usize,
        path: String,
        contents: iced::widget::text_editor::Content,
        dirty: bool,
    },
    /// a sidebar panel contributed by a plugin, identified by the plugin's
    /// id and the panel id it declared via `Capability::SidebarPanel`.
    Plugin {
        plugin_id: String,
        panel_id: String,
    },
    PluginManager,
}

pub struct TabState {
    pub tab: Tab,
    pub content: WorkspaceContent,
    pub is_editing_name: bool,
}

pub struct Rustrest {
    pub collections: Vec<PostmanCollection>,
    pub env: environment::EnvState,
    pub tabs: Vec<TabState>,
    pub active_tab_index: usize,
    pub next_tab_id: usize,
    pub next_request_id: usize,

    pub workspace: workspace::WorkspacesState,

    pub cursor_position: iced::Point,
    pub workbench: workbench::WorkbenchState,

    pub overlays: overlays::OverlaysState,
    pub layout: layout::LayoutState,
    pub git: git::GitState,
    pub sidebar: sidebar::SidebarState,
    pub terminal: terminal::TerminalState,
    pub remote: remote::RemoteState,

    // the id of the always-open main window
    pub main_window_id: iced::window::Id,

    // advances every tick of `spinner_sub` (only running while
    // `any_spinner_active()` is true) to animate loading spinners.
    pub spinner_tick: u64,

    pub plugins: plugins::PluginsState,

    pub settings: settings::SettingsState,
}

impl Rustrest {
    /// whether any spinner-driven loading indicator is currently shown, so
    /// the animation tick subscription only runs while it's actually needed.
    pub fn any_spinner_active(&self) -> bool {
        self.remote.remote_connecting.is_some()
            || self.remote.remote_explorers.values().any(|e| e.loading)
            || self.tabs.iter().any(|t| t.tab.is_loading)
            || self.git.commit_modal.as_ref().is_some_and(|m| m.committing)
            || self.plugins.plugin_manager_busy.is_some()
            || !self.git.git_remote_op_running.is_empty()
            || self.overlays.toast_manager.has_pending()
            || self.tabs.iter().any(|t| {
                matches!(
                    &t.content,
                    WorkspaceContent::CollectionRoot { collection_id, active_sub_tab, .. }
                        if *active_sub_tab == CollectionSubTab::Git
                            && !self.git.git_status_cache.contains_key(collection_id)
                )
            })
    }

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
                // shell sessions can't be serialized; terminal tabs simply
                // don't come back after a restart.
                WorkspaceContent::Terminal { .. } => None,
                // remote file tabs are tied to a live RemoteSession, which
                // also doesn't survive a restart.
                WorkspaceContent::RemoteFile { .. } => None,
                // plugin panels are re-derived from the plugin on demand;
                // nothing to persist.
                WorkspaceContent::Plugin { .. } => None,
                WorkspaceContent::PluginManager => None,
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
            } else if let Some(remote) = &c.remote_dir {
                collection_sources.push(CollectionSource::Remote {
                    profile_id: remote.profile_id,
                    root: remote.root.clone(),
                });
            } else {
                dropped += 1;
            }
        }

        let name = self
            .workspace
            .workspaces
            .iter()
            .find(|w| w.id == self.workspace.active_workspace_id)
            .map(|w| w.name.clone())
            .unwrap_or_else(|| "Workspace".to_string());

        let ws = SavedWorkspace {
            id: self.workspace.active_workspace_id,
            name,
            collection_sources,
            environments: self.env.environments.clone(),
            active_env_index: self.env.active_env_index,
            globals: self.env.globals.clone(),
            collapsed_collections: self.sidebar.collapsed_collections.clone(),
            collapsed_folders: self.sidebar.collapsed_folders.clone(),
            collapsed_saved_responses: self.sidebar.collapsed_saved_responses.clone(),
            remote_profiles: self.remote.remote_profiles.clone(),
            session: self.build_session_snapshot(),
        };
        (ws, dropped)
    }

    /// snapshots the active workspace and writes it back into `self.workspace.workspaces`
    pub fn commit_active_workspace_snapshot(&mut self) -> usize {
        let (ws, dropped) = self.snapshot_active_workspace();
        if let Some(existing) = self.workspace.workspaces.iter_mut().find(|w| w.id == ws.id) {
            *existing = ws;
        } else {
            self.workspace.workspaces.push(ws);
        }
        dropped
    }

    pub fn build_workspace_manifest(&self) -> WorkspaceManifest {
        WorkspaceManifest {
            workspaces: self.workspace.workspaces.clone(),
            active_workspace_id: self.workspace.active_workspace_id,
            next_workspace_id: self.workspace.next_workspace_id,
        }
    }

    /// makes `ws` the live workspace: clears current collections/tabs, reloads
    /// `ws`'s collections from their remembered file/folder locations, adopts
    /// its environments and restores its tabs
    pub fn apply_workspace(&mut self, ws: &SavedWorkspace) -> Vec<String> {
        self.collections.clear();
        for tab in &self.tabs {
            if let WorkspaceContent::Terminal { terminal_id, .. } = tab.content {
                self.terminal.terminal_manager.close(terminal_id);
            }
        }
        self.tabs.clear();
        self.active_tab_index = 0;

        // remote sessions (and their explorer state) belong to the
        // workspace being left; the new workspace has its own profile list.
        self.remote.remote_sessions.clear();
        self.remote.remote_explorers.clear();
        self.remote.remote_connect_pending = None;
        self.sidebar.selected_sidebar_items.clear();
        self.sidebar.sidebar_selection_anchor = None;

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

        self.env.environments = ws.environments.clone();
        self.env.active_env_index = ws.active_env_index;
        self.env.globals = ws.globals.clone();
        self.remote.remote_profiles = ws.remote_profiles.clone();
        self.sidebar.collapsed_collections = ws.collapsed_collections.clone();
        self.sidebar.collapsed_folders = ws.collapsed_folders.clone();
        self.sidebar.collapsed_saved_responses = ws.collapsed_saved_responses.clone();

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
                WorkspaceContent::Terminal { .. } => false,
                WorkspaceContent::RemoteFile { .. } => false,
                WorkspaceContent::Plugin { .. } => false,
                WorkspaceContent::PluginManager => false,
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
                WorkspaceContent::Terminal { .. } => {}
                WorkspaceContent::RemoteFile { .. } => {}
                WorkspaceContent::Plugin { .. } => {}
                WorkspaceContent::PluginManager => {}
            }
        }
    }

    /// after saving/deleting a response snapshot from an open tab's response
    /// pane, mirrors that tab's saved responses into the collection tree
    pub fn sync_active_tab_saved_responses_to_collection(&mut self, idx: usize) {
        let Some(tab_state) = self.tabs.get(idx) else {
            return;
        };
        let (Some(req_id), Some(col_id)) = (tab_state.tab.request_id, tab_state.tab.collection_id)
        else {
            return;
        };
        let saved = tab_state.tab.saved_responses.clone();

        if let Some(col) = self.collections.iter_mut().find(|c| c.id == col_id) {
            if let Some(node) = find_request_mut(&mut col.item, req_id) {
                node.response = saved_responses_to_examples(&saved);
                node.unsaved = true;
            }
        }
    }

    /// re-reads a request's saved responses from the collection tree into any
    /// currently open tab for that request
    pub fn refresh_open_tab_saved_responses(&mut self, collection_id: usize, request_id: usize) {
        let examples = self
            .collections
            .iter_mut()
            .find(|c| c.id == collection_id)
            .and_then(|c| find_request_mut(&mut c.item, request_id))
            .and_then(|node| node.response.clone())
            .unwrap_or_default();
        let saved = examples_to_saved_responses(&examples);

        for tab_state in self.tabs.iter_mut() {
            if tab_state.tab.collection_id == Some(collection_id)
                && tab_state.tab.request_id == Some(request_id)
            {
                tab_state.tab.saved_responses = saved.clone();
                if let Some(viewing) = tab_state.tab.viewing_saved_response {
                    if viewing >= tab_state.tab.saved_responses.len() {
                        tab_state.tab.viewing_saved_response = None;
                    }
                }
            }
        }
    }
}

pub fn init() -> (Rustrest, Task<Message>) {
    let (terminal_event_tx, terminal_event_rx) = tokio::sync::mpsc::unbounded_channel();

    // as a daemon, iced won't open a window on our behalf; open the main
    // window ourselves so `Rustrest` can be constructed with a real id (the
    // id is returned synchronously - only the window actually appearing on
    // screen is asynchronous, tracked by `open_main_window` below).
    let icon = iced::window::icon::from_file_data(crate::APP_ICON, None).ok();
    let (main_window_id, open_main_window) = iced::window::open(iced::window::Settings {
        size: iced::Size::new(1250.0, 850.0),
        icon,
        exit_on_close_request: false,
        ..Default::default()
    });

    let persisted_settings = crate::app_settings::load();

    let mut app = Rustrest {
        collections: Vec::new(),
        env: environment::EnvState {
            environments: Vec::new(),
            active_env_index: None,
            editing_env_index: None,
            editing_env_name: false,
            env_var_value_contents: Vec::new(),
            globals: Vec::new(),
        },
        tabs: vec![],
        active_tab_index: 0,
        next_tab_id: 2,
        next_request_id: 1,
        workspace: workspace::WorkspacesState {
            workspaces: Vec::new(),
            active_workspace_id: 1,
            next_workspace_id: 2,
            editing_workspace_id: None,
        },
        cursor_position: iced::Point::ORIGIN,
        workbench: workbench::WorkbenchState {
            dragging_tab_index: None,
            last_tab_name_click: None,
            tab_rename_input_hovered: false,
            save_request_model: None,
        },
        overlays: overlays::OverlaysState::default(),
        layout: layout::LayoutState {
            sidebar_width: 260.0,
            request_pane_height: 320.0,
            resize_drag: None,
            console_logs: Vec::new(),
            console_collapsed: true,
            console_panel_height: 220.0,
            multiline_heights: std::collections::HashMap::new(),
        },
        git: git::GitState {
            git_status_cache: std::collections::HashMap::new(),
            git_selected_file: None,
            git_diff_cache: None,
            git_remote_op_running: std::collections::HashMap::new(),
            commit_modal: None,
        },
        sidebar: sidebar::SidebarState {
            editing_collection_id: None,
            editing_folder_collection_id: None,
            editing_folder_path: Vec::new(),
            editing_request_collection_id: None,
            editing_request_id: None,
            editing_saved_response: None,
            sidebar_drag: None,
            collapsed_collections: std::collections::HashSet::new(),
            collapsed_folders: std::collections::HashSet::new(),
            collapsed_saved_responses: std::collections::HashSet::new(),
            selected_sidebar_items: std::collections::HashSet::new(),
            sidebar_selection_anchor: None,
            current_modifiers: iced::keyboard::Modifiers::default(),
        },
        terminal: terminal::TerminalState {
            terminal_manager: TerminalManager::new(),
            terminal_event_tx,
            terminal_event_rx: Arc::new(tokio::sync::Mutex::new(terminal_event_rx)),
        },
        remote: remote::RemoteState {
            remote_profiles: Vec::new(),
            remote_sessions: std::collections::HashMap::new(),
            remote_profile_form: crate::ui::remote::RemoteProfileForm::default(),
            remote_connect_pending: None,
            remote_explorers: std::collections::HashMap::new(),
            remote_connecting: None,
            remote_config_open: false,
        },
        main_window_id,
        spinner_tick: 0,
        plugins: plugins::PluginsState {
            plugin_manager: rustrest_plugin_host::PluginManager::new().unwrap_or_else(|e| {
                eprintln!("plugin manager unavailable: {e}");
                rustrest_plugin_host::PluginManager::with_dirs(
                    std::env::temp_dir().join("rustrest-plugins-fallback"),
                    std::env::temp_dir().join("rustrest-plugins-fallback.json"),
                )
                .expect("PluginManager::with_dirs with a temp-dir fallback never fails")
            }),
            plugin_panel_state: std::collections::HashMap::new(),
            plugin_manager_busy: None,
            plugin_manager_view: crate::ui::plugin_manager::PluginManagerView::default(),
            plugin_gallery_entries: None,
            plugin_manager_search: String::new(),
            export_plugin_picker: None,
            right_panel_width: 380.0,
            right_panel_open: None,
            right_panel_tree: None,
        },
        settings: settings::SettingsState {
            settings_open: false,
            settings_tab: SettingsTab::default(),
            theme: persisted_settings.theme,
            close_on_outside_click: persisted_settings.close_on_outside_click,
        },
    };
    app.plugins.plugin_manager.load_all();
    let plugin_load_errors: Vec<String> = app
        .plugins
        .plugin_manager
        .installed()
        .iter()
        .filter_map(|p| {
            p.load_error
                .as_ref()
                .map(|e| format!("Plugin '{}' failed to load: {e}", p.dir_name))
        })
        .collect();

    let load_errors = if let Some(manifest) = crate::workspace::load() {
        app.workspace.workspaces = manifest.workspaces;
        app.workspace.active_workspace_id = manifest.active_workspace_id;
        app.workspace.next_workspace_id = manifest.next_workspace_id;

        if app.workspace.workspaces.is_empty() {
            let default_ws = default_workspace(1, None);
            app.workspace.workspaces.push(default_ws);
            app.workspace.active_workspace_id = 1;
            app.workspace.next_workspace_id = 2;
        }

        let active = app
            .workspace
            .workspaces
            .iter()
            .find(|w| w.id == app.workspace.active_workspace_id)
            .or_else(|| app.workspace.workspaces.first())
            .cloned();

        match active {
            Some(active) => {
                app.workspace.active_workspace_id = active.id;
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
        app.workspace.workspaces = vec![default_ws.clone()];
        app.workspace.active_workspace_id = 1;
        app.workspace.next_workspace_id = 2;
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

    let load_errors_task = if load_errors.is_empty() {
        Task::none()
    } else {
        Task::batch(
            load_errors
                .into_iter()
                .map(|err| Task::done(Message::ShowToast(err, ToastStatus::Error))),
        )
    };

    // silently check for updates on startup; surfaces a toast only if one is found
    let update_check_task = Task::done(Message::CheckForUpdateSilently);

    let plugin_errors_task = Task::batch(
        plugin_load_errors
            .into_iter()
            .map(|err| Task::done(Message::ShowToast(err, ToastStatus::Error))),
    );

    let startup_task = Task::batch([
        open_main_window.map(|_id| Message::None),
        load_errors_task,
        update_check_task,
        plugin_errors_task,
        remote::auto_connect_remote_collections(&app),
    ]);

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
        globals: Vec::new(),
        collapsed_collections: std::collections::HashSet::new(),
        collapsed_folders: std::collections::HashSet::new(),
        collapsed_saved_responses: std::collections::HashSet::new(),
        remote_profiles: Vec::new(),
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
        // the real tree lives on the remote host; this placeholder shows up
        // immediately in the sidebar (dimmed, until connected) and gets
        // replaced once `RemoteConnected` manages to load it.
        CollectionSource::Remote { profile_id, root } => {
            Ok(placeholder_remote_collection(*profile_id, root.clone()))
        }
    }
}

fn placeholder_remote_collection(profile_id: usize, root: String) -> PostmanCollection {
    let name = root
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(&root)
        .to_string();

    PostmanCollection {
        id: 0,
        file_path: None,
        storage_dir: None,
        remote_dir: Some(RemoteDirRef { profile_id, root }),
        unsaved: false,
        info: CollectionInfo {
            name,
            postman_id: None,
            schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                .to_string(),
        },
        item: Vec::new(),
        variable: Some(Vec::new()),
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

/// where SSH host keys seen by the remote-development feature are recorded
/// (trust-on-first-use), separate from any system-wide `~/.ssh/known_hosts`.
pub fn update(app: &mut Rustrest, message: Message) -> Task<Message> {
    match message {
        Message::ContextMenuAction(inner) => {
            app.overlays.active_context_menu = None;
            update(app, *inner)
        }
        Message::None => Task::none(),
        Message::ImportCollectionPressed => collections::import_pressed(),
        // process file contents once loaded from disk
        Message::CollectionLoaded(path, content) => collections::loaded(app, path, content),

        Message::ImportCollectionViaPluginPressed(plugin_id, format_id, extensions) => {
            let mut dialog = rfd::AsyncFileDialog::new();
            if !extensions.is_empty() {
                let ext_refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
                dialog = dialog.add_filter("Supported files", &ext_refs);
            }
            iced::Task::perform(
                async move {
                    let file_handle = dialog.pick_file().await?;
                    let path = file_handle.path().to_path_buf();
                    let bytes = tokio::fs::read(&path).await.ok()?;
                    Some((path, bytes))
                },
                move |result| match result {
                    Some((path, bytes)) => {
                        Message::PluginImportFileLoaded(plugin_id, format_id, Some(path), bytes)
                    }
                    None => Message::None,
                },
            )
        }

        Message::PluginImportFileLoaded(plugin_id, format_id, path, bytes) => {
            let result = app
                .plugins
                .plugin_manager
                .import(&plugin_id, &format_id, bytes);
            plugins::drain_plugin_logs(app);
            match result {
                Ok(value) => match serde_json::from_value::<PostmanCollection>(value) {
                    Ok(mut collection) => {
                        let col_name = collection.info.name.clone();
                        collection.id = app.next_tab_id;
                        collection.file_path = path;
                        app.next_tab_id += 1;
                        collection.assign_request_ids(&mut app.next_request_id);

                        let default_headers: Vec<KeyValuePair> = vec![
                            KeyValuePair::new("Content-Type", "application/json"),
                            KeyValuePair::new(
                                "User-Agent",
                                &format!("{}/{}", APP_NAME, APP_VERSION),
                            ),
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
                    Err(e) => iced::Task::done(Message::ShowToast(
                        format!("Plugin returned an invalid collection: {e}"),
                        ToastStatus::Error,
                    )),
                },
                Err(e) => iced::Task::done(Message::ShowToast(
                    format!("Import failed: {e}"),
                    ToastStatus::Error,
                )),
            }
        }

        // simple inline disk overwrite action
        Message::SaveCollectionPressed(col_id) => collections::save_pressed(app, col_id),
        Message::CollectionFirstSaved(col_id, path) => collections::first_saved(app, col_id, path),

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
                        collection.clear_unsaved();
                        for tab_state in &mut app.tabs {
                            let belongs = match &tab_state.content {
                                WorkspaceContent::HttpRequest => {
                                    tab_state.tab.collection_id == Some(col_id)
                                }
                                WorkspaceContent::CollectionRoot { collection_id, .. } => {
                                    *collection_id == col_id
                                }
                                WorkspaceContent::Terminal { .. } => false,
                                WorkspaceContent::RemoteFile { .. } => false,
                                WorkspaceContent::Plugin { .. } => false,
                                WorkspaceContent::PluginManager => false,
                            };
                            if belongs {
                                tab_state.tab.dirty = false;
                            }
                        }
                        let was_already_repo = crate::collection::git_ops::is_git_repo(&dir);
                        if let Err(e) =
                            crate::collection::git_ops::write_default_gitignore_if_missing(&dir)
                        {
                            return Task::done(Message::ShowToast(
                                format!("Collection saved, but failed to write .gitignore: {e}"),
                                ToastStatus::Error,
                            ));
                        }

                        let dir_for_init = dir.clone();
                        return Task::perform(
                            async move { crate::collection::git_ops::git_init(&dir_for_init).await },
                            move |result| match result {
                                Ok(()) => {
                                    let msg = if was_already_repo {
                                        format!("Collection now stored at {dir:?}")
                                    } else {
                                        format!(
                                            "Collection now stored at {dir:?} (git repo initialized)"
                                        )
                                    };
                                    Message::ShowToast(msg, ToastStatus::Success)
                                }
                                Err(e) => Message::ShowToast(
                                    format!("Collection saved, but git init failed: {e}"),
                                    ToastStatus::Error,
                                ),
                            },
                        );
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
            let canon_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            let existing_id = app.collections.iter().find_map(|c| {
                let dir = c.storage_dir.as_ref()?;
                let canon_dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone());
                (canon_dir == canon_path).then_some(c.id)
            });

            if let Some(existing_id) = existing_id {
                app.sync_collection_tabs(existing_id);
                let existing = app
                    .collections
                    .iter()
                    .find(|c| c.id == existing_id)
                    .expect("looked up by id above");
                let has_unsaved = crate::collection::dir_storage::plan_dir_sync(existing, &path)
                    .map(|plan| !plan.is_empty())
                    .unwrap_or(false);
                let existing_name = existing.info.name.clone();

                collection.id = existing_id;
                let message = if has_unsaved {
                    format!(
                        "\"{existing_name}\" is already open with unsaved changes. Reloading will discard them."
                    )
                } else {
                    format!("\"{existing_name}\" is already open. Reload it from disk?")
                };

                return Task::done(Message::ShowConfirmDialog(ConfirmDialogState {
                    title: "Reload collection from disk?".to_string(),
                    message,
                    confirm_label: "Reload".to_string(),
                    on_confirm: Box::new(Message::ReplaceCollectionConfirmed(
                        existing_id,
                        Box::new(collection),
                    )),
                }));
            }

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

        Message::ReplaceCollectionConfirmed(col_id, new_collection) => {
            app.tabs.retain(|t| {
                !matches!(&t.content, WorkspaceContent::CollectionRoot { collection_id, .. } if *collection_id == col_id)
            });
            if app.active_tab_index >= app.tabs.len() {
                app.active_tab_index = app.tabs.len().saturating_sub(1);
            }

            if let Some(existing) = app.collections.iter_mut().find(|c| c.id == col_id) {
                let mut new_collection = *new_collection;
                new_collection.id = col_id;
                *existing = new_collection;
            }
            app.git.git_status_cache.remove(&col_id);

            Task::done(Message::ShowToast(
                "Collection reloaded from disk".to_string(),
                ToastStatus::Success,
            ))
        }

        // git status/diff panel
        Message::GitStatusRequested(col_id) => git::status_requested(app, col_id),
        Message::GitStatusLoaded(col_id, result) => git::status_loaded(app, col_id, result),
        Message::GitDiffRequested(col_id, file) => git::diff_requested(app, col_id, file),
        Message::GitDiffLoaded(_col_id, file, result) => git::diff_loaded(app, file, result),

        // git remote sync (push/pull/fetch)
        Message::GitPushPressed(col_id) => {
            git::start_remote_op(app, col_id, crate::collection::git_ops::GitRemoteOp::Push)
        }
        Message::GitPullPressed(col_id) => {
            git::start_remote_op(app, col_id, crate::collection::git_ops::GitRemoteOp::Pull)
        }
        Message::GitFetchPressed(col_id) => {
            git::start_remote_op(app, col_id, crate::collection::git_ops::GitRemoteOp::Fetch)
        }
        Message::GitRemoteOpResult(col_id, op, result) => {
            git::remote_op_result(app, col_id, op, result)
        }

        // commit modal
        Message::CommitChangesPressed(col_id) => git::commit_changes_pressed(app, col_id),
        Message::CommitStatusLoaded(col_id, collection_name, snapshot) => {
            git::commit_status_loaded(app, col_id, collection_name, snapshot)
        }
        Message::CommitMessageChanged(action) => git::commit_message_changed(app, action),
        Message::CommitCancelled => git::commit_cancelled(app),
        Message::CommitConfirmed => git::commit_confirmed(app),
        Message::CommitResult(col_id, result) => git::commit_result(app, col_id, result),

        // generic reusable confirm dialog
        Message::ShowConfirmDialog(state) => overlays::show_confirm_dialog(app, state),
        Message::ConfirmDialogAccepted => overlays::confirm_dialog_accepted(app),
        Message::ConfirmDialogCancelled => overlays::confirm_dialog_cancelled(app),
        // end git
        Message::ExportCollectionPressed(col_id) => collections::export_pressed(app, col_id),

        Message::ExportViaPluginPressed(col_id) => {
            let mut formats = app.plugins.plugin_manager.export_formats();
            match formats.len() {
                0 => iced::Task::done(Message::ShowToast(
                    "No export plugins installed".to_string(),
                    ToastStatus::Info,
                )),
                1 => {
                    let (plugin_id, format) = formats.remove(0);
                    iced::Task::done(Message::ExportCollectionViaPluginPressed(
                        col_id,
                        plugin_id,
                        format.id,
                        format.extensions,
                    ))
                }
                _ => {
                    app.plugins.export_plugin_picker = Some((col_id, formats));
                    iced::Task::none()
                }
            }
        }

        Message::CloseExportPluginPicker => {
            app.plugins.export_plugin_picker = None;
            iced::Task::none()
        }

        Message::ExportCollectionViaPluginPressed(col_id, plugin_id, format_id, extensions) => {
            app.plugins.export_plugin_picker = None;
            app.sync_collection_tabs(col_id);
            let Some(collection) = app.collections.iter().find(|c| c.id == col_id) else {
                return iced::Task::none();
            };
            let value = match serde_json::to_value(collection) {
                Ok(v) => v,
                Err(e) => {
                    return iced::Task::done(Message::ShowToast(
                        format!("Export failed: {e}"),
                        ToastStatus::Error,
                    ));
                }
            };
            let default_ext = extensions.first().cloned().unwrap_or_default();
            let default_name = if default_ext.is_empty() {
                collection.info.name.clone()
            } else {
                format!("{}.{}", collection.info.name, default_ext)
            };

            let result = app
                .plugins
                .plugin_manager
                .export(&plugin_id, &format_id, value);
            plugins::drain_plugin_logs(app);
            let bytes = match result {
                Ok(bytes) => bytes,
                Err(e) => {
                    return iced::Task::done(Message::ShowToast(
                        format!("Export failed: {e}"),
                        ToastStatus::Error,
                    ));
                }
            };

            let mut dialog = rfd::AsyncFileDialog::new().set_file_name(&default_name);
            if !extensions.is_empty() {
                let ext_refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
                dialog = dialog.add_filter("Supported files", &ext_refs);
            }
            iced::Task::perform(
                async move {
                    let file_handle = dialog.save_file().await?;
                    let path = file_handle.path().to_path_buf();
                    tokio::fs::write(&path, bytes).await.ok()?;
                    Some(path)
                },
                |result| match result {
                    Some(path) => Message::ShowToast(
                        format!("Collection exported to {:?}", path),
                        ToastStatus::Success,
                    ),
                    None => Message::None,
                },
            )
        }

        Message::SidebarCollectionRootClicked(col_id) => {
            let key = SidebarItemKey::Collection(col_id);
            if !app.sidebar.selected_sidebar_items.contains(&key) {
                app.sidebar.selected_sidebar_items.clear();
                app.sidebar.sidebar_selection_anchor = None;
            }

            let existing_tab_idx = app.tabs.iter().position(|t| {
                if let WorkspaceContent::CollectionRoot { collection_id, .. } = t.content {
                    collection_id == col_id
                } else {
                    false
                }
            });

            if let Some(idx) = existing_tab_idx {
                app.active_tab_index = idx;
                Task::none()
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
                iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
            } else {
                Task::none()
            }
        }

        Message::SidebarRequestClicked {
            req_node,
            collection_id,
            parent_path,
        } => {
            let key = SidebarItemKey::Request {
                collection_id,
                parent_path: parent_path.clone(),
                request_id: req_node.id,
            };
            if !app.sidebar.selected_sidebar_items.contains(&key) {
                app.sidebar.selected_sidebar_items.clear();
                app.sidebar.sidebar_selection_anchor = None;
            }

            app.sidebar.sidebar_drag = Some(SidebarDragItem::Request {
                collection_id,
                parent_path,
                request_id: req_node.id,
            });

            let existing_tab_idx = app.tabs.iter().position(|t| {
                t.tab.request_id == Some(req_node.id)
                    && matches!(t.content, WorkspaceContent::HttpRequest)
            });

            if let Some(idx) = existing_tab_idx {
                app.active_tab_index = idx;
                Task::none()
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
                iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
            }
        }

        Message::SidebarSavedResponseClicked {
            req_node,
            collection_id,
            index,
        } => {
            let existing_tab_idx = app.tabs.iter().position(|t| {
                t.tab.request_id == Some(req_node.id)
                    && matches!(t.content, WorkspaceContent::HttpRequest)
            });

            let (tab_idx, opened_new_tab) = match existing_tab_idx {
                Some(idx) => {
                    app.active_tab_index = idx;
                    (idx, false)
                }
                None => {
                    let new_tab =
                        create_tab_from_request(app.next_tab_id, &req_node, Some(collection_id));
                    app.tabs.push(TabState {
                        tab: new_tab,
                        content: WorkspaceContent::HttpRequest,
                        is_editing_name: false,
                    });
                    app.next_tab_id += 1;
                    app.active_tab_index = app.tabs.len() - 1;
                    (app.active_tab_index, true)
                }
            };

            if let Some(tab_state) = app.tabs.get_mut(tab_idx) {
                if index < tab_state.tab.saved_responses.len() {
                    tab_state.tab.viewing_saved_response = Some(index);
                    tab_state.tab.active_response_tab = ResponseSubTab::Body;
                }
            }

            if opened_new_tab {
                iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
            } else {
                Task::none()
            }
        }

        Message::ShowSavedResponseContextMenu {
            collection_id,
            request_id,
            index,
        } => sidebar::show_saved_response_context_menu(app, collection_id, request_id, index),
        Message::RenameSavedResponsePressed {
            collection_id,
            request_id,
            index,
        } => sidebar::rename_saved_response_pressed(app, collection_id, request_id, index),
        Message::SavedResponseNameChanged {
            collection_id,
            request_id,
            index,
            new_name,
        } => sidebar::saved_response_name_changed(app, collection_id, request_id, index, new_name),
        Message::SaveSavedResponseNamePressed => sidebar::save_saved_response_name_pressed(app),
        Message::DeleteSavedResponsePressed {
            collection_id,
            request_id,
            index,
        } => sidebar::delete_saved_response_pressed(app, collection_id, request_id, index),

        Message::NewTabPressed => workbench::new_tab_pressed(app),
        Message::CloseTabPressed(index) => workbench::close_tab_pressed(app, index),
        Message::NewTerminalTabPressed => workbench::new_terminal_tab_pressed(app),
        Message::TerminalInput(id, bytes) => workbench::terminal_input(app, id, bytes),
        Message::TerminalResized(id, columns, rows, cell_width, cell_height) => {
            workbench::terminal_resized(app, id, columns, rows, cell_width, cell_height)
        }
        Message::TerminalNotice(id, notice) => workbench::terminal_notice(app, id, notice),
        Message::CloseActiveTabShortcut => workbench::close_active_tab_shortcut(app),
        Message::ActiveTabMessage(tab_msg) => workbench::active_tab_message(app, tab_msg),

        // script engine used
        Message::SendPressed => workbench::send_pressed(app),
        // script engine used
        Message::ResponseReceived(tab_id, res) => workbench::response_received(app, tab_id, res),

        Message::TabNameDoubleClick(idx) => workbench::tab_name_double_click(app, idx),
        Message::TabNameChanged(idx, new_name) => workbench::tab_name_changed(app, idx, new_name),
        Message::TabNameSave(idx) => workbench::tab_name_save(app, idx),
        Message::TabRenameBlur => workbench::tab_rename_blur(app),
        Message::TabRenameInputHover(is_hovered) => {
            workbench::tab_rename_input_hover(app, is_hovered)
        }

        Message::EnvSelected(selected_name) => environment::selected(app, selected_name),
        Message::CreateEnvironmentPressed => environment::create_pressed(app),
        Message::DeleteEnvironmentPressed(idx) => environment::delete_pressed(app, idx),

        Message::CollectionSubTabSelected(sub_tab) => collections::sub_tab_selected(app, sub_tab),
        Message::CollectionVariableChanged {
            collection_id,
            index,
            key,
            value,
        } => collections::variable_changed(app, collection_id, index, key, value),
        Message::CollectionVariableToggled {
            collection_id,
            index,
            is_active,
        } => collections::variable_toggled(app, collection_id, index, is_active),
        Message::AddCollectionVariablePressed(collection_id) => {
            collections::add_variable_pressed(app, collection_id)
        }
        Message::DeleteCollectionVariablePressed(collection_id, index) => {
            collections::delete_variable_pressed(app, collection_id, index)
        }
        Message::CreateNewCollectionPressed => collections::create_new_pressed(app),
        Message::DeleteCollectionPressed(col_id) => collections::delete_pressed(app, col_id),

        Message::RenameCollectionPressed(col_id) => sidebar::rename_collection_pressed(app, col_id),
        Message::CollectionNameChanged(col_id, new_name) => {
            sidebar::collection_name_changed(app, col_id, new_name)
        }
        Message::SaveCollectionNamePressed(_col_id) => sidebar::save_collection_name_pressed(app),
        Message::RenameFolderPressed {
            collection_id,
            folder_path,
        } => sidebar::rename_folder_pressed(app, collection_id, folder_path),
        Message::FolderNameChanged {
            collection_id,
            folder_path,
            new_name,
        } => sidebar::folder_name_changed(app, collection_id, folder_path, new_name),
        Message::SaveFolderNamePressed { .. } => sidebar::save_folder_name_pressed(app),

        Message::AddFolderPressed {
            collection_id,
            parent_folder_path,
        } => collections::add_folder_pressed(app, collection_id, parent_folder_path),
        Message::DeleteFolderPressed {
            collection_id,
            folder_path,
        } => collections::delete_folder_pressed(app, collection_id, folder_path),
        Message::AddRequestPressed {
            collection_id,
            parent_folder_path,
        } => collections::add_request_pressed(app, collection_id, parent_folder_path),
        Message::DeleteRequestPressed {
            collection_id,
            parent_folder_path,
            request_id,
        } => {
            collections::delete_request_pressed(app, collection_id, parent_folder_path, request_id)
        }

        // request rename actions
        Message::RenameRequestPressed {
            collection_id,
            request_id,
        } => sidebar::rename_request_pressed(app, collection_id, request_id),
        Message::RequestNameChanged {
            collection_id,
            request_id,
            new_name,
        } => sidebar::request_name_changed(app, collection_id, request_id, new_name),
        Message::SaveRequestNamePressed { .. } => sidebar::save_request_name_pressed(app),
        Message::ToggleSavedResponsesCollapsed(request_id) => {
            sidebar::toggle_saved_responses_collapsed(app, request_id)
        }

        // context menu
        Message::ShowCollectionContextMenu(col_id) => {
            overlays::show_collection_context_menu(app, col_id)
        }
        Message::ShowGitActionsMenu(col_id) => overlays::show_git_actions_menu(app, col_id),
        Message::ShowFolderContextMenu {
            collection_id,
            folder_path,
        } => overlays::show_folder_context_menu(app, collection_id, folder_path),
        Message::ShowRequestContextMenu {
            collection_id,
            folder_path,
            request_id,
        } => overlays::show_request_context_menu(app, collection_id, folder_path, request_id),
        Message::CloseContextMenu => overlays::close_context_menu(app),
        Message::ShowTextFieldContextMenu(target, current_value) => {
            overlays::show_text_field_context_menu(app, target, current_value)
        }
        Message::ShowPluginTextContextMenu(text) => {
            overlays::show_plugin_text_context_menu(app, text)
        }
        Message::CopyToClipboard(text) => overlays::copy_to_clipboard(app, text),
        Message::PasteIntoField(target) => overlays::paste_into_field(app, target),
        Message::TextFieldPasteResolved(target, clipboard_text) => {
            overlays::text_field_paste_resolved(app, target, clipboard_text)
        }

        Message::CursorMoved(position) => layout::cursor_moved(app, position),
        Message::ResizeDragStarted(kind) => layout::resize_drag_started(app, kind),
        Message::ResizeDragEnded => layout::resize_drag_ended(app),

        Message::SidebarDragStarted(item) => sidebar::drag_started(app, item),
        Message::SidebarDropped(target) => sidebar::dropped(app, target),
        Message::SidebarItemToggleSelect(key) => sidebar::item_toggle_select(app, key),
        Message::SidebarItemRangeSelect(key) => sidebar::item_range_select(app, key),
        Message::ClearSidebarSelection => sidebar::clear_selection(app),
        Message::BatchDeleteSelectedPressed => sidebar::batch_delete_selected_pressed(app),
        Message::BatchDeleteConfirmed => sidebar::batch_delete_confirmed(app),
        Message::ModifiersChanged(modifiers) => sidebar::modifiers_changed(app, modifiers),
        Message::ToggleCollectionCollapsed(col_id) => {
            sidebar::toggle_collection_collapsed(app, col_id)
        }
        Message::ToggleFolderCollapsed {
            collection_id,
            folder_path,
        } => sidebar::toggle_folder_collapsed(app, collection_id, folder_path),

        Message::TabDragStarted(idx) => workbench::tab_drag_started(app, idx),
        Message::TabDragEntered(idx) => workbench::tab_drag_entered(app, idx),
        Message::TabDragEnded => workbench::tab_drag_ended(app),

        Message::ToggleConsolePanel => layout::toggle_console_panel(app),
        Message::ClearConsoleLogs => layout::clear_console_logs(app),

        Message::ShowToast(msg, status) => overlays::show_toast(app, msg, status),

        // env actions
        Message::EditEnvironmentPressed(idx) => environment::edit_pressed(app, idx),
        Message::CloseEnvEditorPressed => environment::close_editor_pressed(app),
        Message::AddEnvVariablePressed(env_idx) => environment::add_variable_pressed(app, env_idx),
        Message::DeleteEnvVariablePressed { env_idx, var_idx } => {
            environment::delete_variable_pressed(app, env_idx, var_idx)
        }
        Message::EnvVariableKeyChanged {
            env_idx,
            var_idx,
            key,
        } => environment::variable_key_changed(app, env_idx, var_idx, key),
        Message::EnvVariableValueEditorAction {
            env_idx,
            var_idx,
            action,
        } => environment::variable_value_editor_action(app, env_idx, var_idx, action),
        Message::EnvVariableToggled {
            env_idx,
            var_idx,
            is_active,
        } => environment::variable_toggled(app, env_idx, var_idx, is_active),
        Message::RenameEnvironmentPressed(idx) => environment::rename_pressed(app, idx),
        Message::EnvNameChanged(idx, new_name) => environment::name_changed(app, idx, new_name),
        Message::SaveEnvNamePressed(idx) => environment::save_name_pressed(app, idx),

        // workspace actions
        Message::WorkspaceSelected(name) => workspace::selected(app, name),
        Message::CreateWorkspacePressed => workspace::create_pressed(app),
        Message::DeleteWorkspacePressed(id) => workspace::delete_pressed(app, id),
        Message::RenameWorkspacePressed(id) => workspace::rename_pressed(app, id),
        Message::WorkspaceNameChanged(id, new_name) => workspace::name_changed(app, id, new_name),
        Message::SaveWorkspaceNamePressed(id) => workspace::save_name_pressed(app, id),

        // menu actions
        Message::MenuInteraction(dropdown_msg) => overlays::menu_interaction(app, dropdown_msg),

        // request model actions
        Message::SaveRequestPressed(tab_idx) => workbench::save_request_pressed(app, tab_idx),
        Message::SaveRequestModalCollectionSelected(col_id) => {
            workbench::save_request_modal_collection_selected(app, col_id)
        }
        Message::SaveRequestModalFolderSelected(path) => {
            workbench::save_request_modal_folder_selected(app, path)
        }
        Message::SaveRequestNameChanged(name) => workbench::save_request_name_changed(app, name),
        Message::CloseSaveRequestModal => workbench::close_save_request_modal(app),
        Message::CloseResponseTimingModal => overlays::close_response_timing_modal(app),
        Message::SaveRequestConfirmed => workbench::save_request_confirmed(app),
        Message::SaveActiveRequestShortcut => workbench::save_active_request_shortcut(app), // end save_request_model actions

        // temporary data stores
        Message::AutosaveTick => {
            app.commit_active_workspace_snapshot();
            crate::workspace::save(&app.build_workspace_manifest());
            Task::none()
        }
        Message::SpinnerTick => {
            app.spinner_tick = app.spinner_tick.wrapping_add(1);
            Task::none()
        }

        // self update
        Message::CheckForUpdate => overlays::check_for_update(app),
        // check on startup: same lookup, but stays quiet on "up to date" or errors instead of toasting on every launch
        Message::CheckForUpdateSilently => overlays::check_for_update_silently(),
        Message::SilentUpdateCheckResult(result) => {
            overlays::silent_update_check_result(app, result)
        }
        Message::UpdateCheckResult(result) => overlays::update_check_result(app, result),
        Message::ToastActionPressed(id) => overlays::toast_action_pressed(app, id),
        Message::InstallUpdate => overlays::install_update(app),
        Message::UpdateInstallProgress(progress) => {
            overlays::update_install_progress(app, progress)
        }
        Message::UpdateInstallResult(result) => overlays::update_install_result(app, result),
        // end self update

        // remote development (SSH) - inline "add host" form
        Message::RemoteProfileNameChanged(name) => remote::profile_name_changed(app, name),
        Message::RemoteProfileHostChanged(host) => remote::profile_host_changed(app, host),
        Message::RemoteProfilePortChanged(port) => remote::profile_port_changed(app, port),
        Message::RemoteProfileUsernameChanged(username) => {
            remote::profile_username_changed(app, username)
        }
        Message::RemoteProfileAuthKindChanged(kind) => remote::profile_auth_kind_changed(app, kind),
        Message::RemoteProfileKeyPathChanged(path) => remote::profile_key_path_changed(app, path),
        Message::RemoteAddProfilePressed => remote::add_profile_pressed(app),
        Message::RemoteDeleteProfilePressed(profile_id) => {
            remote::delete_profile_pressed(app, profile_id)
        }

        // remote development (SSH) - connecting
        Message::RemoteConnectPressed(profile_id) => remote::connect_pressed(app, profile_id),
        Message::RemoteConnectSecretChanged(secret) => remote::connect_secret_changed(app, secret),
        Message::RemoteConnectCancelled => remote::connect_cancelled(app),
        Message::RemoteConnectConfirmed => remote::connect_confirmed(app),
        Message::RemoteConnected(profile_id, Ok(session)) => {
            remote::connected_ok(app, profile_id, session)
        }
        Message::RemoteConnected(profile_id, Err(err)) => {
            remote::connected_err(app, profile_id, err)
        }
        Message::RemoteDisconnectPressed(profile_id) => remote::disconnect_pressed(app, profile_id),

        // remote development (SSH) - terminal
        Message::RemoteOpenTerminalPressed(profile_id) => {
            remote::open_terminal_pressed(app, profile_id)
        }
        Message::RemoteShellReady(profile_id, Ok(shell_holder)) => {
            remote::shell_ready_ok(app, profile_id, shell_holder)
        }
        Message::RemoteShellReady(_, Err(err)) => remote::shell_ready_err(err),

        // remote development (SSH) - file explorer
        Message::RemoteExplorerToggled(profile_id) => remote::explorer_toggled(app, profile_id),
        Message::RemoteExplorerPathChanged(profile_id, path) => {
            remote::explorer_path_changed(app, profile_id, path)
        }
        Message::RemoteExplorerGoPressed(profile_id) => {
            remote::explorer_go_pressed(app, profile_id)
        }
        Message::RemoteDirListingLoaded(profile_id, path, result) => {
            remote::dir_listing_loaded(app, profile_id, path, result)
        }
        Message::RemoteEntryClicked(profile_id, path) => {
            remote::entry_clicked(app, profile_id, path)
        }
        Message::RemoteFileLoaded(profile_id, path, result) => {
            remote::file_loaded(app, profile_id, path, result)
        }

        // remote development (SSH) - remote collections
        Message::RemoteImportDirAsCollectionPressed(profile_id, path) => {
            remote::import_dir_as_collection_pressed(app, profile_id, path)
        }
        Message::RemoteCollectionImported(profile_id, root, Ok(collection)) => {
            remote::collection_imported_ok(app, profile_id, root, collection)
        }
        Message::RemoteCollectionImported(profile_id, _, Err(err)) => {
            remote::collection_imported_err(app, profile_id, err)
        }
        Message::RemoteCollectionLoaded(collection_id, Ok(collection)) => {
            remote::collection_loaded_ok(app, collection_id, collection)
        }
        Message::RemoteCollectionLoaded(_, Err(err)) => remote::collection_loaded_err(err),
        Message::RemoteNewCollectionNameChanged(profile_id, name) => {
            remote::new_collection_name_changed(app, profile_id, name)
        }
        Message::RemoteNewCollectionPressed(profile_id) => {
            remote::new_collection_pressed(app, profile_id)
        }

        // remote development (SSH) - open remote file tab
        Message::RemoteFileContentChanged(tab_id, action) => {
            remote::file_content_changed(app, tab_id, action)
        }
        Message::RemoteFileSavePressed(tab_id) => remote::file_save_pressed(app, tab_id),
        Message::RemoteFileSaved(tab_id, result) => remote::file_saved(app, tab_id, result),

        // remote development (SSH)
        Message::OpenRemoteConfig => remote::open_config(app),
        Message::CloseRemoteConfigPressed => remote::close_config_pressed(app),
        Message::WindowCloseRequested(_window_id) => update(app, Message::AppExit),
        // end remote development (SSH)

        // command palette (Ctrl+Shift+P)
        Message::ToggleCommandPalette => overlays::toggle_command_palette(app),
        Message::CommandPaletteQueryChanged(query) => {
            overlays::command_palette_query_changed(app, query)
        }
        Message::CommandPaletteMoveSelection(delta) => {
            overlays::command_palette_move_selection(app, delta)
        }
        Message::CommandPaletteConfirm => overlays::command_palette_confirm(app),
        Message::CommandPaletteClosed => overlays::command_palette_closed(app),
        Message::CommandPaletteItemClicked(action) => {
            overlays::command_palette_item_clicked(app, action)
        }

        // native plugins (wasm)
        Message::OpenPluginManagerPressed => plugins::open_manager_pressed(app),

        // settings (reusable preferences modal)
        Message::OpenSettingsPressed => settings::open_pressed(app),
        Message::CloseSettingsPressed => settings::close_pressed(app),
        Message::SettingsTabSelected(tab) => settings::tab_selected(app, tab),
        Message::ThemeSelected(theme) => settings::theme_selected(app, theme),
        Message::CloseOnOutsideClickToggled(enabled) => {
            settings::close_on_outside_click_toggled(app, enabled)
        }
        Message::TogglePluginEnabled(plugin_id, enabled) => {
            plugins::toggle_enabled(app, plugin_id, enabled)
        }
        Message::InstallPluginPressed => plugins::install_pressed(app),
        Message::PluginInstallFolderPicked(path) => plugins::install_folder_picked(app, path),
        Message::PluginInstallPrepared(result) => plugins::install_prepared(app, result),
        Message::UninstallPluginPressed(plugin_id) => plugins::uninstall_pressed(app, plugin_id),
        Message::UninstallPluginConfirmed(plugin_id) => {
            plugins::uninstall_confirmed(app, plugin_id)
        }
        Message::PluginUninstallFinished(plugin_id, result) => {
            plugins::uninstall_finished(app, plugin_id, result)
        }
        Message::ShowPluginManagerView(view) => plugins::show_manager_view(app, view),
        Message::FetchPluginGallery => plugins::fetch_gallery(app),
        Message::GalleryIndexFetched(result) => plugins::gallery_index_fetched(app, result),
        Message::InstallFromGalleryPressed(entry) => {
            plugins::install_from_gallery_pressed(app, entry)
        }
        Message::PluginManagerSearchChanged(query) => plugins::search_changed(app, query),
        Message::PluginCommand(plugin_id, command_id) => {
            plugins::command(app, plugin_id, command_id)
        }
        Message::OpenPluginPanel(plugin_id, panel_id) => {
            plugins::open_panel(app, plugin_id, panel_id)
        }
        Message::PluginPanelEvent(plugin_id, panel_id, event) => {
            plugins::panel_event(app, plugin_id, panel_id, event)
        }
        Message::PluginProcessTick => plugins::process_tick(app),
        Message::ToggleRightPanel(plugin_id, panel_id) => {
            plugins::toggle_right_panel(app, plugin_id, panel_id)
        }
        Message::RightPanelEvent(plugin_id, panel_id, event) => {
            plugins::right_panel_event(app, plugin_id, panel_id, event)
        }
        Message::ApplyPluginCollectionOp(op) => plugins::apply_collection_op(app, op),

        Message::DismissToast(id) => overlays::dismiss_toast(app, id),
        // exit the application
        Message::AppExit => {
            app.commit_active_workspace_snapshot();
            crate::workspace::save(&app.build_workspace_manifest());
            iced::exit()
        }
    }
}
