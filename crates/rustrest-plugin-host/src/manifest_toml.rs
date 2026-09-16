use crate::error::PluginError;
use rustrest_plugin_api::{
    Capability, CommandDef, FormatDef, MenuItemDef, PanelDef, PluginManifest,
};
use serde::Deserialize;
use std::path::Path;

pub const MANIFEST_FILE_NAME: &str = "plugin.toml";
pub const WASM_FILE_NAME: &str = "plugin.wasm";

#[derive(Debug, Deserialize)]
struct TomlManifest {
    id: String,
    name: String,
    version: String,
    author: String,
    description: String,
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    capabilities: TomlCapabilities,
}

fn default_schema_version() -> u32 {
    rustrest_plugin_api::CURRENT_SCHEMA_VERSION
}

#[derive(Debug, Default, Deserialize)]
struct TomlCapabilities {
    #[serde(default)]
    request_hooks: bool,
    #[serde(default)]
    external_process: bool,
    #[serde(default)]
    commands: Vec<CommandDef>,
    #[serde(default)]
    menu_items: Vec<MenuItemDef>,
    #[serde(default)]
    sidebar_panel: Option<PanelDef>,
    #[serde(default)]
    import_formats: Vec<FormatDef>,
    #[serde(default)]
    export_formats: Vec<FormatDef>,
}

pub fn parse(toml_source: &str) -> Result<PluginManifest, PluginError> {
    let raw: TomlManifest = toml::from_str(toml_source)
        .map_err(|e| PluginError::Manifest(format!("invalid {MANIFEST_FILE_NAME}: {e}")))?;

    if raw.schema_version > rustrest_plugin_api::CURRENT_SCHEMA_VERSION {
        return Err(PluginError::Manifest(format!(
            "plugin '{}' requires manifest schema version {}, this build only understands up to {}",
            raw.id,
            raw.schema_version,
            rustrest_plugin_api::CURRENT_SCHEMA_VERSION
        )));
    }

    let mut capabilities = Vec::new();
    if raw.capabilities.request_hooks {
        capabilities.push(Capability::RequestHooks);
    }
    if raw.capabilities.external_process {
        capabilities.push(Capability::ExternalProcess);
    }
    if !raw.capabilities.commands.is_empty() {
        capabilities.push(Capability::Commands(raw.capabilities.commands));
    }
    if !raw.capabilities.menu_items.is_empty() {
        capabilities.push(Capability::MenuItems(raw.capabilities.menu_items));
    }
    if let Some(panel) = raw.capabilities.sidebar_panel {
        capabilities.push(Capability::SidebarPanel(panel));
    }
    for format in raw.capabilities.import_formats {
        capabilities.push(Capability::ImportFormat(format));
    }
    for format in raw.capabilities.export_formats {
        capabilities.push(Capability::ExportFormat(format));
    }

    Ok(PluginManifest {
        id: raw.id,
        name: raw.name,
        version: raw.version,
        author: raw.author,
        description: raw.description,
        schema_version: raw.schema_version,
        capabilities,
    })
}

/// reads and parses `<dir>/plugin.toml`.
pub fn read_from_dir(dir: &Path) -> Result<PluginManifest, PluginError> {
    let path = dir.join(MANIFEST_FILE_NAME);
    let content = std::fs::read_to_string(&path)?;
    parse(&content)
}
