//! editor state for a collection root tab's Authorization and Scripts sub-tabs.

use crate::collection::collection::PostmanCollection;
use crate::collection::collection::event_script;
use crate::ui::script_editor::ScriptContent;
use crate::ui::tab::auth_form::AuthFormState;
use crate::ui::tab::types::ScriptTab;

#[derive(Debug, Clone)]
pub struct CollectionSettingsState {
    pub auth: AuthFormState,
    pub script_tab: ScriptTab,
    pub pre_request_script: ScriptContent,
    pub post_response_script: ScriptContent,
}

impl CollectionSettingsState {
    pub fn from_collection(collection: &PostmanCollection) -> Self {
        let mut auth = AuthFormState::default();
        match &collection.auth {
            Some(core) => auth.load_from(core),
            // a collection has no parent to inherit from
            None => auth.auth_type = rustrest_core::AuthType::NoAuth,
        }
        Self {
            auth,
            script_tab: ScriptTab::PreRequest,
            pre_request_script: ScriptContent::with_text(&event_script(
                &collection.event,
                "prerequest",
            )),
            post_response_script: ScriptContent::with_text(&event_script(
                &collection.event,
                "test",
            )),
        }
    }
}
