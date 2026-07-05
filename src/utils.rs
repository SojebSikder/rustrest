use crate::collection::collection::{
    CollectionItem, PostmanBody, PostmanBodyRow, PostmanHeader, PostmanUrl,
};

pub fn format_json_or_fallback(raw_body: &str) -> String {
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(raw_body) {
        serde_json::to_string_pretty(&json_value).unwrap_or_else(|_| raw_body.to_string())
    } else {
        format!("// Invalid JSON:\n{}", raw_body)
    }
}

pub fn contains_request_node_by_id(items: &[CollectionItem], target_id: usize) -> bool {
    for item in items {
        match item {
            CollectionItem::Request(node) => {
                if node.id == target_id {
                    return true;
                }
            }
            CollectionItem::Folder {
                item: sub_items, ..
            } => {
                if contains_request_node_by_id(sub_items, target_id) {
                    return true;
                }
            }
        }
    }
    false
}

// recursively updates node in the collection by its ID, syncing tab state with the request
pub fn update_node(
    items: &mut Vec<CollectionItem>,
    target_id: usize,
    tab: &crate::tab::Tab,
) -> bool {
    for item in items.iter_mut() {
        match item {
            CollectionItem::Request(req) => {
                if req.id == target_id {
                    // sync Basic Fields
                    req.name = tab.name.clone();
                    req.request.method = tab.method.to_string();
                    req.request.url = PostmanUrl::String(tab.url.clone());

                    // sync Request Headers
                    req.request.header = Some(
                        tab.request_headers
                            .iter()
                            .filter(|h| !h.key.trim().is_empty())
                            .map(|h| PostmanHeader {
                                key: h.key.clone(),
                                value: h.value.clone(),
                                disabled: Some(!h.is_active),
                            })
                            .collect(),
                    );

                    // sync Request Body types conditionally
                    match tab.body_type {
                        crate::tab::types::BodyType::Raw => {
                            let text_content = tab.request_body.text();
                            if !text_content.trim().is_empty() {
                                req.request.body = Some(PostmanBody {
                                    mode: Some("raw".to_string()),
                                    raw: Some(text_content),
                                    formdata: None,
                                    urlencoded: None,
                                });
                            } else {
                                req.request.body = None;
                            }
                        }
                        crate::tab::types::BodyType::FormData => {
                            req.request.body = Some(PostmanBody {
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
                                                crate::tab::types::FormDataType::File => {
                                                    "file".to_string()
                                                }
                                                crate::tab::types::FormDataType::Text => {
                                                    "text".to_string()
                                                }
                                            }),
                                        })
                                        .collect(),
                                ),
                                urlencoded: None,
                            });
                        }
                        // handle urlencoded if types parse it natively or fall back safely
                        _ => {
                            req.request.body = Some(PostmanBody {
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
                            });
                        }
                    }

                    return true;
                }
            }
            CollectionItem::Folder {
                item: sub_items, ..
            } => {
                if update_node(sub_items, target_id, tab) {
                    return true;
                }
            }
        }
    }
    false
}
