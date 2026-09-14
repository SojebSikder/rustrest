//! SDK for writing native Rustrest plugins.
//!
//! A plugin is a Rust crate compiled to `wasm32-unknown-unknown`. Implement
//! [`Plugin`] for a type, then call [`export_plugin!`] once at the crate
//! root to generate the wasm export glue the host (`rustrest-plugin-host`)
//! calls into:
//!
//! ```ignore
//! #[derive(Default)]
//! struct MyPlugin;
//!
//! impl rustrest_plugin_api::Plugin for MyPlugin {
//!     fn manifest(&self) -> rustrest_plugin_api::PluginManifest {
//!         // ...
//!         # unimplemented!()
//!     }
//! }
//!
//! rustrest_plugin_api::export_plugin!(MyPlugin);
//! ```

mod hooks;
mod host;
mod manifest;
mod plugin;
mod ui;

#[doc(hidden)]
pub mod _internal {
    pub use crate::runtime::{alloc, dealloc, dispatch};
}
mod runtime;

pub use hooks::{RequestContext, ResponseContext, TestResult};
pub use host::log;
pub use manifest::{Capability, CommandDef, FormatDef, MenuItemDef, PanelDef, PluginManifest};
pub use plugin::Plugin;
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
