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

/// One entry a plugin contributes to the bottom status bar. Static: read
/// once from the manifest at load time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusBarItemDef {
    /// stable id, unique within the plugin.
    pub id: String,
    pub label: String,
    /// command id, dispatched the same way as a command-palette entry
    /// (via `Plugin::on_command`) when the item is clicked. `None` renders
    /// it as plain, non-interactive text.
    #[serde(default)]
    pub command_id: Option<String>,
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
    /// a panel docked in the app's right-hand panel (distinct from
    /// `SidebarPanel`, which opens as a tab): rendered with ambient
    /// `RightPanelContext` (the active request/response, if any) and able to
    /// hand back a `RequestPatch` the host applies to the active tab.
    RightPanel(PanelDef),
    ImportFormat(FormatDef),
    ExportFormat(FormatDef),
    /// an entry in the bottom status bar
    StatusBarItem(StatusBarItemDef),
    /// unlocks the `which` / `download_file` / `make_executable` /
    /// `storage_dir` / `run_command` / `process_*` host calls. Declared separately from the other
    /// capabilities because it's the one that lets a plugin touch the
    /// outside world (network, filesystem under its storage dir, processes).
    ExternalProcess,
}

/// manifest schema version
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// stable, unique id (also the plugin's directory name under the plugins dir).
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    /// manifest schema version the plugin was written against
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

fn default_schema_version() -> u32 {
    1
}
