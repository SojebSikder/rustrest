use crate::app::WorkspaceContent;
use crate::collection::collection::{
    GraphQlRequestDetails, GrpcRequestDetails, PostmanBody, PostmanBodyRow, PostmanEvent,
    PostmanGraphQlBody, PostmanHeader, PostmanRequestDetails, PostmanRequestNode,
    PostmanResponseExample, PostmanScript, PostmanScriptExec, PostmanUrl, ProtocolRequestDetails,
    WebSocketRequestDetails,
};
use crate::http_client::HttpMethod;
use crate::ui::tab::Tab;
use crate::ui::tab::graphql::GraphQlTabState;
use crate::ui::tab::grpc::GrpcTabState;
use crate::ui::tab::types::{
    BodyType, FormDataRow, FormDataType, KeyValuePair, RequestSubTab, SavedResponse,
};
use crate::ui::tab::ws::WsTabState;
use iced::widget::text_editor;
use std::path::PathBuf;

pub trait RequestNodeTabExt {
    /// updates this collection request node from a live UI tab, including pre-request & test scripts.
    fn update_from_tab(&mut self, tab: &Tab);
}

/// converts a tab's live saved-response snapshots into the collection JSON shape.
pub fn saved_responses_to_examples(saved: &[SavedResponse]) -> Option<Vec<PostmanResponseExample>> {
    if saved.is_empty() {
        return None;
    }
    Some(
        saved
            .iter()
            .map(|saved| PostmanResponseExample {
                name: saved.name.clone(),
                code: saved.status,
                header: Some(
                    saved
                        .headers
                        .iter()
                        .map(|(key, value)| PostmanHeader {
                            key: key.clone(),
                            value: value.clone(),
                            disabled: None,
                        })
                        .collect(),
                ),
                body: Some(saved.body.clone()),
                response_time: Some(saved.elapsed_ms as u64),
            })
            .collect(),
    )
}

