use super::messages::{TabMessage, ValueField};
use super::types::{
    BodyType, FormDataRow, FormDataType, KeyValuePair, RawType, RequestSubTab, ResponseSubTab,
    ResponseView, SavedResponse,
};
use crate::collection::collection::{PostmanRequestDetails, PostmanRequestNode, PostmanUrl};
use crate::collection::env::Environment;
use crate::collection_adapter::{RequestNodeTabExt, sync_body_from_tab};
use crate::http_client::{HttpMethod, HttpResponse};
use crate::ui::resize_handle::{DividerOrientation, resize_handle};
use crate::ui::tab::types::ScriptTab;
use crate::ui::tab::views;
use crate::{APP_NAME, APP_VERSION};
use iced::widget::text_editor;
use iced::widget::{column, container};
use iced::{Element, Length};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct Tab {
    pub id: usize,
    pub collection_id: Option<usize>, // tracks the parent collection context
    pub request_id: Option<usize>,
    pub name: String,
    pub url: String,
    pub method: HttpMethod,
    pub active_sub_tab: RequestSubTab,
    pub active_response_tab: ResponseSubTab,
    pub body_type: BodyType,
    pub raw_type: RawType,
    pub response_view: ResponseView,
    pub request_params: Vec<KeyValuePair>,
    pub request_params_values: Vec<text_editor::Content>,
    pub request_headers: Vec<KeyValuePair>,
    pub request_headers_values: Vec<text_editor::Content>,
    pub request_cookies: Vec<KeyValuePair>,
    pub request_cookies_values: Vec<text_editor::Content>,
    pub request_auth: text_editor::Content,
    pub request_body: text_editor::Content,
    pub script_tab: ScriptTab,
    pub pre_request_script: text_editor::Content,
    pub post_response_script: text_editor::Content,
    pub body_form_data: Vec<FormDataRow>,
    pub body_form_data_values: Vec<text_editor::Content>,
    pub body_urlencoded: Vec<KeyValuePair>,
    pub body_urlencoded_values: Vec<text_editor::Content>,
    pub binary_file_path: Option<String>,
    pub response: Option<Result<HttpResponse, String>>,
    pub response_body_editor: text_editor::Content,
    /// saved response snapshots for this request, in the order they were saved.
    pub saved_responses: Vec<SavedResponse>,
    /// which response the response pane is currently displaying: the live
    /// response (`None`) or a saved snapshot by index into `saved_responses`.
    pub viewing_saved_response: Option<usize>,
    pub is_loading: bool,
    pub cancel_token: CancellationToken,
    /// true when this tab has edits that haven't been saved yet; drives the
    /// unsaved-changes dot shown in the sidebar and tab strip.
    pub dirty: bool,
}

impl Tab {
    pub fn new(id: usize) -> Self {
        let request_params = vec![KeyValuePair::new("", "")];
        let request_headers = vec![
            KeyValuePair::new("Content-Type", "application/json"),
            KeyValuePair::new("User-Agent", &format!("{}/{}", APP_NAME, APP_VERSION)),
            KeyValuePair::new("Accept", "*/*"),
            // KeyValuePair::new("Accept-Encoding", "gzip, deflate, br"),
            KeyValuePair::new("Connection", "keep-alive"),
        ];
        let request_cookies = vec![KeyValuePair::new("", "")];
        let body_form_data = vec![FormDataRow::new("form_field", "value", FormDataType::Text)];
        let body_urlencoded = vec![KeyValuePair::new("form_key", "form_value")];

        Self {
            id,
            collection_id: None, // Default to standalone request
            request_id: None,
            name: format!("Request {}", id),
            url: String::from("https://jsonplaceholder.typicode.com/todos/1"),
            method: HttpMethod::GET,
            active_sub_tab: RequestSubTab::Params,
            active_response_tab: ResponseSubTab::Body,
            body_type: BodyType::Raw,
            raw_type: RawType::Json,
            response_view: ResponseView::Json,
            request_params_values: contents_for(&request_params),
            request_params,
            request_headers_values: contents_for(&request_headers),
            request_headers,
            request_cookies_values: contents_for(&request_cookies),
            request_cookies,
            request_auth: text_editor::Content::with_text("Bearer your_token_here"),
            request_body: text_editor::Content::with_text("{\n  \"key\": \"value\"\n}"),
            script_tab: ScriptTab::PreRequest,
            pre_request_script: text_editor::Content::with_text(
                "// Executed before the request is sent\n// e.g. pm.environment.set(\"timestamp\", Date.now());",
            ),
            post_response_script: text_editor::Content::with_text(
                "// Executed after receiving a response\n// e.g. pm.test(\"Status code is 200\", function () {\n//     pm.response.to.have.status(200);\n// });",
            ),
            body_form_data_values: contents_for_form_data(&body_form_data),
            body_form_data,
            body_urlencoded_values: contents_for(&body_urlencoded),
            body_urlencoded,
            binary_file_path: None,
            response: None,
            saved_responses: Vec::new(),
            viewing_saved_response: None,
            is_loading: false,
            cancel_token: CancellationToken::new(),
            response_body_editor: text_editor::Content::with_text(""),
            dirty: false,
        }
    }

