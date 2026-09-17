//! Host-side runtime that discovers, sandboxes (via `wasmtime`), and drives
//! Rustrest native plugins built against `rustrest-plugin-api`.

mod codec;
mod error;
mod hostcall;
mod instance;
mod manager;
mod manifest_toml;
mod network;
mod process;
mod state;

pub use error::PluginError;
pub use instance::LoadedPlugin;
pub use manager::PluginManager;

pub use wasmtime::{Engine, Module};

pub use rustrest_plugin_api::{
    Capability, CollectionOperation, CollectionSummary, CommandDef, FormatDef, HttpResponseData,
    MenuItemDef, PanelDef, PluginManifest, ProcessStream, RequestContext, RequestPatch,
    RequestSummary, ResponseContext, RightPanelAction, RightPanelContext, UiEvent, UiNode,
};
