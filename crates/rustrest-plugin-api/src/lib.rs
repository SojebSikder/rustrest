//! SDK for writing native Rustrest plugins.
//!
//! A plugin is a Rust crate compiled to `wasm32-unknown-unknown`, plus a
//! `plugin.toml` manifest (see [`PluginManifest`]) sitting next to it in the
//! same install directory - the host reads that file directly and never
//! executes guest code just to learn what a plugin declares.
//!
//! Implement [`Plugin`] for a type, then call [`export_plugin!`] once at the
//! crate root to generate the wasm export glue the host
//! (`rustrest-plugin-host`) calls into:
//!
//! ```ignore
//! #[derive(Default)]
//! struct MyPlugin;
//!
//! impl rustrest_plugin_api::Plugin for MyPlugin {
//!     fn on_command(&mut self, command_id: &str) -> Result<Option<String>, String> {
//!         // ...
//!         # unimplemented!()
//!     }
//! }
//!
//! rustrest_plugin_api::export_plugin!(MyPlugin);
//! ```

mod hooks;
#[cfg(target_arch = "wasm32")]
mod host;
#[cfg(target_arch = "wasm32")]
mod hostcall;
mod manifest;
pub mod network;
mod plugin;
pub mod process;
mod ui;

// guest-only wasm export glue for `export_plugin!` below - meaningless (and
// not needed) in a native build, since only a `wasm32` plugin crate ever
// invokes that macro.
#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub mod _internal {
    pub use crate::runtime::{alloc, dealloc, dispatch};
}
#[cfg(target_arch = "wasm32")]
mod runtime;

pub use hooks::{
    CollectionOperation, CollectionSummary, RequestContext, RequestPatch, RequestSummary,
    ResponseContext, RightPanelAction, RightPanelContext, TestResult,
};
#[cfg(target_arch = "wasm32")]
pub use host::log;
pub use manifest::{
    CURRENT_SCHEMA_VERSION, Capability, CommandDef, FormatDef, MenuItemDef, PanelDef,
    PluginManifest,
};
#[cfg(target_arch = "wasm32")]
pub use network::http_request;
pub use network::{HttpRequestSpec, HttpResponseData};
pub use plugin::Plugin;
#[cfg(target_arch = "wasm32")]
pub use process::Process;
pub use process::{CommandOutput, ProcessStream};
#[cfg(target_arch = "wasm32")]
pub use process::{storage_read, storage_write};
pub use ui::{UiEvent, UiNode};

/// Generates the `rustrest_alloc` / `rustrest_dealloc` / `rustrest_call`
/// wasm exports the host uses to drive a [`Plugin`] implementation. Call
/// this exactly once, at the crate root, for the plugin's entry-point type.
#[macro_export]
macro_rules! export_plugin {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn rustrest_alloc(len: u32) -> u32 {
            $crate::_internal::alloc(len)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn rustrest_dealloc(ptr: u32, len: u32) {
            $crate::_internal::dealloc(ptr, len)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn rustrest_call(ptr: u32, len: u32) -> u64 {
            static INSTANCE: ::std::sync::Mutex<::std::option::Option<$ty>> =
                ::std::sync::Mutex::new(::std::option::Option::None);
            let mut guard = match INSTANCE.lock() {
                ::std::result::Result::Ok(g) => g,
                ::std::result::Result::Err(poisoned) => poisoned.into_inner(),
            };
            if guard.is_none() {
                *guard = ::std::option::Option::Some(<$ty as ::std::default::Default>::default());
            }
            let plugin = guard.as_mut().expect("plugin instance just initialized");
            $crate::_internal::dispatch(plugin, ptr, len)
        }
    };
}
