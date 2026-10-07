//! Rustrest Cloud environments: uploading, opening and keeping them in sync.

use super::Rustrest;
use super::cloud::{modal_busy, update_status};
use crate::message::Message;
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use iced::widget::text_editor;
use rustrest_cloud::env_sync::{self, EnvJob, EnvOutcome, Shared};
use rustrest_cloud::wire::CloudEnvironment;
use rustrest_cloud::{CloudClient, CloudError};
use rustrest_core::collection::env::EnvCloudLink;

/// teams with at least one environment open in this workspace
pub fn linked_teams(app: &Rustrest) -> Vec<String> {
    let mut teams: Vec<String> = app
        .env
        .environments
        .iter()
        .filter_map(|e| Some(e.cloud.as_ref()?.team_id.clone()))
        .collect();
    teams.sort();
    teams.dedup();
    teams
}

fn position(app: &Rustrest, env_id: &str) -> Option<usize> {
    app.env
        .environments
        .iter()
        .position(|e| e.cloud.as_ref().is_some_and(|l| l.id == env_id))
}

/// keeps environment links across restarts, they live in the workspace
fn persist_workspace(app: &mut Rustrest) {
    app.commit_active_workspace_snapshot();
    crate::workspace::save(&app.build_workspace_manifest());
}

// ---- sync ----

pub fn sync_all(app: &mut Rustrest) -> Task<Message> {
    let teams = linked_teams(app);
    Task::batch(
        teams
            .into_iter()
            .map(|t| Task::done(Message::CloudEnvSync(t))),
    )
}

pub fn sync_team(app: &mut Rustrest, team_id: String) -> Task<Message> {
    let Some(client) = app.cloud.client.clone() else {
        return Task::none();
    };
    if app.cloud.env_runs.contains(&team_id) {
        app.cloud.env_resync.insert(team_id);
        return Task::none();
    }
    // an environment open in the editor syncs once the editor closes
    let jobs: Vec<EnvJob> = app
        .env
        .environments
        .iter()
        .enumerate()
        .filter(|(idx, _)| app.env.editing_env_index != Some(*idx))
        .filter_map(|(_, env)| {
            let link = env.cloud.as_ref().filter(|l| l.team_id == team_id)?;
            Some(EnvJob {
                link: link.clone(),
                mine: Shared::of(env),
            })
        })
        .collect();
    if jobs.is_empty() {
        return Task::none();
    }

    app.cloud.env_runs.insert(team_id.clone());
    update_status(app);
    Task::perform(
        async move {
            let result = env_sync::sync_team(&client, &team_id, jobs).await;
            (team_id, result)
        },
        |(team_id, result)| Message::CloudEnvSynced(team_id, result),
    )
}

pub fn synced(
    app: &mut Rustrest,
    team_id: String,
    result: Result<Vec<(EnvJob, EnvOutcome)>, CloudError>,
) -> Task<Message> {
    app.cloud.env_runs.remove(&team_id);
    app.cloud.save_account();

    let mut toasts = Vec::new();
    let mut changed = false;
    match result {
        Err(CloudError::Unauthorized) => {
            app.cloud.client = None;
            toasts.push((
                "Your Rustrest Cloud session expired. Sign in again to keep syncing.".to_string(),
                ToastStatus::Error,
            ));
        }
        Err(err) => toasts.push((
            format!("Couldn't sync environments: {err}"),
            ToastStatus::Error,
        )),
        Ok(outcomes) => {
            for (job, outcome) in outcomes {
                // closed or switched away from while the sync ran
                let Some(idx) = position(app, &job.link.id) else {
                    continue;
                };
                let env = &app.env.environments[idx];
                let before = (env.cloud.clone(), Shared::of(env));
                if let Some(toast) = record(app, idx, &team_id, job, outcome) {
                    toasts.push(toast);
                }
                let env = &app.env.environments[idx];
                changed |= before != (env.cloud.clone(), Shared::of(env));
            }
        }
    }

    if changed {
        persist_workspace(app);
    }
    let mut tasks: Vec<Task<Message>> = toasts
        .into_iter()
        .map(|(msg, status)| Task::done(Message::ShowToast(msg, status)))
        .collect();
    if app.cloud.env_resync.remove(&team_id) {
        tasks.push(Task::done(Message::CloudEnvSync(team_id)));
    }
    update_status(app);
    Task::batch(tasks)
}

