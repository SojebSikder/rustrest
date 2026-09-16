use crate::collection::collection::{
    CollectionInfo, CollectionItem, PostmanCollection, PostmanRequestDetails, PostmanRequestNode,
    PostmanUrl, PostmanVariable, RemoteDirRef,
};
use crate::collection::env::Environment;
use crate::collection_adapter::{
    create_tab_from_request, examples_to_saved_responses, saved_responses_to_examples,
};
use crate::http_client::send_request;
use crate::message::{Message, ResizeKind, SidebarDragItem, SidebarItemKey};
use crate::session::{SavedSession, SavedTabEntry};
use crate::ui::confirm_dialog::ConfirmDialogState;
use crate::ui::context_menu::{ContextMenu, FieldTarget, apply_field_paste};
use crate::ui::menu::menu::DropdownMenuState;
use crate::ui::menu::menu_message::MenuMessage;
use crate::ui::plugin_manager::PluginManagerAction;
use crate::ui::remote::{PendingRemoteConnect, RemoteAuthKind, join_remote_path};
use crate::ui::save_request_model::types::SaveRequestModalState;
use crate::ui::settings::{AppTheme, SettingsTab};
use crate::ui::sidebar::flatten_visible_sidebar_items;
use crate::ui::tab::types::{KeyValuePair, ResponseSubTab, ResponseView};
use crate::ui::tab::{Tab, TabMessage};
use crate::ui::toast::toast::{ToastManager, ToastStatus};
use crate::updater::{UpdateInfo, check_for_update, perform_update};
use crate::utils::{
    contains_request_node_by_id, find_request_mut, format_json_or_fallback, insert_nested,
    insert_nested_request, move_sidebar_item, remove_nested, remove_nested_request,
    rename_nested_folder, update_node,
};
use crate::workspace::{CollectionSource, SavedWorkspace, WorkspaceManifest};
use crate::{APP_NAME, APP_VERSION};
use iced::Task;
use rustrest_core::remote::{SshAuthMethod, SshProfile};
use rustrest_remote::{AuthMethod, SshConfig};
use rustrest_terminal::TerminalManager;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

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

pub struct ResizeDrag {
    pub kind: ResizeKind,
    pub start_cursor: iced::Point,
    pub start_size: f32,
}

pub const SIDEBAR_WIDTH_RANGE: (f32, f32) = (180.0, 520.0);
pub const REQUEST_PANE_HEIGHT_RANGE: (f32, f32) = (120.0, 700.0);
pub const CONSOLE_PANEL_HEIGHT_RANGE: (f32, f32) = (120.0, 500.0);

pub struct Rustrest {
    pub collections: Vec<PostmanCollection>,
    pub environments: Vec<Environment>,
    pub active_env_index: Option<usize>,
    pub globals: Vec<KeyValuePair>,
    pub remote_profiles: Vec<SshProfile>,
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
    pub editing_request_collection_id: Option<usize>,
    pub editing_request_id: Option<usize>,
    /// (collection_id, request_id, index) of the saved response currently being renamed.
    pub editing_saved_response: Option<(usize, usize, usize)>,
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

    // git status/diff panel + commit modal + generic confirm dialog
    pub git_status_cache: std::collections::HashMap<
        usize,
        Result<crate::collection::git_ops::GitStatusSnapshot, String>,
    >,
    pub git_selected_file: Option<std::path::PathBuf>,
    pub git_diff_cache: Option<(std::path::PathBuf, String)>,
    pub git_remote_op_running:
        std::collections::HashMap<usize, crate::collection::git_ops::GitRemoteOp>,
    pub commit_modal: Option<crate::ui::commit_modal::CommitModalState>,
    pub confirm_dialog: Option<crate::ui::confirm_dialog::ConfirmDialogState>,

    // sidebar drag-and-drop + collapse state
    pub sidebar_drag: Option<SidebarDragItem>,
    pub collapsed_collections: std::collections::HashSet<usize>,
    pub collapsed_folders: std::collections::HashSet<(usize, Vec<String>)>,
    /// request ids whose saved-responses list is collapsed in the sidebar.
    pub collapsed_saved_responses: std::collections::HashSet<usize>,

    // sidebar multi-select (Ctrl/Cmd-click toggle, Shift-click range)
    pub selected_sidebar_items: std::collections::HashSet<SidebarItemKey>,
    pub sidebar_selection_anchor: Option<SidebarItemKey>,
    /// live keyboard modifier state, updated by a `ModifiersChanged` subscription;
    /// the sidebar view reads this to decide what a row click means.
    pub current_modifiers: iced::keyboard::Modifiers,

    // tab bar drag-to-reorder
    pub dragging_tab_index: Option<usize>,

    pub terminal_manager: TerminalManager,
    /// clone of this and hand to every spawned session so its background I/O
    /// thread can report activity; the app-wide subscription drains the
    /// matching receiver and turns each notice into a `Message`.
    pub terminal_event_tx:
        tokio::sync::mpsc::UnboundedSender<(u64, rustrest_terminal::TerminalNotice)>,
    pub terminal_event_rx: Arc<
        tokio::sync::Mutex<
            tokio::sync::mpsc::UnboundedReceiver<(u64, rustrest_terminal::TerminalNotice)>,
        >,
    >,

    // remote development (SSH) - connected sessions keyed by SshProfile::id,
    // the inline "add host" form, a pending connect awaiting its
    // password/passphrase, and per-profile file explorer state.
    pub remote_sessions: std::collections::HashMap<usize, Arc<rustrest_remote::RemoteSession>>,
    pub remote_profile_form: crate::ui::remote::RemoteProfileForm,
    pub remote_connect_pending: Option<crate::ui::remote::PendingRemoteConnect>,
    pub remote_explorers: std::collections::HashMap<usize, crate::ui::remote::RemoteExplorerState>,
    // the id of the profile a connect attempt is currently in flight for, so
    // the "Connect" button can show a spinner once the password prompt closes.
    pub remote_connecting: Option<usize>,

    // the id of the always-open main window
    pub main_window_id: iced::window::Id,
    // whether the "Remote development over SSH" configuration modal is open
    pub remote_config_open: bool,

    // advances every tick of `spinner_sub` (only running while
    // `any_spinner_active()` is true) to animate loading spinners.
    pub spinner_tick: u64,

    // command palette (Ctrl+Shift+P)
    pub command_palette: Option<rustrest_command_palette::PaletteState>,

    // native plugins (wasm, via rustrest-plugin-host)
    pub plugin_manager: rustrest_plugin_host::PluginManager,
    /// last-rendered declarative UI tree for each open plugin panel,
    /// keyed by (plugin_id, panel_id); refreshed on open and after each event.
    pub plugin_panel_state:
        std::collections::HashMap<(String, String), rustrest_plugin_host::UiNode>,
    /// set while an install or uninstall is running on a background thread,
    /// so the plugin manager can show a spinner and disable other actions.
    pub plugin_manager_busy: Option<crate::ui::plugin_manager::PluginManagerAction>,
    /// set while the user is choosing which installed export-format plugin
    /// to export a collection through (only shown when more than one
    /// plugin/format is available - a single option is used directly).
    pub export_plugin_picker: Option<(usize, Vec<(String, rustrest_plugin_host::FormatDef)>)>,

    // settings
    pub settings_open: bool,
    pub settings_tab: SettingsTab,
    pub theme: AppTheme,
    /// whether clicking outside an open modal/command palette dismisses it
    pub close_on_outside_click: bool,
}

impl Rustrest {
    fn persist_settings(&self) {
        crate::app_settings::save(&crate::app_settings::PersistedSettings {
            theme: self.theme,
            close_on_outside_click: self.close_on_outside_click,
        });
    }

