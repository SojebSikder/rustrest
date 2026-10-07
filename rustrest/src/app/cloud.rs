//! Rustrest Cloud: sign-in, uploading/opening team collections, and keeping them in sync.

use super::Rustrest;
use crate::message::Message;
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use rustrest_cloud::sync::{PushOutcome, PushPlan, SyncState};
use rustrest_cloud::wire::{ChangeSet, CloudCollection, CloudEnvironment, Role, Team, TeamMember};
use rustrest_cloud::{CloudClient, CloudError, Resolution, Session, SyncReport};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const DEFAULT_SERVER_URL: &str = "http://localhost:4000";
const STATUS_ID: &str = "cloud-sync";
/// a batch rejected over conflicts shrinks each round, stop after this many
const MAX_PUSH_ROUNDS: usize = 4;

/// sign-in details kept across restarts
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SavedAccount {
    server_url: String,
    email: String,
    session: Session,
    #[serde(default)]
    last_team: Option<String>,
}

fn account_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(crate::APP_NAME).join("cloud.json"))
}

/// where cloud collections are cached locally
fn cache_root() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(crate::APP_NAME).join("cloud"))
}

fn cache_dir(collection_id: &str) -> Option<PathBuf> {
    Some(cache_root()?.join(collection_id))
}

#[derive(Debug, Clone, Default)]
pub struct CloudModal {
    pub server_url: String,
    pub name: String,
    pub email: String,
    pub password: String,
    pub sign_up: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub teams: Vec<Team>,
    pub selected_team: Option<String>,
    pub collections: Vec<CloudCollection>,
    pub new_team_name: String,
    pub invite_email: String,
    pub invite_role: Role,
    /// members of the selected team
    pub members: Vec<TeamMember>,
    /// user id whose removal is waiting for a second click
    pub confirm_remove_member: Option<String>,
    /// (cloud collection id, destination team id) waiting for confirmation
    pub move_target: Option<(String, String)>,
    /// new name while the selected team is being renamed
    pub rename_team: Option<String>,
    /// the selected team's deletion is waiting for a second click
    pub confirm_delete_team: bool,
    /// set when opened from a collection's "Upload to Cloud..." entry
    pub upload_collection: Option<usize>,
    /// environments of the selected team
    pub environments: Vec<CloudEnvironment>,
    /// index of the environment being uploaded, set when opened from the environment editor
    pub upload_environment: Option<usize>,
    /// cloud environment id whose deletion is waiting for a second click
    pub confirm_delete_environment: Option<String>,
}

/// one in-flight sync of one collection
struct SyncRun {
    report: SyncReport,
    skip: HashSet<String>,
    rounds: usize,
    /// pull after pushing, it doesn't push again
    final_pull: bool,
}

#[derive(Default)]
pub struct CloudState {
    pub client: Option<CloudClient>,
    pub email: Option<String>,
    pub modal: Option<CloudModal>,
    /// team picked last in the modal, preselected when it opens again
    pub last_team: Option<String>,
    /// app collection id -> (cache dir, sync state)
    pub linked: HashMap<usize, (PathBuf, SyncState)>,
    runs: HashMap<usize, SyncRun>,
    /// syncs requested while one was already running
    resync: HashSet<usize>,
    /// collections with cloud changes waiting for their unsaved edits to be saved
    pub waiting: HashSet<usize>,
    /// teams whose environments are syncing right now
    pub env_runs: HashSet<String>,
    /// teams to sync environments of again once their current run ends
    pub env_resync: HashSet<String>,
}

impl CloudState {
    pub fn load() -> Self {
        let account = account_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str::<SavedAccount>(&text).ok());
        match account {
            Some(account) => Self {
                client: Some(CloudClient::new(&account.server_url, Some(account.session))),
                email: Some(account.email),
                last_team: account.last_team,
                ..Self::default()
            },
            None => Self::default(),
        }
    }

    pub fn conflict_count(&self, col_id: usize) -> usize {
        self.linked.get(&col_id).map_or(0, |(_, s)| {
            s.conflicts.len() + usize::from(s.meta_conflict.is_some())
        })
    }

    /// (cloud collection id, app collection id) pairs to listen to
    pub fn realtime_targets(&self) -> Vec<(String, usize)> {
        let mut targets: Vec<_> = self
            .linked
            .iter()
            .map(|(col_id, (_, s))| (s.collection_id.clone(), *col_id))
            .collect();
        targets.sort();
        targets
    }

    /// persists the current tokens
    pub(super) fn save_account(&self) {
        let (Some(client), Some(email), Some(path)) = (&self.client, &self.email, account_path())
        else {
            return;
        };

        let Some(session) = client.session() else {
            // refresh token was rejected, we're signed out
            let _ = std::fs::remove_file(path);
            return;
        };
        let account = SavedAccount {
            server_url: client.base_url().to_string(),
            email: email.clone(),
            session,
            last_team: self.last_team.clone(),
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&account) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// links every open directory collection that carries a cloud sync state,
/// and forgets links to collections that were closed or moved
pub fn refresh_links(app: &mut Rustrest) {
    let open: HashMap<usize, &Path> = app
        .collections
        .iter()
        .filter_map(|c| Some((c.id, c.storage_dir.as_deref()?)))
        .collect();

    app.cloud
        .linked
        .retain(|col_id, (dir, _)| open.get(col_id) == Some(&dir.as_path()));

    for (col_id, dir) in open {
        if let std::collections::hash_map::Entry::Vacant(slot) = app.cloud.linked.entry(col_id)
            && let Some(state) = SyncState::load(dir)
        {
            slot.insert((dir.to_path_buf(), state));
        }
    }
}

// ---- modal ----

pub fn open_modal(app: &mut Rustrest, upload_collection: Option<usize>) -> Task<Message> {
    let server_url = app
        .cloud
        .client
        .as_ref()
        .map(|c| c.base_url().to_string())
        .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string());
    app.cloud.modal = Some(CloudModal {
        server_url,
        email: app.cloud.email.clone().unwrap_or_default(),
        selected_team: app.cloud.last_team.clone(),
        upload_collection,
        ..CloudModal::default()
    });
    if app.cloud.client.is_some() {
        return load_teams(app);
    }
    Task::none()
}

