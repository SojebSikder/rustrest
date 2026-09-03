use crate::collection::collection::{CollectionItem, PostmanBody, PostmanBodyRow};
use crate::collection_adapter::RequestNodeTabExt;
use crate::ui::tab::Tab;
use crate::ui::tab::types::{BodyType, FormDataType};

pub use rustrest_core::collection::tree_ops::{
    contains_request_node_by_id, insert_nested, insert_nested_request, remove_nested,
    remove_nested_request, rename_nested_folder,
};

pub fn format_json_or_fallback(raw_body: &str) -> String {
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(raw_body) {
        serde_json::to_string_pretty(&json_value).unwrap_or_else(|_| raw_body.to_string())
    } else {
        format!("// Invalid JSON:\n{}", raw_body)
    }
}

// recursively updates node in the collection by its ID, syncing tab state with the request
pub fn update_node(items: &mut Vec<CollectionItem>, target_id: usize, tab: &Tab) -> bool {
    let Some(req) = rustrest_core::collection::tree_ops::find_request_mut(items, target_id) else {
        return false;
    };

    // sync name / method / url / headers / pre-request and test scripts
    req.update_from_tab(tab);

    // sync Request Body types conditionally
    match tab.body_type {
        BodyType::Raw => {
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
        BodyType::FormData => {
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
                                FormDataType::File => "file".to_string(),
                                FormDataType::Text => "text".to_string(),
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

    true
}
