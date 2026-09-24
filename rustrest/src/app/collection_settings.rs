//! a collection root tab's Authorization and Scripts sub-tabs

use super::{Rustrest, WorkspaceContent};
use crate::collection::collection::set_event_script;
use crate::message::Message;
use crate::ui::collection_settings::CollectionSettingsState;
use crate::ui::tab::messages::AuthMessage;
use crate::ui::tab::types::ScriptTab;
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use iced::widget::text_editor::Action;
use rustrest_core::AuthType;

/// the settings editor of `collection_id`'s root tab, if it's open.
fn settings_mut(app: &mut Rustrest, collection_id: usize) -> Option<&mut CollectionSettingsState> {
    app.tabs.iter_mut().find_map(|t| match &mut t.content {
        WorkspaceContent::CollectionRoot {
            collection_id: id,
            settings: Some(settings),
            ..
        } if *id == collection_id => Some(settings.as_mut()),
        _ => None,
    })
}

/// copies the auth form into the collection. No Auth is stored as `None`,
/// which is what a collection without auth already means.
fn write_auth(app: &mut Rustrest, collection_id: usize) {
    let Some(auth) = settings_mut(app, collection_id).map(|s| s.auth.to_core()) else {
        return;
    };
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        col.auth = (auth.auth_type != AuthType::NoAuth).then_some(auth);
        col.unsaved = true;
    }
}

pub fn auth_message(app: &mut Rustrest, collection_id: usize, msg: AuthMessage) -> Task<Message> {
    let Some(settings) = settings_mut(app, collection_id) else {
        return Task::none();
    };
    let form = &mut settings.auth;

    match msg {
        AuthMessage::OAuth2FetchToken => {
            form.oauth2_fetching_token = true;
            let token_url = form.oauth2_token_url.clone();
            let client_id = form.oauth2_client_id.clone();
            let client_secret = form.oauth2_client_secret.clone();
            let scope = form.oauth2_scope.clone();
            let client_auth = form.oauth2_client_auth;

            Task::perform(
                async move {
                    rustrest_core::auth::fetch_oauth2_client_credentials_token(
                        &token_url,
                        &client_id,
                        &client_secret,
                        &scope,
                        client_auth,
                    )
                    .await
                },
                move |result| {
                    Message::CollectionAuth(
                        collection_id,
                        AuthMessage::OAuth2TokenFetched(result.map(|r| r.access_token)),
                    )
                },
            )
        }
        AuthMessage::OAuth2TokenFetched(result) => {
            form.oauth2_fetching_token = false;
            match result {
                Ok(token) => {
                    form.oauth2_access_token = token;
                    write_auth(app, collection_id);
                    Task::done(Message::ShowToast(
                        "Fetched a new OAuth 2.0 access token".to_string(),
                        ToastStatus::Success,
                    ))
                }
                Err(e) => Task::done(Message::ShowToast(
                    format!("Couldn't fetch OAuth 2.0 access token: {e}"),
                    ToastStatus::Error,
                )),
            }
        }
        msg => {
            // cursor moves/selections in the multiline fields aren't edits
            let before = form.to_core();
            form.update(msg);
            if form.to_core() != before {
                write_auth(app, collection_id);
            }
            Task::none()
        }
    }
}

pub fn script_tab_changed(
    app: &mut Rustrest,
    collection_id: usize,
    tab: ScriptTab,
) -> Task<Message> {
    if let Some(settings) = settings_mut(app, collection_id) {
        settings.script_tab = tab;
    }
    Task::none()
}

pub fn script_action(
    app: &mut Rustrest,
    collection_id: usize,
    tab: ScriptTab,
    action: Action,
) -> Task<Message> {
    let Some(settings) = settings_mut(app, collection_id) else {
        return Task::none();
    };
    let is_edit = action.is_edit();
    let (editor, listen) = match tab {
        ScriptTab::PreRequest => (&mut settings.pre_request_script, "prerequest"),
        ScriptTab::PostResponse => (&mut settings.post_response_script, "test"),
    };
    editor.perform(action);
    if !is_edit {
        return Task::none();
    }

    let script = editor.text();
    if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
        set_event_script(&mut col.event, listen, &script);
        col.unsaved = true;
    }
    Task::none()
}