pub fn close_modal(app: &mut Rustrest) -> Task<Message> {
    app.cloud.modal = None;
    Task::none()
}

pub fn edit_modal(app: &mut Rustrest, edit: impl FnOnce(&mut CloudModal)) -> Task<Message> {
    if let Some(modal) = app.cloud.modal.as_mut() {
        edit(modal);
    }
    Task::none()
}

pub(super) fn modal_busy(app: &mut Rustrest, busy: bool, error: Option<String>) {
    if let Some(modal) = app.cloud.modal.as_mut() {
        modal.busy = busy;
        modal.error = error;
    }
}

pub fn submit_auth(app: &mut Rustrest) -> Task<Message> {
    let Some(modal) = app.cloud.modal.as_mut() else {
        return Task::none();
    };
    let server_url = modal.server_url.trim().to_string();
    let (name, email, password, sign_up) = (
        modal.name.trim().to_string(),
        modal.email.trim().to_string(),
        modal.password.clone(),
        modal.sign_up,
    );
    if server_url.is_empty() || email.is_empty() || password.is_empty() {
        modal.error = Some("Server, email and password are required".to_string());
        return Task::none();
    }
    modal.busy = true;
    modal.error = None;

    let client = CloudClient::new(&server_url, None);
    Task::perform(
        async move {
            if sign_up {
                client.register(&name, &email, &password).await?;
            }
            client.login(&email, &password).await?;
            Ok((client, email))
        },
        |result: Result<(CloudClient, String), CloudError>| {
            Message::CloudSignedIn(result.map(Box::new).map_err(|e| e.to_string()))
        },
    )
}

pub fn signed_in(
    app: &mut Rustrest,
    result: Result<Box<(CloudClient, String)>, String>,
) -> Task<Message> {
    match result {
        Ok(signed_in) => {
            let (client, email) = *signed_in;
            app.cloud.client = Some(client);
            app.cloud.email = Some(email);
            app.cloud.save_account();
            if let Some(modal) = app.cloud.modal.as_mut() {
                modal.password.clear();
            }
            modal_busy(app, false, None);
            Task::batch([load_teams(app), sync_all(app)])
        }
        Err(err) => {
            modal_busy(app, false, Some(err));
            Task::none()
        }
    }
}

pub fn sign_out(app: &mut Rustrest) -> Task<Message> {
    let client = app.cloud.client.take();
    app.cloud.email = None;
    app.cloud.last_team = None;
    if let Some(path) = account_path() {
        let _ = std::fs::remove_file(path);
    }
    if let Some(modal) = app.cloud.modal.as_mut() {
        modal.teams.clear();
        modal.collections.clear();
        modal.environments.clear();
        modal.selected_team = None;
    }
    app.status_bar.clear(STATUS_ID);
    // cloud collections stay open as local copies, they resume syncing on the next signin
    match client {
        Some(client) => Task::perform(async move { client.logout().await }, |_| Message::None),
        None => Task::none(),
    }
}

fn load_teams(app: &mut Rustrest) -> Task<Message> {
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };
    modal_busy(app, true, None);
    Task::perform(async move { client.teams().await }, |result| {
        Message::CloudTeamsLoaded(result.map_err(|e| e.to_string()))
    })
}

pub fn teams_loaded(app: &mut Rustrest, result: Result<Vec<Team>, String>) -> Task<Message> {
    app.cloud.save_account();
    let teams = match result {
        Ok(teams) => teams,
        Err(err) => {
            modal_busy(app, false, Some(err));
            return Task::none();
        }
    };
    let Some(modal) = app.cloud.modal.as_mut() else {
        return Task::none();
    };
    modal.busy = false;
    let selected = modal
        .selected_team
        .clone()
        .filter(|id| teams.iter().any(|t| &t.id == id))
        .or_else(|| teams.first().map(|t| t.id.clone()));
    modal.teams = teams;
    match selected {
        Some(team_id) => select_team(app, team_id),
        None => Task::none(),
    }
}

pub fn select_team(app: &mut Rustrest, team_id: String) -> Task<Message> {
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };
    if app.cloud.last_team.as_ref() != Some(&team_id) {
        app.cloud.last_team = Some(team_id.clone());
        app.cloud.save_account();
    }
    let Some(modal) = app.cloud.modal.as_mut() else {
        return Task::none();
    };
    modal.selected_team = Some(team_id.clone());
    modal.collections.clear();
    modal.environments.clear();
    modal.confirm_delete_environment = None;
    modal.members.clear();
    modal.confirm_remove_member = None;
    modal.move_target = None;
    modal.rename_team = None;
    modal.confirm_delete_team = false;
    modal.busy = true;

    let personal = modal.teams.iter().any(|t| t.id == team_id && t.is_personal);

    let collections = {
        let (client, team_id) = (client.clone(), team_id.clone());
        Task::perform(
            async move { client.collections(&team_id).await },
            |result| Message::CloudCollectionsLoaded(result.map_err(|e| e.to_string())),
        )
    };
    let environments = super::cloud_env::load_environments(client.clone(), team_id.clone());
    if personal {
        return Task::batch([collections, environments]);
    }
    Task::batch([collections, environments, load_members(client, team_id)])
}

