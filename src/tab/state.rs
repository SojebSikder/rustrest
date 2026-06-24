use super::messages::TabMessage;
use super::types::{BodyType, KeyValuePair, RawType, RequestSubTab, ResponseSubTab, ResponseView};
use crate::http_client::{HttpMethod, HttpResponse};
use iced::widget::text_editor;
use tokio_util::sync::CancellationToken;

pub struct Tab {
    pub id: usize,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub active_sub_tab: RequestSubTab,
    pub active_response_tab: ResponseSubTab,
    pub body_type: BodyType,
    pub raw_type: RawType,
    pub response_view: ResponseView,
    pub request_params: Vec<KeyValuePair>,
    pub request_headers: Vec<KeyValuePair>,
    pub request_cookies: Vec<KeyValuePair>,
    pub request_auth: String,
    pub request_body: text_editor::Content,
    pub body_form_data: Vec<KeyValuePair>,
    pub body_urlencoded: Vec<KeyValuePair>,
    pub response: Option<Result<HttpResponse, String>>,
    pub is_loading: bool,
    pub cancel_token: CancellationToken,
}

impl Tab {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            name: format!("Request {}", id),
            url: String::from("https://httpbin.org/json"),
            method: HttpMethod::GET,
            active_sub_tab: RequestSubTab::Params,
            active_response_tab: ResponseSubTab::Body,
            body_type: BodyType::Raw,
            raw_type: RawType::Json,
            response_view: ResponseView::Json,
            request_params: vec![
                KeyValuePair::new("key", "value"),
                KeyValuePair::new("foo", "bar"),
            ],
            request_headers: vec![
                KeyValuePair::new("Content-Type", "application/json"),
                KeyValuePair::new("Accept", "application/json"),
            ],
            request_cookies: vec![KeyValuePair::new("session_id", "xyz123abc")],
            request_auth: String::from("Bearer your_token_here"),
            request_body: text_editor::Content::with_text("{\n  \"key\": \"value\"\n}"),
            body_form_data: vec![KeyValuePair::new("form_field", "value")],
            body_urlencoded: vec![KeyValuePair::new("form_key", "form_value")],
            response: None,
            is_loading: false,
            cancel_token: CancellationToken::new(),
        }
    }

    pub fn update(&mut self, message: TabMessage) {
        match message {
            TabMessage::UrlChanged(url) => self.url = url,
            TabMessage::MethodChanged(method) => self.method = method,
            TabMessage::SubTabSelected(sub_tab) => self.active_sub_tab = sub_tab,
            TabMessage::ResponseSubTabSelected(resp_tab) => self.active_response_tab = resp_tab,
            TabMessage::AuthChanged(auth) => self.request_auth = auth,
            TabMessage::BodyTypeChanged(body_type) => self.body_type = body_type,
            TabMessage::RawTypeChanged(raw_type) => self.raw_type = raw_type,
            TabMessage::ResponseViewChanged(view) => self.response_view = view,
            TabMessage::BodyChanged(action) => self.request_body.perform(action),

            TabMessage::ParamRowChanged(index, kv) => {
                if let Some(row) = self.request_params.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddParamRow => self.request_params.push(KeyValuePair::new("", "")),
            TabMessage::RemoveParamRow(index) => {
                if index < self.request_params.len() {
                    self.request_params.remove(index);
                }
            }

            TabMessage::HeaderRowChanged(index, kv) => {
                if let Some(row) = self.request_headers.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddHeaderRow => self.request_headers.push(KeyValuePair::new("", "")),
            TabMessage::RemoveHeaderRow(index) => {
                if index < self.request_headers.len() {
                    self.request_headers.remove(index);
                }
            }

            TabMessage::CookieRowChanged(index, kv) => {
                if let Some(row) = self.request_cookies.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddCookieRow => self.request_cookies.push(KeyValuePair::new("", "")),
            TabMessage::RemoveCookieRow(index) => {
                if index < self.request_cookies.len() {
                    self.request_cookies.remove(index);
                }
            }

            TabMessage::FormDataRowChanged(index, kv) => {
                if let Some(row) = self.body_form_data.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddFormDataRow => self.body_form_data.push(KeyValuePair::new("", "")),
            TabMessage::RemoveFormDataRow(index) => {
                if index < self.body_form_data.len() {
                    self.body_form_data.remove(index);
                }
            }

            TabMessage::UrlencodedRowChanged(index, kv) => {
                if let Some(row) = self.body_urlencoded.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddUrlencodedRow => self.body_urlencoded.push(KeyValuePair::new("", "")),
            TabMessage::RemoveUrlencodedRow(index) => {
                if index < self.body_urlencoded.len() {
                    self.body_urlencoded.remove(index);
                }
            }
            TabMessage::CancelRequest => {
                if self.is_loading {
                    self.cancel_token.cancel();
                }
            }
        }
    }
}
