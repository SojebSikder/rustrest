use super::messages::{TabMessage, ValueField};
use super::types::{
    BodyType, FormDataRow, FormDataType, KeyValuePair, RawType, RequestSubTab, ResponseSubTab,
    ResponseView, SavedResponse,
};
use crate::collection::collection::{
    PostmanRequestDetails, PostmanRequestNode, PostmanUrl, path_variable_names,
    substitute_path_variables,
};
use crate::collection::env::{Environment, replace_vars_in};
use crate::collection_adapter::{RequestNodeTabExt, sync_body_from_tab};
use crate::http_client::{HttpMethod, HttpResponse};
use crate::message::MultilineFieldKind;
use crate::ui::resize_handle::{DividerOrientation, resize_handle};
use crate::ui::script_editor::ScriptContent;
use crate::ui::tab::types::ScriptTab;
use crate::ui::tab::views;
use crate::ui::tokens::{SPACE_LG, SPACE_MD, editor_surface};
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
    pub graphql_query: text_editor::Content,
    pub graphql_variables: text_editor::Content,
    pub response_view: ResponseView,
    pub request_params: Vec<KeyValuePair>,
    pub request_params_values: Vec<text_editor::Content>,
    /// one row per `:name` segment in the URL's path, kept in URL order;
    /// rebuilt whenever the URL changes, preserving values by key.
    pub path_variables: Vec<KeyValuePair>,
    pub path_variables_values: Vec<text_editor::Content>,
    pub request_headers: Vec<KeyValuePair>,
    pub request_headers_values: Vec<text_editor::Content>,
    pub request_cookies: Vec<KeyValuePair>,
    pub request_cookies_values: Vec<text_editor::Content>,
    pub request_auth: super::auth_form::AuthFormState,
    pub request_body: text_editor::Content,
    pub script_tab: ScriptTab,
    pub pre_request_script: ScriptContent,
    pub post_response_script: ScriptContent,
    pub body_form_data: Vec<FormDataRow>,
    pub body_form_data_values: Vec<text_editor::Content>,
    pub body_urlencoded: Vec<KeyValuePair>,
    pub body_urlencoded_values: Vec<text_editor::Content>,
    pub binary_file_path: Option<String>,
    pub response: Option<Result<HttpResponse, String>>,
    /// what the response body editor shows: `response_fold`'s text with collapsed regions shortened
    pub response_body_editor: text_editor::Content,
    /// full formatted response body plus its JSON fold state
    pub response_fold: super::json_fold::JsonFold,
    /// last scroll position of the response body's horizontal scrollable,
    /// so switching tabs and coming back restores it instead of jumping back.
    pub response_scroll: iced::widget::operation::AbsoluteOffset,
    /// the response editor scrolls itself vertically. this mirrors where it
    /// is so the fold gutter can line up with the text
    pub response_editor_scroll: super::response_scroll::EditorScroll,
    /// an empty editor content that the response pane keeps laid out, ready
    /// to take the next response body (see `replace_response_editor`)
    pub response_editor_spare: text_editor::Content,
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
    /// true once the current response has been detected as
    /// `text/event-stream`; while set, the Body tab shows `sse_log` instead of the plain response body editor.
    pub sse_active: bool,
    pub sse_log: Vec<crate::ui::tab::protocol_common::LogEntry>,
    /// the request's markdown documentation (Docs sub-tab)
    pub docs: crate::ui::docs_view::MarkdownDoc,
}

/// lines per paste in `fill_content`. cosmic-text inserts a paste's lines
/// one `Vec::insert` at a time, so a single paste is quadratic in its line count.
/// small chunks keep it close to linear.
const FILL_CHUNK_LINES: usize = 64;

/// puts `text` into an empty `content` without laying any of it out, and
/// leaves the cursor (and so the scroll position) at the top.
fn fill_content(content: &mut text_editor::Content, text: &str) {
    use text_editor::{Action, Cursor, Edit, Position};

    let mut rest = text;
    while !rest.is_empty() {
        // cut after a '\n' so a "\r\n" pair never straddles two pastes
        let cut = rest
            .match_indices('\n')
            .nth(FILL_CHUNK_LINES - 1)
            .map_or(rest.len(), |(i, _)| i + 1);
        let (chunk, tail) = rest.split_at(cut);
        content.perform(Action::Edit(Edit::Paste(std::sync::Arc::new(
            chunk.to_string(),
        ))));
        rest = tail;
    }
    content.move_to(Cursor {
        position: Position { line: 0, column: 0 },
        selection: None,
    });
}

