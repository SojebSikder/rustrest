//! Rustrest's collection JSON -> OpenAPI 3.0 / Swagger 2.0.
//!
//! Both target shapes are built from one shared walk of the collection tree
//! (`collect_endpoints`) that flattens folders into tags and recovers path
//! templates, query/header parameter names, and a body example from each
//! request; `build_openapi3`/`build_swagger2` then only differ in how that
//! common `Endpoint` list gets laid out.

use serde_json::{json, Map, Value};

struct Endpoint {
    method: String,
    /// OpenAPI-style path, e.g. "/users/{id}".
    path: String,
    base: String,
    tag: Option<String>,
    name: String,
    query_params: Vec<String>,
    path_params: Vec<String>,
    /// (name, disabled)
    headers: Vec<(String, bool)>,
    body: Option<BodyKind>,
}

enum BodyKind {
    Json {
        content_type: String,
        example: Value,
    },
    Form {
        urlencoded: bool,
        fields: Vec<String>,
    },
}

pub fn to_openapi3(collection: &Value) -> Value {
    build_openapi3(collection, collect_endpoints(collection))
}

pub fn to_swagger2(collection: &Value) -> Value {
    build_swagger2(collection, collect_endpoints(collection))
}

fn collect_endpoints(collection: &Value) -> Vec<Endpoint> {
    let mut out = Vec::new();
    if let Some(items) = collection.get("item").and_then(Value::as_array) {
        walk_items(items, None, &mut out);
    }
    out
}

fn walk_items(items: &[Value], tag: Option<&str>, out: &mut Vec<Endpoint>) {
    for item in items {
        if let Some(children) = item.get("item").and_then(Value::as_array) {
            let folder_name = item.get("name").and_then(Value::as_str);
            walk_items(children, folder_name.or(tag), out);
        } else if let Some(req) = item.get("request") {
            out.push(build_endpoint(item, req, tag));
        }
    }
}

fn build_endpoint(item: &Value, req: &Value, tag: Option<&str>) -> Endpoint {
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("request")
        .to_string();
    let method = req
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_uppercase();
    let raw_url = url_to_string(req.get("url"));
    let (base, path_and_query) = split_base_and_path(&raw_url);
    let (path_part, query_part) = split_path_query(&path_and_query);
    let (path, path_params) = postman_url_to_openapi_path(path_part);
    let query_params = query_part.map(query_param_names).unwrap_or_default();

    let mut headers = Vec::new();
    let mut content_type = None;
    if let Some(hdrs) = req.get("header").and_then(Value::as_array) {
        for h in hdrs {
            let key = h.get("key").and_then(Value::as_str).unwrap_or("");
            if key.is_empty() {
                continue;
            }
            if key.eq_ignore_ascii_case("content-type") {
                content_type = h.get("value").and_then(Value::as_str).map(str::to_string);
                continue;
            }
            let disabled = h.get("disabled").and_then(Value::as_bool).unwrap_or(false);
            headers.push((key.to_string(), disabled));
        }
    }

    let body = req.get("body").and_then(|b| {
        if b.is_null() {
            return None;
        }
        let mode = b.get("mode").and_then(Value::as_str).unwrap_or("raw");
        match mode {
            "raw" => {
                let raw = b.get("raw").and_then(Value::as_str)?;
                if raw.trim().is_empty() {
                    return None;
                }
                let example =
                    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()));
                Some(BodyKind::Json {
                    content_type: content_type
                        .clone()
                        .unwrap_or_else(|| "application/json".to_string()),
                    example,
                })
            }
            "formdata" | "urlencoded" => {
                let rows = b.get(mode).and_then(Value::as_array)?;
                let fields = rows
                    .iter()
                    .filter_map(|r| r.get("key").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect();
                Some(BodyKind::Form {
                    urlencoded: mode == "urlencoded",
                    fields,
                })
            }
            _ => None,
        }
    });

    Endpoint {
        method,
        path,
        base,
        tag: tag.map(str::to_string),
        name,
        query_params,
        path_params,
        headers,
        body,
    }
}

/// Rustrest's `PostmanUrl` is just a raw string (or `{raw}`), with no
/// separate host/path split - recovers a server base by peeling off a
/// leading `{{var}}` template (the collection-variable convention this
/// plugin's own importer uses for `baseUrl`) or a literal `scheme://host`.
fn split_base_and_path(url: &str) -> (String, String) {
    if let Some(rest) = url.strip_prefix("{{") {
        if let Some(end) = rest.find("}}") {
            let var_name = rest[..end].trim();
            let path = &rest[end + 2..];
            return (
                format!("{{{var_name}}}"),
                if path.is_empty() {
                    "/".to_string()
                } else {
                    path.to_string()
                },
            );
        }
    }
    if let Some(idx) = url.find("://") {
        let after_scheme = idx + 3;
        return match url[after_scheme..].find('/') {
            Some(slash_rel) => {
                let split_at = after_scheme + slash_rel;
                (url[..split_at].to_string(), url[split_at..].to_string())
            }
            None => (url.to_string(), "/".to_string()),
        };
    }
    (
        "http://localhost".to_string(),
        if url.starts_with('/') {
            url.to_string()
        } else {
            format!("/{url}")
        },
    )
}

