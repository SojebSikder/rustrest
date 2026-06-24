use super::types::{BodyType, KeyValuePair, RawType, RequestSubTab, ResponseSubTab, ResponseView};
use crate::http_client::HttpMethod;
use iced::widget::text_editor;

#[derive(Debug, Clone)]
pub enum TabMessage {
    UrlChanged(String),
    MethodChanged(HttpMethod),
    SubTabSelected(RequestSubTab),
    AuthChanged(String),
    BodyTypeChanged(BodyType),

    BodyChanged(text_editor::Action),
    RawTypeChanged(RawType),

    ParamRowChanged(usize, KeyValuePair),
    AddParamRow,
    RemoveParamRow(usize),

    HeaderRowChanged(usize, KeyValuePair),
    AddHeaderRow,
    RemoveHeaderRow(usize),

    FormDataRowChanged(usize, KeyValuePair),
    AddFormDataRow,
    RemoveFormDataRow(usize),

    UrlencodedRowChanged(usize, KeyValuePair),
    AddUrlencodedRow,
    RemoveUrlencodedRow(usize),

    ResponseViewChanged(ResponseView),
    ResponseSubTabSelected(ResponseSubTab),

    CancelRequest,
}
