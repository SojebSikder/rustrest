//! Insomnia v4 export format: a flat JSON `resources` array, each resource
//! linked to its parent via `parentId`, discriminated by `_type`
//! (`workspace` | `request_group` | `request` | `environment` | ...).
//! https://docs.insomnia.rest/insomnia/import-export-data (legacy JSON export)

use crate::convert::*;
use crate::ids::next_id;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub(crate) const SCHEMA_URL: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

pub fn to_postman(doc: Value) -> Result<Value, String> {
    let obj = doc.as_object().ok_or("not a valid Insomnia v4 export: expected a JSON object")?;
    if obj.get("_type").and_then(Value::as_str) != Some("export") {
        return Err("not a valid Insomnia v4 export: missing \"_type\": \"export\"".to_string());
    }
    let resources = obj
        .get("resources")
        .and_then(Value::as_array)
        .ok_or("not a valid Insomnia v4 export: missing \"resources\" array")?;

    let mut children: HashMap<String, Vec<&Value>> = HashMap::new();
    for r in resources {
        let parent = r.get("parentId").and_then(Value::as_str).unwrap_or("").to_string();
        children.entry(parent).or_default().push(r);
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| sort_key(a).total_cmp(&sort_key(b)));
    }

    let workspaces: Vec<&Value> = resources
        .iter()
        .filter(|r| r.get("_type").and_then(Value::as_str) == Some("workspace"))
        .collect();
    if workspaces.is_empty() {
        return Err("no workspace found in Insomnia export".to_string());
    }

    let (name, items) = if let [ws] = workspaces.as_slice() {
        let id = ws.get("_id").and_then(Value::as_str).unwrap_or("");
        let name = ws
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Imported from Insomnia")
            .to_string();
        (name, build_items(id, &children))
    } else {
        let items = workspaces
            .iter()
            .map(|ws| {
                let id = ws.get("_id").and_then(Value::as_str).unwrap_or("");
                let name = ws.get("name").and_then(Value::as_str).unwrap_or("Workspace");
                json!({
                    "name": name,
                    "item": build_items(id, &children),
                    "event": Value::Null,
                    "description": Value::Null,
                    "protocolProfileBehavior": Value::Null,
                })
            })
            .collect();
        ("Insomnia Import".to_string(), items)
    };

    let variable = collect_environment_variables(&workspaces, &children);

    Ok(json!({
        "info": { "name": name, "_postman_id": Value::Null, "schema": SCHEMA_URL },
        "item": items,
        "variable": variable,
    }))
}

fn sort_key(v: &Value) -> f64 {
    v.get("metaSortKey").and_then(Value::as_f64).unwrap_or(0.0)
}

