//! Native Rustrest plugin: import/export of Insomnia collections, both the
//! legacy v4 JSON export and the v5.1 YAML export. Build with:
//!
//! ```text
//! rustup target add wasm32-unknown-unknown
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! then copy `target/wasm32-unknown-unknown/release/rustrest_plugin_insomnia.wasm`
//! to `<plugins-dir>/insomnia/plugin.wasm` (the containing directory name
//! must match the plugin's `manifest().id`, here `"insomnia"`).
//!
//! Conversions target Rustrest's own collection JSON model, which is
//! Postman Collection v2.1 shaped (see
//! `rustrest-core::collection::model::PostmanCollection`); see `convert.rs`
//! for the field-level mapping and its documented limitations (no
//! structured auth, no cookie jar, single flattened base environment).

mod codec;
mod convert;
mod ids;
mod v4;
mod v5;

use rustrest_plugin_api::{Capability, FormatDef, Plugin, PluginManifest};

#[derive(Default)]
struct InsomniaPlugin;

impl Plugin for InsomniaPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            id: "insomnia".to_string(),
            name: "Insomnia Collections".to_string(),
            version: "0.1.0".to_string(),
            author: "Rustrest".to_string(),
            description: "Import/export Insomnia v4 (JSON) and v5.1 (YAML) collections."
                .to_string(),
            capabilities: vec![
                Capability::ImportFormat(FormatDef {
                    id: "insomnia".to_string(),
                    title: "Insomnia (v4 JSON / v5.1 YAML)".to_string(),
                    extensions: vec!["json".to_string(), "yaml".to_string(), "yml".to_string()],
                }),
                Capability::ExportFormat(FormatDef {
                    id: "insomnia-v4".to_string(),
                    title: "Insomnia v4 (JSON)".to_string(),
                    extensions: vec!["json".to_string()],
                }),
                Capability::ExportFormat(FormatDef {
                    id: "insomnia-v5".to_string(),
                    title: "Insomnia v5.1 (YAML)".to_string(),
                    extensions: vec!["yaml".to_string()],
                }),
            ],
        }
    }

    fn import(&mut self, format_id: &str, bytes: Vec<u8>) -> Result<serde_json::Value, String> {
        if format_id != "insomnia" {
            return Err(format!("unknown import format: {format_id}"));
        }
        // v4 is JSON, v5.1 is YAML - sniff on the first non-whitespace byte
        // rather than trusting the file extension, since both `.json` and
        // `.yaml` end up mapped to the same "insomnia" import id.
        let looks_like_json = bytes
            .iter()
            .find(|b| !b.is_ascii_whitespace())
            .is_some_and(|b| *b == b'{');

        if looks_like_json {
            v4::to_postman(codec::decode_json(&bytes)?)
        } else {
            v5::to_postman(codec::decode_yaml(&bytes)?)
        }
    }

    fn export(
        &mut self,
        format_id: &str,
        collection: serde_json::Value,
    ) -> Result<Vec<u8>, String> {
        match format_id {
            "insomnia-v4" => codec::encode_json(&v4::from_postman(&collection)),
            "insomnia-v5" => codec::encode_yaml(&v5::from_postman(&collection)),
            other => Err(format!("unknown export format: {other}")),
        }
    }
}

rustrest_plugin_api::export_plugin!(InsomniaPlugin);