fn load_members(client: CloudClient, team_id: String) -> Task<Message> {
    Task::perform(
        async move {
            let members = client.members(&team_id).await.map_err(|e| e.to_string());
            (team_id, members)
        },
        |(team_id, result)| Message::CloudMembersLoaded(team_id, result),
    )
}

pub fn members_loaded(
    app: &mut Rustrest,
    team_id: String,
    result: Result<Vec<TeamMember>, String>,
) -> Task<Message> {
    let Some(modal) = app.cloud.modal.as_mut() else {
        return Task::none();
    };
    // a reply for a team that's no longer selected
    if modal.selected_team.as_ref() != Some(&team_id) {
        return Task::none();
    }
    match result {
        Ok(members) => modal.members = members,
        Err(err) => modal.error = Some(err),
    }
    Task::none()
}

pub fn collections_loaded(
    app: &mut Rustrest,
    result: Result<Vec<CloudCollection>, String>,
) -> Task<Message> {
    match result {
        Ok(collections) => {
            if let Some(modal) = app.cloud.modal.as_mut() {
                modal.collections = collections;
            }
            modal_busy(app, false, None);
        }
        Err(err) => modal_busy(app, false, Some(err)),
    }
    Task::none()
}

pub fn create_team(app: &mut Rustrest) -> Task<Message> {
    let (Some(client), Some(modal)) = (app.cloud.client.clone(), app.cloud.modal.as_mut()) else {
        return Task::none();
    };
    let name = modal.new_team_name.trim().to_string();
    if name.is_empty() {
        return Task::none();
    }
    modal.busy = true;
    Task::perform(async move { client.create_team(&name).await }, |result| {
        Message::CloudTeamCreated(result.map_err(|e| e.to_string()))
    })
}

pub fn team_created(app: &mut Rustrest, result: Result<Team, String>) -> Task<Message> {
    match result {
        Ok(team) => {
            if let Some(modal) = app.cloud.modal.as_mut() {
                modal.new_team_name.clear();
                modal.selected_team = Some(team.id.clone());
            }
            load_teams(app)
        }
        Err(err) => {
            modal_busy(app, false, Some(err));
            Task::none()
        }
    }
}

pub fn start_rename_team(app: &mut Rustrest) -> Task<Message> {
    if let Some(modal) = app.cloud.modal.as_mut() {
        modal.rename_team = modal
            .teams
            .iter()
            .find(|t| Some(&t.id) == modal.selected_team.as_ref())
            .map(|t| t.name.clone());
        modal.confirm_delete_team = false;
    }
    Task::none()
}

pub fn rename_team(app: &mut Rustrest) -> Task<Message> {
    let (Some(client), Some(modal)) = (app.cloud.client.clone(), app.cloud.modal.as_mut()) else {
        return Task::none();
    };
    let (Some(team_id), Some(name)) = (
        modal.selected_team.clone(),
        modal
            .rename_team
            .as_deref()
            .map(str::trim)
            .map(str::to_string),
    ) else {
        return Task::none();
    };
    if name.is_empty() {
        return Task::none();
    }
    modal.busy = true;
    modal.error = None;
    Task::perform(
        async move {
            client
                .rename_team(&team_id, &name)
                .await
                .map(|team| format!("Renamed the team to '{}'", team.name))
        },
        |result| Message::CloudTeamUpdated(result.map_err(|e| e.to_string())),
    )
}

pub fn delete_team(app: &mut Rustrest) -> Task<Message> {
    let (Some(client), Some(modal)) = (app.cloud.client.clone(), app.cloud.modal.as_mut()) else {
        return Task::none();
    };
    let Some(team) = modal
        .teams
        .iter()
        .find(|t| Some(&t.id) == modal.selected_team.as_ref())
        .cloned()
    else {
        return Task::none();
    };

    // first click only asks for confirmation
    if !modal.confirm_delete_team {
        modal.confirm_delete_team = true;
        modal.rename_team = None;
        return Task::none();
    }
    modal.confirm_delete_team = false;
    modal.selected_team = None;
    modal.busy = true;
    modal.error = None;
    Task::perform(
        async move {
            client
                .delete_team(&team.id)
                .await
                .map(|_| format!("Deleted team '{}'", team.name))
        },
        |result| Message::CloudTeamUpdated(result.map_err(|e| e.to_string())),
    )
}

pub fn team_updated(app: &mut Rustrest, result: Result<String, String>) -> Task<Message> {
    match result {
        Ok(toast) => {
            if let Some(modal) = app.cloud.modal.as_mut() {
                modal.rename_team = None;
            }
            modal_busy(app, false, None);
            // a deleted team's collections 404 on sync, which unlinks them
            Task::batch([
                load_teams(app),
                sync_all(app),
                Task::done(Message::ShowToast(toast, ToastStatus::Success)),
            ])
        }
        Err(err) => {
            modal_busy(app, false, Some(err));
            Task::none()
        }
    }
}

