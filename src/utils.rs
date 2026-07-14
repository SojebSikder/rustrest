use crate::collection::collection::{
    CollectionItem, PostmanBody, PostmanBodyRow, PostmanFolder, PostmanHeader, PostmanUrl,
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
            CollectionItem::Folder(folder) => {
                if contains_request_node_by_id(&folder.item, target_id) {
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
                    req.request.url = Some(PostmanUrl::String(tab.url.clone()));

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
            CollectionItem::Folder(folder) => {
                if update_node(&mut folder.item, target_id, tab) {
                    return true;
                }
            }
        }
    }
    false
}

// collection, request operations

/// inserts a nested folder into the collection at the specified path
pub fn insert_nested(items: &mut Vec<CollectionItem>, path: &[String]) {
    if path.is_empty() {
        items.push(CollectionItem::Folder(PostmanFolder {
            name: "New Folder".to_string(),
            description: None,
            item: Vec::new(),
            protocol_profile_behavior: None,
        }));
        return;
    }
    for item in items.iter_mut() {
        if let CollectionItem::Folder(folder) = item {
            if folder.name == path[0] {
                insert_nested(&mut folder.item, &path[1..]);
                return;
            }
        }
    }
}

/// inserts a nested request into the collection at the specified path
pub fn insert_nested_request(
    items: &mut Vec<CollectionItem>,
    path: &[String],
    new_req: CollectionItem,
) {
    if path.is_empty() {
        items.push(new_req);
        return;
    }
    for item in items.iter_mut() {
        if let CollectionItem::Folder(folder) = item {
            if folder.name == path[0] {
                insert_nested_request(&mut folder.item, &path[1..], new_req);
                return;
            }
        }
    }
}

/// removes a nested request from the collection at the specified path
pub fn remove_nested_request(items: &mut Vec<CollectionItem>, path: &[String], req_id: usize) {
    if path.is_empty() {
        items.retain(|item| {
            if let CollectionItem::Request(req) = item {
                req.id != req_id
            } else {
                true
            }
        });
        return;
    }
    for item in items.iter_mut() {
        if let CollectionItem::Folder(folder) = item {
            if folder.name == path[0] {
                remove_nested_request(&mut folder.item, &path[1..], req_id);
                return;
            }
        }
    }
}

/// removes a nested request from the collection at the specified path
pub fn remove_nested(items: &mut Vec<CollectionItem>, path: &[String]) {
    if path.is_empty() {
        return;
    }

    if path.len() == 1 {
        items.retain(|item| {
            if let CollectionItem::Folder(folder) = item {
                folder.name != path[0]
            } else {
                true
            }
        });
        return;
    }

    for item in items.iter_mut() {
        if let CollectionItem::Folder(folder) = item {
            if folder.name == path[0] {
                remove_nested(&mut folder.item, &path[1..]);
                return;
            }
        }
    }
}

pub fn rename_nested_folder(
    items: &mut Vec<CollectionItem>,
    path: &[String],
    new_val: &str,
) -> bool {
    if path.is_empty() {
        return false;
    }
    for item in items.iter_mut() {
        if let CollectionItem::Folder(folder) = item {
            if folder.name == path[0] {
                if path.len() == 1 {
                    folder.name = new_val.to_string();
                    return true;
                } else {
                    return rename_nested_folder(&mut folder.item, &path[1..], new_val);
                }
            }
        }
    }
    false
}
