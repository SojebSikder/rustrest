use super::types::{
    BodyType, FormDataRow, FormDataType, KeyValuePair, RawType, RequestSubTab, ResponseSubTab,
    ResponseView,
};
use crate::ui::context_menu::TabFieldTarget;
use crate::{http_client::HttpMethod, ui::tab::types::ScriptTab};
use iced::widget::text_editor;
use rustrest_core::{
    AuthLocation, AuthType, ClientAuthStyle, JwtAlgorithm, OAuth1SignatureMethod, OAuth2GrantType,
};

/// every field edit the Authorization tab can produce, grouped out of
/// `TabMessage` since there's one per `RequestAuth` field.
#[derive(Debug, Clone)]
pub enum AuthMessage {
    TypeChanged(AuthType),

    CustomRawAction(text_editor::Action),

    BearerTokenChanged(String),

    ApiKeyKeyChanged(String),
    ApiKeyValueChanged(String),
    ApiKeyAddToChanged(AuthLocation),

    BasicUsernameChanged(String),
    BasicPasswordChanged(String),

    JwtAlgorithmChanged(JwtAlgorithm),
    JwtSecretChanged(String),
    JwtPayloadAction(text_editor::Action),
    JwtHeaderPrefixChanged(String),
    JwtAddToChanged(AuthLocation),

    OAuth1SignatureMethodChanged(OAuth1SignatureMethod),
    OAuth1ConsumerKeyChanged(String),
    OAuth1ConsumerSecretChanged(String),
    OAuth1TokenChanged(String),
    OAuth1TokenSecretChanged(String),
    OAuth1RealmChanged(String),
    OAuth1AddToChanged(AuthLocation),

    OAuth2GrantTypeChanged(OAuth2GrantType),
    OAuth2AccessTokenChanged(String),
    OAuth2HeaderPrefixChanged(String),
    OAuth2AddToChanged(AuthLocation),
    OAuth2TokenUrlChanged(String),
    OAuth2ClientIdChanged(String),
    OAuth2ClientSecretChanged(String),
    OAuth2ScopeChanged(String),
    OAuth2ClientAuthChanged(ClientAuthStyle),
    /// intercepted at the app level (needs to spawn a network request)
    /// before reaching `Tab::update`.
    OAuth2FetchToken,
    /// the client-credentials token exchange resolved; also intercepted at
    /// the app level so a toast can report success/failure.
    OAuth2TokenFetched(Result<String, String>),
}

/// identifies which per-row "Value" editor a `ValueEditorAction` targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueField {
    Param,
    Header,
    Cookie,
    Urlencoded,
    FormData,
}

#[derive(Debug, Clone)]
pub enum TabMessage {
    UrlChanged(String),
    MethodSelected(String),
    MethodChanged(HttpMethod),
    SubTabSelected(RequestSubTab),
    Auth(AuthMessage),
    BodyTypeChanged(BodyType),

    SelectBinaryFile,
    BinaryFileSelected(String),
    SelectFormDataFile(usize),
    FormDataRowTypeChanged(usize, FormDataType),

    BodyChanged(text_editor::Action),
    RawTypeChanged(RawType),
    GraphQlQueryAction(text_editor::Action),
    GraphQlVariablesAction(text_editor::Action),

    ParamRowChanged(usize, KeyValuePair),
    AddParamRow,
    RemoveParamRow(usize),

    HeaderRowChanged(usize, KeyValuePair),
    AddHeaderRow,
    RemoveHeaderRow(usize),

    FormDataRowChanged(usize, FormDataRow),
    AddFormDataRow,
    /// shows/hides the Content-Type column of every form-data table (an app preference);
    /// intercepted at the app level before reaching `Tab::update`.
    ToggleFormDataContentType,
    RemoveFormDataRow(usize),

    UrlencodedRowChanged(usize, KeyValuePair),
    AddUrlencodedRow,
    RemoveUrlencodedRow(usize),

    ResponseViewChanged(ResponseView),
    ResponseSubTabSelected(ResponseSubTab),

    /// saves the current live response as a new named snapshot.
    SaveResponse,
    /// switches the response pane between the live response (`None`) and a
    /// saved snapshot at the given index.
    ViewSavedResponse(Option<usize>),
    DeleteSavedResponse(usize),

    CookieRowChanged(usize, KeyValuePair),
    AddCookieRow,
    RemoveCookieRow(usize),
    ResponseBodyEditorAction(iced::widget::text_editor::Action),