    pub fn view<Message>(
        &self,
        wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
        on_send: Message,
        request_pane_height: f32,
        on_resize_start: Message,
    ) -> Element<'_, Message>
    where
        Message: Clone + 'static,
    {
        let request_bar = views::request::render_request_bar(self, wrap_msg, on_send);
        let configuration_pane = views::request::render_configuration_pane(self, wrap_msg);
        let response_content = views::response::render_response_pane(self, wrap_msg);

        column![
            request_bar,
            container(configuration_pane).height(Length::Fixed(request_pane_height)),
            resize_handle(DividerOrientation::Horizontal, on_resize_start),
            container(response_content)
                .height(Length::Fill)
                .width(Length::Fill)
                .padding(15)
                .style(container::bordered_box)
        ]
        .height(Length::Fill)
        .width(Length::Fill)
        .spacing(10)
        .into()
    }

    pub fn to_postman_request_node(&self, req_id: usize, name: &str) -> PostmanRequestNode {
        let mut node = PostmanRequestNode {
            id: req_id,
            name: name.to_string(),
            request: PostmanRequestDetails {
                method: self.method.to_string(),
                url: Some(PostmanUrl::String(self.url.clone())),
                header: None,
                body: None,
            },
            event: None,
            unsaved: false,
            response: None,
        };

        node.update_from_tab(self);
        sync_body_from_tab(&mut node, self);
        node.id = req_id;
        node.name = name.to_string();
        node
    }

    /// rebuilds `request_params` from the query string of `self.url`
    pub fn sync_params_from_url(&mut self) {
        if let Ok(parsed_url) = url::Url::parse(&self.url)
            .or_else(|_| url::Url::parse(&format!("http://localhost/{}", self.url)))
        {
            let inactive_params: Vec<(String, String)> = self
                .request_params
                .iter()
                .filter(|p| !p.is_active)
                .map(|p| (p.key.clone(), p.value.clone()))
                .collect();

            self.request_params.clear();
            for (key, value) in parsed_url.query_pairs() {
                let k = key.into_owned();
                let v = value.into_owned();

                let is_active = !inactive_params.iter().any(|(ik, iv)| ik == &k && iv == &v);

                let mut kv = KeyValuePair::new(&k, &v);
                kv.is_active = is_active;
                self.request_params.push(kv);
            }

            if self.request_params.is_empty() || !self.request_params.last().unwrap().key.is_empty()
            {
                self.request_params.push(KeyValuePair::new("", ""));
            }

            self.request_params_values = contents_for(&self.request_params);
        }
    }

