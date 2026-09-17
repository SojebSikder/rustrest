//! Workspace switching/creation/rename. A workspace bundles a set of
//! collections, environments, and open tabs; switching one in replaces the
//! live `Rustrest` collections/tabs/environments wholesale (see
//! `Rustrest::apply_workspace`, which stays a cross-cutting method since it
//! touches nearly every other domain).

use super::{Rustrest, remote};
use crate::collection::env::Environment;
use crate::message::Message;
use crate::session::SavedSession;
use crate::ui::toast::toast::ToastStatus;
use crate::workspace::SavedWorkspace;
use iced::Task;

#[derive(Default)]
pub struct WorkspacesState {
    pub workspaces: Vec<SavedWorkspace>,
    pub active_workspace_id: usize,
    pub next_workspace_id: usize,
    pub editing_workspace_id: Option<usize>,
}

pub fn selected(app: &mut Rustrest, name: String) -> Task<Message> {
    let target_id = app
        .workspace
        .workspaces
        .iter()
        .find(|w| w.name == name)
        .map(|w| w.id);
    let Some(target_id) = target_id else {
        return Task::none();
    };
    if target_id == app.workspace.active_workspace_id {
        return Task::none();
    }

    let dropped = app.commit_active_workspace_snapshot();
    crate::workspace::save(&app.build_workspace_manifest());

    let target = app
        .workspace
        .workspaces
        .iter()
        .find(|w| w.id == target_id)
        .cloned();
    let Some(target) = target else {
        return Task::none();
    };

    let load_errors = app.apply_workspace(&target);
    app.workspace.active_workspace_id = target_id;

    let mut tasks = vec![
        Task::done(Message::ShowToast(
            format!("Switched to workspace '{}'", target.name),
            ToastStatus::Success,
        )),
        remote::auto_connect_remote_collections(app),
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

pub fn create_pressed(app: &mut Rustrest) -> Task<Message> {
    app.commit_active_workspace_snapshot();
    crate::workspace::save(&app.build_workspace_manifest());

    let new_id = app.workspace.next_workspace_id;
    app.workspace.next_workspace_id += 1;

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
    app.workspace.workspaces.push(new_ws.clone());
    app.apply_workspace(&new_ws);
    app.workspace.active_workspace_id = new_id;
    crate::workspace::save(&app.build_workspace_manifest());

    Task::done(Message::ShowToast(
        format!("Created workspace '{}'", new_ws.name),
        ToastStatus::Success,
    ))
}

pub fn delete_pressed(app: &mut Rustrest, id: usize) -> Task<Message> {
    if app.workspace.workspaces.len() <= 1 {
        return Task::done(Message::ShowToast(
            "Can't delete the only workspace".to_string(),
            ToastStatus::Error,
        ));
    }

    let deleted_name = app
        .workspace
        .workspaces
        .iter()
        .find(|w| w.id == id)
        .map(|w| w.name.clone());
    app.workspace.workspaces.retain(|w| w.id != id);

    let mut load_errors = Vec::new();
    if id == app.workspace.active_workspace_id {
        if let Some(next) = app.workspace.workspaces.first().cloned() {
            load_errors = app.apply_workspace(&next);
            app.workspace.active_workspace_id = next.id;
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

pub fn rename_pressed(app: &mut Rustrest, id: usize) -> Task<Message> {
    app.workspace.editing_workspace_id = Some(id);
    Task::none()
}

pub fn name_changed(app: &mut Rustrest, id: usize, new_name: String) -> Task<Message> {
    if let Some(ws) = app.workspace.workspaces.iter_mut().find(|w| w.id == id) {
        ws.name = new_name;
    }
    Task::none()
}

pub fn save_name_pressed(app: &mut Rustrest, id: usize) -> Task<Message> {
    app.workspace.editing_workspace_id = None;
    if let Some(ws) = app.workspace.workspaces.iter_mut().find(|w| w.id == id) {
        if ws.name.trim().is_empty() {
            ws.name = format!("Workspace {id}");
        }
    }
    crate::workspace::save(&app.build_workspace_manifest());
    Task::none()
}
