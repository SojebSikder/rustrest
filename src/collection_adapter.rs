use crate::collection::collection::{
    PostmanEvent, PostmanHeader, PostmanRequestNode, PostmanScript, PostmanScriptExec, PostmanUrl,
};
use crate::http_client::HttpMethod;
use crate::ui::tab::Tab;
use crate::ui::tab::types::{BodyType, FormDataRow, FormDataType, KeyValuePair, RequestSubTab};

pub trait RequestNodeTabExt {
    /// updates this collection request node from a live UI tab, including pre-request & test scripts.
    fn update_from_tab(&mut self, tab: &Tab);
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

    if let Some(headers) = &node.request.header {
        tab.request_headers = headers
            .iter()
            .map(|h| {
                let mut kv = KeyValuePair::new(&h.key, &h.value);
                kv.is_active = !h.disabled.unwrap_or(false);
                kv
            })
            .collect();
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