pub fn invite(app: &mut Rustrest) -> Task<Message> {
    let (Some(client), Some(modal)) = (app.cloud.client.clone(), app.cloud.modal.as_mut()) else {
        return Task::none();
    };
    let (Some(team_id), email) = (
        modal.selected_team.clone(),
        modal.invite_email.trim().to_string(),
    ) else {
        return Task::none();
    };
    if email.is_empty() {
        return Task::none();
    }
    let role = modal.invite_role;
    modal.busy = true;
    Task::perform(
        async move {
            client
                .add_member(&team_id, &email, role)
                .await
                .map(|_| format!("Added {email} as {}", role.to_string().to_lowercase()))
        },
        |result| Message::CloudInvited(result.map_err(|e| e.to_string())),
    )
}

pub fn invited(app: &mut Rustrest, result: Result<String, String>) -> Task<Message> {
    if result.is_ok()
        && let Some(modal) = app.cloud.modal.as_mut()
    {
        modal.invite_email.clear();
    }
    members_changed(app, result)
}

pub fn set_member_role(app: &mut Rustrest, user_id: String, role: Role) -> Task<Message> {
    let (Some(client), Some(modal)) = (app.cloud.client.clone(), app.cloud.modal.as_mut()) else {
        return Task::none();
    };
    let Some(team_id) = modal.selected_team.clone() else {
        return Task::none();
    };
    let Some(member) = modal.members.iter().find(|m| m.user_id == user_id) else {
        return Task::none();
    };
    if member.role == role {
        return Task::none();
    }
    let who = member_label(member);
    modal.busy = true;
    modal.error = None;
    Task::perform(
        async move {
            client
                .update_member_role(&team_id, &user_id, role)
                .await
                .map(|_| format!("{who} is now {}", role.to_string().to_lowercase()))
        },
        |result| Message::CloudMembersChanged(result.map_err(|e| e.to_string())),
    )
}

pub fn remove_member(app: &mut Rustrest, user_id: String) -> Task<Message> {
    let (Some(client), Some(modal)) = (app.cloud.client.clone(), app.cloud.modal.as_mut()) else {
        return Task::none();
    };
    let Some(team_id) = modal.selected_team.clone() else {
        return Task::none();
    };

    // first click only asks for confirmation
    if modal.confirm_remove_member.as_ref() != Some(&user_id) {
        modal.confirm_remove_member = Some(user_id);
        return Task::none();
    }
    modal.confirm_remove_member = None;
    let who = modal
        .members
        .iter()
        .find(|m| m.user_id == user_id)
        .map(member_label)
        .unwrap_or_else(|| "Member".to_string());
    modal.busy = true;
    modal.error = None;
    Task::perform(
        async move {
            client
                .remove_member(&team_id, &user_id)
                .await
                .map(|_| format!("Removed {who} from the team"))
        },
        |result| Message::CloudMembersChanged(result.map_err(|e| e.to_string())),
    )
}

/// after an invite, role change or removal, reload teams (our own role or
/// membership may have changed) and with them the members
pub fn members_changed(app: &mut Rustrest, result: Result<String, String>) -> Task<Message> {
    match result {
        Ok(toast) => {
            modal_busy(app, false, None);
            Task::batch([
                load_teams(app),
                Task::done(Message::ShowToast(toast, ToastStatus::Success)),
            ])
        }
        Err(err) => {
            modal_busy(app, false, Some(err));
            Task::none()
        }
    }
}

pub fn member_label(member: &TeamMember) -> String {
    member
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| member.email.clone())
        .unwrap_or_else(|| member.user_id.clone())
}

// ---- move ----

pub fn move_collection(app: &mut Rustrest) -> Task<Message> {
    let (Some(client), Some(modal)) = (app.cloud.client.clone(), app.cloud.modal.as_mut()) else {
        return Task::none();
    };
    let Some((collection_id, team_id)) = modal.move_target.take() else {
        return Task::none();
    };
    modal.busy = true;
    modal.error = None;
    Task::perform(
        async move { client.move_collection(&collection_id, &team_id).await },
        |result| Message::CloudCollectionMoved(result.map_err(|e| e.to_string())),
    )
}

pub fn collection_moved(
    app: &mut Rustrest,
    result: Result<CloudCollection, String>,
) -> Task<Message> {
    let moved = match result {
        Ok(moved) => moved,
        Err(err) => {
            modal_busy(app, false, Some(err));
            return Task::none();
        }
    };

    // keep the local link pointing at the new team
    let linked = app
        .cloud
        .linked
        .iter_mut()
        .find(|(_, (_, s))| s.collection_id == moved.id)
        .map(|(col_id, (_, s))| {
            s.team_id = moved.team_id.clone();
            *col_id
        });
    if let Some(col_id) = linked {
        save_state(app, col_id);
    }

    let team_name = app
        .cloud
        .modal
        .as_ref()
        .and_then(|m| m.teams.iter().find(|t| t.id == moved.team_id))
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "the other team".to_string());
    let selected = app
        .cloud
        .modal
        .as_ref()
        .and_then(|m| m.selected_team.clone());
    let reload = match selected {
        Some(team_id) => select_team(app, team_id),
        None => Task::none(),
    };

    Task::batch([
        reload,
        Task::done(Message::ShowToast(
            format!("Moved '{}' to {team_name}", moved.name),
            ToastStatus::Success,
        )),
    ])
}

// ---- upload / open ----

