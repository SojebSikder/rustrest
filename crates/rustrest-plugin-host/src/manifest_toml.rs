use crate::error::PluginError;
use rustrest_plugin_api::{
    Capability, CommandDef, FormatDef, LanguageServerDef, MenuItemDef, PanelDef, PluginManifest,
    StatusBarItemDef,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE_NAME: &str = "plugin.toml";
pub const WASM_FILE_NAME: &str = "plugin.wasm";
/// a zed extension's manifest - accepted in place of `plugin.toml`, so a zed theme extension can be installed as is.
pub const ZED_MANIFEST_FILE_NAME: &str = "extension.toml";
/// theme family JSON files an extension contributes, auto-discovered like zed's `themes/` directory (no manifest entry needed).
pub const THEMES_DIR_NAME: &str = "themes";

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
    right_panel: Option<PanelDef>,
    #[serde(default)]
    import_formats: Vec<FormatDef>,
    #[serde(default)]
    export_formats: Vec<FormatDef>,
    #[serde(default)]
    status_bar_items: Vec<StatusBarItemDef>,
    #[serde(default)]
    language_servers: Vec<LanguageServerDef>,
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
    if let Some(panel) = raw.capabilities.right_panel {
        capabilities.push(Capability::RightPanel(panel));
    }
    for format in raw.capabilities.import_formats {
        capabilities.push(Capability::ImportFormat(format));
    }
    for format in raw.capabilities.export_formats {
        capabilities.push(Capability::ExportFormat(format));
    }
    for item in raw.capabilities.status_bar_items {
        capabilities.push(Capability::StatusBarItem(item));
    }

    for server in raw.capabilities.language_servers {
        capabilities.push(Capability::LanguageServer(server));
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

#[derive(Debug, Deserialize)]
struct ZedManifest {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    description: String,
}

/// parses a Zed `extension.toml`. Only its metadata is used, whatever it contributes that
/// Rustrest understands (themes) is discovered from the directory layout, as Zed does.
pub fn parse_zed(toml_source: &str) -> Result<PluginManifest, PluginError> {
    let raw: ZedManifest = toml::from_str(toml_source)
        .map_err(|e| PluginError::Manifest(format!("invalid {ZED_MANIFEST_FILE_NAME}: {e}")))?;
    Ok(PluginManifest {
        id: raw.id,
        name: raw.name,
        version: raw.version,
        author: raw.authors.join(", "),
        description: raw.description,
        schema_version: rustrest_plugin_api::CURRENT_SCHEMA_VERSION,
        capabilities: Vec::new(),
    })
}

/// the manifest file in `dir`: `plugin.toml`, else a Zed `extension.toml`.
pub fn manifest_path(dir: &Path) -> Option<PathBuf> {
    [MANIFEST_FILE_NAME, ZED_MANIFEST_FILE_NAME]
        .iter()
        .map(|name| dir.join(name))
        .find(|p| p.is_file())
}

/// reads and parses `<dir>/plugin.toml` (or `<dir>/extension.toml`).
pub fn read_from_dir(dir: &Path) -> Result<PluginManifest, PluginError> {
    let path = manifest_path(dir).ok_or_else(|| {
        PluginError::Manifest(format!(
            "missing {MANIFEST_FILE_NAME} (or {ZED_MANIFEST_FILE_NAME})"
        ))
    })?;
    let content = std::fs::read_to_string(&path)?;
    if path
        .file_name()
        .is_some_and(|n| n == ZED_MANIFEST_FILE_NAME)
    {
        parse_zed(&content)
    } else {
        parse(&content)
    }
}

/// `themes/*.json` files in an extension directory, sorted.
pub fn theme_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join(THEMES_DIR_NAME))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_zed_extension_manifest() {
        let manifest = parse_zed(
            r#"
id = "catppuccin"
name = "Catppuccin"
description = "Soothing pastel theme"
version = "0.2.0"
schema_version = 1
authors = ["A <a@example.com>", "B"]
repository = "https://github.com/catppuccin/zed"
"#,
        )
        .unwrap();
        assert_eq!(manifest.id, "catppuccin");
        assert_eq!(manifest.author, "A <a@example.com>, B");
        assert!(manifest.capabilities.is_empty());
    }
}
