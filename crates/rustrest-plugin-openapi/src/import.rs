//! OpenAPI 3.x / Swagger 2.0 -> Rustrest's own collection JSON (Postman
//! Collection v2.1 shaped, see
//! `rustrest-core::collection::model::PostmanCollection`).
//!
//! Both spec flavors are normalized into a common `Endpoint` list before a
//! single shared builder turns them into Postman folders/requests, since the
//! folder/grouping/body/header logic is identical once each flavor's own
//! quirks (v2's flat `in: body`/`in: formData` parameters vs v3's separate
//! `requestBody`) have been extracted.

use crate::schema::example_from_schema;
use serde_json::{json, Map, Value};

const SCHEMA_URL: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

struct Endpoint {
    method: String,
    path: String,
    name: String,
    tag: Option<String>,
    query_params: Vec<(String, bool)>,
    header_params: Vec<(String, bool)>,
    body: Option<(String, Value)>,
    form_params: Option<(&'static str, Vec<(String, bool)>)>,
}

pub fn to_postman(doc: Value) -> Result<Value, String> {
    if doc.get("openapi").and_then(Value::as_str).is_some() {
        openapi_v3_to_postman(&doc)
    } else if doc.get("swagger").and_then(Value::as_str) == Some("2.0") {
        swagger_v2_to_postman(&doc)
    } else {
        Err("not a recognized OpenAPI 3.x or Swagger 2.0 document (missing \"openapi\" or \"swagger\": \"2.0\")".to_string())
    }
}

fn openapi_v3_to_postman(doc: &Value) -> Result<Value, String> {
    let paths = doc
        .get("paths")
        .and_then(Value::as_object)
        .ok_or("missing \"paths\" object")?;
    let base_url = openapi_base_url(doc);
    let title = doc
        .pointer("/info/title")
        .and_then(Value::as_str)
        .unwrap_or("Imported API")
        .to_string();

    let mut endpoints = Vec::new();
    for (path, path_item) in paths {
        let Some(path_item) = path_item.as_object() else {
            continue;
        };
        let shared_params = path_item
            .get("parameters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for method in [
            "get", "put", "post", "delete", "options", "head", "patch", "trace",
        ] {
            let Some(op) = path_item.get(method).and_then(Value::as_object) else {
                continue;
            };
            let mut params = shared_params.clone();
            if let Some(op_params) = op.get("parameters").and_then(Value::as_array) {
                params.extend(op_params.iter().cloned());
            }
            endpoints.push(openapi_endpoint(method, path, op, &params, doc));
        }
    }

    Ok(build_collection(&title, &base_url, endpoints, doc))
}

fn openapi_base_url(doc: &Value) -> String {
    let servers = doc.get("servers").and_then(Value::as_array);
    let Some(server) = servers.and_then(|s| s.first()) else {
        return "http://localhost".to_string();
    };
    let mut url = server
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("http://localhost")
        .to_string();
    if let Some(variables) = server.get("variables").and_then(Value::as_object) {
        for (name, var) in variables {
            let default = var.get("default").and_then(Value::as_str).unwrap_or("");
            url = url.replace(&format!("{{{name}}}"), default);
        }
    }
    url
}

fn openapi_endpoint(
    method: &str,
    path: &str,
    op: &Map<String, Value>,
    params: &[Value],
    doc: &Value,
) -> Endpoint {
    let name = op
        .get("summary")
        .and_then(Value::as_str)
        .or_else(|| op.get("operationId").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| format!("{} {}", method.to_uppercase(), path));
    let tag = op
        .get("tags")
        .and_then(Value::as_array)
        .and_then(|t| t.first())
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut query_params = Vec::new();
    let mut header_params = Vec::new();
    for p in params {
        let Some(name) = p.get("name").and_then(Value::as_str) else {
            continue;
        };
        let required = p.get("required").and_then(Value::as_bool).unwrap_or(false);
        match p.get("in").and_then(Value::as_str) {
            Some("query") => query_params.push((name.to_string(), required)),
            Some("header") => header_params.push((name.to_string(), required)),
            _ => {}
        }
    }

    let body = op
        .get("requestBody")
        .and_then(|rb| rb.get("content"))
        .and_then(Value::as_object)
        .and_then(|content| {
            let media_type = if content.contains_key("application/json") {
                "application/json"
            } else {
                content.keys().next()?.as_str()
            };
            let schema = content.get(media_type)?.get("schema")?;
            Some((media_type.to_string(), example_from_schema(schema, doc, 0)))
        });

    Endpoint {
        method: method.to_uppercase(),
        path: path.to_string(),
        name,
        tag,
        query_params,
        header_params,
        body: body.map(|(mt, example)| (mt, example)),
        form_params: None,
    }
}

fn swagger_v2_to_postman(doc: &Value) -> Result<Value, String> {
    let paths = doc
        .get("paths")
        .and_then(Value::as_object)
        .ok_or("missing \"paths\" object")?;
    let base_url = swagger_base_url(doc);
    let title = doc
        .pointer("/info/title")
        .and_then(Value::as_str)
        .unwrap_or("Imported API")
        .to_string();

    let mut endpoints = Vec::new();
    for (path, path_item) in paths {
        let Some(path_item) = path_item.as_object() else {
            continue;
        };
        let shared_params = path_item
            .get("parameters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for method in ["get", "put", "post", "delete", "options", "head", "patch"] {
            let Some(op) = path_item.get(method).and_then(Value::as_object) else {
                continue;
            };
            let mut params = shared_params.clone();
            if let Some(op_params) = op.get("parameters").and_then(Value::as_array) {
                params.extend(op_params.iter().cloned());
            }
            endpoints.push(swagger_endpoint(method, path, op, &params, doc));
        }
    }

    Ok(build_collection(&title, &base_url, endpoints, doc))
}

fn swagger_base_url(doc: &Value) -> String {
    let scheme = doc
        .get("schemes")
        .and_then(Value::as_array)
        .and_then(|s| s.first())
        .and_then(Value::as_str)
        .unwrap_or("https");
    let host = doc
        .get("host")
        .and_then(Value::as_str)
        .unwrap_or("localhost");
    let base_path = doc.get("basePath").and_then(Value::as_str).unwrap_or("");
    format!("{scheme}://{host}{base_path}")
}

fn swagger_endpoint(
    method: &str,
    path: &str,
    op: &Map<String, Value>,
    params: &[Value],
    doc: &Value,
) -> Endpoint {
    let name = op
        .get("summary")
        .and_then(Value::as_str)
        .or_else(|| op.get("operationId").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| format!("{} {}", method.to_uppercase(), path));
    let tag = op
        .get("tags")
        .and_then(Value::as_array)
        .and_then(|t| t.first())
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut query_params = Vec::new();
    let mut header_params = Vec::new();
    let mut form_fields = Vec::new();
    let mut body = None;
    for p in params {
        let Some(name) = p.get("name").and_then(Value::as_str) else {
            continue;
        };
        let required = p.get("required").and_then(Value::as_bool).unwrap_or(false);
        match p.get("in").and_then(Value::as_str) {
            Some("query") => query_params.push((name.to_string(), required)),
            Some("header") => header_params.push((name.to_string(), required)),
            Some("formData") => form_fields.push((name.to_string(), required)),
            Some("body") => {
                if let Some(schema) = p.get("schema") {
                    body = Some((
                        "application/json".to_string(),
                        example_from_schema(schema, doc, 0),
                    ));
                }
            }
            _ => {}
        }
    }

    let form_params = if form_fields.is_empty() {
        None
    } else {
        let consumes = op
            .get("consumes")
            .and_then(Value::as_array)
            .or_else(|| doc.get("consumes").and_then(Value::as_array));
        let is_multipart =
            consumes.is_some_and(|c| c.iter().any(|v| v.as_str() == Some("multipart/form-data")));
        Some((
            if is_multipart {
                "multipart/form-data"
            } else {
                "application/x-www-form-urlencoded"
            },
            form_fields,
        ))
    };

    Endpoint {
        method: method.to_uppercase(),
        path: path.to_string(),
        name,
        tag,
        query_params,
        header_params,
        body,
        form_params,
    }
}

/// groups endpoints by their first tag (preserving first-appearance order),
/// tag-less endpoints land as top-level items in document order.
fn build_collection(title: &str, base_url: &str, endpoints: Vec<Endpoint>, _doc: &Value) -> Value {
    let mut top_level: Vec<Value> = Vec::new();
    let mut folders: Vec<(String, Vec<Value>)> = Vec::new();

    for ep in endpoints {
        let request_item = endpoint_to_request_item(&ep);
        match ep.tag {
            None => top_level.push(request_item),
            Some(tag) => match folders.iter_mut().find(|(name, _)| *name == tag) {
                Some((_, items)) => items.push(request_item),
                None => folders.push((tag, vec![request_item])),
            },
        }
    }

    let mut items = top_level;
    for (tag, folder_items) in folders {
        items.push(json!({
            "name": tag,
            "item": folder_items,
            "event": Value::Null,
            "description": Value::Null,
            "protocolProfileBehavior": Value::Null,
        }));
    }

    json!({
        "info": { "name": title, "_postman_id": Value::Null, "schema": SCHEMA_URL },
        "item": items,
        "variable": [{ "key": "baseUrl", "value": base_url, "type": Value::Null }],
    })
}

fn endpoint_to_request_item(ep: &Endpoint) -> Value {
    let mut url = format!("{{{{baseUrl}}}}{}", path_to_postman_url(&ep.path));
    if !ep.query_params.is_empty() {
        let query = ep
            .query_params
            .iter()
            .map(|(name, _)| format!("{name}={{{{{name}}}}}"))
            .collect::<Vec<_>>()
            .join("&");
        url.push('?');
        url.push_str(&query);
    }

    let mut headers: Vec<Value> = ep
        .header_params
        .iter()
        .map(|(name, required)| json!({ "key": name, "value": format!("{{{{{name}}}}}"), "disabled": !required }))
        .collect();

    let body = if let Some((media_type, example)) = &ep.body {
        headers.push(json!({ "key": "Content-Type", "value": media_type, "disabled": false }));
        let raw = serde_json::to_string_pretty(example).unwrap_or_default();
        json!({ "mode": "raw", "raw": raw, "formdata": Value::Null, "urlencoded": Value::Null })
    } else if let Some((content_type, fields)) = &ep.form_params {
        let mode = if *content_type == "multipart/form-data" {
            "formdata"
        } else {
            "urlencoded"
        };
        let rows: Vec<Value> = fields
            .iter()
            .map(|(name, required)| json!({ "key": name, "value": "", "disabled": !required, "type": "text" }))
            .collect();
        json!({
            "mode": mode,
            "raw": Value::Null,
            "formdata": if mode == "formdata" { Value::Array(rows.clone()) } else { Value::Null },
            "urlencoded": if mode == "urlencoded" { Value::Array(rows) } else { Value::Null },
        })
    } else {
        Value::Null
    };

    json!({
        "name": ep.name,
        "event": Value::Null,
        "request": { "method": ep.method, "url": url, "header": headers, "body": body },
    })
}

/// OpenAPI/Swagger path templates use single-brace `{param}`; Rustrest
/// resolves double-brace `{{param}}` templates against the active
/// environment, matching the collection `variable` convention used
/// throughout this app.
fn path_to_postman_url(path: &str) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                out.push_str("{{");
                out.push_str(&after[..end]);
                out.push_str("}}");
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}