/// converts a request node's saved response examples into the tab's live representation.
pub fn examples_to_saved_responses(examples: &[PostmanResponseExample]) -> Vec<SavedResponse> {
    examples
        .iter()
        .map(|example| SavedResponse {
            name: example.name.clone(),
            status: example.code,
            body: example.body.clone().unwrap_or_default(),
            headers: example
                .header
                .as_ref()
                .map(|headers| {
                    headers
                        .iter()
                        .map(|h| (h.key.clone(), h.value.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            elapsed_ms: example.response_time.unwrap_or(0) as u128,
            timings: Default::default(),
            request_size: 0,
            response_size: 0,
        })
        .collect()
}

/// syncs a request node's body from a live UI tab's body editor state.
pub fn sync_body_from_tab(node: &mut PostmanRequestNode, tab: &Tab) {
    node.request.body = match tab.body_type {
        BodyType::None => None,
        BodyType::Raw => {
            let text_content = tab.request_body.text();
            if text_content.trim().is_empty() {
                None
            } else {
                Some(PostmanBody {
                    mode: Some("raw".to_string()),
                    raw: Some(text_content),
                    formdata: None,
                    urlencoded: None,
                    graphql: None,
                })
            }
        }
        BodyType::GraphQl => {
            let query = tab.graphql_query.text();
            let variables = tab.graphql_variables.text();
            Some(PostmanBody {
                mode: Some("graphql".to_string()),
                raw: None,
                formdata: None,
                urlencoded: None,
                graphql: Some(PostmanGraphQlBody {
                    query,
                    variables: (!variables.trim().is_empty()).then_some(variables),
                }),
            })
        }
        BodyType::FormData => Some(PostmanBody {
            mode: Some("formdata".to_string()),
            raw: None,
            graphql: None,
            formdata: Some(
                tab.body_form_data
                    .iter()
                    .map(|r| PostmanBodyRow {
                        key: r.key.clone(),
                        value: Some(r.value.clone()),
                        disabled: Some(!r.is_active),
                        r#type: Some(match r.field_type {
                            FormDataType::File => "file".to_string(),
                            FormDataType::Text => "text".to_string(),
                        }),
                    })
                    .collect(),
            ),
            urlencoded: None,
        }),
        // handle urlencoded and binary if types parse it natively or fall back safely
        BodyType::XWwwFormUrlencoded | BodyType::Binary => Some(PostmanBody {
            mode: Some("urlencoded".to_string()),
            raw: None,
            formdata: None,
            graphql: None,
            urlencoded: Some(
                tab.body_urlencoded
                    .iter()
                    .map(|u| PostmanBodyRow {
                        key: u.key.clone(),
                        value: Some(u.value.clone()),
                        disabled: Some(!u.is_active),
                        r#type: Some("text".to_string()),
                    })
                    .collect(),
            ),
        }),
    };
}

impl RequestNodeTabExt for PostmanRequestNode {
    fn update_from_tab(&mut self, tab: &Tab) {
        self.name = tab.name.clone();
        self.request.method = tab.method.to_string();
        self.request.url = Some(PostmanUrl::String(tab.url.clone()));

        // update headers
        let headers: Vec<PostmanHeader> = tab
            .request_headers
            .iter()
            .map(|kv| PostmanHeader {
                key: kv.key.clone(),
                value: kv.value.clone(),
                disabled: if kv.is_active { None } else { Some(true) },
            })
            .collect();
        self.request.header = if headers.is_empty() {
            None
        } else {
            Some(headers)
        };

        // sync authorization
        let auth_text = tab.request_auth.text();
        self.request.auth = if auth_text.trim().is_empty() {
            None
        } else {
            Some(auth_text)
        };

        // sync scripts into postman events
        let mut events = Vec::new();

        let pre_script = tab.pre_request_script.text();
        if !pre_script.trim().is_empty() {
            events.push(PostmanEvent {
                listen: "prerequest".to_string(),
                script: Some(PostmanScript {
                    r#type: Some("text/javascript".to_string()),
                    exec: Some(PostmanScriptExec::from_string(&pre_script)),
                }),
            });
        }

        let post_script = tab.post_response_script.text();
        if !post_script.trim().is_empty() {
            events.push(PostmanEvent {
                listen: "test".to_string(),
                script: Some(PostmanScript {
                    r#type: Some("text/javascript".to_string()),
                    exec: Some(PostmanScriptExec::from_string(&post_script)),
                }),
            });
        }

        self.event = if events.is_empty() {
            None
        } else {
            Some(events)
        };

        // sync saved response snapshots
        self.response = saved_responses_to_examples(&tab.saved_responses);
    }
}

// helper to recursively transform a Postman Request Node into our app's live tab state
pub fn create_tab_from_request(
    id: usize,
    node: &PostmanRequestNode,
    collection_id: Option<usize>,
) -> Tab {
    let mut tab = Tab::new(id);
    tab.name = node.name.clone();
    tab.url = node
        .request
        .url
        .as_ref()
        .map(|u| u.to_string())
        .unwrap_or_default();
    tab.collection_id = collection_id;
    tab.request_id = Some(node.id);
    tab.sync_params_from_url();

    tab.method = match node.request.method.to_uppercase().as_str() {
        "GET" => HttpMethod::GET,
        "POST" => HttpMethod::POST,
        "PUT" => HttpMethod::PUT,
        "DELETE" => HttpMethod::DELETE,
        "PATCH" => HttpMethod::PATCH,
        "HEAD" => HttpMethod::HEAD,
        "OPTIONS" => HttpMethod::OPTIONS,
        custom => HttpMethod::Custom(custom.to_string()),
    };

    // import scripts (Pre-request and Post-response)
    if let Some(events) = &node.event {
        for event in events {
            if let Some(script) = &event.script {
                if let Some(exec) = &script.exec {
                    let script_code = exec.to_string_contents();
                    match event.listen.as_str() {
                        "prerequest" => {
                            tab.pre_request_script =
                                iced::widget::text_editor::Content::with_text(&script_code);
                        }
                        "test" => {
                            tab.post_response_script =
                                iced::widget::text_editor::Content::with_text(&script_code);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if let Some(examples) = &node.response {
        tab.saved_responses = examples_to_saved_responses(examples);
    }

    if let Some(headers) = &node.request.header {
        tab.request_headers = headers
            .iter()
            .map(|h| {
                let mut kv = KeyValuePair::new(&h.key, &h.value);
                kv.is_active = !h.disabled.unwrap_or(false);
                kv
            })
            .collect();
        tab.request_headers_values = crate::ui::tab::contents_for(&tab.request_headers);
    }

    if let Some(auth) = &node.request.auth {
        tab.request_auth = iced::widget::text_editor::Content::with_text(auth);
    }

    if let Some(body) = &node.request.body {
        if let Some(mode) = &body.mode {
            match mode.as_str() {
                "raw" => {
                    if let Some(raw_text) = &body.raw {
                        tab.request_body = iced::widget::text_editor::Content::with_text(raw_text);
                        tab.body_type = BodyType::Raw;
                        tab.active_sub_tab = RequestSubTab::Body;
                    }
                }
                "formdata" => {
                    tab.body_type = BodyType::FormData;
                    tab.active_sub_tab = RequestSubTab::Body;
                    if let Some(rows) = &body.formdata {
                        tab.body_form_data = rows
                            .iter()
                            .map(|r| {
                                let f_type = match r.r#type.as_deref() {
                                    Some("file") => FormDataType::File,
                                    _ => FormDataType::Text,
                                };
                                let mut row = FormDataRow::new(
                                    &r.key,
                                    &r.value.clone().unwrap_or_default(),
                                    f_type,
                                );
                                row.is_active = !r.disabled.unwrap_or(false);
                                row
                            })
                            .collect();
                        tab.body_form_data_values =
                            crate::ui::tab::contents_for_form_data(&tab.body_form_data);
                    }
                }
                "graphql" => {
                    tab.body_type = BodyType::GraphQl;
                    tab.active_sub_tab = RequestSubTab::Body;
                    if let Some(graphql) = &body.graphql {
                        tab.graphql_query =
                            iced::widget::text_editor::Content::with_text(&graphql.query);
                        if let Some(variables) = &graphql.variables {
                            tab.graphql_variables =
                                iced::widget::text_editor::Content::with_text(variables);
                        }
                    }
                }
                "urlencoded" => {
                    tab.body_type = BodyType::Raw; // default to raw fallback safely
                    tab.active_sub_tab = RequestSubTab::Body;

                    if let Some(rows) = &body.urlencoded {
                        tab.body_urlencoded = rows
                            .iter()
                            .map(|r| {
                                let mut kv =
                                    KeyValuePair::new(&r.key, &r.value.clone().unwrap_or_default());
                                kv.is_active = !r.disabled.unwrap_or(false);
                                kv
                            })
                            .collect();
                        tab.body_urlencoded_values =
                            crate::ui::tab::contents_for(&tab.body_urlencoded);

                        let encoded_string = rows
                            .iter()
                            .filter(|r| !r.disabled.unwrap_or(false))
                            .map(|r| {
                                format!(
                                    "{}={}",
                                    urlencoding::encode(&r.key),
                                    urlencoding::encode(&r.value.as_deref().unwrap_or(""))
                                )
                            })
                            .collect::<Vec<String>>()
                            .join("&");

                        tab.request_body =
                            iced::widget::text_editor::Content::with_text(&encoded_string);
                    }
                }
                _ => {}
            }
        }
    }

    tab
}

/// picks the workspace content a saved request should reopen into: a
/// WebSocket/GraphQL/gRPC panel if the node carries one of those, otherwise
/// the plain HTTP request tab
pub fn workspace_content_for_request(node: &PostmanRequestNode) -> WorkspaceContent {
    match &node.protocol_request {
        Some(ProtocolRequestDetails::WebSocket(details)) => {
            WorkspaceContent::WebSocket(ws_state_from_details(details))
        }
        Some(ProtocolRequestDetails::GraphQl(details)) => {
            WorkspaceContent::GraphQl(graphql_state_from_details(details))
        }
        Some(ProtocolRequestDetails::Grpc(details)) => {
            WorkspaceContent::Grpc(grpc_state_from_details(details))
        }
        None => WorkspaceContent::HttpRequest,
    }
}

/// builds the `(request, protocol_request)` pair a `PostmanRequestNode` needs
/// to persist a WebSocket/GraphQL/gRPC tab's current live state; `None` for
/// any other workspace content
pub fn protocol_request_details(
    content: &WorkspaceContent,
) -> Option<(PostmanRequestDetails, ProtocolRequestDetails)> {
    match content {
        WorkspaceContent::WebSocket(state) => {
            let details = ws_state_to_details(state);
            let request =
                placeholder_request_details("WEBSOCKET", &details.url, details.headers.clone());
            Some((request, ProtocolRequestDetails::WebSocket(details)))
        }
        WorkspaceContent::GraphQl(state) => {
            let details = graphql_state_to_details(state);
            let request =
                placeholder_request_details("GRAPHQL", &details.url, details.headers.clone());
            Some((request, ProtocolRequestDetails::GraphQl(details)))
        }
        WorkspaceContent::Grpc(state) => {
            let details = grpc_state_to_details(state);
            let request =
                placeholder_request_details("GRPC", &details.endpoint, details.metadata.clone());
            Some((request, ProtocolRequestDetails::Grpc(details)))
        }
        _ => None,
    }
}

/// updates an existing collection node in place from a WebSocket/GraphQL/gRPC
/// tab's current live state; no-op (returns `false`) for any other content kind.
pub fn update_protocol_node_from_content(
    node: &mut PostmanRequestNode,
    tab_name: &str,
    content: &WorkspaceContent,
) -> bool {
    let Some((request, protocol_request)) = protocol_request_details(content) else {
        return false;
    };
    node.name = tab_name.to_string();
    node.request = request;
    node.protocol_request = Some(protocol_request);
    true
}

fn placeholder_request_details(
    method: &str,
    url: &str,
    headers: Vec<PostmanHeader>,
) -> PostmanRequestDetails {
    PostmanRequestDetails {
        method: method.to_string(),
        url: Some(PostmanUrl::String(url.to_string())),
        header: if headers.is_empty() {
            None
        } else {
            Some(headers)
        },
        body: None,
        auth: None,
    }
}

fn kv_pairs_to_postman_headers(pairs: &[KeyValuePair]) -> Vec<PostmanHeader> {
    pairs
        .iter()
        .filter(|kv| !kv.key.is_empty())
        .map(|kv| PostmanHeader {
            key: kv.key.clone(),
            value: kv.value.clone(),
            disabled: if kv.is_active { None } else { Some(true) },
        })
        .collect()
}

fn postman_headers_to_kv(headers: &[PostmanHeader]) -> Vec<KeyValuePair> {
    headers
        .iter()
        .map(|h| {
            let mut kv = KeyValuePair::new(&h.key, &h.value);
            kv.is_active = !h.disabled.unwrap_or(false);
            kv
        })
        .collect()
}

fn ws_state_to_details(state: &WsTabState) -> WebSocketRequestDetails {
    WebSocketRequestDetails {
        url: state.url.clone(),
        headers: kv_pairs_to_postman_headers(&state.headers),
    }
}

fn ws_state_from_details(details: &WebSocketRequestDetails) -> WsTabState {
    let mut headers = postman_headers_to_kv(&details.headers);
    if headers.is_empty() {
        headers.push(KeyValuePair::new("", ""));
    }
    WsTabState {
        url: details.url.clone(),
        headers,
        ..WsTabState::default()
    }
}

fn graphql_state_to_details(state: &GraphQlTabState) -> GraphQlRequestDetails {
    GraphQlRequestDetails {
        url: state.url.clone(),
        headers: kv_pairs_to_postman_headers(&state.headers),
        query: state.query.text(),
        variables: state.variables.text(),
        operation_name: (!state.operation_name.trim().is_empty())
            .then(|| state.operation_name.clone()),
        subscription_url: None,
    }
}

fn graphql_state_from_details(details: &GraphQlRequestDetails) -> GraphQlTabState {
    let mut headers = postman_headers_to_kv(&details.headers);
    if headers.is_empty() {
        headers.push(KeyValuePair::new("Content-Type", "application/json"));
    }
    GraphQlTabState {
        url: details.url.clone(),
        headers,
        query: text_editor::Content::with_text(&details.query),
        variables: text_editor::Content::with_text(&details.variables),
        operation_name: details.operation_name.clone().unwrap_or_default(),
        ..GraphQlTabState::default()
    }
}

fn grpc_state_to_details(state: &GrpcTabState) -> GrpcRequestDetails {
    GrpcRequestDetails {
        endpoint: state.endpoint.clone(),
        use_tls: state.use_tls,
        service: state.selected_service.clone().unwrap_or_default(),
        method: state.selected_method.clone().unwrap_or_default(),
        request_json: state.request_json.text(),
        metadata: kv_pairs_to_postman_headers(&state.metadata),
        proto_files: state
            .proto_files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
    }
}

fn grpc_state_from_details(details: &GrpcRequestDetails) -> GrpcTabState {
    let mut metadata = postman_headers_to_kv(&details.metadata);
    if metadata.is_empty() {
        metadata.push(KeyValuePair::new("", ""));
    }
    GrpcTabState {
        endpoint: details.endpoint.clone(),
        use_tls: details.use_tls,
        proto_files: details.proto_files.iter().map(PathBuf::from).collect(),
        metadata,
        selected_service: (!details.service.is_empty()).then(|| details.service.clone()),
        selected_method: (!details.method.is_empty()).then(|| details.method.clone()),
        request_json: text_editor::Content::with_text(&details.request_json),
        ..GrpcTabState::default()
    }
}
