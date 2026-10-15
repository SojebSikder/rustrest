//! The "Generate Code" modal: compiles the active request the same way
//! Send does (except scripts/plugin hooks) and hands it to a code generator.

use super::{Rustrest, WorkspaceContent};
use crate::collection::env::Environment;
use crate::message::Message;
use crate::ui::codegen_modal::{CodegenMessage, CodegenModalState};
use crate::ui::tab::Tab;
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use rustrest_core::KeyValuePair;
use rustrest_core::codegen::{CodegenRequest, RequestParts};

pub fn open(app: &mut Rustrest) -> Task<Message> {
    let Some(tab_state) = app
        .tabs
        .get(app.active_tab_index)
        .filter(|t| matches!(t.content, WorkspaceContent::HttpRequest))
    else {
        return Task::done(Message::ShowToast(
            "Open an HTTP request to generate code for it".to_string(),
            ToastStatus::Info,
        ));
    };
    let tab = &tab_state.tab;

    let state = CodegenModalState::new(
        tab.name.clone(),
        build_request(app, tab, true),
        build_request(app, tab, false),
        &app.overlays.codegen_prefs,
    );
    app.overlays.codegen_modal = Some(state);
    Task::none()
}

pub fn update(app: &mut Rustrest, msg: CodegenMessage) -> Task<Message> {
    let prefs = &mut app.overlays.codegen_prefs;
    let Some(modal) = app.overlays.codegen_modal.as_mut() else {
        return Task::none();
    };

    match msg {
        CodegenMessage::LanguageSelected(lang) => prefs.generator_id = lang.id,
        CodegenMessage::OptionToggled(key, value) => {
            prefs
                .options
                .entry(prefs.generator_id)
                .or_default()
                .set(key, value);
        }
        CodegenMessage::ResolveVariablesToggled(value) => prefs.resolve_variables = value,
        // selection/navigation only, the snippet is read-only
        CodegenMessage::EditorAction(action) => {
            if !action.is_edit() {
                modal.code.perform(action);
            }
            return Task::none();
        }
        CodegenMessage::Copy => {
            let Ok(code) = modal.snippet(prefs) else {
                return Task::none();
            };
            let label = prefs.generator().label();
            return Task::batch([
                iced::clipboard::write(code),
                Task::done(Message::ShowToast(
                    format!("Copied {label} snippet to clipboard"),
                    ToastStatus::Success,
                )),
            ]);
        }
        CodegenMessage::Close => {
            app.overlays.codegen_modal = None;
            return Task::none();
        }
    }

    modal.regenerate(prefs);
    Task::none()
}

/// compiles `tab` into a generator-ready request. With `resolve`, variables
/// come from the active environment, the collection and globals (in Send's
/// precedence); without it, `{{var}}` placeholders are left as typed.
fn build_request(app: &Rustrest, tab: &Tab, resolve: bool) -> Result<CodegenRequest, String> {
    let collection = tab
        .collection_id
        .and_then(|c_id| app.collections.iter().find(|c| c.id == c_id));
    let collection_auth = collection.and_then(|c| c.auth.as_ref());

    let (env, collection_vars) = if resolve {
        let collection_vars = collection.map(|c| c.get_native_variables());
        let mut env = app
            .env
            .active_env_index
            .and_then(|i| app.env.environments.get(i))
            .cloned()
            .unwrap_or_else(|| Environment::new("__codegen"));

        // globals are the lowest-precedence source, as in `send_request`
        let is_defined = |vars: &[KeyValuePair], k: &str| {
            vars.iter()
                .any(|kv| kv.is_active && kv.key.trim() == k.trim())
        };
        for global in app.env.globals.iter().filter(|g| g.is_active) {
            let shadowed = is_defined(&env.variables, &global.key)
                || collection_vars
                    .as_deref()
                    .is_some_and(|vars| is_defined(vars, &global.key));
            if !shadowed {
                env.variables.push(global.clone());
            }
        }
        (Some(env), collection_vars)
    } else {
        (None, None)
    };

    let (url, raw_body, form_data, headers, cookies) =
        tab.compile_request_fields(&env, collection_vars.as_deref(), collection_auth)?;

    Ok(CodegenRequest::from_parts(RequestParts {
        method: tab.method.to_string(),
        url,
        body_type: tab.body_type,
        raw_body,
        form_data,
        binary_file_path: tab.binary_file_path.clone(),
        headers,
        cookies,
    }))
}
