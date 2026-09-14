//! Field-level conversions shared by the v4 (JSON) and v5.1 (YAML) importers
//! / exporters: header rows, request bodies, urls, and template variable
//! syntax. Insomnia's request/header/body shapes are the same across both
//! export flavors - only the surrounding container (flat resource list with
//! `parentId` for v4, a nested `collection: [...]` tree for v5.1) differs.
//!
//! The target/source of these conversions is Rustrest's own collection
//! model (`rustrest-core::collection::model::PostmanCollection`), which is
//! Postman Collection v2.1 shaped. This plugin builds/reads that shape as
//! plain `serde_json::Value` trees rather than mirroring the struct, since
//! `serde_json::Value` is exactly what crosses the `Plugin::import`/`export`
//! boundary.

use serde_json::{Map, Value, json};

/// Insomnia headers `[{name, value, disabled}]` -> Postman headers
/// `[{key, value, disabled}]`.
pub fn headers_to_postman(headers: Option<&Value>) -> Value {
    let rows = headers.and_then(Value::as_array).cloned().unwrap_or_default();
    Value::Array(
        rows.into_iter()
            .map(|h| {
                json!({
                    "key": h.get("name").and_then(Value::as_str).unwrap_or(""),
                    "value": insomnia_tpl_to_postman(h.get("value").and_then(Value::as_str).unwrap_or("")),
                    "disabled": h.get("disabled").and_then(Value::as_bool).unwrap_or(false),
                })
            })
            .collect(),
    )
}

/// Postman headers `[{key, value, disabled}]` -> Insomnia headers
/// `[{name, value, disabled}]`.
pub fn headers_to_insomnia(headers: Option<&Value>) -> Value {
    let rows = headers.and_then(Value::as_array).cloned().unwrap_or_default();
    Value::Array(
        rows.into_iter()
            .map(|h| {
                json!({
                    "name": h.get("key").and_then(Value::as_str).unwrap_or(""),
                    "value": postman_tpl_to_insomnia(h.get("value").and_then(Value::as_str).unwrap_or("")),
                    "disabled": h.get("disabled").and_then(Value::as_bool).unwrap_or(false),
                })
            })
            .collect(),
    )
}

/// Insomnia body `{mimeType, text?, params?}` -> Postman body
/// `{mode, raw, formdata, urlencoded}` (or `null` for no body).
pub fn body_to_postman(body: Option<&Value>) -> Value {
    let Some(body) = body else { return Value::Null };
    let mime = body.get("mimeType").and_then(Value::as_str).unwrap_or("");
    let params = body.get("params").and_then(Value::as_array);
    let text = body.get("text").and_then(Value::as_str);

    if mime.is_empty() && params.is_none() && text.is_none() {
        return Value::Null;
    }

    if let Some(params) = params {
        let rows: Vec<Value> = params
            .iter()
            .map(|p| {
                json!({
                    "key": p.get("name").and_then(Value::as_str).unwrap_or(""),
                    "value": p.get("value").and_then(Value::as_str).unwrap_or(""),
                    "disabled": p.get("disabled").and_then(Value::as_bool).unwrap_or(false),
                    "type": if p.get("type").and_then(Value::as_str) == Some("file") { "file" } else { "text" },
                })
            })
            .collect();
        let mode = if mime.contains("multipart") { "formdata" } else { "urlencoded" };
        return json!({
            "mode": mode,
            "raw": Value::Null,
            "formdata": if mode == "formdata" { Value::Array(rows.clone()) } else { Value::Null },
            "urlencoded": if mode == "urlencoded" { Value::Array(rows) } else { Value::Null },
        });
    }

    let raw = insomnia_tpl_to_postman(text.unwrap_or(""));
    json!({ "mode": "raw", "raw": raw, "formdata": Value::Null, "urlencoded": Value::Null })
}

/// Postman body `{mode, raw, formdata, urlencoded}` -> Insomnia body
/// `{mimeType, text?, params?}` (or `{}` for no body).
pub fn body_to_insomnia(body: Option<&Value>) -> Value {
    let Some(body) = body else { return json!({}) };
    if body.is_null() {
        return json!({});
    }
    let mode = body.get("mode").and_then(Value::as_str).unwrap_or("raw");
    match mode {
        "formdata" | "urlencoded" => {
            let rows = body.get(mode).and_then(Value::as_array).cloned().unwrap_or_default();
            let params: Vec<Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "name": r.get("key").and_then(Value::as_str).unwrap_or(""),
                        "value": r.get("value").and_then(Value::as_str).unwrap_or(""),
                        "disabled": r.get("disabled").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect();
            let mime = if mode == "formdata" { "multipart/form-data" } else { "application/x-www-form-urlencoded" };
            json!({ "mimeType": mime, "params": params })
        }
        _ => {
            let raw = body.get("raw").and_then(Value::as_str).unwrap_or("");
            if raw.is_empty() {
                return json!({});
            }
            let text = postman_tpl_to_insomnia(raw);
            json!({ "mimeType": guess_mime(raw), "text": text })
        }
    }
}

fn guess_mime(raw: &str) -> &'static str {
    let trimmed = raw.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        "application/json"
    } else {
        "text/plain"
    }
}

/// Postman's `url` field is either a plain string or `{raw, ...}`.
pub fn url_to_string(url: Option<&Value>) -> String {
    match url {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(o)) => o.get("raw").and_then(Value::as_str).unwrap_or("").to_string(),
        _ => String::new(),
    }
}

/// flattens an Insomnia environment `data` object into Postman collection
/// `variable` entries (`[{key, value, type}]`), or `Value::Null` if empty.
pub fn env_data_to_variables(data: &Map<String, Value>) -> Value {
    if data.is_empty() {
        return Value::Null;
    }
    Value::Array(
        data.iter()
            .map(|(k, v)| json!({ "key": k, "value": v, "type": Value::Null }))
            .collect(),
    )
}

/// builds an Insomnia environment `data` object from Postman collection
/// `variable` entries.
pub fn variables_to_env_data(variables: Option<&Value>) -> Option<Map<String, Value>> {
    let vars = variables.and_then(Value::as_array).filter(|v| !v.is_empty())?;
    let mut data = Map::new();
    for v in vars {
        if let Some(key) = v.get("key").and_then(Value::as_str) {
            data.insert(key.to_string(), v.get("value").cloned().unwrap_or(Value::Null));
        }
    }
    Some(data)
}

/// Insomnia templates reference environment data as `{{ _.name }}`; Postman
/// uses `{{name}}` directly. Rewrites every `{{ ... }}` span in `s`.
pub fn insomnia_tpl_to_postman(s: &str) -> String {
    replace_templates(s, |inner| {
        let name = inner.trim().strip_prefix("_.").unwrap_or(inner.trim());
        format!("{{{{{}}}}}", name.trim())
    })
}

/// the reverse of [`insomnia_tpl_to_postman`].
pub fn postman_tpl_to_insomnia(s: &str) -> String {
    replace_templates(s, |inner| format!("{{{{ _.{} }}}}", inner.trim()))
}

fn replace_templates(s: &str, f: impl Fn(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                out.push_str(&f(&after[..end]));
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
    out
}