    /// whether any spinner-driven loading indicator is currently shown, so
    /// the animation tick subscription only runs while it's actually needed.
    pub fn any_spinner_active(&self) -> bool {
        self.remote_connecting.is_some()
            || self.remote_explorers.values().any(|e| e.loading)
            || self.tabs.iter().any(|t| t.tab.is_loading)
            || self.commit_modal.as_ref().is_some_and(|m| m.committing)
            || self.plugin_manager_busy.is_some()
            || !self.git_remote_op_running.is_empty()
            || self.toast_manager.has_pending()
            || self.tabs.iter().any(|t| {
                matches!(
                    &t.content,
                    WorkspaceContent::CollectionRoot { collection_id, active_sub_tab, .. }
                        if *active_sub_tab == CollectionSubTab::Git
                            && !self.git_status_cache.contains_key(collection_id)
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
            globals: self.globals.clone(),
            collapsed_collections: self.collapsed_collections.clone(),
            collapsed_folders: self.collapsed_folders.clone(),
            collapsed_saved_responses: self.collapsed_saved_responses.clone(),
            remote_profiles: self.remote_profiles.clone(),
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
        for tab in &self.tabs {
            if let WorkspaceContent::Terminal { terminal_id, .. } = tab.content {
                self.terminal_manager.close(terminal_id);
            }
        }
        self.tabs.clear();
        self.active_tab_index = 0;

        // remote sessions (and their explorer state) belong to the
        // workspace being left; the new workspace has its own profile list.
        self.remote_sessions.clear();
        self.remote_explorers.clear();
        self.remote_connect_pending = None;
        self.selected_sidebar_items.clear();
        self.sidebar_selection_anchor = None;

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
        self.globals = ws.globals.clone();
        self.remote_profiles = ws.remote_profiles.clone();
        self.collapsed_collections = ws.collapsed_collections.clone();
        self.collapsed_folders = ws.collapsed_folders.clone();
        self.collapsed_saved_responses = ws.collapsed_saved_responses.clone();

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

    /// if `key` is one of 2+ currently multi-selected sidebar rows, returns the
    /// batch `MultiSelection` context menu instead of the single-item one.
    pub fn context_menu_for_sidebar_item(
        &self,
        key: SidebarItemKey,
        default: ContextMenu,
    ) -> ContextMenu {
        if self.selected_sidebar_items.len() > 1 && self.selected_sidebar_items.contains(&key) {
            ContextMenu::MultiSelection(self.selected_sidebar_items.iter().cloned().collect())
        } else {
            default
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
        environments: Vec::new(),
        active_env_index: None,
        globals: Vec::new(),
        remote_profiles: Vec::new(),
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
        editing_request_collection_id: None,
        editing_request_id: None,
        editing_saved_response: None,
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
        git_status_cache: std::collections::HashMap::new(),
        git_selected_file: None,
        git_diff_cache: None,
        git_remote_op_running: std::collections::HashMap::new(),
        commit_modal: None,
        confirm_dialog: None,
        sidebar_drag: None,
        collapsed_collections: std::collections::HashSet::new(),
        collapsed_folders: std::collections::HashSet::new(),
        collapsed_saved_responses: std::collections::HashSet::new(),
        selected_sidebar_items: std::collections::HashSet::new(),
        sidebar_selection_anchor: None,
        current_modifiers: iced::keyboard::Modifiers::default(),
        dragging_tab_index: None,
        terminal_manager: TerminalManager::new(),
        terminal_event_tx,
        terminal_event_rx: Arc::new(tokio::sync::Mutex::new(terminal_event_rx)),
        remote_sessions: std::collections::HashMap::new(),
        remote_profile_form: crate::ui::remote::RemoteProfileForm::default(),
        remote_connect_pending: None,
        remote_explorers: std::collections::HashMap::new(),
        remote_connecting: None,
        main_window_id,
        remote_config_open: false,
        spinner_tick: 0,
        command_palette: None,
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
        export_plugin_picker: None,
        settings_open: false,
        settings_tab: SettingsTab::default(),
        theme: persisted_settings.theme,
        close_on_outside_click: persisted_settings.close_on_outside_click,
    };
    app.plugin_manager.load_all();
    let plugin_load_errors: Vec<String> = app
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
        auto_connect_remote_collections(&app),
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

fn persist_collection_if_known_location(
    app: &mut Rustrest,
    col_id: usize,
    success_msg: String,
) -> Task<Message> {
    if let Some(collection) = app.collections.iter().find(|c| c.id == col_id) {
        if let Some(remote) = &collection.remote_dir {
            let profile_id = remote.profile_id;
            let root = remote.root.clone();
            return match app.remote_sessions.get(&profile_id).cloned() {
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

/// forwards any pending `host_log()` lines from plugins into the app's
/// existing console panel, so plugin activity shows up alongside request
/// logs without a separate UI surface.
fn drain_plugin_logs(app: &mut Rustrest) {
    app.console_logs.extend(app.plugin_manager.drain_logs());
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
                WorkspaceContent::Terminal { .. } => "Terminal".to_string(),
                WorkspaceContent::RemoteFile { path, .. } => path.clone(),
                WorkspaceContent::Plugin { panel_id, .. } => panel_id.clone(),
                WorkspaceContent::PluginManager => "Manage Plugins".to_string(),
            };
        }
    }

    app.tab_rename_input_hovered = false;
    app.sync_tab_to_collection(idx);
}

/// closes the tab at `index`, if any. closing the last remaining tab leaves
/// `app.tabs` empty, which shows the workspace's empty-state screen.
fn close_tab(app: &mut Rustrest, index: usize) {
    if let Some(tab_state) = app.tabs.get(index) {
        if tab_state.tab.is_loading {
            tab_state.tab.cancel_token.cancel();
        }
        if let WorkspaceContent::Terminal { terminal_id, .. } = tab_state.content {
            app.terminal_manager.close(terminal_id);
        }
    } else {
        return;
    }

    app.tabs.remove(index);
    if app.active_tab_index >= app.tabs.len() && !app.tabs.is_empty() {
        app.active_tab_index = app.tabs.len() - 1;
    }
}

/// where SSH host keys seen by the remote-development feature are recorded
/// (trust-on-first-use), separate from any system-wide `~/.ssh/known_hosts`.
fn remote_known_hosts_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_NAME)
        .join("remote_known_hosts")
}

/// connects to `profile` over SSH (provisioning the remote agent if needed)
/// and reports the outcome as a `RemoteConnected` message. `secret` is the
/// password/passphrase to use, or empty for agent auth / a keyless key.
fn spawn_remote_connect(profile: &SshProfile, secret: String) -> Task<Message> {
    let auth = match &profile.auth_method {
        SshAuthMethod::Password => AuthMethod::Password(secret.clone()),
        SshAuthMethod::PrivateKey { path } => AuthMethod::PrivateKey {
            path: path.clone(),
            passphrase: if secret.is_empty() {
                None
            } else {
                Some(secret.clone())
            },
        },
        SshAuthMethod::Agent => AuthMethod::Agent,
    };
    let config = SshConfig {
        host: profile.host.clone(),
        port: profile.port,
        username: profile.username.clone(),
        auth,
    };
    let known_hosts_path = remote_known_hosts_path();
    let app_version = APP_VERSION.to_string();
    let app_version_for_download = app_version.clone();
    let profile_id = profile.id;

    Task::perform(
        async move {
            rustrest_remote::RemoteSession::connect(
                &config,
                &known_hosts_path,
                &app_version,
                move |platform| async move {
                    let target = platform.target_triple()?.to_string();
                    tokio::task::spawn_blocking(move || {
                        crate::remote_agent::provision(&target, &app_version_for_download)
                    })
                    .await
                    .map_err(|e| rustrest_remote::RemoteError::Remote(e.to_string()))?
                    .map_err(rustrest_remote::RemoteError::Remote)
                },
            )
            .await
            .map_err(|e| e.to_string())
        },
        move |result| Message::RemoteConnected(profile_id, result.map(Arc::new)),
    )
}

/// attempts a silent (no user-entered secret) connect for every distinct SSH
/// profile referenced by a remote-backed collection currently in
/// `app.collections` that isn't already connected - covers auto-reconnect on
/// startup and workspace switches. profiles whose auth needs an interactive
/// secret (password, or a passphrase-protected key) are skipped; those stay
/// offline until the user clicks Connect.
fn auto_connect_remote_collections(app: &Rustrest) -> Task<Message> {
    let mut seen = std::collections::HashSet::new();
    let mut tasks = Vec::new();

    for col in &app.collections {
        let Some(remote) = &col.remote_dir else {
            continue;
        };
        if app.remote_sessions.contains_key(&remote.profile_id) {
            continue;
        }
        if !seen.insert(remote.profile_id) {
            continue;
        }
        let Some(profile) = app
            .remote_profiles
            .iter()
            .find(|p| p.id == remote.profile_id)
        else {
            continue;
        };
        if matches!(profile.auth_method, SshAuthMethod::Password) {
            continue;
        }
        tasks.push(spawn_remote_connect(profile, String::new()));
    }

    Task::batch(tasks)
}

fn start_git_remote_op(
    app: &mut Rustrest,
    col_id: usize,
    op: crate::collection::git_ops::GitRemoteOp,
) -> Task<Message> {
    use crate::collection::git_ops::GitRemoteOp;

    let dir = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .and_then(|c| c.storage_dir.clone());

    let Some(dir) = dir else {
        return Task::none();
    };

    app.git_remote_op_running.insert(col_id, op);

    Task::perform(
        async move {
            match op {
                GitRemoteOp::Push => crate::collection::git_ops::git_push(&dir).await,
                GitRemoteOp::Pull => crate::collection::git_ops::git_pull(&dir).await,
                GitRemoteOp::Fetch => crate::collection::git_ops::git_fetch(&dir).await,
            }
        },
        move |result| Message::GitRemoteOpResult(col_id, op, result),
    )
}

pub fn update(app: &mut Rustrest, message: Message) -> Task<Message> {
    match message {
        Message::ContextMenuAction(inner) => {
            app.active_context_menu = None;
            update(app, *inner)
        }
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
            let result = app.plugin_manager.import(&plugin_id, &format_id, bytes);
            drain_plugin_logs(app);
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
        Message::SaveCollectionPressed(col_id) => {
            app.sync_collection_tabs(col_id);

            let has_known_location = app
                .collections
                .iter()
                .find(|c| c.id == col_id)
                .map(|c| c.storage_dir.is_some() || c.file_path.is_some() || c.remote_dir.is_some())
                .unwrap_or(false);

            if !has_known_location {
                // never been saved anywhere, behave like export (first-time save)
                return iced::Task::done(Message::ExportCollectionPressed(col_id));
            }

            if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
                col.clear_unsaved();
            }
            for tab_state in &mut app.tabs {
                let belongs = match &tab_state.content {
                    WorkspaceContent::HttpRequest => tab_state.tab.collection_id == Some(col_id),
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
            app.git_status_cache.remove(&col_id);

            Task::done(Message::ShowToast(
                "Collection reloaded from disk".to_string(),
                ToastStatus::Success,
            ))
        }

        // git status/diff panel
        Message::GitStatusRequested(col_id) => {
            let dir = app
                .collections
                .iter()
                .find(|c| c.id == col_id)
                .and_then(|c| c.storage_dir.clone());

            match dir {
                Some(dir) => Task::perform(
                    async move { crate::collection::git_ops::git_status(&dir).await },
                    move |result| Message::GitStatusLoaded(col_id, result),
                ),
                None => Task::none(),
            }
        }
        Message::GitStatusLoaded(col_id, result) => {
            app.git_status_cache.insert(col_id, result);
            Task::none()
        }
        Message::GitDiffRequested(col_id, file) => {
            app.git_selected_file = Some(file.clone());
            let dir = app
                .collections
                .iter()
                .find(|c| c.id == col_id)
                .and_then(|c| c.storage_dir.clone());

            match dir {
                Some(dir) => {
                    let file_for_result = file.clone();
                    Task::perform(
                        async move { crate::collection::git_ops::git_diff_file(&dir, &file).await },
                        move |result| Message::GitDiffLoaded(col_id, file_for_result, result),
                    )
                }
                None => Task::none(),
            }
        }
        Message::GitDiffLoaded(_col_id, file, result) => {
            if app.git_selected_file.as_ref() == Some(&file) {
                let content = match result {
                    Ok(diff) if diff.trim().is_empty() => "(no textual differences)".to_string(),
                    Ok(diff) => diff,
                    Err(e) => format!("Failed to load diff: {e}"),
                };
                app.git_diff_cache = Some((file, content));
            }
            Task::none()
        }

        // git remote sync (push/pull/fetch)
        Message::GitPushPressed(col_id) => {
            start_git_remote_op(app, col_id, crate::collection::git_ops::GitRemoteOp::Push)
        }
        Message::GitPullPressed(col_id) => {
            start_git_remote_op(app, col_id, crate::collection::git_ops::GitRemoteOp::Pull)
        }
        Message::GitFetchPressed(col_id) => {
            start_git_remote_op(app, col_id, crate::collection::git_ops::GitRemoteOp::Fetch)
        }
        Message::GitRemoteOpResult(col_id, op, result) => {
            app.git_remote_op_running.remove(&col_id);
            let label = op.label();
            match result {
                Ok(output) => {
                    let toast_msg = if output.is_empty() {
                        format!("{label} completed")
                    } else {
                        output
                    };
                    Task::batch([
                        Task::done(Message::ShowToast(toast_msg, ToastStatus::Success)),
                        Task::done(Message::GitStatusRequested(col_id)),
                    ])
                }
                Err(e) => Task::done(Message::ShowToast(
                    format!("{label} failed: {e}"),
                    ToastStatus::Error,
                )),
            }
        }

        // commit modal
        Message::CommitChangesPressed(col_id) => {
            app.sync_collection_tabs(col_id);
            let Some(collection) = app.collections.iter().find(|c| c.id == col_id) else {
                return Task::none();
            };
            let Some(dir) = collection.storage_dir.clone() else {
                return Task::none();
            };
            let collection_name = collection.info.name.clone();

            Task::perform(
                async move { crate::collection::git_ops::git_status(&dir).await },
                move |result| match result {
                    Ok(snapshot) => {
                        Message::CommitStatusLoaded(col_id, collection_name.clone(), snapshot)
                    }
                    Err(e) => Message::ShowToast(
                        format!("Failed to read git status: {e}"),
                        ToastStatus::Error,
                    ),
                },
            )
        }
        Message::CommitStatusLoaded(col_id, collection_name, snapshot) => {
            if snapshot.files.is_empty() {
                app.git_status_cache.insert(col_id, Ok(snapshot));
                return Task::done(Message::ShowToast(
                    "No changes to commit".to_string(),
                    ToastStatus::Info,
                ));
            }
            app.git_status_cache.insert(col_id, Ok(snapshot.clone()));
            app.commit_modal = Some(crate::ui::commit_modal::CommitModalState {
                collection_id: col_id,
                collection_name,
                message: iced::widget::text_editor::Content::new(),
                files: snapshot.files,
                committing: false,
            });
            Task::none()
        }
        Message::CommitMessageChanged(action) => {
            if let Some(modal) = app.commit_modal.as_mut() {
                modal.message.perform(action);
            }
            Task::none()
        }
        Message::CommitCancelled => {
            app.commit_modal = None;
            Task::none()
        }
        Message::CommitConfirmed => {
            let Some(modal) = app.commit_modal.as_ref() else {
                return Task::none();
            };
            let Some(collection) = app.collections.iter().find(|c| c.id == modal.collection_id)
            else {
                return Task::none();
            };
            let Some(dir) = collection.storage_dir.clone() else {
                return Task::none();
            };
            let col_id = modal.collection_id;
            let message = modal.message.text();
            if let Some(modal) = app.commit_modal.as_mut() {
                modal.committing = true;
            }

            Task::perform(
                async move { crate::collection::git_ops::git_commit_all(&dir, &message).await },
                move |result| Message::CommitResult(col_id, result),
            )
        }
        Message::CommitResult(col_id, result) => match result {
            Ok(()) => {
                app.commit_modal = None;
                Task::batch([
                    Task::done(Message::ShowToast(
                        "Changes committed".to_string(),
                        ToastStatus::Success,
                    )),
                    Task::done(Message::GitStatusRequested(col_id)),
                ])
            }
            Err(e) => {
                if let Some(modal) = app.commit_modal.as_mut() {
                    modal.committing = false;
                }
                Task::done(Message::ShowToast(
                    format!("Commit failed: {e}"),
                    ToastStatus::Error,
                ))
            }
        },

        // generic reusable confirm dialog
        Message::ShowConfirmDialog(state) => {
            app.confirm_dialog = Some(state);
            Task::none()
        }
        Message::ConfirmDialogAccepted => {
            if let Some(state) = app.confirm_dialog.take() {
                return Task::done(*state.on_confirm);
            }
            Task::none()
        }
        Message::ConfirmDialogCancelled => {
            app.confirm_dialog = None;
            Task::none()
        }
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

        Message::ExportViaPluginPressed(col_id) => {
            let mut formats = app.plugin_manager.export_formats();
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
                    app.export_plugin_picker = Some((col_id, formats));
                    iced::Task::none()
                }
            }
        }

        Message::CloseExportPluginPicker => {
            app.export_plugin_picker = None;
            iced::Task::none()
        }

        Message::ExportCollectionViaPluginPressed(col_id, plugin_id, format_id, extensions) => {
            app.export_plugin_picker = None;
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

            let result = app.plugin_manager.export(&plugin_id, &format_id, value);
            drain_plugin_logs(app);
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
            if !app.selected_sidebar_items.contains(&key) {
                app.selected_sidebar_items.clear();
                app.sidebar_selection_anchor = None;
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
            if !app.selected_sidebar_items.contains(&key) {
                app.selected_sidebar_items.clear();
                app.sidebar_selection_anchor = None;
            }

            app.sidebar_drag = Some(SidebarDragItem::Request {
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
        } => {
            app.active_context_menu = Some(ContextMenu::SavedResponse {
                col_id: collection_id,
                req_id: request_id,
                index,
            });
            app.context_menu_position = app.cursor_position;
            Task::none()
        }

        Message::RenameSavedResponsePressed {
            collection_id,
            request_id,
            index,
        } => {
            app.editing_saved_response = Some((collection_id, request_id, index));
            app.active_context_menu = None;
            Task::none()
        }

        Message::SavedResponseNameChanged {
            collection_id,
            request_id,
            index,
            new_name,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                if let Some(node) = find_request_mut(&mut col.item, request_id) {
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

        Message::SaveSavedResponseNamePressed => {
            app.editing_saved_response = None;
            Task::none()
        }

        Message::DeleteSavedResponsePressed {
            collection_id,
            request_id,
            index,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                if let Some(node) = find_request_mut(&mut col.item, request_id) {
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
            if app.editing_saved_response == Some((collection_id, request_id, index)) {
                app.editing_saved_response = None;
            }
            app.refresh_open_tab_saved_responses(collection_id, request_id);
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
            iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
        }

        Message::CloseTabPressed(index) => {
            close_tab(app, index);
            Task::none()
        }

        Message::NewTerminalTabPressed => {
            let tx = app.terminal_event_tx.clone();
            match app.terminal_manager.spawn(80, 24, move |id, notice| {
                let _ = tx.send((id, notice));
            }) {
                Ok(terminal_id) => {
                    let widget_id = iced::widget::Id::unique();
                    let mut term_tab = Tab::new(app.next_tab_id);
                    term_tab.name = format!("Terminal {}", terminal_id + 1);
                    app.tabs.push(TabState {
                        tab: term_tab,
                        content: WorkspaceContent::Terminal {
                            terminal_id,
                            widget_id: widget_id.clone(),
                        },
                        is_editing_name: false,
                    });
                    app.next_tab_id += 1;
                    app.active_tab_index = app.tabs.len() - 1;

                    Task::batch([
                        iced::widget::operation::snap_to_end(
                            crate::ui::workspace::tab_bar_scroll_id(),
                        ),
                        iced::widget::operation::focus(widget_id),
                    ])
                }
                Err(err) => Task::done(Message::ShowToast(
                    format!("Failed to start terminal: {err}"),
                    ToastStatus::Error,
                )),
            }
        }

        Message::TerminalInput(id, bytes) => {
            if let Some(session) = app.terminal_manager.get(id) {
                session.scroll_to_bottom();
                session.write(bytes);
            }
            Task::none()
        }

        Message::TerminalResized(id, columns, rows, cell_width, cell_height) => {
            app.terminal_manager
                .resize(id, columns, rows, cell_width, cell_height);
            Task::none()
        }

        Message::TerminalNotice(id, notice) => {
            if notice == rustrest_terminal::TerminalNotice::Exited {
                if let Some(idx) = app.tabs.iter().position(|t| {
                    matches!(t.content, WorkspaceContent::Terminal { terminal_id, .. } if terminal_id == id)
                }) {
                    close_tab(app, idx);
                }
            }
            Task::none()
        }

        Message::CloseActiveTabShortcut => {
            close_tab(app, app.active_tab_index);
            Task::none()
        }

        Message::ActiveTabMessage(tab_msg) => {
            if let TabMessage::ShowFieldContextMenu(target, value) = tab_msg {
                app.active_context_menu = Some(ContextMenu::TextField {
                    target: FieldTarget::Tab(target),
                    current_value: value,
                });
                app.context_menu_position = app.cursor_position;
                return Task::none();
            }
            let active_idx = app.active_tab_index;
            let mut syncs_saved_responses = false;
            if let Some(tab_state) = app.tabs.get_mut(active_idx) {
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
                    syncs_saved_responses = matches!(
                        tab_msg,
                        TabMessage::SaveResponse | TabMessage::DeleteSavedResponse(_)
                    );
                    tab_state.tab.update(tab_msg);
                }
            }
            // saving/deleting a response snapshot mutates the request's saved-response
            // list directly, so mirror it into the collection tree (sidebar) right away
            // instead of waiting for an explicit "Save Request".
            if syncs_saved_responses {
                app.sync_active_tab_saved_responses_to_collection(active_idx);
            }
            Task::none()
        }

        // script engine used
        Message::SendPressed => {
            if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
                if !matches!(tab_state.content, WorkspaceContent::HttpRequest) {
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

                let mut script_globals: std::collections::HashMap<String, String> = app
                    .globals
                    .iter()
                    .filter(|v| v.is_active)
                    .map(|v| (v.key.clone(), v.value.clone()))
                    .collect();

                let script_vars_snapshot = script_vars.clone();

                let pre_script_text = tab.pre_request_script.text();
                match crate::script_engine::ScriptRunner::run_pre_request(
                    &pre_script_text,
                    &mut script_vars,
                    &mut script_headers,
                    &mut script_globals,
                ) {
                    Ok(logs) => app.console_logs.extend(logs),
                    Err(e) => return Task::done(Message::ShowToast(e, ToastStatus::Error)),
                }

                for (k, v) in &script_globals {
                    if let Some(existing) = app.globals.iter_mut().find(|kv| &kv.key == k) {
                        existing.value = v.clone();
                        existing.is_active = true;
                    } else {
                        let mut kv = KeyValuePair::new(k, v);
                        kv.is_active = true;
                        app.globals.push(kv);
                    }
                }

                if let Some(idx) = app.active_env_index {
                    if let Some(env) = app.environments.get_mut(idx) {
                        for (k, v) in &script_vars {
                            if script_vars_snapshot.get(k) != Some(v) {
                                if let Some(existing) =
                                    env.variables.iter_mut().find(|kv| &kv.key == k)
                                {
                                    existing.value = v.clone();
                                    existing.is_active = true;
                                } else {
                                    let mut kv = KeyValuePair::new(k, v);
                                    kv.is_active = true;
                                    env.variables.push(kv);
                                }
                            }
                        }
                    }
                }

                let mut effective_env = app
                    .active_env_index
                    .and_then(|i| app.environments.get(i))
                    .cloned()
                    .unwrap_or_else(|| Environment::new("__script"));

                // globals are the lowest-precedence variable source; environment/script
                // variables (applied next) override them for the same key
                for (k, v) in &script_globals {
                    if !effective_env.variables.iter().any(|kv| &kv.key == k) {
                        let mut kv = KeyValuePair::new(k, v);
                        kv.is_active = true;
                        effective_env.variables.push(kv);
                    }
                }

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

                // native plugin request hooks run after the per-request JS
                // script, as an app-wide policy layer (e.g. injecting an
                // auth header for every request). method changes aren't
                // applied back in v1 - only url/headers/body/variables are.
                let plugin_ctx = rustrest_plugin_host::RequestContext {
                    method: tab.method.to_string(),
                    url: final_url,
                    headers: filtered_headers,
                    body: compiled_body,
                    variables: script_vars.clone(),
                };
                let plugin_ctx = app.plugin_manager.run_pre_request_hooks(plugin_ctx);
                app.console_logs.extend(app.plugin_manager.drain_logs());
                let final_url = plugin_ctx.url;
                let filtered_headers = plugin_ctx.headers;
                let compiled_body = plugin_ctx.body;

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

                let mut test_results = Vec::new();
                if let Ok(resp) = &res {
                    let script_text = tab.post_response_script.text();
                    if !script_text.trim().is_empty() {
                        let mut base_vars: std::collections::HashMap<String, String> = app
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

                        if let Some(c_id) = tab.collection_id {
                            if let Some(col) = app.collections.iter().find(|c| c.id == c_id) {
                                for kv in col.get_native_variables() {
                                    base_vars.entry(kv.key).or_insert(kv.value);
                                }
                            }
                        }

                        let base_vars_snapshot = base_vars.clone();

                        let base_globals: std::collections::HashMap<String, String> = app
                            .globals
                            .iter()
                            .filter(|v| v.is_active)
                            .map(|v| (v.key.clone(), v.value.clone()))
                            .collect();

                        let exec_ctx = crate::script_engine::ScriptExecutionContext {
                            variables: base_vars,
                            globals: base_globals,
                            response_status: resp.status,
                            response_body: resp.body.clone(),
                            response_headers: resp.headers.clone(),
                        };

                        match crate::script_engine::ScriptRunner::run_post_response(
                            &script_text,
                            &exec_ctx,
                        ) {
                            Ok((updated_vars, updated_globals, results, logs)) => {
                                app.console_logs.extend(logs);
                                test_results = results;
                                if let Some(idx) = app.active_env_index {
                                    if let Some(env) = app.environments.get_mut(idx) {
                                        for (k, v) in updated_vars {
                                            if base_vars_snapshot.get(&k) == Some(&v) {
                                                continue;
                                            }
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
                                for (k, v) in updated_globals {
                                    if let Some(existing) =
                                        app.globals.iter_mut().find(|kv| kv.key == k)
                                    {
                                        existing.value = v;
                                        existing.is_active = true;
                                    } else {
                                        let mut kv = KeyValuePair::new(&k, &v);
                                        kv.is_active = true;
                                        app.globals.push(kv);
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

                // native plugin response hooks run after the per-request JS
                // test script, same app-wide-policy ordering as the
                // pre-request side. response status/headers/body are
                // read-only here (mirroring `pm.response` in JS scripts);
                // only variables and additional test results are applied back.
                if let Ok(resp) = &res {
                    let plugin_vars: std::collections::HashMap<String, String> = app
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
                    let plugin_vars_snapshot = plugin_vars.clone();

                    let plugin_ctx = rustrest_plugin_host::ResponseContext {
                        status: resp.status,
                        headers: resp.headers.clone(),
                        body: resp.body.clone(),
                        variables: plugin_vars,
                        test_results: Vec::new(),
                    };
                    let plugin_ctx = app.plugin_manager.run_post_response_hooks(plugin_ctx);
                    app.console_logs.extend(app.plugin_manager.drain_logs());

                    if let Some(idx) = app.active_env_index {
                        if let Some(env) = app.environments.get_mut(idx) {
                            for (k, v) in plugin_ctx.variables {
                                if plugin_vars_snapshot.get(&k) == Some(&v) {
                                    continue;
                                }
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

                    test_results.extend(plugin_ctx.test_results.into_iter().map(|t| {
                        crate::http_client::TestResult {
                            name: t.name,
                            passed: t.passed,
                        }
                    }));
                }

                let mut res = res;
                if let Ok(resp) = &mut res {
                    resp.test_results = test_results;
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
                tab_state.tab.dirty = true;
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
            let new_idx = app.environments.len() - 1;
            app.active_env_index = Some(new_idx);

            // open the environment editor on the newly created environment
            app.editing_env_index = Some(new_idx);
            app.editing_env_name = false;

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
            let mut collection_id_for_git = None;
            if let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) {
                if let WorkspaceContent::CollectionRoot {
                    collection_id,
                    ref mut active_sub_tab,
                    ..
                } = tab_state.content
                {
                    *active_sub_tab = sub_tab.clone();
                    if sub_tab == CollectionSubTab::Git {
                        collection_id_for_git = Some(collection_id);
                    }
                }
            }
            if let Some(col_id) = collection_id_for_git {
                return iced::Task::done(Message::GitStatusRequested(col_id));
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
                col.unsaved = true;
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
                col.unsaved = true;
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
                col.unsaved = true;
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
                col.unsaved = true;
            }
            Task::none()
        }

        Message::CreateNewCollectionPressed => {
            let col_id = app.next_tab_id;
            app.next_tab_id += 1;

            let col_name = format!("New Collection");
            let new_col = PostmanCollection {
                id: col_id,
                info: CollectionInfo {
                    name: col_name.clone(),
                    postman_id: None,
                    schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                        .to_string(),
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
                },
                is_editing_name: false,
            });
            app.next_tab_id += 1;
            app.active_tab_index = app.tabs.len() - 1;
            iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
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
                col.unsaved = true;

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

                // make sure the new folder is visible in the sidebar
                app.collapsed_collections.remove(&collection_id);
                for i in 0..=parent_folder_path.len() {
                    app.collapsed_folders
                        .remove(&(collection_id, parent_folder_path[..i].to_vec()));
                }

                // open the newly created folder for renaming
                let mut new_folder_path = parent_folder_path.clone();
                new_folder_path.push("New Folder".to_string());
                app.editing_folder_collection_id = Some(collection_id);
                app.editing_folder_path = new_folder_path;
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
                    unsaved: true,
                    response: None,
                };

                let tab_request_node = new_request_node.clone();

                insert_nested_request(
                    &mut col.item,
                    &parent_folder_path,
                    CollectionItem::Request(new_request_node),
                );

                // make sure the new request is visible in the sidebar
                app.collapsed_collections.remove(&collection_id);
                for i in 0..=parent_folder_path.len() {
                    app.collapsed_folders
                        .remove(&(collection_id, parent_folder_path[..i].to_vec()));
                }

                // open the newly created request in a new tab
                let new_tab = create_tab_from_request(
                    app.next_tab_id,
                    &tab_request_node,
                    Some(collection_id),
                );
                app.tabs.push(TabState {
                    tab: new_tab,
                    content: WorkspaceContent::HttpRequest,
                    is_editing_name: false,
                });
                app.next_tab_id += 1;
                app.active_tab_index = app.tabs.len() - 1;
                return iced::widget::operation::snap_to_end(
                    crate::ui::workspace::tab_bar_scroll_id(),
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

        // request rename actions
        Message::RenameRequestPressed {
            collection_id,
            request_id,
        } => {
            app.editing_request_collection_id = Some(collection_id);
            app.editing_request_id = Some(request_id);
            app.active_context_menu = None;
            Task::none()
        }

        Message::RequestNameChanged {
            collection_id,
            request_id,
            new_name,
        } => {
            if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                if let Some(node) = find_request_mut(&mut col.item, request_id) {
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

        Message::SaveRequestNamePressed { .. } => {
            app.editing_request_collection_id = None;
            app.editing_request_id = None;
            Task::none()
        }

        Message::ToggleSavedResponsesCollapsed(request_id) => {
            if !app.collapsed_saved_responses.remove(&request_id) {
                app.collapsed_saved_responses.insert(request_id);
            }
            Task::none()
        }

        // context menu
        Message::ShowCollectionContextMenu(col_id) => {
            let key = SidebarItemKey::Collection(col_id);
            app.active_context_menu =
                Some(app.context_menu_for_sidebar_item(key, ContextMenu::Collection(col_id)));
            app.context_menu_position = app.cursor_position;
            Task::none()
        }

        Message::ShowGitActionsMenu(col_id) => {
            app.active_context_menu = Some(ContextMenu::GitActions(col_id));
            app.context_menu_position = app.cursor_position;
            Task::none()
        }

        Message::ShowFolderContextMenu {
            collection_id,
            folder_path,
        } => {
            let key = SidebarItemKey::Folder {
                collection_id,
                path: folder_path.clone(),
            };
            app.active_context_menu = Some(app.context_menu_for_sidebar_item(
                key,
                ContextMenu::Folder {
                    col_id: collection_id,
                    path: folder_path,
                },
            ));
            app.context_menu_position = app.cursor_position;
            Task::none()
        }

        Message::ShowRequestContextMenu {
            collection_id,
            folder_path,
            request_id,
        } => {
            let key = SidebarItemKey::Request {
                collection_id,
                parent_path: folder_path.clone(),
                request_id,
            };
            app.active_context_menu = Some(app.context_menu_for_sidebar_item(
                key,
                ContextMenu::Request {
                    col_id: collection_id,
                    folder_path,
                    req_id: request_id,
                },
            ));
            app.context_menu_position = app.cursor_position;
            Task::none()
        }

        Message::CloseContextMenu => {
            app.active_context_menu = None;
            Task::none()
        }

        Message::ShowTextFieldContextMenu(target, current_value) => {
            app.active_context_menu = Some(ContextMenu::TextField {
                target,
                current_value,
            });
            app.context_menu_position = app.cursor_position;
            Task::none()
        }

        Message::CopyToClipboard(text) => {
            app.active_context_menu = None;
            iced::clipboard::write(text)
        }

        Message::PasteIntoField(target) => {
            app.active_context_menu = None;
            iced::clipboard::read().map(move |clipboard_text| {
                Message::TextFieldPasteResolved(target.clone(), clipboard_text)
            })
        }

        Message::TextFieldPasteResolved(target, clipboard_text) => {
            if let Some(text) = clipboard_text {
                apply_field_paste(app, target, text);
            }
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

        Message::SidebarDragStarted(item) => {
            let key = SidebarItemKey::from_drag_item(&item);
            if !app.selected_sidebar_items.contains(&key) {
                app.selected_sidebar_items.clear();
                app.sidebar_selection_anchor = None;
            }
            app.sidebar_drag = Some(item);
            Task::none()
        }

        Message::SidebarDropped(target) => {
            if let Some(drag) = app.sidebar_drag.take() {
                let dragged_key = SidebarItemKey::from_drag_item(&drag);
                let is_batch = app.selected_sidebar_items.len() > 1
                    && app.selected_sidebar_items.contains(&dragged_key);

                // when the dragged row is part of a larger selection, move the
                // whole selection together; otherwise just the one dragged item.
                let items_to_move: Vec<SidebarDragItem> = if is_batch {
                    let mut keys: Vec<SidebarItemKey> =
                        app.selected_sidebar_items.iter().cloned().collect();
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
                            fc == collection_id
                                && fp.len() < path.len()
                                && path.starts_with(fp.as_slice())
                        }),
                        SidebarItemKey::Request {
                            collection_id,
                            parent_path,
                            ..
                        } => !folder_paths.iter().any(|(fc, fp)| {
                            fc == collection_id && parent_path.starts_with(fp.as_slice())
                        }),
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

                app.selected_sidebar_items.clear();
                app.sidebar_selection_anchor = None;
            }
            Task::none()
        }

        Message::SidebarItemToggleSelect(key) => {
            if !app.selected_sidebar_items.remove(&key) {
                app.selected_sidebar_items.insert(key.clone());
            }
            app.sidebar_selection_anchor = Some(key);
            Task::none()
        }

        Message::SidebarItemRangeSelect(key) => {
            let flat = flatten_visible_sidebar_items(app);
            let anchor = app
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
                    app.selected_sidebar_items = flat[lo..=hi].iter().cloned().collect();
                }
                _ => {
                    app.selected_sidebar_items.insert(key);
                }
            }
            Task::none()
        }

        Message::ClearSidebarSelection => {
            app.selected_sidebar_items.clear();
            app.sidebar_selection_anchor = None;
            Task::none()
        }

        Message::BatchDeleteSelectedPressed => {
            let count = app.selected_sidebar_items.len();
            if count == 0 {
                return Task::none();
            }
            app.confirm_dialog = Some(ConfirmDialogState {
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

        Message::BatchDeleteConfirmed => {
            let items: Vec<SidebarItemKey> = app.selected_sidebar_items.drain().collect();
            app.sidebar_selection_anchor = None;

            let collections_to_delete: std::collections::HashSet<usize> = items
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

        Message::ModifiersChanged(modifiers) => {
            app.current_modifiers = modifiers;
            Task::none()
        }

        Message::ToggleCollectionCollapsed(col_id) => {
            if !app.collapsed_collections.remove(&col_id) {
                app.collapsed_collections.insert(col_id);
            }
            Task::none()
        }

        Message::ToggleFolderCollapsed {
            collection_id,
            folder_path,
        } => {
            let key = (collection_id, folder_path);
            if !app.collapsed_folders.remove(&key) {
                app.collapsed_folders.insert(key);
            }
            Task::none()
        }

        Message::TabDragStarted(idx) => {
            app.active_tab_index = idx;
            app.dragging_tab_index = Some(idx);
            Task::none()
        }

        Message::TabDragEntered(idx) => {
            if let Some(from) = app.dragging_tab_index {
                if from != idx && from < app.tabs.len() && idx < app.tabs.len() {
                    let was_active = app.active_tab_index;
                    let moved_was_active = was_active == from;

                    let tab = app.tabs.remove(from);
                    app.tabs.insert(idx, tab);
                    app.dragging_tab_index = Some(idx);

                    app.active_tab_index = if moved_was_active {
                        idx
                    } else if from < was_active && idx >= was_active {
                        was_active - 1
                    } else if from > was_active && idx <= was_active {
                        was_active + 1
                    } else {
                        was_active
                    };
                }
            }
            Task::none()
        }

        Message::TabDragEnded => {
            app.dragging_tab_index = None;
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

            let mut tasks = vec![
                Task::done(Message::ShowToast(
                    format!("Switched to workspace '{}'", target.name),
                    ToastStatus::Success,
                )),
                auto_connect_remote_collections(app),
            ];
            if dropped > 0 {
                tasks.push(Task::done(Message::ShowToast(
                    format!(
                        "{dropped} unsaved collection(s) weren't carried over, save them to disk first to keep them across workspace switches"
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
                globals: Vec::new(),
                collapsed_collections: std::collections::HashSet::new(),
                collapsed_folders: std::collections::HashSet::new(),
                collapsed_saved_responses: std::collections::HashSet::new(),
                remote_profiles: Vec::new(),
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
                    MenuMessage::CommandPalette => {
                        return update(app, Message::ToggleCommandPalette);
                    }
                    MenuMessage::CheckForUpdate => {
                        return update(app, Message::CheckForUpdate);
                    }
                    MenuMessage::HelpAbout => {
                        let version_info = format!("{} v{}", APP_NAME, APP_VERSION);
                        return update(app, Message::ShowToast(version_info, ToastStatus::Info));
                    }
                    MenuMessage::OpenPluginManager => {
                        return update(app, Message::OpenPluginManagerPressed);
                    }
                    MenuMessage::OpenSettings => {
                        return update(app, Message::OpenSettingsPressed);
                    }
                    MenuMessage::Plugin(plugin_id, command_id) => {
                        return update(app, Message::PluginCommand(plugin_id, command_id));
                    }
                    MenuMessage::ImportViaPlugin(plugin_id, format_id, extensions) => {
                        return update(
                            app,
                            Message::ImportCollectionViaPluginPressed(
                                plugin_id, format_id, extensions,
                            ),
                        );
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
                    if let (Some(req_id), Some(col_id)) =
                        (tab_state.tab.request_id, tab_state.tab.collection_id)
                    {
                        app.sync_tab_to_collection(tab_idx);
                        if let Some(tab_state) = app.tabs.get_mut(tab_idx) {
                            tab_state.tab.dirty = false;
                        }
                        if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
                            if let Some(node) = find_request_mut(&mut col.item, req_id) {
                                node.unsaved = false;
                            }
                        }
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

            let already_linked_req_id =
                app.tabs.get(modal.tab_index).and_then(|t| t.tab.request_id);

            if let Some(req_id) = already_linked_req_id {
                if let Some(tab_state) = app.tabs.get_mut(modal.tab_index) {
                    tab_state.tab.name = name;
                    tab_state.tab.dirty = false;
                }
                app.sync_tab_to_collection(modal.tab_index);
                if let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) {
                    if let Some(node) = find_request_mut(&mut col.item, req_id) {
                        node.unsaved = false;
                    }
                }
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
                tab_state.tab.dirty = false;
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
                    WorkspaceContent::Terminal { .. } => Task::none(),
                    WorkspaceContent::RemoteFile { .. } => {
                        let tab_id = tab_state.tab.id;
                        update(app, Message::RemoteFileSavePressed(tab_id))
                    }
                    WorkspaceContent::Plugin { .. } => Task::none(),
                    WorkspaceContent::PluginManager => Task::none(),
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
        Message::SpinnerTick => {
            app.spinner_tick = app.spinner_tick.wrapping_add(1);
            Task::none()
        }

        // self update
        Message::CheckForUpdate => {
            let toast_task = crate::ui::toast::toast::show_pending_and_schedule(
                &mut app.toast_manager,
                "Checking for updates…".to_string(),
                crate::ui::toast::toast::TOAST_DURATION,
            );
            let check_task = iced::Task::perform(
                async {
                    tokio::task::spawn_blocking(check_for_update)
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::UpdateCheckResult,
            );
            iced::Task::batch([toast_task, check_task])
        }

        // check on startup: same lookup, but stays quiet on "up to date" or errors instead of toasting on every launch
        Message::CheckForUpdateSilently => iced::Task::perform(
            async {
                tokio::task::spawn_blocking(check_for_update)
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
            },
            Message::SilentUpdateCheckResult,
        ),

        Message::SilentUpdateCheckResult(Ok(Some(info))) => {
            update(app, Message::UpdateCheckResult(Ok(Some(info))))
        }
        Message::SilentUpdateCheckResult(_) => Task::none(),

        Message::UpdateCheckResult(Ok(Some(info))) => {
            let msg = format!("Update available: v{}", info.version);
            app.available_update = Some(info);
            let (id, task) = crate::ui::toast::toast::show_with_action_and_schedule(
                &mut app.toast_manager,
                msg,
                ToastStatus::Info,
                crate::ui::toast::toast::TOAST_DURATION_LONG,
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
            let toast_task = crate::ui::toast::toast::show_pending_and_schedule(
                &mut app.toast_manager,
                "Downloading update…".to_string(),
                crate::ui::toast::toast::TOAST_DURATION_LONG,
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
            crate::ui::toast::toast::TOAST_DURATION_LONG,
        ),

        Message::UpdateInstallResult(Err(e)) => crate::ui::toast::toast::show_and_schedule(
            &mut app.toast_manager,
            format!("Update failed: {e}"),
            ToastStatus::Error,
            crate::ui::toast::toast::TOAST_DURATION,
        ),
        // end self update

        // remote development (SSH) - inline "add host" form
        Message::RemoteProfileNameChanged(name) => {
            app.remote_profile_form.name = name;
            Task::none()
        }
        Message::RemoteProfileHostChanged(host) => {
            app.remote_profile_form.host = host;
            Task::none()
        }
        Message::RemoteProfilePortChanged(port) => {
            app.remote_profile_form.port = port;
            Task::none()
        }
        Message::RemoteProfileUsernameChanged(username) => {
            app.remote_profile_form.username = username;
            Task::none()
        }
        Message::RemoteProfileAuthKindChanged(kind) => {
            app.remote_profile_form.auth_kind = kind;
            Task::none()
        }
        Message::RemoteProfileKeyPathChanged(path) => {
            app.remote_profile_form.key_path = path;
            Task::none()
        }
        Message::RemoteAddProfilePressed => {
            let form = app.remote_profile_form.clone();
            if form.name.trim().is_empty()
                || form.host.trim().is_empty()
                || form.username.trim().is_empty()
            {
                return Task::done(Message::ShowToast(
                    "Name, host, and username are required".to_string(),
                    ToastStatus::Error,
                ));
            }
            let Ok(port) = form.port.trim().parse::<u16>() else {
                return Task::done(Message::ShowToast(
                    "Port must be a number".to_string(),
                    ToastStatus::Error,
                ));
            };
            let auth_method = match form.auth_kind {
                RemoteAuthKind::Password => SshAuthMethod::Password,
                RemoteAuthKind::PrivateKey => SshAuthMethod::PrivateKey {
                    path: std::path::PathBuf::from(form.key_path.trim()),
                },
                RemoteAuthKind::Agent => SshAuthMethod::Agent,
            };
            let id = app
                .remote_profiles
                .iter()
                .map(|p| p.id)
                .max()
                .map(|m| m + 1)
                .unwrap_or(0);
            app.remote_profiles.push(SshProfile {
                id,
                name: form.name.trim().to_string(),
                host: form.host.trim().to_string(),
                port,
                username: form.username.trim().to_string(),
                auth_method,
            });
            app.remote_profile_form = crate::ui::remote::RemoteProfileForm::default();
            app.commit_active_workspace_snapshot();
            crate::workspace::save(&app.build_workspace_manifest());
            Task::none()
        }
        Message::RemoteDeleteProfilePressed(profile_id) => {
            app.remote_profiles.retain(|p| p.id != profile_id);
            app.remote_sessions.remove(&profile_id);
            app.remote_explorers.remove(&profile_id);
            app.commit_active_workspace_snapshot();
            crate::workspace::save(&app.build_workspace_manifest());
            Task::none()
        }

        // remote development (SSH) - connecting
        Message::RemoteConnectPressed(profile_id) => {
            app.remote_connect_pending = Some(PendingRemoteConnect {
                profile_id,
                secret: String::new(),
            });
            Task::none()
        }
        Message::RemoteConnectSecretChanged(secret) => {
            if let Some(pending) = &mut app.remote_connect_pending {
                pending.secret = secret;
            }
            Task::none()
        }
        Message::RemoteConnectCancelled => {
            app.remote_connect_pending = None;
            Task::none()
        }
        Message::RemoteConnectConfirmed => {
            let Some(pending) = app.remote_connect_pending.take() else {
                return Task::none();
            };
            let Some(profile) = app
                .remote_profiles
                .iter()
                .find(|p| p.id == pending.profile_id)
                .cloned()
            else {
                return Task::none();
            };
            app.remote_connecting = Some(profile.id);
            spawn_remote_connect(&profile, pending.secret)
        }
        Message::RemoteConnected(profile_id, Ok(session)) => {
            if app.remote_connecting == Some(profile_id) {
                app.remote_connecting = None;
            }
            app.remote_sessions.insert(profile_id, session.clone());
            let name = app
                .remote_profiles
                .iter()
                .find(|p| p.id == profile_id)
                .map(|p| p.name.clone())
                .unwrap_or_default();

            // reload any remote-backed collections waiting on this profile
            let mut tasks = vec![Task::done(Message::ShowToast(
                format!("Connected to {name}"),
                ToastStatus::Success,
            ))];
            for col in &app.collections {
                let Some(remote) = &col.remote_dir else {
                    continue;
                };
                if remote.profile_id != profile_id {
                    continue;
                }
                let col_id = col.id;
                let root = remote.root.clone();
                let session = session.clone();
                tasks.push(Task::perform(
                    async move {
                        rustrest_remote::collection_sync::load_collection_from_remote_dir(
                            &session, &root,
                        )
                        .await
                        .map_err(|e| e.to_string())
                    },
                    move |result| Message::RemoteCollectionLoaded(col_id, result.map(Box::new)),
                ));
            }
            Task::batch(tasks)
        }
        Message::RemoteConnected(profile_id, Err(err)) => {
            if app.remote_connecting == Some(profile_id) {
                app.remote_connecting = None;
            }
            Task::done(Message::ShowToast(
                format!("Connect failed: {err}"),
                ToastStatus::Error,
            ))
        }
        Message::RemoteDisconnectPressed(profile_id) => {
            app.remote_sessions.remove(&profile_id);
            app.remote_explorers.remove(&profile_id);
            Task::none()
        }

        // remote development (SSH) - terminal
        Message::RemoteOpenTerminalPressed(profile_id) => {
            let Some(session) = app.remote_sessions.get(&profile_id).cloned() else {
                return Task::none();
            };
            Task::perform(
                async move {
                    session
                        .open_shell(80, 24)
                        .await
                        .map(|shell| Arc::new(std::sync::Mutex::new(Some(shell))))
                        .map_err(|e| e.to_string())
                },
                move |result| Message::RemoteShellReady(profile_id, result),
            )
        }
        Message::RemoteShellReady(profile_id, Ok(shell_holder)) => {
            let Some(shell) = shell_holder.lock().unwrap().take() else {
                return Task::none();
            };
            let tx = app.terminal_event_tx.clone();
            let (terminal_id, feed, commands) =
                app.terminal_manager
                    .spawn_remote(80, 24, move |id, notice| {
                        let _ = tx.send((id, notice));
                    });
            rustrest_remote::bridge_shell_to_terminal(shell, feed, commands);

            let profile_name = app
                .remote_profiles
                .iter()
                .find(|p| p.id == profile_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "remote".to_string());

            let widget_id = iced::widget::Id::unique();
            let mut term_tab = Tab::new(app.next_tab_id);
            term_tab.name = format!("{profile_name} (remote)");
            app.tabs.push(TabState {
                tab: term_tab,
                content: WorkspaceContent::Terminal {
                    terminal_id,
                    widget_id: widget_id.clone(),
                },
                is_editing_name: false,
            });
            app.next_tab_id += 1;
            app.active_tab_index = app.tabs.len() - 1;

            Task::batch([
                iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id()),
                iced::widget::operation::focus(widget_id),
            ])
        }
        Message::RemoteShellReady(_, Err(err)) => Task::done(Message::ShowToast(
            format!("Failed to open remote terminal: {err}"),
            ToastStatus::Error,
        )),

        // remote development (SSH) - file explorer
        Message::RemoteExplorerToggled(profile_id) => {
            let explorer = app.remote_explorers.entry(profile_id).or_default();
            explorer.visible = !explorer.visible;
            if explorer.path.is_empty() {
                explorer.path = ".".to_string();
            }
            let should_load = explorer.visible && explorer.entries.is_empty();
            if should_load {
                Task::done(Message::RemoteExplorerGoPressed(profile_id))
            } else {
                Task::none()
            }
        }
        Message::RemoteExplorerPathChanged(profile_id, path) => {
            let explorer = app.remote_explorers.entry(profile_id).or_default();
            explorer.path = path;
            Task::none()
        }
        Message::RemoteExplorerGoPressed(profile_id) => {
            let Some(session) = app.remote_sessions.get(&profile_id).cloned() else {
                return Task::none();
            };
            let explorer = app.remote_explorers.entry(profile_id).or_default();
            explorer.loading = true;
            explorer.error = None;
            let path = explorer.path.clone();
            let path_for_result = path.clone();

            Task::perform(
                async move { session.list_dir(&path).await.map_err(|e| e.to_string()) },
                move |result| {
                    Message::RemoteDirListingLoaded(profile_id, path_for_result.clone(), result)
                },
            )
        }
        Message::RemoteDirListingLoaded(profile_id, path, result) => {
            let explorer = app.remote_explorers.entry(profile_id).or_default();
            explorer.loading = false;
            if explorer.path == path {
                match result {
                    Ok(entries) => {
                        explorer.entries = entries;
                        explorer.error = None;
                    }
                    Err(err) => explorer.error = Some(err),
                }
            }
            Task::none()
        }
        Message::RemoteEntryClicked(profile_id, path) => {
            let Some(explorer) = app.remote_explorers.get(&profile_id) else {
                return Task::none();
            };
            let is_dir = explorer
                .entries
                .iter()
                .find(|e| join_remote_path(&explorer.path, &e.name) == path)
                .map(|e| e.is_dir)
                .unwrap_or(false);

            if is_dir {
                Task::batch([
                    Task::done(Message::RemoteExplorerPathChanged(profile_id, path)),
                    Task::done(Message::RemoteExplorerGoPressed(profile_id)),
                ])
            } else {
                let Some(session) = app.remote_sessions.get(&profile_id).cloned() else {
                    return Task::none();
                };
                app.remote_explorers.entry(profile_id).or_default().loading = true;
                let path_for_result = path.clone();
                Task::perform(
                    async move { session.read_file(&path).await.map_err(|e| e.to_string()) },
                    move |result| {
                        Message::RemoteFileLoaded(profile_id, path_for_result.clone(), result)
                    },
                )
            }
        }
        Message::RemoteFileLoaded(profile_id, path, result) => {
            if let Some(explorer) = app.remote_explorers.get_mut(&profile_id) {
                explorer.loading = false;
            }
            match result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    let mut file_tab = Tab::new(app.next_tab_id);
                    file_tab.name = path.clone();
                    app.tabs.push(TabState {
                        tab: file_tab,
                        content: WorkspaceContent::RemoteFile {
                            profile_id,
                            path,
                            contents: iced::widget::text_editor::Content::with_text(&text),
                            dirty: false,
                        },
                        is_editing_name: false,
                    });
                    app.next_tab_id += 1;
                    app.active_tab_index = app.tabs.len() - 1;
                    Task::none()
                }
                Err(err) => Task::done(Message::ShowToast(
                    format!("Failed to open remote file: {err}"),
                    ToastStatus::Error,
                )),
            }
        }

        // remote development (SSH) - remote collections
        Message::RemoteImportDirAsCollectionPressed(profile_id, path) => {
            let Some(session) = app.remote_sessions.get(&profile_id).cloned() else {
                return Task::none();
            };
            app.remote_explorers.entry(profile_id).or_default().loading = true;
            let path_for_result = path.clone();
            Task::perform(
                async move {
                    rustrest_remote::collection_sync::load_collection_from_remote_dir(
                        &session, &path,
                    )
                    .await
                    .map_err(|e| e.to_string())
                },
                move |result| {
                    Message::RemoteCollectionImported(
                        profile_id,
                        path_for_result.clone(),
                        result.map(Box::new),
                    )
                },
            )
        }
        Message::RemoteCollectionImported(profile_id, root, Ok(mut collection)) => {
            if let Some(explorer) = app.remote_explorers.get_mut(&profile_id) {
                explorer.loading = false;
            }
            collection.remote_dir = Some(RemoteDirRef {
                profile_id,
                root: root.clone(),
            });

            let existing_id = app.collections.iter().find_map(|c| {
                let remote = c.remote_dir.as_ref()?;
                (remote.profile_id == profile_id && remote.root == root).then_some(c.id)
            });

            if let Some(existing_id) = existing_id {
                app.sync_collection_tabs(existing_id);
                let existing_name = app
                    .collections
                    .iter()
                    .find(|c| c.id == existing_id)
                    .map(|c| c.info.name.clone())
                    .unwrap_or_default();
                collection.id = existing_id;

                return Task::done(Message::ShowConfirmDialog(ConfirmDialogState {
                    title: "Reload collection from remote host?".to_string(),
                    message: format!(
                        "\"{existing_name}\" is already open. Reloading will discard any unsaved changes."
                    ),
                    confirm_label: "Reload".to_string(),
                    on_confirm: Box::new(Message::ReplaceCollectionConfirmed(
                        existing_id,
                        collection,
                    )),
                }));
            }

            let col_name = collection.info.name.clone();
            collection.id = app.next_tab_id;
            app.next_tab_id += 1;
            collection.assign_request_ids(&mut app.next_request_id);
            app.collections.push(*collection);

            Task::done(Message::ShowToast(
                format!("Collection '{}' imported from remote host", col_name),
                ToastStatus::Success,
            ))
        }
        Message::RemoteCollectionImported(profile_id, _, Err(err)) => {
            if let Some(explorer) = app.remote_explorers.get_mut(&profile_id) {
                explorer.loading = false;
            }
            Task::done(Message::ShowToast(
                format!("Failed to import remote collection: {err}"),
                ToastStatus::Error,
            ))
        }

        Message::RemoteCollectionLoaded(collection_id, Ok(mut collection)) => {
            let remote_dir = app
                .collections
                .iter()
                .find(|c| c.id == collection_id)
                .and_then(|c| c.remote_dir.clone());

            if remote_dir.is_some() {
                collection.id = collection_id;
                collection.remote_dir = remote_dir;
                collection.assign_request_ids(&mut app.next_request_id);
                if let Some(existing) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                    *existing = *collection;
                }
            }
            Task::none()
        }
        Message::RemoteCollectionLoaded(_, Err(err)) => Task::done(Message::ShowToast(
            format!("Failed to load remote collection: {err}"),
            ToastStatus::Error,
        )),

        Message::RemoteNewCollectionNameChanged(profile_id, name) => {
            if let Some(explorer) = app.remote_explorers.get_mut(&profile_id) {
                explorer.new_collection_name = name;
            }
            Task::none()
        }
        Message::RemoteNewCollectionPressed(profile_id) => {
            let Some(explorer) = app.remote_explorers.get(&profile_id) else {
                return Task::none();
            };
            let name = explorer.new_collection_name.trim().to_string();
            if name.is_empty() {
                return Task::done(Message::ShowToast(
                    "Enter a name for the new collection".to_string(),
                    ToastStatus::Error,
                ));
            }
            let Some(session) = app.remote_sessions.get(&profile_id).cloned() else {
                return Task::none();
            };
            let root = join_remote_path(&explorer.path, &name);
            app.remote_explorers.entry(profile_id).or_default().loading = true;
            let collection = PostmanCollection {
                id: 0,
                file_path: None,
                storage_dir: None,
                remote_dir: None,
                unsaved: false,
                info: CollectionInfo {
                    name: name.clone(),
                    postman_id: None,
                    schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                        .to_string(),
                },
                item: Vec::new(),
                variable: Some(Vec::new()),
            };
            let root_for_result = root.clone();
            Task::perform(
                async move {
                    rustrest_remote::collection_sync::sync_collection_to_remote_dir(
                        &session,
                        &collection,
                        &root,
                    )
                    .await
                    .map(|()| collection)
                    .map_err(|e| e.to_string())
                },
                move |result| {
                    Message::RemoteCollectionImported(
                        profile_id,
                        root_for_result.clone(),
                        result.map(Box::new),
                    )
                },
            )
        }

        // remote development (SSH) - open remote file tab
        Message::RemoteFileContentChanged(tab_id, action) => {
            if let Some(tab_state) = app.tabs.iter_mut().find(|t| t.tab.id == tab_id) {
                if let WorkspaceContent::RemoteFile {
                    contents, dirty, ..
                } = &mut tab_state.content
                {
                    let is_edit = matches!(action, iced::widget::text_editor::Action::Edit(_));
                    contents.perform(action);
                    if is_edit {
                        *dirty = true;
                    }
                }
            }
            Task::none()
        }
        Message::RemoteFileSavePressed(tab_id) => {
            let Some(tab_state) = app.tabs.iter().find(|t| t.tab.id == tab_id) else {
                return Task::none();
            };
            let WorkspaceContent::RemoteFile {
                profile_id,
                path,
                contents,
                ..
            } = &tab_state.content
            else {
                return Task::none();
            };
            let Some(session) = app.remote_sessions.get(profile_id).cloned() else {
                return Task::done(Message::ShowToast(
                    "Not connected to that remote host anymore".to_string(),
                    ToastStatus::Error,
                ));
            };
            let path = path.clone();
            let bytes = contents.text().into_bytes();

            Task::perform(
                async move {
                    session
                        .write_file(&path, bytes)
                        .await
                        .map_err(|e| e.to_string())
                },
                move |result| Message::RemoteFileSaved(tab_id, result),
            )
        }
        Message::RemoteFileSaved(tab_id, result) => match result {
            Ok(()) => {
                if let Some(tab_state) = app.tabs.iter_mut().find(|t| t.tab.id == tab_id) {
                    if let WorkspaceContent::RemoteFile { dirty, .. } = &mut tab_state.content {
                        *dirty = false;
                    }
                }
                Task::done(Message::ShowToast(
                    "Saved".to_string(),
                    ToastStatus::Success,
                ))
            }
            Err(err) => Task::done(Message::ShowToast(
                format!("Save failed: {err}"),
                ToastStatus::Error,
            )),
        },
        // remote development (SSH)
        Message::OpenRemoteConfig => {
            app.remote_config_open = true;
            Task::none()
        }
        Message::CloseRemoteConfigPressed => {
            app.remote_config_open = false;
            Task::none()
        }
        Message::WindowCloseRequested(_window_id) => update(app, Message::AppExit),
        // end remote development (SSH)

        // command palette (Ctrl+Shift+P)
        Message::ToggleCommandPalette => {
            if app.command_palette.take().is_some() {
                Task::none()
            } else {
                app.command_palette = Some(rustrest_command_palette::PaletteState::new());
                iced::widget::operation::focus(crate::ui::command_palette::input_id())
            }
        }
        Message::CommandPaletteQueryChanged(query) => {
            if let Some(state) = app.command_palette.as_mut() {
                state.query = query;
                state.selected = 0;
            }
            Task::none()
        }
        Message::CommandPaletteMoveSelection(delta) => {
            if let Some(mut state) = app.command_palette.take() {
                let len = crate::ui::command_palette::matches_for(app, &state).len();
                state.move_selection(delta, len);
                app.command_palette = Some(state);
            }
            Task::none()
        }
        Message::CommandPaletteConfirm => {
            let Some(state) = app.command_palette.take() else {
                return Task::none();
            };
            let matches = crate::ui::command_palette::matches_for(app, &state);
            match matches.get(state.selected) {
                Some(cmd) => {
                    let action = cmd.action.clone();
                    update(app, crate::ui::command_palette::to_message(action))
                }
                None => Task::none(),
            }
        }
        Message::CommandPaletteClosed => {
            app.command_palette = None;
            Task::none()
        }
        Message::CommandPaletteItemClicked(action) => {
            app.command_palette = None;
            update(app, crate::ui::command_palette::to_message(action))
        }

        // native plugins (wasm)
        Message::OpenPluginManagerPressed => {
            let existing = app
                .tabs
                .iter()
                .position(|t| matches!(t.content, WorkspaceContent::PluginManager));
            if let Some(idx) = existing {
                app.active_tab_index = idx;
                return Task::none();
            }

            let mut tab = Tab::new(app.next_tab_id);
            tab.name = "Manage Plugins".to_string();
            app.next_tab_id += 1;
            app.tabs.push(TabState {
                tab,
                content: WorkspaceContent::PluginManager,
                is_editing_name: false,
            });
            app.active_tab_index = app.tabs.len() - 1;
            iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
        }

        // settings (reusable preferences modal)
        Message::OpenSettingsPressed => {
            app.settings_open = true;
            Task::none()
        }
        Message::CloseSettingsPressed => {
            app.settings_open = false;
            Task::none()
        }
        Message::SettingsTabSelected(tab) => {
            app.settings_tab = tab;
            Task::none()
        }
        Message::ThemeSelected(theme) => {
            app.theme = theme;
            app.persist_settings();
            Task::none()
        }
        Message::CloseOnOutsideClickToggled(enabled) => {
            app.close_on_outside_click = enabled;
            app.persist_settings();
            Task::none()
        }
        Message::TogglePluginEnabled(plugin_id, enabled) => {
            app.plugin_manager.set_enabled(&plugin_id, enabled);
            Task::none()
        }
        Message::InstallPluginPressed => {
            if app.plugin_manager_busy.is_some() {
                return Task::none();
            }
            // set busy immediately (not just once a file is picked) so the
            // spinner shows right away and the plugin-manager modal's
            // outside-click-to-close is suppressed for the whole file-picker
            // + install duration - opening the native dialog can otherwise
            // cause a spurious "click outside" event once focus returns.
            app.plugin_manager_busy = Some(PluginManagerAction::Installing);
            iced::Task::perform(
                async move {
                    let folder = rfd::AsyncFileDialog::new().pick_folder().await?;
                    Some(folder.path().to_path_buf())
                },
                Message::PluginInstallFolderPicked,
            )
        }
        Message::PluginInstallFolderPicked(path) => {
            let Some(source) = path else {
                app.plugin_manager_busy = None;
                return Task::none();
            };
            // the actual wasm compilation (the slow part of an install) runs
            // on a background thread via `prepare_install`, so it doesn't
            // freeze the UI; `PluginInstallPrepared` finishes on the main
            // thread with the already-compiled module.
            let engine = app.plugin_manager.engine_handle();
            let plugins_dir = app.plugin_manager.plugins_dir().to_path_buf();
            iced::Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        rustrest_plugin_host::PluginManager::prepare_install(
                            &engine,
                            &plugins_dir,
                            &source,
                        )
                        .map_err(|e| e.to_string())
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::PluginInstallPrepared,
            )
        }
        Message::PluginInstallPrepared(result) => {
            app.plugin_manager_busy = None;
            let task = match result {
                Ok((dir_name, manifest, module)) => {
                    match app
                        .plugin_manager
                        .finish_install(dir_name, manifest, module)
                    {
                        Ok(id) => Task::done(Message::ShowToast(
                            format!("Plugin '{id}' installed successfully"),
                            ToastStatus::Success,
                        )),
                        Err(e) => Task::done(Message::ShowToast(
                            format!("Failed to install plugin: {e}"),
                            ToastStatus::Error,
                        )),
                    }
                }
                Err(e) => Task::done(Message::ShowToast(
                    format!("Failed to install plugin: {e}"),
                    ToastStatus::Error,
                )),
            };
            drain_plugin_logs(app);
            task
        }
        Message::UninstallPluginPressed(plugin_id) => {
            if app.plugin_manager_busy.is_some() {
                return Task::none();
            }
            app.confirm_dialog = Some(ConfirmDialogState {
                title: "Uninstall Plugin".to_string(),
                message: format!(
                    "Uninstall '{plugin_id}'? This removes it from disk and can't be undone."
                ),
                confirm_label: "Uninstall".to_string(),
                on_confirm: Box::new(Message::UninstallPluginConfirmed(plugin_id)),
            });
            Task::none()
        }
        Message::UninstallPluginConfirmed(plugin_id) => {
            let Some(dir_name) = app.plugin_manager.dir_name_for(&plugin_id) else {
                return Task::done(Message::ShowToast(
                    format!("Plugin '{plugin_id}' not found"),
                    ToastStatus::Error,
                ));
            };
            let plugin_dir = app.plugin_manager.plugins_dir().join(&dir_name);
            app.plugin_manager_busy = Some(PluginManagerAction::Uninstalling(plugin_id.clone()));
            iced::Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&plugin_dir))
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r.map_err(|e| e.to_string()))
                },
                move |result| Message::PluginUninstallFinished(plugin_id.clone(), result),
            )
        }
        Message::PluginUninstallFinished(plugin_id, result) => {
            app.plugin_manager_busy = None;
            let task = match result {
                Ok(()) => {
                    app.plugin_manager.drop_plugin(&plugin_id);
                    Task::done(Message::ShowToast(
                        format!("Plugin '{plugin_id}' uninstalled"),
                        ToastStatus::Success,
                    ))
                }
                Err(e) => Task::done(Message::ShowToast(
                    format!("Failed to uninstall plugin: {e}"),
                    ToastStatus::Error,
                )),
            };
            drain_plugin_logs(app);
            task
        }
        Message::PluginCommand(plugin_id, command_id) => {
            let task = match app.plugin_manager.run_command(&plugin_id, &command_id) {
                Ok(Some(msg)) => Task::done(Message::ShowToast(msg, ToastStatus::Info)),
                Ok(None) => Task::none(),
                Err(e) => Task::done(Message::ShowToast(
                    format!("Plugin command failed: {e}"),
                    ToastStatus::Error,
                )),
            };
            drain_plugin_logs(app);
            task
        }
        Message::OpenPluginPanel(plugin_id, panel_id) => {
            let existing = app.tabs.iter().position(|t| {
                matches!(
                    &t.content,
                    WorkspaceContent::Plugin { plugin_id: p, panel_id: pa }
                        if *p == plugin_id && *pa == panel_id
                )
            });
            if let Some(idx) = existing {
                app.active_tab_index = idx;
                return Task::none();
            }

            match app.plugin_manager.render_panel(&plugin_id, &panel_id) {
                Ok(tree) => {
                    let title = app
                        .plugin_manager
                        .sidebar_panels()
                        .into_iter()
                        .find(|(pid, panel)| *pid == plugin_id && panel.id == panel_id)
                        .map(|(_, panel)| panel.title)
                        .unwrap_or_else(|| panel_id.clone());

                    app.plugin_panel_state
                        .insert((plugin_id.clone(), panel_id.clone()), tree);

                    let mut tab = Tab::new(app.next_tab_id);
                    tab.name = title;
                    app.next_tab_id += 1;
                    app.tabs.push(TabState {
                        tab,
                        content: WorkspaceContent::Plugin {
                            plugin_id,
                            panel_id,
                        },
                        is_editing_name: false,
                    });
                    app.active_tab_index = app.tabs.len() - 1;
                    Task::none()
                }
                Err(e) => Task::done(Message::ShowToast(
                    format!("Failed to open plugin panel: {e}"),
                    ToastStatus::Error,
                )),
            }
        }
        Message::PluginPanelEvent(plugin_id, panel_id, event) => {
            let task = match app.plugin_manager.panel_event(&plugin_id, &panel_id, event) {
                Ok(Some(tree)) => {
                    app.plugin_panel_state.insert((plugin_id, panel_id), tree);
                    Task::none()
                }
                Ok(None) => Task::none(),
                Err(e) => Task::done(Message::ShowToast(
                    format!("Plugin panel action failed: {e}"),
                    ToastStatus::Error,
                )),
            };
            drain_plugin_logs(app);
            task
        }
        Message::PluginProcessTick => {
            let touched = app.plugin_manager.pump_processes();
            if !touched.is_empty() {
                let open_panels: Vec<(String, String)> = app
                    .tabs
                    .iter()
                    .filter_map(|t| match &t.content {
                        WorkspaceContent::Plugin {
                            plugin_id,
                            panel_id,
                        } if touched.contains(plugin_id) => {
                            Some((plugin_id.clone(), panel_id.clone()))
                        }
                        _ => None,
                    })
                    .collect();
                for (plugin_id, panel_id) in open_panels {
                    if let Ok(tree) = app.plugin_manager.render_panel(&plugin_id, &panel_id) {
                        app.plugin_panel_state.insert((plugin_id, panel_id), tree);
                    }
                }
                drain_plugin_logs(app);
            }
            Task::none()
        }

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
