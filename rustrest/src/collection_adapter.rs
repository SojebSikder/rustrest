use crate::collection::collection::{
    PostmanBody, PostmanBodyRow, PostmanEvent, PostmanHeader, PostmanRequestNode,
    PostmanResponseExample, PostmanScript, PostmanScriptExec, PostmanUrl,
};
use crate::http_client::HttpMethod;
use crate::ui::tab::Tab;
use crate::ui::tab::types::{
    BodyType, FormDataRow, FormDataType, KeyValuePair, RequestSubTab, SavedResponse,
};

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
                })
            }
        }
        BodyType::FormData => Some(PostmanBody {
            mode: Some("formdata".to_string()),
            raw: None,
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
