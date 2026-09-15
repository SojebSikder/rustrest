//! Insomnia v5.1 export format: a single YAML document with a nested
//! `collection: [...]` item tree (folders carry `children`, requests carry
//! `url`/`method`/...) instead of v4's flat `resources` + `parentId` list.
//! `type: collection.insomnia.rest/5.0` is the documented schema id as of
//! writing; Insomnia app v5.1 exports against it (there's no dedicated
//! "5.1" schema version), so imports accept any `collection.insomnia.rest/`
//! type and exports pin the 5.0 schema id for maximum compatibility.

use crate::convert::*;
use crate::ids::next_id;
use serde_json::{Value, json};

const DOC_TYPE: &str = "collection.insomnia.rest/5.0";

pub fn to_postman(doc: Value) -> Result<Value, String> {
    let obj = doc.as_object().ok_or("not a valid Insomnia v5.1 document: expected a YAML mapping")?;
    let doc_type = obj.get("type").and_then(Value::as_str).unwrap_or("");
    if !doc_type.starts_with("collection.insomnia.rest/") {
        return Err(format!(
            "not a valid Insomnia v5.1 collection (unexpected \"type\": {doc_type:?})"
        ));
    }

    let name = obj.get("name").and_then(Value::as_str).unwrap_or("Imported from Insomnia").to_string();
    let items: Vec<Value> = obj
        .get("collection")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(item_to_postman).collect())
        .unwrap_or_default();

    let variable = obj
        .get("environments")
        .and_then(|e| e.get("data"))
        .and_then(Value::as_object)
        .map(env_data_to_variables)
        .unwrap_or(Value::Null);

    Ok(json!({
        "info": { "name": name, "_postman_id": Value::Null, "schema": crate::v4::SCHEMA_URL },
        "item": items,
        "variable": variable,
    }))
}

fn item_to_postman(item: &Value) -> Option<Value> {
    if let Some(children) = item.get("children").and_then(Value::as_array) {
        Some(json!({
            "name": item.get("name").and_then(Value::as_str).unwrap_or("Folder"),
            "item": children.iter().filter_map(item_to_postman).collect::<Vec<_>>(),
            "event": Value::Null,
            "description": item.get("description").and_then(Value::as_str),
            "protocolProfileBehavior": Value::Null,
        }))
    } else if item.get("url").is_some() {
        let name = item.get("name").and_then(Value::as_str).unwrap_or("Request");
        let method = item.get("method").and_then(Value::as_str).unwrap_or("GET").to_string();
        let url = insomnia_tpl_to_postman(item.get("url").and_then(Value::as_str).unwrap_or(""));
        let headers = headers_to_postman(item.get("headers"));
        let body = body_to_postman(item.get("body"));
        Some(json!({
            "name": name,
            "event": Value::Null,
            "request": { "method": method, "url": url, "header": headers, "body": body },
        }))
    } else {
        rustrest_plugin_api::log(&format!(
            "insomnia import: skipping unrecognized collection item {:?}",
            item.get("name")
        ));
        None
    }
}

pub fn from_postman(collection: &Value) -> Value {
    let name = collection.pointer("/info/name").and_then(Value::as_str).unwrap_or("Exported Collection");
    let items: Vec<Value> = collection
        .get("item")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(postman_item_to_insomnia).collect())
        .unwrap_or_default();

    let mut doc = json!({
        "type": DOC_TYPE,
        "name": name,
        "meta": { "id": next_id("wrk"), "created": 0, "modified": 0 },
        "collection": items,
        "cookieJar": { "name": "Default Jar", "cookies": [], "meta": { "id": next_id("jar") } },
    });

    if let Some(data) = variables_to_env_data(collection.get("variable")) {
        doc["environments"] = json!({
            "name": "Base Environment",
            "data": data,
            "meta": { "id": next_id("env") },
        });
    }

    doc
}

fn postman_item_to_insomnia(item: &Value) -> Value {
    if let Some(children) = item.get("item").and_then(Value::as_array) {
        json!({
            "name": item.get("name").and_then(Value::as_str).unwrap_or("Folder"),
            "meta": { "id": next_id("fld") },
            "children": children.iter().map(postman_item_to_insomnia).collect::<Vec<_>>(),
        })
    } else {
        let req = item.get("request");
        let method = req.and_then(|r| r.get("method")).and_then(Value::as_str).unwrap_or("GET").to_string();
        let url = postman_tpl_to_insomnia(&url_to_string(req.and_then(|r| r.get("url"))));
        let headers = headers_to_insomnia(req.and_then(|r| r.get("header")));
        let body = body_to_insomnia(req.and_then(|r| r.get("body")));
        json!({
            "name": item.get("name").and_then(Value::as_str).unwrap_or("Request"),
            "url": url,
            "method": method,
            "headers": headers,
            "body": body,
            "meta": { "id": next_id("req"), "isPrivate": false },
        })
    }
}