impl Tab {
    /// replaces the response body text, resetting folds. `json` enables
    /// folding (the JSON view of a body that parsed as JSON).
    pub fn set_response_text(&mut self, text: &str, json: bool) {
        let foldable = json && !text.starts_with("// Invalid JSON");
        self.response_fold = super::json_fold::JsonFold::new(text, foldable);
        self.replace_response_editor(text);
    }

    /// swaps in a response editor showing `text`.
    fn replace_response_editor(&mut self, text: &str) {
        let mut content =
            std::mem::replace(&mut self.response_editor_spare, text_editor::Content::new());
        fill_content(&mut content, text);
        self.response_body_editor = content;
        self.response_editor_scroll.reset();
    }

    /// applies a fold change by rewriting only the rows that differ, as one edit.
    fn apply_fold_change(&mut self, change: impl FnOnce(&mut super::json_fold::JsonFold)) {
        use text_editor::{Action, Cursor, Edit, Position};

        let old = self.response_fold.display_lines().to_vec();
        change(&mut self.response_fold);
        let new = self.response_fold.display_lines();

        let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let max_suffix = old.len().min(new.len()) - prefix;
        let suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(max_suffix)
            .take_while(|(a, b)| a == b)
            .count();
        let old_rows = &old[prefix..old.len() - suffix];
        let new_rows = &new[prefix..new.len() - suffix];

        let (Some(last_old), false) = (old_rows.last(), new_rows.is_empty()) else {
            if old_rows.len() != new_rows.len() || !old_rows.is_empty() {
                self.replace_response_editor(&self.response_fold.display_text());
            }
            return;
        };

        // columns are byte offsets
        self.response_body_editor.move_to(Cursor {
            position: Position {
                line: prefix + old_rows.len() - 1,
                column: self.response_fold.row_text(last_old).len(),
            },
            selection: Some(Position {
                line: prefix,
                column: 0,
            }),
        });
        let replacement = self.response_fold.rows_text(new_rows);
        self.response_body_editor
            .perform(Action::Edit(Edit::Paste(std::sync::Arc::new(replacement))));
        self.response_body_editor.move_to(Cursor {
            position: Position {
                line: prefix,
                column: 0,
            },
            selection: None,
        });
        self.response_editor_scroll
            .follow_cursor(prefix, self.response_body_editor.line_count());
    }

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
            graphql_query: text_editor::Content::with_text("query {\n  \n}"),
            graphql_variables: text_editor::Content::with_text("{}"),
            response_view: ResponseView::Json,
            request_params_values: contents_for(&request_params),
            request_params,
            path_variables: Vec::new(),
            path_variables_values: Vec::new(),
            request_headers_values: contents_for(&request_headers),
            request_headers,
            request_cookies_values: contents_for(&request_cookies),
            request_cookies,
            request_auth: super::auth_form::AuthFormState::default(),
            request_body: text_editor::Content::with_text("{\n  \"key\": \"value\"\n}"),
            script_tab: ScriptTab::PreRequest,
            pre_request_script: ScriptContent::with_text(
                "// Executed before the request is sent\n// e.g. pm.environment.set(\"timestamp\", Date.now());",
            ),
            post_response_script: ScriptContent::with_text(
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
            response_fold: Default::default(),
            response_scroll: Default::default(),
            response_editor_scroll: Default::default(),
            response_editor_spare: text_editor::Content::new(),
            dirty: false,
            sse_active: false,
            sse_log: Vec::new(),
            docs: crate::ui::docs_view::MarkdownDoc::new(""),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn view<'a, Message>(
        &'a self,
        wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
        on_send: Message,
        request_pane_height: f32,
        on_resize_start: Message,
        multiline_height: impl Fn(MultilineFieldKind) -> f32 + Copy + 'a,
        on_multiline_resize_start: impl Fn(MultilineFieldKind) -> Message + Copy + 'a,
        spinner_tick: u64,
        theme: &iced::Theme,
        show_form_data_content_type: bool,
    ) -> Element<'a, Message>
    where
        Message: Clone + 'static,
    {
        let request_bar = views::request::render_request_bar(self, wrap_msg, on_send);
        let configuration_pane = views::request::render_configuration_pane(
            self,
            wrap_msg,
            multiline_height,
            on_multiline_resize_start,
            spinner_tick,
            theme,
            show_form_data_content_type,
        );
        let response_content = views::response::render_response_pane(self, wrap_msg);

        column![
            request_bar,
            container(configuration_pane).height(Length::Fixed(request_pane_height)),
            resize_handle(DividerOrientation::Horizontal, on_resize_start),
            container(response_content)
                .height(Length::Fill)
                .width(Length::Fill)
                .padding(SPACE_LG)
                .style(editor_surface)
        ]
        .height(Length::Fill)
        .width(Length::Fill)
        .spacing(SPACE_MD)
        .into()
    }

