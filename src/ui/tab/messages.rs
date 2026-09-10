use super::types::{
    BodyType, FormDataRow, FormDataType, KeyValuePair, RawType, RequestSubTab, ResponseSubTab,
    ResponseView,
};
use crate::ui::context_menu::TabFieldTarget;
use crate::{http_client::HttpMethod, ui::tab::types::ScriptTab};
use iced::widget::text_editor;

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
    AuthChanged(text_editor::Action),
    BodyTypeChanged(BodyType),

    SelectBinaryFile,
    BinaryFileSelected(String),
    SelectFormDataFile(usize),
    FormDataRowTypeChanged(usize, FormDataType),

    BodyChanged(text_editor::Action),
    RawTypeChanged(RawType),

    ParamRowChanged(usize, KeyValuePair),
    AddParamRow,
    RemoveParamRow(usize),

    HeaderRowChanged(usize, KeyValuePair),
    AddHeaderRow,
    RemoveHeaderRow(usize),

    FormDataRowChanged(usize, FormDataRow),
    AddFormDataRow,
    RemoveFormDataRow(usize),

    UrlencodedRowChanged(usize, KeyValuePair),
    AddUrlencodedRow,
    RemoveUrlencodedRow(usize),

    ResponseViewChanged(ResponseView),
    ResponseSubTabSelected(ResponseSubTab),

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
            | TabMessage::RemoveCookieRow(_) => true,

            TabMessage::AuthChanged(action)
            | TabMessage::BodyChanged(action)
            | TabMessage::PreRequestScriptChanged(action)
            | TabMessage::PostResponseScriptChanged(action) => matches!(action, Action::Edit(_)),

            TabMessage::ValueEditorAction(_, _, action) => matches!(action, Action::Edit(_)),

            TabMessage::SubTabSelected(_)
            | TabMessage::ResponseViewChanged(_)
            | TabMessage::ResponseSubTabSelected(_)
            | TabMessage::ResponseBodyEditorAction(_)
            | TabMessage::ScriptTabChanged(_)
            | TabMessage::CancelRequest
            | TabMessage::ShowFieldContextMenu(_, _) => false,
        }
    }
}
