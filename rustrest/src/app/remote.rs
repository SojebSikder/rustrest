//! Remote development over SSH: connection profiles, live sessions, the
//! inline "add host" form, the per-profile file explorer, opening a remote
//! terminal/file tab, and importing/creating remote-backed collections.

use super::{Rustrest, WorkspaceContent};
use crate::collection::collection::{CollectionInfo, PostmanCollection, RemoteDirRef};
use crate::message::Message;
use crate::ui::confirm_dialog::ConfirmDialogState;
use crate::ui::remote::{
    PendingRemoteConnect, RemoteAuthKind, RemoteExplorerState, RemoteProfileForm, join_remote_path,
};
use crate::ui::tab::Tab;
use crate::ui::toast::toast::ToastStatus;
use crate::{APP_NAME, APP_VERSION};
use iced::Task;
use rustrest_core::remote::{SshAuthMethod, SshProfile};
use rustrest_remote::{AuthMethod, RemoteSession, SshConfig};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct RemoteState {
    pub remote_profiles: Vec<SshProfile>,
    /// connected sessions keyed by `SshProfile::id`.
    pub remote_sessions: HashMap<usize, Arc<RemoteSession>>,
    pub remote_profile_form: RemoteProfileForm,
    pub remote_connect_pending: Option<PendingRemoteConnect>,
    pub remote_explorers: HashMap<usize, RemoteExplorerState>,
    /// the id of the profile a connect attempt is currently in flight for, so
    /// the "Connect" button can show a spinner once the password prompt closes.
    pub remote_connecting: Option<usize>,
    /// whether the "Remote development over SSH" configuration modal is open
    pub remote_config_open: bool,
}

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
pub fn auto_connect_remote_collections(app: &Rustrest) -> Task<Message> {
    let mut seen = std::collections::HashSet::new();
    let mut tasks = Vec::new();

    for col in &app.collections {
        let Some(remote) = &col.remote_dir else {
            continue;
        };
        if app.remote.remote_sessions.contains_key(&remote.profile_id) {
            continue;
        }
        if !seen.insert(remote.profile_id) {
            continue;
        }
        let Some(profile) = app
            .remote
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

pub fn profile_name_changed(app: &mut Rustrest, name: String) -> Task<Message> {
    app.remote.remote_profile_form.name = name;
    Task::none()
}

pub fn profile_host_changed(app: &mut Rustrest, host: String) -> Task<Message> {
    app.remote.remote_profile_form.host = host;
    Task::none()
}

pub fn profile_port_changed(app: &mut Rustrest, port: String) -> Task<Message> {
    app.remote.remote_profile_form.port = port;
    Task::none()
}

pub fn profile_username_changed(app: &mut Rustrest, username: String) -> Task<Message> {
    app.remote.remote_profile_form.username = username;
    Task::none()
}

pub fn profile_auth_kind_changed(app: &mut Rustrest, kind: RemoteAuthKind) -> Task<Message> {
    app.remote.remote_profile_form.auth_kind = kind;
    Task::none()
}

pub fn profile_key_path_changed(app: &mut Rustrest, path: String) -> Task<Message> {
    app.remote.remote_profile_form.key_path = path;
    Task::none()
}

pub fn add_profile_pressed(app: &mut Rustrest) -> Task<Message> {
    let form = app.remote.remote_profile_form.clone();
    if form.name.trim().is_empty() || form.host.trim().is_empty() || form.username.trim().is_empty()
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
        .remote
        .remote_profiles
        .iter()
        .map(|p| p.id)
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    app.remote.remote_profiles.push(SshProfile {
        id,
        name: form.name.trim().to_string(),
        host: form.host.trim().to_string(),
        port,
        username: form.username.trim().to_string(),
        auth_method,
    });
    app.remote.remote_profile_form = RemoteProfileForm::default();
    app.commit_active_workspace_snapshot();
    crate::workspace::save(&app.build_workspace_manifest());
    Task::none()
}

pub fn delete_profile_pressed(app: &mut Rustrest, profile_id: usize) -> Task<Message> {
    app.remote.remote_profiles.retain(|p| p.id != profile_id);
    app.remote.remote_sessions.remove(&profile_id);
    app.remote.remote_explorers.remove(&profile_id);
    app.commit_active_workspace_snapshot();
    crate::workspace::save(&app.build_workspace_manifest());
    Task::none()
}

pub fn connect_pressed(app: &mut Rustrest, profile_id: usize) -> Task<Message> {
    app.remote.remote_connect_pending = Some(PendingRemoteConnect {
        profile_id,
        secret: String::new(),
    });
    Task::none()
}

pub fn connect_secret_changed(app: &mut Rustrest, secret: String) -> Task<Message> {
    if let Some(pending) = &mut app.remote.remote_connect_pending {
        pending.secret = secret;
    }
    Task::none()
}

pub fn connect_cancelled(app: &mut Rustrest) -> Task<Message> {
    app.remote.remote_connect_pending = None;
    Task::none()
}

pub fn connect_confirmed(app: &mut Rustrest) -> Task<Message> {
    let Some(pending) = app.remote.remote_connect_pending.take() else {
        return Task::none();
    };
    let Some(profile) = app
        .remote
        .remote_profiles
        .iter()
        .find(|p| p.id == pending.profile_id)
        .cloned()
    else {
        return Task::none();
    };
    app.remote.remote_connecting = Some(profile.id);
    spawn_remote_connect(&profile, pending.secret)
}

pub fn connected_ok(
    app: &mut Rustrest,
    profile_id: usize,
    session: Arc<RemoteSession>,
) -> Task<Message> {
    if app.remote.remote_connecting == Some(profile_id) {
        app.remote.remote_connecting = None;
    }
    app.remote
        .remote_sessions
        .insert(profile_id, session.clone());
    let name = app
        .remote
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
                rustrest_remote::collection_sync::load_collection_from_remote_dir(&session, &root)
                    .await
                    .map_err(|e| e.to_string())
            },
            move |result| Message::RemoteCollectionLoaded(col_id, result.map(Box::new)),
        ));
    }
    Task::batch(tasks)
}

