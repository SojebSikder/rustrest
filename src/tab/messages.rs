use super::types::{BodyType, KeyValuePair, RequestSubTab};
use crate::http_client::HttpMethod;

#[derive(Debug, Clone)]
pub enum TabMessage {
    UrlChanged(String),
    MethodChanged(HttpMethod),
    SubTabSelected(RequestSubTab),
    AuthChanged(String),
    BodyTypeChanged(BodyType),
    BodyChanged(String),

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
}
