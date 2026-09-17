//! Native wasm plugins (`rustrest-plugin-host`): the Manage Plugins tab
//! (install/uninstall/enable/browse-gallery), dispatching commands and
//! sidebar-panel-as-tab events, and the generic right-panel host (see
//! `rustrest_plugin_host::RightPanelAction` - any plugin declaring
//! `Capability::RightPanel` is driven through here, not just an AI-agent-style
//! plugin).

use super::{Rustrest, Tab, TabState, WorkspaceContent};
use crate::message::Message;
use crate::plugin_gallery::GalleryEntry;
use crate::ui::plugin_manager::{PluginManagerAction, PluginManagerView};
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use rustrest_plugin_host::{
    FormatDef, PluginManager, RightPanelAction, RightPanelContext, UiEvent, UiNode,
};
use std::collections::HashMap;

pub struct PluginsState {
    pub plugin_manager: PluginManager,
    /// last-rendered declarative UI tree for each open plugin panel,
    /// keyed by (plugin_id, panel_id); refreshed on open and after each event.
    pub plugin_panel_state: HashMap<(String, String), UiNode>,
    /// set while an install or uninstall is running on a background thread,
    /// so the plugin manager can show a spinner and disable other actions.
    pub plugin_manager_busy: Option<PluginManagerAction>,
    /// which sub-view of the Manage Plugins tab is showing (installed, browse).
    pub plugin_manager_view: PluginManagerView,
    /// cached result of the last plugin gallery index fetch; `None` until
    /// the Browse view has been opened at least once.
    pub plugin_gallery_entries: Option<Result<Vec<GalleryEntry>, String>>,
    /// current text in the Manage Plugins search box, applied to whichever
    /// of Installed/Browse is showing.
    pub plugin_manager_search: String,
    /// set while the user is choosing which installed export-format plugin
    /// to export a collection through (only shown when more than one
    /// plugin/format is available - a single option is used directly).
    pub export_plugin_picker: Option<(usize, Vec<(String, FormatDef)>)>,

    // right panel (generic dock for a plugin's `RightPanel` capability - e.g.
    // an AI agent plugin - independent of the tab strip)
    pub right_panel_width: f32,
    /// (plugin_id, panel_id) of the currently open right panel, if any.
    pub right_panel_open: Option<(String, String)>,
    /// last-rendered declarative UI tree for the open right panel.
    pub right_panel_tree: Option<UiNode>,
}

/// forwards any pending `host_log()` lines from plugins into the app's
/// existing console panel, so plugin activity shows up alongside request
/// logs without a separate UI surface.
pub fn drain_plugin_logs(app: &mut Rustrest) {
    app.layout
        .console_logs
        .extend(app.plugins.plugin_manager.drain_logs());
}

/// snapshots the active tab (if it's an HTTP request tab) into the ambient
/// context handed to a `RightPanel` plugin on every render/event.
pub fn build_right_panel_context(app: &Rustrest) -> RightPanelContext {
    let Some(tab_state) = app.tabs.get(app.active_tab_index) else {
        return RightPanelContext::default();
    };
    if !matches!(tab_state.content, WorkspaceContent::HttpRequest) {
        return RightPanelContext::default();
    }
    let tab = &tab_state.tab;

    let active_request = Some(rustrest_plugin_host::RequestContext {
        method: tab.method.to_string(),
        url: tab.url.clone(),
        headers: tab
            .request_headers
            .iter()
            .filter(|h| h.is_active)
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect(),
        body: tab.request_body.text(),
        variables: std::collections::HashMap::new(),
    });

    let active_response = tab.response.as_ref().map(|result| match result {
        Ok(resp) => rustrest_plugin_host::ResponseContext {
            status: resp.status,
            headers: resp.headers.clone(),
            body: resp.body.clone(),
            variables: std::collections::HashMap::new(),
            test_results: Vec::new(),
        },
        Err(err) => rustrest_plugin_host::ResponseContext {
            status: 0,
            headers: std::collections::HashMap::new(),
            body: err.clone(),
            variables: std::collections::HashMap::new(),
            test_results: Vec::new(),
        },
    });

    RightPanelContext {
        active_request,
        active_response,
    }
}