    pub fn update(&mut self, message: TabMessage) {
        if message.is_content_edit() {
            self.dirty = true;
        }
        match message {
            TabMessage::UrlChanged(new_url) => {
                self.url = new_url;
                self.sync_params_from_url();
            }

            TabMessage::ParamRowChanged(index, kv) => {
                if let Some(row) = self.request_params.get_mut(index) {
                    *row = kv;
                }
                self.url = sync_params_to_url(&self.url, &self.request_params);
            }

            TabMessage::RemoveParamRow(index) => {
                if index < self.request_params.len() {
                    self.request_params.remove(index);
                }
                if index < self.request_params_values.len() {
                    self.request_params_values.remove(index);
                }
                self.url = sync_params_to_url(&self.url, &self.request_params);
            }

            TabMessage::AddParamRow => {
                self.request_params.push(KeyValuePair::new("", ""));
                self.request_params_values.push(text_editor::Content::new());
            }

            TabMessage::MethodChanged(method) => self.method = method,

            TabMessage::MethodSelected(method_str) => {
                self.method = match method_str.to_uppercase().trim() {
                    "GET" => HttpMethod::GET,
                    "POST" => HttpMethod::POST,
                    "PUT" => HttpMethod::PUT,
                    "DELETE" => HttpMethod::DELETE,
                    "PATCH" => HttpMethod::PATCH,
                    "HEAD" => HttpMethod::HEAD,
                    "OPTIONS" => HttpMethod::OPTIONS,
                    custom => HttpMethod::Custom(custom.to_string()),
                };
            }

            TabMessage::SubTabSelected(sub_tab) => self.active_sub_tab = sub_tab,
            TabMessage::ResponseSubTabSelected(resp_tab) => self.active_response_tab = resp_tab,
            TabMessage::AuthChanged(action) => self.request_auth.perform(action),
            TabMessage::BodyTypeChanged(body_type) => self.body_type = body_type,
            TabMessage::RawTypeChanged(raw_type) => self.raw_type = raw_type,
            TabMessage::ResponseViewChanged(view) => self.response_view = view,
            TabMessage::BodyChanged(action) => self.request_body.perform(action),

            TabMessage::ScriptTabChanged(script_tab) => {
                self.script_tab = script_tab;
            }
            TabMessage::PreRequestScriptChanged(action) => {
                self.pre_request_script.perform(action);
            }
            TabMessage::PostResponseScriptChanged(action) => {
                self.post_response_script.perform(action);
            }

            TabMessage::HeaderRowChanged(index, kv) => {
                if let Some(row) = self.request_headers.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddHeaderRow => {
                self.request_headers.push(KeyValuePair::new("", ""));
                self.request_headers_values
                    .push(text_editor::Content::new());
            }
            TabMessage::RemoveHeaderRow(index) => {
                if index < self.request_headers.len() {
                    self.request_headers.remove(index);
                }
                if index < self.request_headers_values.len() {
                    self.request_headers_values.remove(index);
                }
            }

            TabMessage::CookieRowChanged(index, kv) => {
                if let Some(row) = self.request_cookies.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddCookieRow => {
                self.request_cookies.push(KeyValuePair::new("", ""));
                self.request_cookies_values
                    .push(text_editor::Content::new());
            }
            TabMessage::RemoveCookieRow(index) => {
                if index < self.request_cookies.len() {
                    self.request_cookies.remove(index);
                }
                if index < self.request_cookies_values.len() {
                    self.request_cookies_values.remove(index);
                }
            }
            TabMessage::ResponseBodyEditorAction(action) => {
                if let iced::widget::text_editor::Action::Edit(_) = action {
                } else {
                    self.response_body_editor.perform(action);
                }
            }

            TabMessage::FormDataRowChanged(index, updated_row) => {
                if let Some(row) = self.body_form_data.get_mut(index) {
                    *row = updated_row;
                }
            }
            TabMessage::AddFormDataRow => {
                self.body_form_data
                    .push(FormDataRow::new("", "", FormDataType::Text));
                self.body_form_data_values.push(text_editor::Content::new());
            }
            TabMessage::RemoveFormDataRow(index) => {
                if index < self.body_form_data.len() {
                    self.body_form_data.remove(index);
                }
                if index < self.body_form_data_values.len() {
                    self.body_form_data_values.remove(index);
                }
            }
            TabMessage::FormDataRowTypeChanged(index, new_type) => {
                if let Some(row) = self.body_form_data.get_mut(index) {
                    row.field_type = new_type;
                    row.value.clear();
                }
                if let Some(content) = self.body_form_data_values.get_mut(index) {
                    *content = text_editor::Content::new();
                }
            }

            TabMessage::UrlencodedRowChanged(index, kv) => {
                if let Some(row) = self.body_urlencoded.get_mut(index) {
                    *row = kv;
                }
            }
            TabMessage::AddUrlencodedRow => {
                self.body_urlencoded.push(KeyValuePair::new("", ""));
                self.body_urlencoded_values
                    .push(text_editor::Content::new());
            }
            TabMessage::RemoveUrlencodedRow(index) => {
                if index < self.body_urlencoded.len() {
                    self.body_urlencoded.remove(index);
                }
                if index < self.body_urlencoded_values.len() {
                    self.body_urlencoded_values.remove(index);
                }
            }

            TabMessage::ValueEditorAction(field, index, action) => {
                let contents: &mut Vec<text_editor::Content> = match field {
                    ValueField::Param => &mut self.request_params_values,
                    ValueField::Header => &mut self.request_headers_values,
                    ValueField::Cookie => &mut self.request_cookies_values,
                    ValueField::Urlencoded => &mut self.body_urlencoded_values,
                    ValueField::FormData => &mut self.body_form_data_values,
                };
                if let Some(content) = contents.get_mut(index) {
                    content.perform(action);
                    let text = content.text();
                    match field {
                        ValueField::Param => {
                            if let Some(row) = self.request_params.get_mut(index) {
                                row.value = text;
                            }
                            self.url = sync_params_to_url(&self.url, &self.request_params);
                        }
                        ValueField::Header => {
                            if let Some(row) = self.request_headers.get_mut(index) {
                                row.value = text;
                            }
                        }
                        ValueField::Cookie => {
                            if let Some(row) = self.request_cookies.get_mut(index) {
                                row.value = text;
                            }
                        }
                        ValueField::Urlencoded => {
                            if let Some(row) = self.body_urlencoded.get_mut(index) {
                                row.value = text;
                            }
                        }
                        ValueField::FormData => {
                            if let Some(row) = self.body_form_data.get_mut(index) {
                                row.value = text;
                            }
                        }
                    }
                }
            }

            TabMessage::SelectBinaryFile => {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.binary_file_path = Some(path.display().to_string());
                }
            }
            TabMessage::BinaryFileSelected(path) => {
                self.binary_file_path = Some(path);
            }
            TabMessage::SelectFormDataFile(index) => {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    if let Some(row) = self.body_form_data.get_mut(index) {
                        row.value = path.display().to_string();
                    }
                }
            }

            TabMessage::CancelRequest => {
                if self.is_loading {
                    self.cancel_token.cancel();
                }
            }

            TabMessage::SaveResponse => {
                if let Some(Ok(resp)) = &self.response {
                    self.saved_responses.push(SavedResponse {
                        name: format!("Response {}", self.saved_responses.len() + 1),
                        status: resp.status,
                        body: resp.body.clone(),
                        headers: resp.headers.clone(),
                        elapsed_ms: resp.elapsed.as_millis(),
                    });
                }
            }
            TabMessage::ViewSavedResponse(index) => {
                self.viewing_saved_response = index;
            }
            TabMessage::DeleteSavedResponse(index) => {
                if index < self.saved_responses.len() {
                    self.saved_responses.remove(index);
                }
                self.viewing_saved_response = match self.viewing_saved_response {
                    Some(current) if current == index => None,
                    Some(current) if current > index => Some(current - 1),
                    other => other,
                };
            }

            // handled at the app level (needs the global cursor position), before
            // this message ever reaches `Tab::update`
            TabMessage::ShowFieldContextMenu(..) => {}
        }
    }