pub fn upload(app: &mut Rustrest) -> Task<Message> {
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };
    let Some(modal) = app.cloud.modal.as_ref() else {
        return Task::none();
    };
    let (Some(col_id), Some(team_id)) = (modal.upload_collection, modal.selected_team.clone())
    else {
        return Task::none();
    };
    if app.cloud.linked.contains_key(&col_id) {
        modal_busy(
            app,
            false,
            Some("This collection already syncs with the cloud".into()),
        );
        return Task::none();
    }
    let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) else {
        return Task::none();
    };
    if col.remote_dir.is_some() {
        modal_busy(
            app,
            false,
            Some("Collections on an SSH host can't be uploaded; open a local copy first".into()),
        );
        return Task::none();
    }
    // uids go on the live tree first, so whatever is edited during the
    // upload still lines up with what was sent
    rustrest_cloud::convert::ensure_uids(col);
    let mut snapshot = col.clone();
    modal_busy(app, true, None);

    Task::perform(
        async move {
            let state = rustrest_cloud::sync::upload(&client, &team_id, &mut snapshot).await?;
            Ok(state)
        },
        move |result: Result<SyncState, CloudError>| {
            Message::CloudUploaded(col_id, result.map(Box::new).map_err(|e| e.to_string()))
        },
    )
}

pub fn uploaded(
    app: &mut Rustrest,
    col_id: usize,
    result: Result<Box<SyncState>, String>,
) -> Task<Message> {
    let state = match result {
        Ok(state) => *state,
        Err(err) => {
            modal_busy(app, false, Some(err));
            return Task::none();
        }
    };
    let Some(dir) = cache_dir(&state.collection_id) else {
        return Task::none();
    };
    let Some(col) = app.collections.iter_mut().find(|c| c.id == col_id) else {
        return Task::none();
    };

    // from now on the collection lives in the cloud cache, whatever file or
    // folder it came from is left as it was
    col.storage_dir = Some(dir.clone());
    col.file_path = None;
    // the whole tree was just published and is about to be written out
    col.clear_unsaved();
    let name = col.info.name.clone();
    if let Err(err) = write_local(app, col_id, &dir, &state) {
        modal_busy(app, false, Some(err));
        return Task::none();
    }
    app.cloud.linked.insert(col_id, (dir, state));
    app.cloud.modal = None;
    Task::batch([
        Task::done(Message::ShowToast(
            format!("'{name}' is now synced with Rustrest Cloud"),
            ToastStatus::Success,
        )),
        Task::done(Message::CloudSync(col_id)),
    ])
}

pub fn open_collection(app: &mut Rustrest, collection_id: String) -> Task<Message> {
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };
    if let Some((col_id, _)) = app
        .cloud
        .linked
        .iter()
        .find(|(_, (_, s))| s.collection_id == collection_id)
    {
        let col_id = *col_id;
        app.cloud.modal = None;
        app.sidebar.collapsed_collections.remove(&col_id);
        return Task::done(Message::ShowToast(
            "That collection is already open".to_string(),
            ToastStatus::Info,
        ));
    }
    modal_busy(app, true, None);
    Task::perform(
        async move {
            let mut next_id = 0;
            rustrest_cloud::sync::download(&client, &collection_id, &mut next_id).await
        },
        |result| {
            Message::CloudDownloaded(
                result
                    .map(|(col, state)| Box::new((col, state)))
                    .map_err(|e| e.to_string()),
            )
        },
    )
}

pub fn downloaded(
    app: &mut Rustrest,
    result: Result<Box<(crate::collection::collection::PostmanCollection, SyncState)>, String>,
) -> Task<Message> {
    let (mut collection, state) = match result {
        Ok(pair) => *pair,
        Err(err) => {
            modal_busy(app, false, Some(err));
            return Task::none();
        }
    };
    let Some(dir) = cache_dir(&state.collection_id) else {
        return Task::none();
    };
    collection.id = app.next_tab_id;
    app.next_tab_id += 1;
    collection.storage_dir = Some(dir.clone());
    collection.assign_request_ids(&mut app.next_request_id);

    let col_id = collection.id;
    let name = collection.info.name.clone();
    app.collections.push(collection);
    super::sidebar::collapse_collection_tree(app, col_id);

    if let Err(err) = write_local(app, col_id, &dir, &state) {
        app.collections.retain(|c| c.id != col_id);
        modal_busy(app, false, Some(err));
        return Task::none();
    }
    app.cloud.linked.insert(col_id, (dir, state));
    app.cloud.modal = None;
    Task::done(Message::ShowToast(
        format!("Opened '{name}' from Rustrest Cloud"),
        ToastStatus::Success,
    ))
}

/// writes the collection tree and its sync state to the cache dir, telling
/// the file watcher the write is ours
fn write_local(
    app: &mut Rustrest,
    col_id: usize,
    dir: &Path,
    state: &SyncState,
) -> Result<(), String> {
    let col = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .ok_or("collection closed")?;
    crate::collection::dir_storage::save_collection_to_dir_clean(col, dir)?;
    state.save(dir)?;
    super::file_watch::record_current_as_baseline(app, col_id);
    Ok(())
}

fn save_state(app: &Rustrest, col_id: usize) {
    if let Some((dir, state)) = app.cloud.linked.get(&col_id)
        && let Err(err) = state.save(dir)
    {
        eprintln!("cloud: {err}");
    }
}

// ---- sync ----

/// syncs every linked collection and environment (after sign-in, and
/// periodically as a fallback for missed realtime events)
pub fn sync_all(app: &mut Rustrest) -> Task<Message> {
    refresh_links(app);
    let ids: Vec<usize> = app.cloud.linked.keys().copied().collect();
    let environments = super::cloud_env::sync_all(app);

    Task::batch(
        ids.into_iter()
            .map(|id| Task::done(Message::CloudSync(id)))
            .chain([environments]),
    )
}

