use crate::collection::collection::CollectionItem;

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
