//! Bytes <-> `serde_json::Value` codecs for Insomnia's two export shapes:
//! v4 is plain JSON, v5.1 is YAML. `serde_json::Value` implements the
//! generic serde data model, so it can be produced/consumed by either
//! format's (de)serializer without an intermediate typed struct.

use serde_json::Value;

pub fn decode_json(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("invalid JSON: {e}"))
}

pub fn decode_yaml(bytes: &[u8]) -> Result<Value, String> {
    serde_yaml::from_slice(bytes).map_err(|e| format!("invalid YAML: {e}"))
}

pub fn encode_json(value: &Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(value).map_err(|e| format!("failed to encode JSON: {e}"))
}

pub fn encode_yaml(value: &Value) -> Result<Vec<u8>, String> {
    serde_yaml::to_string(value)
        .map(|s| s.into_bytes())
        .map_err(|e| format!("failed to encode YAML: {e}"))
}
