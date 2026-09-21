//! Generates a plausible sample JSON value from an OpenAPI/JSON-Schema
//! `schema` object, for use as a request body's raw content. Resolves
//! `$ref` pointers against the whole document (`#/components/schemas/...`
//! in OpenAPI 3.x, `#/definitions/...` in Swagger 2.0 - both are plain JSON
//! Pointers, so `Value::pointer` handles either uniformly).

use serde_json::{Map, Value, json};

const MAX_DEPTH: usize = 8;

pub fn example_from_schema(schema: &Value, doc: &Value, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return Value::Null;
    }

    if let Some(ref_path) = schema.get("$ref").and_then(Value::as_str) {
        return match ref_path.strip_prefix('#').and_then(|p| doc.pointer(p)) {
            Some(target) => example_from_schema(target, doc, depth + 1),
            None => Value::Null,
        };
    }

    if let Some(example) = schema.get("example") {
        return example.clone();
    }
    if let Some(examples) = schema.get("examples") {
        match examples {
            Value::Array(arr) => {
                if let Some(first) = arr.first() {
                    return first.clone();
                }
            }
            Value::Object(map) => {
                if let Some(first) = map.values().next() {
                    return first.get("value").cloned().unwrap_or_else(|| first.clone());
                }
            }
            _ => {}
        }
    }
    if let Some(default) = schema.get("default") {
        return default.clone();
    }
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        if let Some(first) = enum_values.first() {
            return first.clone();
        }
    }

    for combinator in ["allOf", "oneOf", "anyOf"] {
        if let Some(subschemas) = schema.get(combinator).and_then(Value::as_array) {
            if combinator == "allOf" {
                let mut merged = Map::new();
                for sub in subschemas {
                    let resolved = resolve(sub, doc, depth);
                    if let Some(props) = resolved.get("properties").and_then(Value::as_object) {
                        for (k, v) in props {
                            merged.insert(k.clone(), example_from_schema(v, doc, depth + 1));
                        }
                    }
                }
                if !merged.is_empty() {
                    return Value::Object(merged);
                }
            } else if let Some(first) = subschemas.first() {
                return example_from_schema(first, doc, depth + 1);
            }
        }
    }

    let ty = schema.get("type").and_then(Value::as_str).unwrap_or_else(|| {
        if schema.get("properties").is_some() {
            "object"
        } else if schema.get("items").is_some() {
            "array"
        } else {
            ""
        }
    });

    match ty {
        "object" => {
            let mut obj = Map::new();
            if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                for (key, prop_schema) in props {
                    obj.insert(key.clone(), example_from_schema(prop_schema, doc, depth + 1));
                }
            } else if let Some(additional) = schema.get("additionalProperties") {
                if additional.is_object() {
                    obj.insert("key1".to_string(), example_from_schema(additional, doc, depth + 1));
                }
            }
            Value::Object(obj)
        }
        "array" => {
            let item_example = schema
                .get("items")
                .map(|items| example_from_schema(items, doc, depth + 1))
                .unwrap_or(Value::Null);
            Value::Array(vec![item_example])
        }
        "string" => match schema.get("format").and_then(Value::as_str) {
            Some("date-time") => json!("2024-01-01T00:00:00Z"),
            Some("date") => json!("2024-01-01"),
            Some("email") => json!("user@example.com"),
            Some("uuid") => json!("00000000-0000-0000-0000-000000000000"),
            Some("uri") | Some("url") => json!("https://example.com"),
            _ => json!("string"),
        },
        "integer" => schema.get("minimum").cloned().unwrap_or(json!(0)),
        "number" => schema.get("minimum").cloned().unwrap_or(json!(0.0)),
        "boolean" => json!(true),
        _ => Value::Null,
    }
}

fn resolve<'a>(schema: &'a Value, doc: &'a Value, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return schema.clone();
    }
    match schema.get("$ref").and_then(Value::as_str) {
        Some(ref_path) => ref_path.strip_prefix('#').and_then(|p| doc.pointer(p)).cloned().unwrap_or(Value::Null),
        None => schema.clone(),
    }
}