    /// a keystroke/cursor action in a per-row "Value" multiline editor
    ValueEditorAction(ValueField, usize, text_editor::Action),

    // scripts
    ScriptTabChanged(ScriptTab),
    PreRequestScriptChanged(text_editor::Action),
    PostResponseScriptChanged(text_editor::Action),

    CancelRequest,

    /// opens the shared Copy/Paste context menu for a field in this tab;
    /// intercepted at the app level before reaching `Tab::update`.
    ShowFieldContextMenu(TabFieldTarget, String),

    /// opens the Postman-style response timing/size modal for the live
    /// response (`None`) or a saved snapshot at the given index; intercepted
    /// at the app level before reaching `Tab::update`.
    ShowResponseTimingModal(Option<usize>),

    /// copies the given text straight to the clipboard, with no context menu
    /// in between (e.g. a row's "Copy" button); intercepted at the app level
    /// before reaching `Tab::update`.
    CopyToClipboard(String),

    DocsAction(text_editor::Action),
    DocsModeSelected(crate::ui::docs_view::DocsMode),
    /// a link in the docs preview; intercepted at the app level before
    /// reaching `Tab::update`.
    DocsLinkClicked(String),
}

impl TabMessage {
    /// whether this message represents an actual content edit, as opposed to
    /// a pure navigation/UI-state change (switching sub-tabs, moving a text
    /// cursor, etc). Drives the tab's unsaved-changes indicator.
    pub fn is_content_edit(&self) -> bool {
        use iced::widget::text_editor::Action;
        match self {
            TabMessage::UrlChanged(_)
            | TabMessage::MethodSelected(_)
            | TabMessage::MethodChanged(_)
            | TabMessage::BodyTypeChanged(_)
            | TabMessage::RawTypeChanged(_)
            | TabMessage::SelectBinaryFile
            | TabMessage::BinaryFileSelected(_)
            | TabMessage::SelectFormDataFile(_)
            | TabMessage::FormDataRowTypeChanged(_, _)
            | TabMessage::ParamRowChanged(_, _)
            | TabMessage::AddParamRow
            | TabMessage::RemoveParamRow(_)
            | TabMessage::HeaderRowChanged(_, _)
            | TabMessage::AddHeaderRow
            | TabMessage::RemoveHeaderRow(_)
            | TabMessage::FormDataRowChanged(_, _)
            | TabMessage::AddFormDataRow
            | TabMessage::RemoveFormDataRow(_)
            | TabMessage::UrlencodedRowChanged(_, _)
            | TabMessage::AddUrlencodedRow
            | TabMessage::RemoveUrlencodedRow(_)
            | TabMessage::CookieRowChanged(_, _)
            | TabMessage::AddCookieRow
            | TabMessage::RemoveCookieRow(_)
            | TabMessage::SaveResponse
            | TabMessage::DeleteSavedResponse(_) => true,

            TabMessage::BodyChanged(action)
            | TabMessage::GraphQlQueryAction(action)
            | TabMessage::GraphQlVariablesAction(action)
            | TabMessage::PreRequestScriptChanged(action)
            | TabMessage::PostResponseScriptChanged(action)
            | TabMessage::DocsAction(action) => matches!(action, Action::Edit(_)),

            TabMessage::ValueEditorAction(_, _, action) => matches!(action, Action::Edit(_)),

            // every Authorization-tab field edit counts as content, incl. a
            // fetched OAuth2 token becoming part of the saved auth config -
            // except a bare cursor move in one of its text editors, and
            // kicking off the token fetch itself (its *result* is the edit).
            TabMessage::Auth(auth_msg) => match auth_msg {
                AuthMessage::CustomRawAction(action) | AuthMessage::JwtPayloadAction(action) => {
                    matches!(action, Action::Edit(_))
                }
                AuthMessage::OAuth2FetchToken => false,
                _ => true,
            },

            TabMessage::SubTabSelected(_)
            | TabMessage::ToggleFormDataContentType
            | TabMessage::ResponseViewChanged(_)
            | TabMessage::ResponseSubTabSelected(_)
            | TabMessage::ResponseBodyEditorAction(_)
            | TabMessage::ScriptTabChanged(_)
            | TabMessage::CancelRequest
            | TabMessage::ViewSavedResponse(_)
            | TabMessage::ShowFieldContextMenu(_, _)
            | TabMessage::ShowResponseTimingModal(_)
            | TabMessage::CopyToClipboard(_)
            | TabMessage::DocsModeSelected(_)
            | TabMessage::DocsLinkClicked(_) => false,
        }
    }
}