fn parse_http_method(s: &str) -> crate::http_client::HttpMethod {
    use crate::http_client::HttpMethod;
    match s.trim().to_uppercase().as_str() {
        "GET" => HttpMethod::GET,
        "POST" => HttpMethod::POST,
        "PUT" => HttpMethod::PUT,
        "DELETE" => HttpMethod::DELETE,
        "PATCH" => HttpMethod::PATCH,
        "HEAD" => HttpMethod::HEAD,
        "OPTIONS" => HttpMethod::OPTIONS,
        other => HttpMethod::Custom(other.to_string()),
    }
}

/// applies a `RightPanelAction` returned by any of `render_right_panel`/
/// `on_right_panel_event` the same way, regardless of which call produced it.
pub fn handle_right_panel_action(app: &mut Rustrest, action: RightPanelAction) {
    match action {
        RightPanelAction::None => {}
        RightPanelAction::UpdateUi(tree) => {
            app.plugins.right_panel_tree = Some(tree);
        }
        RightPanelAction::ApplyPatch(patch) => {
            apply_request_patch_to_active_tab(app, patch);
        }
        RightPanelAction::UpdateUiAndApplyPatch(tree, patch) => {
            app.plugins.right_panel_tree = Some(tree);
            apply_request_patch_to_active_tab(app, patch);
        }
    }
}

/// applies a `RequestPatch` a `RightPanel` plugin handed back onto the
/// active tab. A no-op if the active tab isn't an HTTP request tab.
fn apply_request_patch_to_active_tab(
    app: &mut Rustrest,
    patch: rustrest_plugin_host::RequestPatch,
) {
    let Some(tab_state) = app.tabs.get_mut(app.active_tab_index) else {
        return;
    };
    if !matches!(tab_state.content, WorkspaceContent::HttpRequest) {
        return;
    }
    let tab = &mut tab_state.tab;

    if let Some(method) = patch.method {
        tab.method = parse_http_method(&method);
    }
    if let Some(url) = patch.url {
        tab.url = url;
    }
    if let Some(headers) = patch.headers {
        tab.request_headers = headers
            .into_iter()
            .map(|(k, v)| crate::ui::tab::types::KeyValuePair::new(&k, &v))
            .collect();
        tab.request_headers_values = crate::ui::tab::contents_for(&tab.request_headers);
    }
    if let Some(body) = patch.body {
        tab.request_body = iced::widget::text_editor::Content::with_text(&body);
    }
    if let Some(script) = patch.pre_request_script {
        tab.pre_request_script = iced::widget::text_editor::Content::with_text(&script);
    }
    if let Some(script) = patch.post_response_script {
        tab.post_response_script = iced::widget::text_editor::Content::with_text(&script);
    }
    tab.dirty = true;
}

pub fn open_manager_pressed(app: &mut Rustrest) -> Task<Message> {
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

pub fn toggle_enabled(app: &mut Rustrest, plugin_id: String, enabled: bool) -> Task<Message> {
    app.plugins.plugin_manager.set_enabled(&plugin_id, enabled);
    Task::none()
}

pub fn install_pressed(app: &mut Rustrest) -> Task<Message> {
    if app.plugins.plugin_manager_busy.is_some() {
        return Task::none();
    }
    // set busy immediately (not just once a file is picked) so the
    // spinner shows right away and the plugin-manager modal's
    // outside-click-to-close is suppressed for the whole file-picker
    // + install duration - opening the native dialog can otherwise
    // cause a spurious "click outside" event once focus returns.
    app.plugins.plugin_manager_busy = Some(PluginManagerAction::Installing);
    iced::Task::perform(
        async move {
            let folder = rfd::AsyncFileDialog::new().pick_folder().await?;
            Some(folder.path().to_path_buf())
        },
        Message::PluginInstallFolderPicked,
    )
}

pub fn install_folder_picked(
    app: &mut Rustrest,
    path: Option<std::path::PathBuf>,
) -> Task<Message> {
    let Some(source) = path else {
        app.plugins.plugin_manager_busy = None;
        return Task::none();
    };
    // the actual wasm compilation (the slow part of an install) runs
    // on a background thread via `prepare_install`, so it doesn't
    // freeze the UI; `PluginInstallPrepared` finishes on the main
    // thread with the already-compiled module.
    let engine = app.plugins.plugin_manager.engine_handle();
    let plugins_dir = app.plugins.plugin_manager.plugins_dir().to_path_buf();
    iced::Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                rustrest_plugin_host::PluginManager::prepare_install(&engine, &plugins_dir, &source)
                    .map_err(|e| e.to_string())
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()))
        },
        Message::PluginInstallPrepared,
    )
}