/// applies one environment's outcome, returning a toast worth showing
fn record(
    app: &mut Rustrest,
    idx: usize,
    team_id: &str,
    job: EnvJob,
    outcome: EnvOutcome,
) -> Option<(String, ToastStatus)> {
    let first_upload = job.link.rev == 0;
    let env = &mut app.env.environments[idx];
    let name = env.name.clone();

    match outcome {
        EnvOutcome::Synced {
            rev,
            server,
            conflicts,
            reverted,
        } => {
            // edits made while the sync ran are merged on top, not lost
            let target = env_sync::merge(&job.mine, &Shared::of(env), &server).shared;
            let applied = env_sync::apply(env, &target);
            if let Some(link) = env.cloud.as_mut() {
                link.rev = rev;
                link.base_name = server.name.clone();
                link.base = server.data_value();
            }
            if target != server {
                app.cloud.env_resync.insert(team_id.to_string());
            }
            if applied && app.env.editing_env_index == Some(idx) {
                reload_editor(app, idx);
            }

            if reverted {
                Some((
                    format!(
                        "You can only view this team's environments, so your changes to \
                         '{name}' were undone. Uncheck Sync on a variable to keep your own value."
                    ),
                    ToastStatus::Error,
                ))
            } else if conflicts > 0 {
                Some((
                    format!(
                        "'{name}': {conflicts} change(s) were also made by a teammate; \
                         kept yours"
                    ),
                    ToastStatus::Info,
                ))
            } else if first_upload {
                Some((
                    format!("'{name}' is now synced with Rustrest Cloud"),
                    ToastStatus::Success,
                ))
            } else {
                None
            }
        }
        EnvOutcome::Removed => {
            env.cloud = None;
            Some((
                format!("'{name}' is no longer in Rustrest Cloud; kept it as a local environment"),
                ToastStatus::Info,
            ))
        }
        // the upload never landed, so there's nothing in the cloud to stay linked to
        EnvOutcome::Failed(err) if first_upload => {
            env.cloud = None;
            Some((
                format!("Couldn't upload '{name}': {err}"),
                ToastStatus::Error,
            ))
        }
        EnvOutcome::Failed(err) => {
            Some((format!("Couldn't sync '{name}': {err}"), ToastStatus::Error))
        }
    }
}

/// the editor keeps one text editor per variable value; rebuild them after
/// the variables were replaced
fn reload_editor(app: &mut Rustrest, idx: usize) {
    if let Some(env) = app.env.environments.get(idx) {
        app.env.env_var_value_contents = env
            .variables
            .iter()
            .map(|v| text_editor::Content::with_text(&v.value))
            .collect();
    }
}

/// the editor closed: publish what was edited in it
pub fn editor_closed(app: &mut Rustrest, idx: usize) -> Task<Message> {
    let team = app
        .env
        .environments
        .get(idx)
        .and_then(|e| Some(e.cloud.as_ref()?.team_id.clone()));
    match team {
        Some(team_id) => sync_team(app, team_id),
        None => Task::none(),
    }
}

/// a realtime hint: sync the team unless we're already at `rev`
pub fn realtime_changed(
    app: &mut Rustrest,
    team_id: String,
    env_id: String,
    rev: i64,
) -> Task<Message> {
    let reload = reload_modal_list(app, &team_id);
    let behind = app.env.environments.iter().any(|e| {
        e.cloud
            .as_ref()
            .is_some_and(|l| l.id == env_id && l.rev < rev)
    });
    if !behind {
        return reload;
    }
    Task::batch([reload, sync_team(app, team_id)])
}

pub fn realtime_deleted(
    app: &mut Rustrest,
    team_id: String,
    env_id: Option<String>,
) -> Task<Message> {
    let reload = reload_modal_list(app, &team_id);
    let affected = app.env.environments.iter().any(|e| {
        e.cloud
            .as_ref()
            .is_some_and(|l| l.team_id == team_id && env_id.as_ref().is_none_or(|id| *id == l.id))
    });
    if !affected {
        return reload;
    }
    // the sync finds it gone and unlinks it
    Task::batch([reload, sync_team(app, team_id)])
}

pub fn unlink(app: &mut Rustrest, idx: usize) -> Task<Message> {
    let Some(env) = app.env.environments.get_mut(idx) else {
        return Task::none();
    };
    if env.cloud.take().is_none() {
        return Task::none();
    }
    let name = env.name.clone();
    persist_workspace(app);
    Task::done(Message::ShowToast(
        format!("Stopped syncing '{name}'; it stays in the cloud for your team"),
        ToastStatus::Info,
    ))
}

// ---- modal ----

pub fn load_environments(client: CloudClient, team_id: String) -> Task<Message> {
    Task::perform(
        async move {
            let envs = client
                .environments(&team_id)
                .await
                .map_err(|e| e.to_string());
            (team_id, envs)
        },
        |(team_id, result)| Message::CloudEnvironmentsLoaded(team_id, result),
    )
}

/// refreshes the modal's list when it shows `team_id`
fn reload_modal_list(app: &Rustrest, team_id: &str) -> Task<Message> {
    let shown = app
        .cloud
        .modal
        .as_ref()
        .is_some_and(|m| m.selected_team.as_deref() == Some(team_id));
    match (&app.cloud.client, shown) {
        (Some(client), true) => load_environments(client.clone(), team_id.to_string()),
        _ => Task::none(),
    }
}