    pub fn to_postman_request_node(&self, req_id: usize, name: &str) -> PostmanRequestNode {
        let mut node = PostmanRequestNode {
            id: req_id,
            uid: None,
            name: name.to_string(),
            request: PostmanRequestDetails {
                method: self.method.to_string(),
                url: Some(PostmanUrl::String(self.url.clone())),
                header: None,
                body: None,
                auth: None,
                description: None,
            },
            event: None,
            unsaved: false,
            response: None,
            protocol_request: None,
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

    /// rebuilds `path_variables` from the `:name` segments of `self.url`,
    /// keeping the value of any variable that was already there.
    pub fn sync_path_variables_from_url(&mut self) {
        self.rebuild_path_variables(false);
    }

    /// sets path variable values by key (e.g. from a saved request), then
    /// aligns the rows with the URL.
    pub fn load_path_variables(&mut self, values: impl IntoIterator<Item = (String, String)>) {
        self.path_variables = values
            .into_iter()
            .map(|(key, value)| KeyValuePair::new(&key, &value))
            .collect();
        self.rebuild_path_variables(true);
    }

    fn rebuild_path_variables(&mut self, force: bool) {
        let names = path_variable_names(&self.url);
        let unchanged = names.len() == self.path_variables.len()
            && names
                .iter()
                .zip(&self.path_variables)
                .all(|(name, kv)| *name == kv.key);
        if unchanged && !force {
            // keeps the value editors (and their cursors) intact while typing
            return;
        }
        let previous = std::mem::take(&mut self.path_variables);

        self.path_variables = names
            .iter()
            .map(|name| {
                let value = previous
                    .iter()
                    .find(|kv| kv.key == *name)
                    .map_or("", |kv| kv.value.as_str());
                KeyValuePair::new(name, value)
            })
            .collect();
        self.path_variables_values = contents_for(&self.path_variables);
    }

    pub fn update(&mut self, message: TabMessage) {
        if message.is_content_edit() {
            self.dirty = true;
        }
        match message {
            TabMessage::UrlChanged(new_url) => {
                self.url = new_url;
                self.sync_params_from_url();
                self.sync_path_variables_from_url();
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
            TabMessage::Auth(auth_msg) => self.request_auth.update(auth_msg),
            TabMessage::BodyTypeChanged(body_type) => self.body_type = body_type,
            TabMessage::RawTypeChanged(raw_type) => self.raw_type = raw_type,
            TabMessage::ResponseViewChanged(view) => self.response_view = view,
            TabMessage::BodyChanged(action) => self.request_body.perform(action),
            TabMessage::BeautifyBody => {
                if let Some(pretty) = crate::utils::beautify_json(&self.request_body.text()) {
                    use text_editor::{Action, Edit};
                    self.request_body.perform(Action::SelectAll);
                    self.request_body
                        .perform(Action::Edit(Edit::Paste(std::sync::Arc::new(pretty))));
                }
            }
            TabMessage::GraphQlQueryAction(action) => self.graphql_query.perform(action),
            TabMessage::GraphQlVariablesAction(action) => self.graphql_variables.perform(action),

            TabMessage::ScriptTabChanged(script_tab) => {
                self.script_tab = script_tab;
            }
            TabMessage::PreRequestScriptChanged(event) => {
                self.pre_request_script.update(event);
            }
            TabMessage::PostResponseScriptChanged(event) => {
                self.post_response_script.update(event);
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
            TabMessage::ResponseBodyScrolled(offset) => self.response_scroll = offset,
            TabMessage::ToggleResponseFold(line) => {
                self.apply_fold_change(|fold| fold.toggle(line));
            }
            TabMessage::ExpandAllResponseFolds => {
                self.apply_fold_change(|fold| fold.expand_all());
            }
            TabMessage::CollapseAllResponseFolds => {
                self.apply_fold_change(|fold| fold.collapse_all());
            }
            TabMessage::ResponseBodyEditorAction(action) => {
                if action.is_edit() {
                    return;
                }
                let scrolled_lines = match action {
                    text_editor::Action::Scroll { lines } => Some(lines),
                    _ => None,
                };
                let cursor_before = self.response_body_editor.cursor().position;
                self.response_body_editor.perform(action);

                let total_lines = self.response_body_editor.line_count();
                let cursor = self.response_body_editor.cursor().position;
                if let Some(lines) = scrolled_lines {
                    self.response_editor_scroll.scroll_lines(lines, total_lines);
                } else if cursor != cursor_before {
                    self.response_editor_scroll
                        .follow_cursor(cursor.line, total_lines);
                }
            }

            TabMessage::FormDataRowChanged(index, updated_row) => {
                if let Some(row) = self.body_form_data.get_mut(index) {
                    *row = updated_row;
                }
            }
            TabMessage::ToggleFormDataContentType => {}
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
                    row.files.clear();
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
                    ValueField::PathVariable => &mut self.path_variables_values,
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
                        ValueField::PathVariable => {
                            if let Some(row) = self.path_variables.get_mut(index) {
                                row.value = text;
                            }
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
                // appends, so files from different folders can share one key
                if let Some(paths) = rfd::FileDialog::new().pick_files()
                    && let Some(row) = self.body_form_data.get_mut(index)
                {
                    for path in paths {
                        let path = path.display().to_string();
                        if !row.files.contains(&path) {
                            row.files.push(path);
                        }
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
                        timings: resp.timings,
                        request_size: resp.request_size,
                        response_size: resp.response_size,
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
            TabMessage::ShowResponseTimingModal(_) => {}
            TabMessage::GenerateCode => {}
            TabMessage::CopyToClipboard(_) => {}
            TabMessage::DocsLinkClicked(_) => {}

            TabMessage::DocsAction(action) => {
                self.docs.perform(action);
            }
            TabMessage::DocsModeSelected(mode) => self.docs.mode = mode,
        }
    }

    pub fn compile_request_fields(
        &self,
        env: &Option<Environment>,
        collection_vars: Option<&[KeyValuePair]>, // fallback variables parsed from the Postman Collection
        collection_auth: Option<&rustrest_core::RequestAuth>, // used when this request inherits auth
    ) -> Result<
        (
            String,                // URL
            String,                // Raw Body
            Vec<FormDataRow>,      // Form Data
            Vec<(String, String)>, // Headers
            Vec<(String, String)>, // Cookies
        ),
        String,
    > {
        let resolve = |val: &str| -> String {
            if let Some(e) = env {
                // pass collection variables into the environment to allow tiered variable parsing
                e.replace_vars(val, collection_vars)
            } else if let Some(col_vars) = collection_vars {
                // standalone environment handler context fallback
                replace_vars_in(val, &[col_vars])
            } else {
                val.to_string()
            }
        };

        // path variables first, so their values can use `{{vars}}` too.
        // an empty value leaves `:name` in place
        let url_with_path_vars = substitute_path_variables(&self.url, |name| {
            self.path_variables
                .iter()
                .find(|kv| kv.key == name && !kv.value.is_empty())
                .map(|kv| kv.value.clone())
        });
        let mut resolved_url = resolve(&url_with_path_vars);
        let mut resolved_body = resolve(&self.request_body.text());

        // strip out comment lines if the body is Raw JSON
        if self.body_type == BodyType::Raw && self.raw_type == RawType::Json {
            resolved_body = strip_json_comments(&resolved_body);
        } else if self.body_type == BodyType::GraphQl {
            let query = resolve(&self.graphql_query.text());
            let variables = resolve(&self.graphql_variables.text());
            resolved_body = build_graphql_body(&query, &variables);
        }

        let mut resolved_headers: Vec<(String, String)> = self
            .request_headers
            .iter()
            .filter(|h| h.is_active)
            .map(|h| (resolve(&h.key), resolve(&h.value)))
            .collect();

        if self.body_type == BodyType::XWwwFormUrlencoded {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());

            for row in self
                .body_urlencoded
                .iter()
                .filter(|r| r.is_active && !r.key.is_empty())
            {
                serializer.append_pair(&resolve(&row.key), &resolve(&row.value));
            }

            resolved_body = serializer.finish();
            let has_content_type = resolved_headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));

            if !has_content_type && !resolved_body.is_empty() {
                resolved_headers.push((
                    "Content-Type".to_string(),
                    "application/x-www-form-urlencoded".to_string(),
                ));
            }
        }

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
                files: row.files.iter().map(|f| resolve(f)).collect(),
                content_type: resolve(&row.content_type),
            })
            .collect();

        // every secret-ish auth field gets the same {{var}} interpolation
        // as headers/body before being applied.
        let auth_form = self
            .request_auth
            .to_core()
            .resolve_inherited(collection_auth);
        let resolved_auth = rustrest_core::RequestAuth {
            custom_raw: resolve(&auth_form.custom_raw),
            bearer_token: resolve(&auth_form.bearer_token),
            api_key_key: resolve(&auth_form.api_key_key),
            api_key_value: resolve(&auth_form.api_key_value),
            basic_username: resolve(&auth_form.basic_username),
            basic_password: resolve(&auth_form.basic_password),
            jwt_secret: resolve(&auth_form.jwt_secret),
            jwt_payload: resolve(&auth_form.jwt_payload),
            oauth1_consumer_key: resolve(&auth_form.oauth1_consumer_key),
            oauth1_consumer_secret: resolve(&auth_form.oauth1_consumer_secret),
            oauth1_token: resolve(&auth_form.oauth1_token),
            oauth1_token_secret: resolve(&auth_form.oauth1_token_secret),
            oauth2_access_token: resolve(&auth_form.oauth2_access_token),
            oauth2_client_id: resolve(&auth_form.oauth2_client_id),
            oauth2_client_secret: resolve(&auth_form.oauth2_client_secret),
            oauth2_scope: resolve(&auth_form.oauth2_scope),
            oauth2_token_url: resolve(&auth_form.oauth2_token_url),
            ..auth_form
        };

        let applied = resolved_auth.apply(&self.method.to_string(), &resolved_url)?;
        resolved_headers.extend(applied.headers);
        if !applied.query_params.is_empty() {
            resolved_url = append_query_params(&resolved_url, &applied.query_params);
        }

        Ok((
            resolved_url,
            resolved_body,
            resolved_form_data,
            resolved_headers,
            resolved_cookies,
        ))
    }
}

/// appends extra query params onto an already-resolved URL (used for
/// query-placed auth, e.g. an API key sent as `?api_key=...`). Leaves a
/// malformed URL untouched - `send_request` will surface the real parse
/// error once it tries to actually send it.
fn append_query_params(url_str: &str, params: &[(String, String)]) -> String {
    match url::Url::parse(url_str) {
        Ok(mut parsed) => {
            {
                let mut query = parsed.query_pairs_mut();
                for (key, value) in params {
                    query.append_pair(key, value);
                }
            }
            parsed.to_string()
        }
        Err(_) => url_str.to_string(),
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

/// rewrites the query string of `url_str` from `params`. works on the raw
/// text rather than through `url::Url`, which would percent-encode `{{var}}`
/// placeholders and rejects `{{base_url}}/...`.
fn sync_params_to_url(url_str: &str, params: &[KeyValuePair]) -> String {
    let (without_fragment, fragment) = match url_str.find('#') {
        Some(i) => url_str.split_at(i),
        None => (url_str, ""),
    };
    let base = without_fragment
        .split_once('?')
        .map_or(without_fragment, |(base, _)| base);

    let query = params
        .iter()
        .filter(|p| p.is_active && !p.key.is_empty())
        .map(|p| {
            format!(
                "{}={}",
                encode_query_component(&p.key, true),
                encode_query_component(&p.value, false)
            )
        })
        .collect::<Vec<_>>()
        .join("&");

    if query.is_empty() {
        format!("{base}{fragment}")
    } else {
        format!("{base}?{query}{fragment}")
    }
}

/// escapes only what would change how `sync_params_from_url` splits and
/// decodes the query, leaving everything else (notably `{{var}}`) readable.
fn encode_query_component(s: &str, is_key: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '%' => out.push_str("%25"),
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            '+' => out.push_str("%2B"),
            ' ' => out.push_str("%20"),
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            '=' if is_key => out.push_str("%3D"),
            _ => out.push(c),
        }
    }
    out
}

/// builds the `{"query": ..., "variables": ...}` payload GraphQL-over-HTTP
/// expects, parsing `variables_text` as JSON (falling back to `null` if it's
/// blank or invalid, rather than failing the request over a typo).
fn build_graphql_body(query: &str, variables_text: &str) -> String {
    let variables_value: serde_json::Value = if variables_text.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(variables_text).unwrap_or(serde_json::Value::Null)
    };

    serde_json::json!({ "query": query, "variables": variables_value }).to_string()
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