pub fn sync(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    refresh_links(app);
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };
    let Some((_, state)) = app.cloud.linked.get(&col_id) else {
        return Task::none();
    };
    if app.cloud.runs.contains_key(&col_id) {
        app.cloud.resync.insert(col_id);
        return Task::none();
    }
    let Some(col) = app.collections.iter().find(|c| c.id == col_id) else {
        return Task::none();
    };
    // never publish unsaved work. wait for the save, which syncs again
    if crate::ui::unsaved::collection_is_unsaved(app, col) {
        app.cloud.waiting.insert(col_id);
        update_status(app);
        return Task::none();
    }
    app.cloud.waiting.remove(&col_id);

    app.cloud.runs.insert(
        col_id,
        SyncRun {
            report: SyncReport::default(),
            skip: HashSet::new(),
            rounds: 0,
            final_pull: false,
        },
    );
    let task = fetch_changes(client, col_id, state);
    update_status(app);
    task
}

fn fetch_changes(client: CloudClient, col_id: usize, state: &SyncState) -> Task<Message> {
    let (collection_id, since) = (state.collection_id.clone(), state.seq);
    Task::perform(
        async move { client.changes(&collection_id, since).await },
        move |result| Message::CloudPulled(col_id, result.map(Box::new)),
    )
}

pub fn pulled(
    app: &mut Rustrest,
    col_id: usize,
    result: Result<Box<ChangeSet>, CloudError>,
) -> Task<Message> {
    let changes = match result {
        Ok(changes) => changes,
        Err(err) => return finish(app, col_id, Some(err)),
    };
    let Some(col_pos) = app.collections.iter().position(|c| c.id == col_id) else {
        return finish(app, col_id, None);
    };
    // edits started while the request was out: drop this round (nothing
    // was merged yet, so nothing is lost) and sync after the save
    if crate::ui::unsaved::collection_is_unsaved(app, &app.collections[col_pos]) {
        app.cloud.waiting.insert(col_id);
        return finish(app, col_id, None);
    }
    let Some((dir, state)) = app.cloud.linked.get_mut(&col_id) else {
        return finish(app, col_id, None);
    };
    let dir = dir.clone();

    let col = &mut app.collections[col_pos];
    let uids_added = rustrest_cloud::convert::ensure_uids(col);
    let (pulled, meta_pulled) = state.apply_changes(col, &changes, &mut app.next_request_id);
    if let Some(run) = app.cloud.runs.get_mut(&col_id) {
        run.report.pulled += pulled;
        run.report.meta_pulled |= meta_pulled;
    }

    if pulled > 0 || meta_pulled || uids_added {
        let state = app.cloud.linked[&col_id].1.clone();
        if let Err(err) = write_local(app, col_id, &dir, &state) {
            return finish(app, col_id, Some(CloudError::Decode(err)));
        }
        super::file_watch::refresh_after_external_change(app, col_id);
    } else {
        save_state(app, col_id);
    }

    let final_pull = app.cloud.runs.get(&col_id).is_some_and(|r| r.final_pull);
    if final_pull {
        return finish(app, col_id, None);
    }
    push(app, col_id)
}

fn push(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    let (Some(client), Some(col), Some((_, state)), Some(run)) = (
        app.cloud.client.clone(),
        app.collections.iter().find(|c| c.id == col_id),
        app.cloud.linked.get(&col_id),
        app.cloud.runs.get_mut(&col_id),
    ) else {
        return finish(app, col_id, None);
    };
    let plan = state.plan_push(col, &run.skip);
    if plan.is_empty() || run.rounds >= MAX_PUSH_ROUNDS {
        // nothing (more) to send: one catch-up pull, then done
        run.final_pull = true;
        return fetch_changes(client, col_id, state);
    }
    run.rounds += 1;
    let collection_id = state.collection_id.clone();
    Task::perform(
        async move {
            let outcome = rustrest_cloud::sync::send_push(&client, &collection_id, &plan).await;
            (plan, outcome)
        },
        move |(plan, outcome)| Message::CloudPushed(col_id, Box::new((plan, outcome))),
    )
}

pub fn pushed(
    app: &mut Rustrest,
    col_id: usize,
    pushed: Box<(PushPlan, PushOutcome)>,
) -> Task<Message> {
    let (plan, outcome) = *pushed;
    let (Some((_, state)), Some(run)) = (
        app.cloud.linked.get_mut(&col_id),
        app.cloud.runs.get_mut(&col_id),
    ) else {
        return finish(app, col_id, None);
    };
    let recorded = state.record_push(&plan, outcome, &mut run.skip, &mut run.report);
    save_state(app, col_id);
    match recorded {
        // conflicts were set aside; send the rest
        Ok(true) => push(app, col_id),
        Ok(false) => {
            if let (Some(client), Some((_, state)), Some(run)) = (
                app.cloud.client.clone(),
                app.cloud.linked.get(&col_id),
                app.cloud.runs.get_mut(&col_id),
            ) {
                run.final_pull = true;
                return fetch_changes(client, col_id, state);
            }
            finish(app, col_id, None)
        }
        Err(err) => finish(app, col_id, Some(err)),
    }
}

