//! Safe wrapper over the `log` host call (dispatched host-side by
//! `rustrest-plugin-host::hostcall`). `wasm32`-only

#[cfg(target_arch = "wasm32")]
use crate::hostcall;

/// logs a line, attributed to this plugin, into the app's console panel.
#[cfg(target_arch = "wasm32")]
pub fn log(message: &str) {
    let _: Result<(), String> = hostcall::call("log", message);
}