    pub fn compile_request_fields(
        &self,
        env: &Option<Environment>,
        collection_vars: Option<&[KeyValuePair]>, // fallback variables parsed from the Postman Collection
    ) -> (
        String,                // URL
        String,                // Raw Body
        Vec<FormDataRow>,      // Form Data
        Vec<(String, String)>, // Headers
        Vec<(String, String)>, // Cookies
        String,                // Auth
    ) {
        let resolve = |val: &str| -> String {
            if let Some(e) = env {
                // pass collection variables into the environment to allow tiered variable parsing
                e.replace_vars(val, collection_vars)
            } else if let Some(col_vars) = collection_vars {
                // standalone environment handler context fallback
                let mut output = val.to_string();
                for var in col_vars {
                    if var.is_active && !var.key.trim().is_empty() {
                        let placeholder = format!("{{{{{}}}}}", var.key.trim());
                        output = output.replace(&placeholder, &var.value);
                    }
                }
                output
            } else {
                val.to_string()
            }
        };

        let resolved_url = resolve(&self.url);
        let resolved_auth = resolve(&self.request_auth.text());
        let mut resolved_body = resolve(&self.request_body.text());

        // strip out comment lines if the body is Raw JSON
        if self.body_type == BodyType::Raw && self.raw_type == RawType::Json {
            resolved_body = strip_json_comments(&resolved_body);
        }

        let resolved_headers = self
            .request_headers
            .iter()
            .filter(|h| h.is_active)
            .map(|h| (resolve(&h.key), resolve(&h.value)))
            .collect();

        let resolved_cookies = self
            .request_cookies
            .iter()
            .filter(|c| c.is_active)
            .map(|c| (resolve(&c.key), resolve(&c.value)))
            .collect();

        let resolved_form_data = self
            .body_form_data
            .iter()
            .map(|row| FormDataRow {
                is_active: row.is_active,
                key: resolve(&row.key),
                value: resolve(&row.value),
                field_type: row.field_type,
            })
            .collect();

        (
            resolved_url,
            resolved_body,
            resolved_form_data,
            resolved_headers,
            resolved_cookies,
            resolved_auth,
        )
    }
}

