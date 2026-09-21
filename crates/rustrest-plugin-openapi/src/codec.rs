//! Bytes <-> `serde_json::Value` codecs. Both OpenAPI 3.x and Swagger 2.0
//! documents are commonly distributed as either JSON or YAML, so every
//! import/export path in this plugin goes through one of these four
//! functions rather than assuming a format from the file extension.

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

/// sniffs JSON, YAML from the first non-whitespace byte rather than
/// trusting the file extension, since both flavors are mapped to the same
/// "openapi" import id.
pub fn looks_like_json(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'{')
}
