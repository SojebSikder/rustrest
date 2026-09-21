//! Native Rustrest plugin: import/export of OpenAPI 3.0/3.1 and Swagger 2.0
//! API definitions, as either JSON or YAML. Build with:
//!
//! ```text
//! rustup target add wasm32-unknown-unknown
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! then copy both `target/wasm32-unknown-unknown/release/rustrest_plugin_openapi.wasm`
//! and `plugin.toml` into `<plugins-dir>/openapi/` (the containing
//! directory name must match the `id` declared in `plugin.toml`, here
//! `"openapi"`).
//!
//! Conversions target Rustrest's own collection JSON model, which is
//! Postman Collection v2.1 shaped (see
//! `rustrest-core::collection::model::PostmanCollection`); see `import.rs`
//! and `export.rs` for the field-level mapping and its documented
//! limitations (parameters are always typed as strings, request bodies are
//! reconstructed from a single example rather than a full schema, no
//! response examples).

mod codec;
mod export;
mod import;
mod schema;

use rustrest_plugin_api::Plugin;

#[derive(Default)]
struct OpenApiPlugin;

impl Plugin for OpenApiPlugin {
    fn import(&mut self, format_id: &str, bytes: Vec<u8>) -> Result<serde_json::Value, String> {
        if format_id != "openapi" {
            return Err(format!("unknown import format: {format_id}"));
        }
        let doc = if codec::looks_like_json(&bytes) { codec::decode_json(&bytes)? } else { codec::decode_yaml(&bytes)? };
        import::to_postman(doc)
    }

    fn export(&mut self, format_id: &str, collection: serde_json::Value) -> Result<Vec<u8>, String> {
        match format_id {
            "openapi-3.0" => codec::encode_json(&export::to_openapi3(&collection)),
            "openapi-3.0-yaml" => codec::encode_yaml(&export::to_openapi3(&collection)),
            "swagger-2.0" => codec::encode_json(&export::to_swagger2(&collection)),
            "swagger-2.0-yaml" => codec::encode_yaml(&export::to_swagger2(&collection)),
            other => Err(format!("unknown export format: {other}")),
        }
    }
}

rustrest_plugin_api::export_plugin!(OpenApiPlugin);
