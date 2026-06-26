use super::types::{
    BodyType, FormDataRow, FormDataType, KeyValuePair, RawType, RequestSubTab, ResponseSubTab,
    ResponseView,
};
use crate::http_client::HttpMethod;
use iced::widget::text_editor;

#[derive(Debug, Clone)]
pub enum TabMessage {
    UrlChanged(String),
    MethodSelected(String),
    MethodChanged(HttpMethod),
    SubTabSelected(RequestSubTab),
    AuthChanged(String),
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

    CancelRequest,
}