fn finish(app: &mut Rustrest, col_id: usize, error: Option<CloudError>) -> Task<Message> {
    let run = app.cloud.runs.remove(&col_id);
    app.cloud.save_account();
    let name = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .map(|c| c.info.name.clone())
        .unwrap_or_default();

    let mut tasks = Vec::new();
    match error {
        Some(CloudError::Unauthorized) => {
            app.cloud.client = None;
            tasks.push(Task::done(Message::ShowToast(
                "Your Rustrest Cloud session expired. Sign in again to keep syncing.".to_string(),
                ToastStatus::Error,
            )));
        }

        // deleted in the cloud (or access revoked)
        Some(err) if err.is_not_found() => tasks.push(unlink(app, col_id)),
        Some(err) => tasks.push(Task::done(Message::ShowToast(
            format!("Couldn't sync '{name}': {err}"),
            ToastStatus::Error,
        ))),
        None => {
            if let Some(run) = &run {
                let conflicts = app.cloud.conflict_count(col_id);
                if conflicts > 0 && run.report.conflicts > 0 {
                    tasks.push(Task::done(Message::ShowToast(
                        format!(
                            "'{name}' has {conflicts} change(s) that conflict with the cloud. \
                             Open Cloud to resolve them."
                        ),
                        ToastStatus::Error,
                    )));
                }
                for rejected in &run.report.rejected {
                    eprintln!("cloud: rejected {rejected}");
                }
            }
        }
    }

    if app.cloud.resync.remove(&col_id) {
        tasks.push(Task::done(Message::CloudSync(col_id)));
    }
    update_status(app);
    Task::batch(tasks)
}

pub(super) fn update_status(app: &mut Rustrest) {
    let syncing = app.cloud.runs.len() + app.cloud.env_runs.len();
    let conflicts: usize = app
        .cloud
        .linked
        .keys()
        .map(|id| app.cloud.conflict_count(*id))
        .sum();
    let waiting = app
        .cloud
        .waiting
        .iter()
        .filter(|id| app.cloud.linked.contains_key(id))
        .count();

    if syncing > 0 {
        app.status_bar.set(STATUS_ID, "Syncing with cloud...", true);
    } else if conflicts > 0 {
        app.status_bar.set_with_action(
            STATUS_ID,
            format!("Cloud: {conflicts} conflict(s) to resolve"),
            false,
            Some(Message::OpenCloudModal),
        );
    } else if waiting > 0 {
        app.status_bar
            .set(STATUS_ID, "Cloud: save to sync your changes", false);
    } else {
        app.status_bar.clear(STATUS_ID);
    }
}

/// a realtime hint: sync the collection unless we're already at `seq`
pub fn realtime_changed(app: &mut Rustrest, collection_id: String, seq: i64) -> Task<Message> {
    let target = app
        .cloud
        .linked
        .iter()
        .find(|(_, (_, s))| s.collection_id == collection_id && s.seq < seq)
        .map(|(col_id, _)| *col_id);
    match target {
        Some(col_id) => sync(app, col_id),
        None => Task::none(),
    }
}

pub fn delete_collection_pressed(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    if !app.cloud.linked.contains_key(&col_id) {
        return Task::none();
    }
    let name = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .map(|c| c.info.name.clone())
        .unwrap_or_default();

    Task::done(Message::ShowConfirmDialog(
        crate::ui::confirm_dialog::ConfirmDialogState {
            title: "Delete collection from Rustrest Cloud?".to_string(),
            message: format!(
                "\"{name}\" will be deleted from the cloud for everyone in its team and closed \
                 here. This can't be undone."
            ),
            confirm_label: "Delete".to_string(),
            on_confirm: Box::new(Message::CloudDeleteCollectionConfirmed(col_id)),
        },
    ))
}

pub fn delete_collection(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    let Some(client) = app.cloud.client.clone() else {
        return Task::done(Message::ShowToast(
            "Sign in to Rustrest Cloud to delete this collection".to_string(),
            ToastStatus::Error,
        ));
    };
    let Some((_, state)) = app.cloud.linked.get(&col_id) else {
        return Task::none();
    };
    let collection_id = state.collection_id.clone();

    Task::perform(
        async move {
            client
                .delete_collection(&collection_id)
                .await
                .map_err(|e| e.to_string())
        },
        move |result| Message::CloudCollectionDeleted(col_id, result),
    )
}

pub fn collection_deleted(
    app: &mut Rustrest,
    col_id: usize,
    result: Result<(), String>,
) -> Task<Message> {
    if let Err(err) = result {
        return Task::done(Message::ShowToast(
            format!("Couldn't delete from the cloud: {err}"),
            ToastStatus::Error,
        ));
    }
    let name = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .map(|c| c.info.name.clone())
        .unwrap_or_default();

    // cache dir is ours, nothing else points at it once the collection is gone
    if let Some((dir, _)) = app.cloud.linked.remove(&col_id) {
        let _ = std::fs::remove_dir_all(dir);
    }
    app.cloud.waiting.remove(&col_id);
    app.cloud.resync.remove(&col_id);
    update_status(app);

    Task::batch([
        super::collections::delete_pressed(app, col_id),
        Task::done(Message::ShowToast(
            format!("Deleted '{name}' from Rustrest Cloud"),
            ToastStatus::Success,
        )),
    ])
}

/// the collection (or its whole team) was deleted in the cloud
pub fn realtime_deleted(app: &mut Rustrest, collection_id: String) -> Task<Message> {
    let target = app
        .cloud
        .linked
        .iter()
        .find(|(_, (_, s))| s.collection_id == collection_id)
        .map(|(col_id, _)| *col_id);
    match target {
        Some(col_id) => unlink(app, col_id),
        None => Task::none(),
    }
}