pub fn environments_loaded(
    app: &mut Rustrest,
    team_id: String,
    result: Result<Vec<CloudEnvironment>, String>,
) -> Task<Message> {
    let Some(modal) = app.cloud.modal.as_mut() else {
        return Task::none();
    };
    // a reply for a team that's no longer selected
    if modal.selected_team.as_ref() != Some(&team_id) {
        return Task::none();
    }
    match result {
        Ok(envs) => modal.environments = envs,
        Err(err) => modal.error = Some(err),
    }
    Task::none()
}

pub fn open_upload(app: &mut Rustrest, idx: usize) -> Task<Message> {
    // the cloud modal takes over from the editor
    app.env.editing_env_index = None;
    app.env.editing_env_name = false;
    app.env.env_var_value_contents = Vec::new();
    let task = super::cloud::open_modal(app, None);
    if let Some(modal) = app.cloud.modal.as_mut() {
        modal.upload_environment = Some(idx);
    }
    task
}

/// links the environment to the selected team; the sync that follows uploads it
pub fn upload(app: &mut Rustrest) -> Task<Message> {
    let Some(modal) = app.cloud.modal.as_ref() else {
        return Task::none();
    };
    let (Some(idx), Some(team_id)) = (modal.upload_environment, modal.selected_team.clone()) else {
        return Task::none();
    };
    let Some(env) = app.env.environments.get_mut(idx) else {
        return Task::none();
    };
    if env.cloud.is_some() {
        modal_busy(
            app,
            false,
            Some("This environment already syncs with the cloud".into()),
        );
        return Task::none();
    }
    env.cloud = Some(EnvCloudLink {
        id: rustrest_cloud::convert::new_uid(),
        team_id: team_id.clone(),
        ..EnvCloudLink::default()
    });
    app.cloud.modal = None;
    persist_workspace(app);
    sync_team(app, team_id)
}

pub fn open(app: &mut Rustrest, env_id: String) -> Task<Message> {
    if let Some(idx) = position(app, &env_id) {
        app.env.active_env_index = Some(idx);
        app.cloud.modal = None;
        return Task::done(Message::ShowToast(
            "That environment is already open".to_string(),
            ToastStatus::Info,
        ));
    }
    let Some(remote) = app
        .cloud
        .modal
        .as_ref()
        .and_then(|m| m.environments.iter().find(|e| e.id == env_id))
    else {
        return Task::none();
    };
    let env = env_sync::from_cloud(remote);
    let name = env.name.clone();
    app.env.environments.push(env);
    app.env.active_env_index = Some(app.env.environments.len() - 1);
    app.cloud.modal = None;
    persist_workspace(app);
    Task::done(Message::ShowToast(
        format!("Opened '{name}' from Rustrest Cloud"),
        ToastStatus::Success,
    ))
}

pub fn delete(app: &mut Rustrest, env_id: String) -> Task<Message> {
    let (Some(client), Some(modal)) = (app.cloud.client.clone(), app.cloud.modal.as_mut()) else {
        return Task::none();
    };
    // first click only asks for confirmation
    if modal.confirm_delete_environment.as_ref() != Some(&env_id) {
        modal.confirm_delete_environment = Some(env_id);
        return Task::none();
    }
    modal.confirm_delete_environment = None;
    let name = modal
        .environments
        .iter()
        .find(|e| e.id == env_id)
        .map(|e| e.name.clone())
        .unwrap_or_default();
    modal.busy = true;
    modal.error = None;
    Task::perform(
        async move {
            let result = client
                .delete_environment(&env_id)
                .await
                .map(|_| format!("Deleted '{name}' from Rustrest Cloud"))
                .map_err(|e| e.to_string());
            (env_id, result)
        },
        |(env_id, result)| Message::CloudEnvironmentDeleted(env_id, result),
    )
}

pub fn deleted(
    app: &mut Rustrest,
    env_id: String,
    result: Result<String, String>,
) -> Task<Message> {
    let toast = match result {
        Ok(toast) => toast,
        Err(err) => {
            modal_busy(app, false, Some(err));
            return Task::none();
        }
    };
    modal_busy(app, false, None);
    if let Some(modal) = app.cloud.modal.as_mut() {
        modal.environments.retain(|e| e.id != env_id);
    }
    // gone for the whole team, so it goes from here too
    let close = match position(app, &env_id) {
        Some(idx) => {
            let task = super::environment::delete_pressed(app, idx);
            persist_workspace(app);
            task
        }
        None => Task::none(),
    };
    Task::batch([
        close,
        Task::done(Message::ShowToast(toast, ToastStatus::Success)),
    ])
}
