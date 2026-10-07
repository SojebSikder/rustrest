//! Environments (the `pm.environment`-style variable sets a request can be
//! sent against) and the inline environment editor's rename/variable-edit state.

use super::Rustrest;
use crate::collection::env::Environment;
use crate::message::Message;
use crate::ui::tab::types::KeyValuePair;
use iced::Task;
use iced::widget::text_editor;

#[derive(Default)]
pub struct EnvState {
    pub environments: Vec<Environment>,
    pub active_env_index: Option<usize>,
    pub editing_env_index: Option<usize>,
    pub editing_env_name: bool,
    pub env_var_value_contents: Vec<text_editor::Content>,
    pub globals: Vec<KeyValuePair>,
}

pub fn selected(app: &mut Rustrest, idx: Option<usize>) -> Task<Message> {
    app.env.active_env_index = idx.filter(|i| *i < app.env.environments.len());
    Task::none()
}

pub fn create_pressed(app: &mut Rustrest) -> Task<Message> {
    let new_count = app.env.environments.len() + 1;
    let new_env_name = format!("Environment {}", new_count);

    app.env.environments.push(Environment {
        name: new_env_name,
        variables: Vec::new(),
        ..Environment::default()
    });

    let new_idx = app.env.environments.len() - 1;
    app.env.active_env_index = Some(new_idx);

    app.env.editing_env_index = Some(new_idx);
    app.env.editing_env_name = false;
    app.env.env_var_value_contents = Vec::new();

    Task::none()
}

pub fn delete_pressed(app: &mut Rustrest, idx: usize) -> Task<Message> {
    if idx < app.env.environments.len() {
        app.env.environments.remove(idx);

        if app.env.editing_env_index == Some(idx) {
            app.env.editing_env_index = None;
            app.env.env_var_value_contents = Vec::new();
        }

        if app.env.environments.is_empty() {
            app.env.active_env_index = None;
        } else if let Some(active) = app.env.active_env_index {
            if active == idx {
                app.env.active_env_index = Some(idx.saturating_sub(1));
            } else if active > idx {
                app.env.active_env_index = Some(active - 1);
            }
        }
    }

    Task::none()
}

pub fn edit_pressed(app: &mut Rustrest, idx: usize) -> Task<Message> {
    app.env.editing_env_index = Some(idx);
    app.env.editing_env_name = false;
    app.env.env_var_value_contents = app
        .env
        .environments
        .get(idx)
        .map(|env| {
            env.variables
                .iter()
                .map(|v| text_editor::Content::with_text(&v.value))
                .collect()
        })
        .unwrap_or_default();
    Task::none()
}

pub fn close_editor_pressed(app: &mut Rustrest) -> Task<Message> {
    let closed = app.env.editing_env_index.take();
    app.env.editing_env_name = false;
    app.env.env_var_value_contents = Vec::new();

    match closed {
        Some(idx) => super::cloud_env::editor_closed(app, idx),
        None => Task::none(),
    }
}

pub fn add_variable_pressed(app: &mut Rustrest, env_idx: usize) -> Task<Message> {
    if let Some(env) = app.env.environments.get_mut(env_idx) {
        env.variables.push(KeyValuePair::new("", ""));
        if app.env.editing_env_index == Some(env_idx) {
            app.env
                .env_var_value_contents
                .push(text_editor::Content::new());
        }
    }
    Task::none()
}

pub fn delete_variable_pressed(
    app: &mut Rustrest,
    env_idx: usize,
    var_idx: usize,
) -> Task<Message> {
    if let Some(env) = app.env.environments.get_mut(env_idx) {
        if var_idx < env.variables.len() {
            let removed = env.variables.remove(var_idx);
            let key = removed.key.trim();

            if !env.variables.iter().any(|v| v.key.trim() == key) {
                env.local_keys.remove(key);
            }
            if app.env.editing_env_index == Some(env_idx)
                && var_idx < app.env.env_var_value_contents.len()
            {
                app.env.env_var_value_contents.remove(var_idx);
            }
        }
    }
    Task::none()
}

pub fn variable_key_changed(
    app: &mut Rustrest,
    env_idx: usize,
    var_idx: usize,
    key: String,
) -> Task<Message> {
    if let Some(env) = app.env.environments.get_mut(env_idx) {
        if let Some(var) = env.variables.get_mut(var_idx) {
            let old = std::mem::replace(&mut var.key, key).trim().to_string();
            let new = var.key.trim().to_string();

            // a local variable stays local while its key is being edited
            if old != new && env.local_keys.contains(&old) {
                if !env.variables.iter().any(|v| v.key.trim() == old) {
                    env.local_keys.remove(&old);
                }
                if !new.is_empty() {
                    env.local_keys.insert(new);
                }
            }
        }
    }
    Task::none()
}

/// `sync` false keeps the variable's value on this machine
pub fn variable_sync_toggled(
    app: &mut Rustrest,
    env_idx: usize,
    var_idx: usize,
    sync: bool,
) -> Task<Message> {
    if let Some(env) = app.env.environments.get_mut(env_idx) {
        if let Some(key) = env.variables.get(var_idx).map(|v| v.key.trim().to_string()) {
            if key.is_empty() {
                return Task::none();
            }
            if sync {
                env.local_keys.remove(&key);
            } else {
                env.local_keys.insert(key);
            }
        }
    }
    Task::none()
}

pub fn variable_value_editor_action(
    app: &mut Rustrest,
    env_idx: usize,
    var_idx: usize,
    action: text_editor::Action,
) -> Task<Message> {
    if app.env.editing_env_index == Some(env_idx) {
        if let Some(content) = app.env.env_var_value_contents.get_mut(var_idx) {
            content.perform(action);
            let text = content.text();
            if let Some(env) = app.env.environments.get_mut(env_idx) {
                if let Some(var) = env.variables.get_mut(var_idx) {
                    var.value = text;
                }
            }
        }
    }
    Task::none()
}

pub fn variable_toggled(
    app: &mut Rustrest,
    env_idx: usize,
    var_idx: usize,
    is_active: bool,
) -> Task<Message> {
    if let Some(env) = app.env.environments.get_mut(env_idx) {
        if let Some(var) = env.variables.get_mut(var_idx) {
            var.is_active = is_active;
        }
    }
    Task::none()
}

pub fn rename_pressed(app: &mut Rustrest, idx: usize) -> Task<Message> {
    if idx < app.env.environments.len() {
        app.env.editing_env_name = true;
    }
    Task::none()
}

pub fn name_changed(app: &mut Rustrest, idx: usize, new_name: String) -> Task<Message> {
    if let Some(env) = app.env.environments.get_mut(idx) {
        env.name = new_name;
    }
    Task::none()
}

pub fn save_name_pressed(app: &mut Rustrest, idx: usize) -> Task<Message> {
    app.env.editing_env_name = false;
    if let Some(env) = app.env.environments.get_mut(idx) {
        if env.name.trim().is_empty() {
            env.name = format!("Environment {}", idx + 1);
        }
    }
    Task::none()
}