/// stops syncing a collection that's gone from the cloud, keeping the local
/// copy (unsaved edits included) as a plain local collection
fn unlink(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    let Some((dir, _)) = app.cloud.linked.remove(&col_id) else {
        return Task::none();
    };
    let _ = std::fs::remove_file(dir.join(rustrest_cloud::sync::STATE_FILE));
    app.cloud.waiting.remove(&col_id);
    app.cloud.resync.remove(&col_id);
    update_status(app);
    let name = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .map(|c| c.info.name.clone())
        .unwrap_or_default();
    Task::done(Message::ShowToast(
        format!("'{name}' is no longer in Rustrest Cloud; kept it as a local collection"),
        ToastStatus::Info,
    ))
}

// ---- conflicts ----

pub fn resolve(
    app: &mut Rustrest,
    col_id: usize,
    uid: Option<String>,
    resolution: Resolution,
) -> Task<Message> {
    let Some(col_pos) = app.collections.iter().position(|c| c.id == col_id) else {
        return Task::none();
    };
    let Some((dir, state)) = app.cloud.linked.get_mut(&col_id) else {
        return Task::none();
    };
    let dir = dir.clone();
    let col = &mut app.collections[col_pos];
    match uid {
        Some(uid) => state.resolve(col, &uid, resolution, &mut app.next_request_id),
        None => state.resolve_meta(col, resolution),
    }
    let state = state.clone();

    if resolution == Resolution::TakeTheirs {
        if let Err(err) = write_local(app, col_id, &dir, &state) {
            return Task::done(Message::ShowToast(err, ToastStatus::Error));
        }
        super::file_watch::refresh_after_external_change(app, col_id);
    } else {
        save_state(app, col_id);
    }
    update_status(app);
    Task::done(Message::CloudSync(col_id))
}

/// what the modal shows for one conflicted item: (uid or None for the
/// collection settings, label, whether the server side deleted it)
pub fn conflict_rows(app: &Rustrest, col_id: usize) -> Vec<(Option<String>, String, bool)> {
    let Some((_, state)) = app.cloud.linked.get(&col_id) else {
        return Vec::new();
    };
    let local_names = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .map(|c| {
            let mut names = HashMap::new();
            collect_names(&c.item, &mut names);
            names
        })
        .unwrap_or_default();

    let mut rows: Vec<_> = state
        .conflicts
        .iter()
        .map(|(uid, conflict)| {
            let label = local_names
                .get(uid)
                .cloned()
                .or_else(|| conflict.server_name().map(str::to_string))
                .unwrap_or_else(|| "Untitled".to_string());
            (Some(uid.clone()), label, conflict.server.is_none())
        })
        .collect();
    if state.meta_conflict.is_some() {
        rows.insert(0, (None, "Collection settings".to_string(), false));
    }
    rows
}

fn collect_names(
    items: &[crate::collection::collection::CollectionItem],
    out: &mut HashMap<String, String>,
) {
    use crate::collection::collection::CollectionItem;
    for item in items {
        match item {
            CollectionItem::Request(r) => {
                if let Some(uid) = &r.uid {
                    out.insert(uid.clone(), r.name.clone());
                }
            }
            CollectionItem::Folder(f) => {
                if let Some(uid) = &f.uid {
                    out.insert(uid.clone(), f.name.clone());
                }
                collect_names(&f.item, out);
            }
        }
    }
}

/// realtime stream subscription key: reconnects whenever the account or
/// the set of linked collections or environment teams changes
#[derive(Clone)]
pub struct RealtimeTarget {
    pub client: CloudClient,
    pub collection_ids: Vec<String>,
    /// teams whose environment events to receive
    pub team_ids: Vec<String>,
}

impl std::hash::Hash for RealtimeTarget {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        "rustrest-cloud-realtime".hash(state);
        self.client.base_url().hash(state);
        self.collection_ids.hash(state);
        self.team_ids.hash(state);
    }
}

pub fn realtime_stream(
    target: &RealtimeTarget,
) -> iced::futures::stream::BoxStream<'static, Message> {
    use iced::futures::{SinkExt, StreamExt};
    let target = target.clone();
    iced::stream::channel(32, async move |mut output| {
        loop {
            match rustrest_cloud::realtime::connect(
                &target.client,
                target.collection_ids.clone(),
                target.team_ids.clone(),
            )
            .await
            {
                Ok(mut events) => {
                    while let Some(event) = events.recv().await {
                        use rustrest_cloud::realtime::RealtimeEvent;
                        let message = match event {
                            RealtimeEvent::CollectionChanged {
                                collection_id, seq, ..
                            } => Message::CloudRealtimeChanged(collection_id, seq),
                            RealtimeEvent::CollectionDeleted { collection_id } => {
                                Message::CloudRealtimeDeleted(collection_id)
                            }
                            RealtimeEvent::EnvironmentChanged {
                                team_id,
                                environment_id,
                                rev,
                            } => Message::CloudRealtimeEnvChanged(team_id, environment_id, rev),
                            RealtimeEvent::EnvironmentDeleted {
                                team_id,
                                environment_id,
                            } => Message::CloudRealtimeEnvDeleted(team_id, environment_id),
                            RealtimeEvent::Denied(_) => continue,
                        };
                        let _ = output.send(message).await;
                    }
                }
                // signed out / expired: the periodic sync reports it
                Err(CloudError::Unauthorized) => return,
                Err(_) => {}
            }
            // dropped or unreachable: missed events are caught by the next sync anyway, so just retry calmly
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    })
    .boxed()
}