fn split_path_query(path_and_query: &str) -> (&str, Option<&str>) {
    match path_and_query.find('?') {
        Some(idx) => (&path_and_query[..idx], Some(&path_and_query[idx + 1..])),
        None => (path_and_query, None),
    }
}

fn query_param_names(qs: &str) -> Vec<String> {
    qs.split('&')
        .filter_map(|pair| pair.split('=').next())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .collect()
}

/// reverse of the importer's path templating: `{{name}}` -> `{name}`.
fn postman_url_to_openapi_path(path: &str) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut rest = path;
    let mut params = Vec::new();
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let name = after[..end].trim();
                out.push('{');
                out.push_str(name);
                out.push('}');
                params.push(name.to_string());
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("{{");
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    (out, params)
}

fn url_to_string(url: Option<&Value>) -> String {
    match url {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(o)) => o
            .get("raw")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

fn operation_id(method: &str, path: &str) -> String {
    let slug: String = path
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("{}{}", method.to_lowercase(), slug)
}

fn collection_variable(collection: &Value, name: &str) -> Option<String> {
    collection
        .get("variable")
        .and_then(Value::as_array)?
        .iter()
        .find(|v| v.get("key").and_then(Value::as_str) == Some(name))
        .and_then(|v| v.get("value"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn build_openapi3(collection: &Value, endpoints: Vec<Endpoint>) -> Value {
    let title = collection
        .pointer("/info/name")
        .and_then(Value::as_str)
        .unwrap_or("Exported API");

    let mut bases: Vec<String> = Vec::new();
    for ep in &endpoints {
        if !bases.contains(&ep.base) {
            bases.push(ep.base.clone());
        }
    }
    let mut servers: Vec<Value> = bases
        .iter()
        .map(|base| openapi_server(base, collection))
        .collect();
    if servers.is_empty() {
        servers.push(json!({ "url": "http://localhost" }));
    }

    let mut paths = Map::new();
    for ep in &endpoints {
        let path_entry = paths.entry(ep.path.clone()).or_insert_with(|| json!({}));
        path_entry[ep.method.to_lowercase().as_str()] = build_openapi_operation(ep);
    }

    json!({
        "openapi": "3.0.3",
        "info": { "title": title, "version": "1.0.0" },
        "servers": servers,
        "paths": Value::Object(paths),
    })
}

fn openapi_server(base: &str, collection: &Value) -> Value {
    match base.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        Some(var_name) => {
            let default = collection_variable(collection, var_name)
                .unwrap_or_else(|| "http://localhost".to_string());
            json!({ "url": base, "variables": { var_name: { "default": default } } })
        }
        None => json!({ "url": base }),
    }
}

fn build_openapi_operation(ep: &Endpoint) -> Value {
    let mut parameters = Vec::new();
    for name in &ep.path_params {
        parameters.push(
            json!({ "name": name, "in": "path", "required": true, "schema": { "type": "string" } }),
        );
    }
    for name in &ep.query_params {
        parameters.push(json!({ "name": name, "in": "query", "required": false, "schema": { "type": "string" } }));
    }
    for (name, disabled) in &ep.headers {
        parameters.push(json!({ "name": name, "in": "header", "required": !disabled, "schema": { "type": "string" } }));
    }

    let mut operation = Map::new();
    operation.insert("summary".to_string(), json!(ep.name));
    operation.insert(
        "operationId".to_string(),
        json!(operation_id(&ep.method, &ep.path)),
    );
    if let Some(tag) = &ep.tag {
        operation.insert("tags".to_string(), json!([tag]));
    }
    if !parameters.is_empty() {
        operation.insert("parameters".to_string(), Value::Array(parameters));
    }
    if let Some(body) = &ep.body {
        operation.insert("requestBody".to_string(), openapi_request_body(body));
    }
    operation.insert(
        "responses".to_string(),
        json!({ "200": { "description": "Successful response" } }),
    );
    Value::Object(operation)
}

fn openapi_request_body(body: &BodyKind) -> Value {
    match body {
        BodyKind::Json {
            content_type,
            example,
        } => json!({ "content": { content_type: { "example": example } } }),
        BodyKind::Form { urlencoded, fields } => {
            let content_type = if *urlencoded {
                "application/x-www-form-urlencoded"
            } else {
                "multipart/form-data"
            };
            let properties: Map<String, Value> = fields
                .iter()
                .map(|f| (f.clone(), json!({ "type": "string" })))
                .collect();
            json!({ "content": { content_type: { "schema": { "type": "object", "properties": properties } } } })
        }
    }
}

fn build_swagger2(collection: &Value, endpoints: Vec<Endpoint>) -> Value {
    let title = collection
        .pointer("/info/name")
        .and_then(Value::as_str)
        .unwrap_or("Exported API");
    let base = endpoints
        .first()
        .map(|e| e.base.clone())
        .unwrap_or_else(|| "http://localhost".to_string());
    let (scheme, host, base_path) = split_swagger_base(&base, collection);

    let mut paths = Map::new();
    for ep in &endpoints {
        let path_entry = paths.entry(ep.path.clone()).or_insert_with(|| json!({}));
        path_entry[ep.method.to_lowercase().as_str()] = build_swagger_operation(ep);
    }

    json!({
        "swagger": "2.0",
        "info": { "title": title, "version": "1.0.0" },
        "host": host,
        "basePath": base_path,
        "schemes": [scheme],
        "paths": Value::Object(paths),
    })
}

fn split_swagger_base(base: &str, collection: &Value) -> (String, String, String) {
    let resolved = match base.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        Some(var_name) => collection_variable(collection, var_name)
            .unwrap_or_else(|| "https://localhost".to_string()),
        None => base.to_string(),
    };

    match resolved.find("://") {
        Some(idx) => {
            let scheme = resolved[..idx].to_string();
            let after_scheme = &resolved[idx + 3..];
            match after_scheme.find('/') {
                Some(slash) => (
                    scheme,
                    after_scheme[..slash].to_string(),
                    after_scheme[slash..].to_string(),
                ),
                None => (scheme, after_scheme.to_string(), String::new()),
            }
        }
        None => ("https".to_string(), resolved, String::new()),
    }
}

fn build_swagger_operation(ep: &Endpoint) -> Value {
    let mut parameters = Vec::new();
    for name in &ep.path_params {
        parameters.push(json!({ "name": name, "in": "path", "required": true, "type": "string" }));
    }
    for name in &ep.query_params {
        parameters
            .push(json!({ "name": name, "in": "query", "required": false, "type": "string" }));
    }
    for (name, disabled) in &ep.headers {
        parameters
            .push(json!({ "name": name, "in": "header", "required": !disabled, "type": "string" }));
    }

    let mut consumes = Vec::new();
    if let Some(body) = &ep.body {
        match body {
            BodyKind::Json {
                content_type,
                example,
            } => {
                consumes.push(content_type.clone());
                parameters.push(json!({ "name": "body", "in": "body", "required": true, "schema": schema_from_example(example) }));
            }
            BodyKind::Form { urlencoded, fields } => {
                consumes.push(
                    if *urlencoded {
                        "application/x-www-form-urlencoded"
                    } else {
                        "multipart/form-data"
                    }
                    .to_string(),
                );
                for field in fields {
                    parameters.push(json!({ "name": field, "in": "formData", "required": false, "type": "string" }));
                }
            }
        }
    }

    let mut operation = Map::new();
    operation.insert("summary".to_string(), json!(ep.name));
    operation.insert(
        "operationId".to_string(),
        json!(operation_id(&ep.method, &ep.path)),
    );
    if let Some(tag) = &ep.tag {
        operation.insert("tags".to_string(), json!([tag]));
    }
    if !consumes.is_empty() {
        operation.insert("consumes".to_string(), json!(consumes));
    }
    if !parameters.is_empty() {
        operation.insert("parameters".to_string(), Value::Array(parameters));
    }
    operation.insert(
        "responses".to_string(),
        json!({ "200": { "description": "Successful response" } }),
    );
    Value::Object(operation)
}

fn schema_from_example(example: &Value) -> Value {
    match example {
        Value::Object(map) => {
            let properties: Map<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), schema_from_example(v)))
                .collect();
            json!({ "type": "object", "properties": properties })
        }
        Value::Array(items) => {
            let item_schema = items
                .first()
                .map(schema_from_example)
                .unwrap_or_else(|| json!({}));
            json!({ "type": "array", "items": item_schema })
        }
        Value::String(_) => json!({ "type": "string" }),
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({ "type": "integer" }),
        Value::Number(_) => json!({ "type": "number" }),
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Null => json!({}),
    }
}
