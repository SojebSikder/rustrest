//! Generates a placeholder JSON body from a message descriptor, so the
//! request editor starts out showing every field the method expects instead
//! of an empty `{}`.

use prost_reflect::{FieldDescriptor, Kind, MessageDescriptor};
use serde_json::{Map, Value};

const MAX_DEPTH: u8 = 6;

pub fn json_skeleton(desc: &MessageDescriptor) -> Value {
    build(desc, MAX_DEPTH)
}

fn build(desc: &MessageDescriptor, depth: u8) -> Value {
    let mut obj = Map::new();
    for field in desc.fields() {
        obj.insert(field.json_name().to_string(), field_value(&field, depth));
    }
    Value::Object(obj)
}

fn field_value(field: &FieldDescriptor, depth: u8) -> Value {
    if field.is_map() {
        return Value::Object(Map::new());
    }
    if field.is_list() {
        return Value::Array(vec![]);
    }
    scalar_value(field, depth)
}

fn scalar_value(field: &FieldDescriptor, depth: u8) -> Value {
    match field.kind() {
        Kind::Message(nested) => {
            if depth == 0 {
                Value::Object(Map::new())
            } else {
                build(&nested, depth - 1)
            }
        }
        Kind::Enum(e) => e
            .values()
            .next()
            .map(|v| Value::String(v.name().to_string()))
            .unwrap_or(Value::Null),
        Kind::String | Kind::Bytes => Value::String(String::new()),
        Kind::Bool => Value::Bool(false),
        Kind::Double | Kind::Float => Value::from(0.0f64),
        _ => Value::from(0),
    }
}