type PrepareResult = Result<
    (
        String,
        rustrest_plugin_host::PluginManifest,
        rustrest_plugin_host::Module,
    ),
    String,
>;

pub fn install_prepared(app: &mut Rustrest, result: PrepareResult) -> Task<Message> {
    app.plugins.plugin_manager_busy = None;
    let task = match result {
        Ok((dir_name, manifest, module)) => {
            match app
                .plugins
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

pub fn uninstall_pressed(app: &mut Rustrest, plugin_id: String) -> Task<Message> {
    if app.plugins.plugin_manager_busy.is_some() {
        return Task::none();
    }
    app.overlays.confirm_dialog = Some(crate::ui::confirm_dialog::ConfirmDialogState {
        title: "Uninstall Plugin".to_string(),
        message: format!("Uninstall '{plugin_id}'? This removes it from disk and can't be undone."),
        confirm_label: "Uninstall".to_string(),
        on_confirm: Box::new(Message::UninstallPluginConfirmed(plugin_id)),
    });
    Task::none()
}

pub fn uninstall_confirmed(app: &mut Rustrest, plugin_id: String) -> Task<Message> {
    let Some(dir_name) = app.plugins.plugin_manager.dir_name_for(&plugin_id) else {
        return Task::done(Message::ShowToast(
            format!("Plugin '{plugin_id}' not found"),
            ToastStatus::Error,
        ));
    };
    let plugin_dir = app.plugins.plugin_manager.plugins_dir().join(&dir_name);
    app.plugins.plugin_manager_busy = Some(PluginManagerAction::Uninstalling(plugin_id.clone()));
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

pub fn uninstall_finished(
    app: &mut Rustrest,
    plugin_id: String,
    result: Result<(), String>,
) -> Task<Message> {
    app.plugins.plugin_manager_busy = None;
    let task = match result {
        Ok(()) => {
            app.plugins.plugin_manager.drop_plugin(&plugin_id);
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

pub fn show_manager_view(app: &mut Rustrest, view: PluginManagerView) -> Task<Message> {
    app.plugins.plugin_manager_view = view;
    if view == PluginManagerView::Browse && app.plugins.plugin_gallery_entries.is_none() {
        return super::update(app, Message::FetchPluginGallery);
    }
    Task::none()
}

pub fn fetch_gallery(app: &mut Rustrest) -> Task<Message> {
    if app.plugins.plugin_manager_busy.is_some() {
        return Task::none();
    }
    app.plugins.plugin_manager_busy = Some(PluginManagerAction::FetchingGallery);
    iced::Task::perform(
        async {
            tokio::task::spawn_blocking(crate::plugin_gallery::fetch_index)
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
        },
        Message::GalleryIndexFetched,
    )
}

pub fn gallery_index_fetched(
    app: &mut Rustrest,
    result: Result<Vec<GalleryEntry>, String>,
) -> Task<Message> {
    app.plugins.plugin_manager_busy = None;
    let task = if let Err(e) = &result {
        Task::done(Message::ShowToast(
            format!("Failed to fetch plugin gallery: {e}"),
            ToastStatus::Error,
        ))
    } else {
        Task::none()
    };
    app.plugins.plugin_gallery_entries = Some(result);
    task
}

pub fn install_from_gallery_pressed(app: &mut Rustrest, entry: GalleryEntry) -> Task<Message> {
    if app.plugins.plugin_manager_busy.is_some() {
        return Task::none();
    }
    app.plugins.plugin_manager_busy =
        Some(PluginManagerAction::InstallingFromGallery(entry.id.clone()));
    let engine = app.plugins.plugin_manager.engine_handle();
    let plugins_dir = app.plugins.plugin_manager.plugins_dir().to_path_buf();
    iced::Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                crate::plugin_gallery::download_and_prepare(&engine, &plugins_dir, &entry)
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()))
        },
        Message::PluginInstallPrepared,
    )
}

pub fn search_changed(app: &mut Rustrest, query: String) -> Task<Message> {
    app.plugins.plugin_manager_search = query;
    Task::none()
}

pub fn command(app: &mut Rustrest, plugin_id: String, command_id: String) -> Task<Message> {
    let task = match app
        .plugins
        .plugin_manager
        .run_command(&plugin_id, &command_id)
    {
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

pub fn open_panel(app: &mut Rustrest, plugin_id: String, panel_id: String) -> Task<Message> {
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

    match app
        .plugins
        .plugin_manager
        .render_panel(&plugin_id, &panel_id)
    {
        Ok(tree) => {
            let title = app
                .plugins
                .plugin_manager
                .sidebar_panels()
                .into_iter()
                .find(|(pid, panel)| *pid == plugin_id && panel.id == panel_id)
                .map(|(_, panel)| panel.title)
                .unwrap_or_else(|| panel_id.clone());

            app.plugins
                .plugin_panel_state
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

pub fn panel_event(
    app: &mut Rustrest,
    plugin_id: String,
    panel_id: String,
    event: UiEvent,
) -> Task<Message> {
    let task = match app
        .plugins
        .plugin_manager
        .panel_event(&plugin_id, &panel_id, event)
    {
        Ok(Some(tree)) => {
            app.plugins
                .plugin_panel_state
                .insert((plugin_id, panel_id), tree);
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

pub fn process_tick(app: &mut Rustrest) -> Task<Message> {
    let mut touched = app.plugins.plugin_manager.pump_processes();
    touched.extend(app.plugins.plugin_manager.pump_network());
    if !touched.is_empty() {
        let open_panels: Vec<(String, String)> = app
            .tabs
            .iter()
            .filter_map(|t| match &t.content {
                WorkspaceContent::Plugin {
                    plugin_id,
                    panel_id,
                } if touched.contains(plugin_id) => Some((plugin_id.clone(), panel_id.clone())),
                _ => None,
            })
            .collect();
        for (plugin_id, panel_id) in open_panels {
            if let Ok(tree) = app
                .plugins
                .plugin_manager
                .render_panel(&plugin_id, &panel_id)
            {
                app.plugins
                    .plugin_panel_state
                    .insert((plugin_id, panel_id), tree);
            }
        }
        if let Some((plugin_id, panel_id)) = app.plugins.right_panel_open.clone()
            && touched.contains(&plugin_id)
        {
            let ctx = build_right_panel_context(app);
            if let Ok(action) = app
                .plugins
                .plugin_manager
                .render_right_panel(&plugin_id, &panel_id, ctx)
            {
                handle_right_panel_action(app, action);
            }
        }
        drain_plugin_logs(app);
    }
    Task::none()
}

pub fn toggle_right_panel(
    app: &mut Rustrest,
    plugin_id: String,
    panel_id: String,
) -> Task<Message> {
    if app.plugins.right_panel_open.as_ref() == Some(&(plugin_id.clone(), panel_id.clone())) {
        app.plugins.right_panel_open = None;
        app.plugins.right_panel_tree = None;
        return Task::none();
    }
    let ctx = build_right_panel_context(app);
    let task = match app
        .plugins
        .plugin_manager
        .render_right_panel(&plugin_id, &panel_id, ctx)
    {
        Ok(action) => {
            app.plugins.right_panel_open = Some((plugin_id, panel_id));
            handle_right_panel_action(app, action);
            Task::none()
        }
        Err(e) => Task::done(Message::ShowToast(
            format!("Failed to open right panel: {e}"),
            ToastStatus::Error,
        )),
    };
    drain_plugin_logs(app);
    task
}

pub fn right_panel_event(
    app: &mut Rustrest,
    plugin_id: String,
    panel_id: String,
    event: UiEvent,
) -> Task<Message> {
    let ctx = build_right_panel_context(app);
    let task = match app
        .plugins
        .plugin_manager
        .right_panel_event(&plugin_id, &panel_id, ctx, event)
    {
        Ok(action) => {
            handle_right_panel_action(app, action);
            Task::none()
        }
        Err(e) => Task::done(Message::ShowToast(
            format!("Right panel action failed: {e}"),
            ToastStatus::Error,
        )),
    };
    drain_plugin_logs(app);
    task
}
