//! Host-side runtime that discovers, sandboxes (via `wasmtime`), and drives
//! Rustrest native plugins built against `rustrest-plugin-api`.

mod codec;
mod error;
mod instance;
mod manager;
mod state;

pub use error::PluginError;
pub use instance::LoadedPlugin;
pub use manager::PluginManager;

pub use wasmtime::Module;

pub use rustrest_plugin_api::{
    Capability, CommandDef, FormatDef, MenuItemDef, PanelDef, PluginManifest, RequestContext,
    ResponseContext, UiEvent, UiNode,
};
