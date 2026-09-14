use serde::{Deserialize, Serialize};

/// One command a plugin contributes to the command palette / menu bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDef {
    /// stable id, unique within the plugin, passed back via `Plugin::on_command`.
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
}

/// One entry a plugin contributes to the top menu bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItemDef {
    /// menu group to append to (e.g. "File"); groups that don't already
    /// exist are created as a new top-level "Plugins" group by the host.
    pub group: String,
    pub label: String,
    /// command id, dispatched the same way as a command-palette entry.
    pub command_id: String,
}

/// A sidebar-hosted panel a plugin contributes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelDef {
    pub id: String,
    pub title: String,
}

/// A collection import format a plugin can decode into Rustrest's own
/// collection JSON model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatDef {
    pub id: String,
    pub title: String,
    /// file-picker filter extensions, e.g. `["yaml", "yml"]`.
    pub extensions: Vec<String>,
}

/// Declared once by `Plugin::manifest`; the host only ever calls into
/// capabilities a plugin actually opted into.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum Capability {
    /// participate in every outgoing request / incoming response.
    RequestHooks,
    Commands(Vec<CommandDef>),
    MenuItems(Vec<MenuItemDef>),
    SidebarPanel(PanelDef),
    ImportFormat(FormatDef),
    ExportFormat(FormatDef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// stable, unique id (also the plugin's directory name under the plugins dir).
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}
