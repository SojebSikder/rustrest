use crate::collection::CollectionItem;

pub fn format_json_or_fallback(raw_body: &str) -> String {
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(raw_body) {
        serde_json::to_string_pretty(&json_value).unwrap_or_else(|_| raw_body.to_string())
    } else {
        format!("// Invalid JSON:\n{}", raw_body)
    }
}

pub fn contains_request_node(items: &[CollectionItem], name: &str) -> bool {
    for item in items {
        match item {
            CollectionItem::Request(node) => {
                if node.name == name {
                    return true;
                }
            }
            CollectionItem::Folder {
                item: sub_items, ..
            } => {
                if contains_request_node(sub_items, name) {
                    return true;
                }
            }
        }
    }
    false
}