/// builds a multiline "Value" editor per pair, aligned by index; used
/// whenever a `Vec<KeyValuePair>` is replaced wholesale (row list rebuilt
/// from the URL, or loaded from an imported request) so the editors stay in
/// sync with the data they display.
pub fn contents_for(pairs: &[KeyValuePair]) -> Vec<text_editor::Content> {
    pairs
        .iter()
        .map(|p| text_editor::Content::with_text(&p.value))
        .collect()
}

/// same as [`contents_for`], for form-data rows.
pub fn contents_for_form_data(rows: &[FormDataRow]) -> Vec<text_editor::Content> {
    rows.iter()
        .map(|r| text_editor::Content::with_text(&r.value))
        .collect()
}

fn sync_params_to_url(url_str: &str, params: &[KeyValuePair]) -> String {
    let mut parsed_url = match url::Url::parse(url_str) {
        Ok(u) => u,
        Err(_) => return url_str.to_string(),
    };

    parsed_url.set_query(None);
    let mut query_serializer = parsed_url.query_pairs_mut();

    for pair in params {
        if pair.is_active && !pair.key.is_empty() {
            query_serializer.append_pair(&pair.key, &pair.value);
        }
    }

    drop(query_serializer);
    parsed_url.to_string()
}

// helper function to strip line comments (`//`) and block comments (`/* */`) from a JSON payload string.
fn strip_json_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    let mut in_string = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                result.push('\n');
            }
            continue;
        }

        if in_block_comment {
            if c == '*' {
                if let Some(&'/') = chars.peek() {
                    chars.next(); // consume '/'
                    in_block_comment = false;
                }
            }
            continue;
        }

        if in_string {
            if c == '"' && !escaped {
                in_string = false;
            }
            escaped = c == '\\' && !escaped;
            result.push(c);
            continue;
        }

        if c == '"' {
            in_string = true;
            result.push(c);
        } else if c == '/' {
            match chars.peek() {
                Some(&'/') => {
                    chars.next(); // consume '/'
                    in_line_comment = true;
                }
                Some(&'*') => {
                    chars.next(); // consume '*'
                    in_block_comment = true;
                }
                _ => {
                    result.push(c);
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}