fn build_items(parent_id: &str, children: &HashMap<String, Vec<&Value>>) -> Vec<Value> {
    let Some(kids) = children.get(parent_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for r in kids {
        match r.get("_type").and_then(Value::as_str) {
            Some("request_group") => {
                let id = r.get("_id").and_then(Value::as_str).unwrap_or("");
                out.push(json!({
                    "name": r.get("name").and_then(Value::as_str).unwrap_or("Folder"),
                    "item": build_items(id, children),
                    "event": Value::Null,
                    "description": r.get("description").and_then(Value::as_str),
                    "protocolProfileBehavior": Value::Null,
                }));
            }
            Some("request") | Some("grpc_request") | Some("websocket_request") => {
                out.push(request_resource_to_postman(r));
            }
            _ => {
                rustrest_plugin_api::log(&format!(
                    "insomnia import: skipping unsupported resource type {:?}",
                    r.get("_type")
                ));
            }
        }
    }
    out
}

fn request_resource_to_postman(r: &Value) -> Value {
    let name = r.get("name").and_then(Value::as_str).unwrap_or("Request");
    let method = r.get("method").and_then(Value::as_str).unwrap_or("GET").to_string();
    let url = insomnia_tpl_to_postman(r.get("url").and_then(Value::as_str).unwrap_or(""));
    let headers = headers_to_postman(r.get("headers"));
    let body = body_to_postman(r.get("body"));
    json!({
        "name": name,
        "event": Value::Null,
        "request": { "method": method, "url": url, "header": headers, "body": body },
    })
}

fn collect_environment_variables(workspaces: &[&Value], children: &HashMap<String, Vec<&Value>>) -> Value {
    let mut data = Map::new();
    for ws in workspaces {
        let ws_id = ws.get("_id").and_then(Value::as_str).unwrap_or("");
        // merges the base environment and its direct sub-environments (in
        // document order, later entries winning); deeper nesting is rare
        // and skipped to keep this a flat variable list.
        if let Some(kids) = children.get(ws_id) {
            for r in kids {
                if r.get("_type").and_then(Value::as_str) == Some("environment") {
                    merge_env_data(r, &mut data);
                    let env_id = r.get("_id").and_then(Value::as_str).unwrap_or("");
                    if let Some(sub_envs) = children.get(env_id) {
                        for sub in sub_envs {
                            merge_env_data(sub, &mut data);
                        }
                    }
                }
            }
        }
    }
    env_data_to_variables(&data)
}

fn merge_env_data(env: &Value, data: &mut Map<String, Value>) {
    if let Some(env_data) = env.get("data").and_then(Value::as_object) {
        for (k, v) in env_data {
            data.insert(k.clone(), v.clone());
        }
    }
}

pub fn from_postman(collection: &Value) -> Value {
    let name = collection.pointer("/info/name").and_then(Value::as_str).unwrap_or("Exported Collection");
    let ws_id = next_id("wrk");
    let mut resources = vec![json!({
        "_id": ws_id, "parentId": Value::Null, "modified": 0, "created": 0,
        "name": name, "description": "", "scope": "collection", "_type": "workspace",
    })];

    if let Some(items) = collection.get("item").and_then(Value::as_array) {
        let mut sort_key = 0i64;
        walk_items(items, &ws_id, &mut resources, &mut sort_key);
    }

    if let Some(data) = variables_to_env_data(collection.get("variable")) {
        resources.push(json!({
            "_id": next_id("env"), "parentId": ws_id, "modified": 0, "created": 0,
            "name": "Base Environment", "data": data, "dataPropertyOrder": Value::Null,
            "color": Value::Null, "isPrivate": false, "metaSortKey": 0, "_type": "environment",
        }));
    }

    resources.push(json!({
        "_id": next_id("jar"), "parentId": ws_id, "name": "Default Jar", "cookies": [], "_type": "cookie_jar",
    }));

    json!({
        "_type": "export",
        "__export_format": 4,
        "__export_date": "1970-01-01T00:00:00.000Z",
        "__export_source": "rustrest.plugin.insomnia:v0.1.0",
        "resources": resources,
    })
}

fn walk_items(items: &[Value], parent_id: &str, out: &mut Vec<Value>, sort_key: &mut i64) {
    for item in items {
        *sort_key += 1;
        if let Some(children) = item.get("item").and_then(Value::as_array) {
            let id = next_id("fld");
            out.push(json!({
                "_id": id, "parentId": parent_id, "modified": 0, "created": 0,
                "name": item.get("name").and_then(Value::as_str).unwrap_or("Folder"),
                "description": item.get("description").and_then(Value::as_str).unwrap_or(""),
                "environment": {}, "environmentPropertyOrder": Value::Null,
                "metaSortKey": *sort_key, "_type": "request_group",
            }));
            walk_items(children, &id, out, sort_key);
        } else {
            let req = item.get("request");
            let method = req.and_then(|r| r.get("method")).and_then(Value::as_str).unwrap_or("GET");
            let url = postman_tpl_to_insomnia(&url_to_string(req.and_then(|r| r.get("url"))));
            let headers = headers_to_insomnia(req.and_then(|r| r.get("header")));
            let body = body_to_insomnia(req.and_then(|r| r.get("body")));
            out.push(json!({
                "_id": next_id("req"), "parentId": parent_id, "modified": 0, "created": 0,
                "url": url, "name": item.get("name").and_then(Value::as_str).unwrap_or("Request"),
                "description": "", "method": method, "body": body, "parameters": [],
                "headers": headers, "authentication": {}, "metaSortKey": *sort_key,
                "isPrivate": false, "settingStoreCookies": true, "settingSendCookies": true,
                "settingDisableRenderRequestBody": false, "settingEncodeUrl": true,
                "settingRebuildPath": true, "settingFollowRedirects": "global", "_type": "request",
            }));
        }
    }
}
