//! Host-side runtime that discovers, sandboxes (via `wasmtime`), and drives
//! Rustrest native plugins built against `rustrest-plugin-api`.

mod codec;
mod error;
mod files;
mod hostcall;
mod instance;
mod manager;
mod manifest_toml;
mod network;
mod process;
mod state;

pub use error::PluginError;
pub use instance::LoadedPlugin;
pub use manager::{PluginManager, PreparedPlugin};

pub use wasmtime::{Engine, Module};

pub use rustrest_plugin_api::{
    API_VERSION as PLUGIN_API_VERSION, CURRENT_SCHEMA_VERSION as PLUGIN_SCHEMA_VERSION, Capability,
    CollectionOperation, CollectionSummary, CommandDef, EnvSummary, FormatDef, HttpResponseData,
    MenuItemDef, PanelDef, PickFilesResult, PickedFile, PluginManifest, ProcessStream,
    RequestContext, RequestPatch, RequestSummary, ResponseContext, RightPanelAction,
    RightPanelContext, StatusBarItemDef, UiEvent, UiNode,
};