pub fn connected_err(app: &mut Rustrest, profile_id: usize, err: String) -> Task<Message> {
    if app.remote.remote_connecting == Some(profile_id) {
        app.remote.remote_connecting = None;
    }
    Task::done(Message::ShowToast(
        format!("Connect failed: {err}"),
        ToastStatus::Error,
    ))
}

pub fn disconnect_pressed(app: &mut Rustrest, profile_id: usize) -> Task<Message> {
    app.remote.remote_sessions.remove(&profile_id);
    app.remote.remote_explorers.remove(&profile_id);
    Task::none()
}

pub fn open_terminal_pressed(app: &mut Rustrest, profile_id: usize) -> Task<Message> {
    let Some(session) = app.remote.remote_sessions.get(&profile_id).cloned() else {
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

pub fn shell_ready_ok(
    app: &mut Rustrest,
    profile_id: usize,
    shell_holder: Arc<std::sync::Mutex<Option<rustrest_remote::ShellChannel>>>,
) -> Task<Message> {
    let Some(shell) = shell_holder.lock().unwrap().take() else {
        return Task::none();
    };
    let tx = app.terminal.terminal_event_tx.clone();
    let (terminal_id, feed, commands) =
        app.terminal
            .terminal_manager
            .spawn_remote(80, 24, move |id, notice| {
                let _ = tx.send((id, notice));
            });
    rustrest_remote::bridge_shell_to_terminal(shell, feed, commands);

    let profile_name = app
        .remote
        .remote_profiles
        .iter()
        .find(|p| p.id == profile_id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "remote".to_string());

    let widget_id = iced::widget::Id::unique();
    let mut term_tab = Tab::new(app.next_tab_id);
    term_tab.name = format!("{profile_name} (remote)");
    app.tabs.push(super::TabState {
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

pub fn shell_ready_err(err: String) -> Task<Message> {
    Task::done(Message::ShowToast(
        format!("Failed to open remote terminal: {err}"),
        ToastStatus::Error,
    ))
}

pub fn explorer_toggled(app: &mut Rustrest, profile_id: usize) -> Task<Message> {
    let explorer = app.remote.remote_explorers.entry(profile_id).or_default();
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

pub fn explorer_path_changed(app: &mut Rustrest, profile_id: usize, path: String) -> Task<Message> {
    let explorer = app.remote.remote_explorers.entry(profile_id).or_default();
    explorer.path = path;
    Task::none()
}

pub fn explorer_go_pressed(app: &mut Rustrest, profile_id: usize) -> Task<Message> {
    let Some(session) = app.remote.remote_sessions.get(&profile_id).cloned() else {
        return Task::none();
    };
    let explorer = app.remote.remote_explorers.entry(profile_id).or_default();
    explorer.loading = true;
    explorer.error = None;
    let path = explorer.path.clone();
    let path_for_result = path.clone();

    Task::perform(
        async move { session.list_dir(&path).await.map_err(|e| e.to_string()) },
        move |result| Message::RemoteDirListingLoaded(profile_id, path_for_result.clone(), result),
    )
}

pub fn dir_listing_loaded(
    app: &mut Rustrest,
    profile_id: usize,
    path: String,
    result: Result<Vec<rustrest_remote::RemoteEntry>, String>,
) -> Task<Message> {
    let explorer = app.remote.remote_explorers.entry(profile_id).or_default();
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

pub fn entry_clicked(app: &mut Rustrest, profile_id: usize, path: String) -> Task<Message> {
    let Some(explorer) = app.remote.remote_explorers.get(&profile_id) else {
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
        let Some(session) = app.remote.remote_sessions.get(&profile_id).cloned() else {
            return Task::none();
        };
        app.remote
            .remote_explorers
            .entry(profile_id)
            .or_default()
            .loading = true;
        let path_for_result = path.clone();
        Task::perform(
            async move { session.read_file(&path).await.map_err(|e| e.to_string()) },
            move |result| Message::RemoteFileLoaded(profile_id, path_for_result.clone(), result),
        )
    }
}

pub fn file_loaded(
    app: &mut Rustrest,
    profile_id: usize,
    path: String,
    result: Result<Vec<u8>, String>,
) -> Task<Message> {
    if let Some(explorer) = app.remote.remote_explorers.get_mut(&profile_id) {
        explorer.loading = false;
    }
    match result {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let mut file_tab = Tab::new(app.next_tab_id);
            file_tab.name = path.clone();
            app.tabs.push(super::TabState {
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

pub fn import_dir_as_collection_pressed(
    app: &mut Rustrest,
    profile_id: usize,
    path: String,
) -> Task<Message> {
    let Some(session) = app.remote.remote_sessions.get(&profile_id).cloned() else {
        return Task::none();
    };
    app.remote
        .remote_explorers
        .entry(profile_id)
        .or_default()
        .loading = true;
    let path_for_result = path.clone();
    Task::perform(
        async move {
            rustrest_remote::collection_sync::load_collection_from_remote_dir(&session, &path)
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

pub fn collection_imported_ok(
    app: &mut Rustrest,
    profile_id: usize,
    root: String,
    mut collection: Box<PostmanCollection>,
) -> Task<Message> {
    if let Some(explorer) = app.remote.remote_explorers.get_mut(&profile_id) {
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
            on_confirm: Box::new(Message::ReplaceCollectionConfirmed(existing_id, collection)),
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

pub fn collection_imported_err(
    app: &mut Rustrest,
    profile_id: usize,
    err: String,
) -> Task<Message> {
    if let Some(explorer) = app.remote.remote_explorers.get_mut(&profile_id) {
        explorer.loading = false;
    }
    Task::done(Message::ShowToast(
        format!("Failed to import remote collection: {err}"),
        ToastStatus::Error,
    ))
}

pub fn collection_loaded_ok(
    app: &mut Rustrest,
    collection_id: usize,
    mut collection: Box<PostmanCollection>,
) -> Task<Message> {
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

pub fn collection_loaded_err(err: String) -> Task<Message> {
    Task::done(Message::ShowToast(
        format!("Failed to load remote collection: {err}"),
        ToastStatus::Error,
    ))
}

pub fn new_collection_name_changed(
    app: &mut Rustrest,
    profile_id: usize,
    name: String,
) -> Task<Message> {
    if let Some(explorer) = app.remote.remote_explorers.get_mut(&profile_id) {
        explorer.new_collection_name = name;
    }
    Task::none()
}

pub fn new_collection_pressed(app: &mut Rustrest, profile_id: usize) -> Task<Message> {
    let Some(explorer) = app.remote.remote_explorers.get(&profile_id) else {
        return Task::none();
    };
    let name = explorer.new_collection_name.trim().to_string();
    if name.is_empty() {
        return Task::done(Message::ShowToast(
            "Enter a name for the new collection".to_string(),
            ToastStatus::Error,
        ));
    }
    let Some(session) = app.remote.remote_sessions.get(&profile_id).cloned() else {
        return Task::none();
    };
    let root = join_remote_path(&explorer.path, &name);
    app.remote
        .remote_explorers
        .entry(profile_id)
        .or_default()
        .loading = true;
    let collection = PostmanCollection {
        id: 0,
        file_path: None,
        storage_dir: None,
        remote_dir: None,
        unsaved: false,
        auth: None,
        event: None,
        info: CollectionInfo {
            name: name.clone(),
            postman_id: None,
            schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                .to_string(),
            description: None,
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

pub fn file_content_changed(
    app: &mut Rustrest,
    tab_id: usize,
    action: iced::widget::text_editor::Action,
) -> Task<Message> {
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

pub fn file_save_pressed(app: &mut Rustrest, tab_id: usize) -> Task<Message> {
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
    let Some(session) = app.remote.remote_sessions.get(profile_id).cloned() else {
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

pub fn file_saved(app: &mut Rustrest, tab_id: usize, result: Result<(), String>) -> Task<Message> {
    match result {
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
    }
}

pub fn open_config(app: &mut Rustrest) -> Task<Message> {
    app.remote.remote_config_open = true;
    Task::none()
}

pub fn close_config_pressed(app: &mut Rustrest) -> Task<Message> {
    app.remote.remote_config_open = false;
    Task::none()
}
