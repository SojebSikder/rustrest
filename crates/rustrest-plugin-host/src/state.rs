use crate::error::PluginError;
use std::sync::{Arc, Mutex};
use wasmtime::{Caller, Extern, Linker};

/// per-plugin `wasmtime::Store` data. Kept deliberately tiny: a plugin id
/// (for attributing log lines) and a shared sink the host drains after each
/// call into the app's existing console panel.
pub struct PluginState {
    pub plugin_id: String,
    pub logs: Arc<Mutex<Vec<String>>>,
}

/// links the small set of host functions plugins can import. Anything not
/// linked here simply isn't available to a plugin - there is no dynamic
/// permission negotiation in v1, just a fixed, minimal host surface.
pub fn link_host_functions(linker: &mut Linker<PluginState>) -> Result<(), PluginError> {
    linker.func_wrap(
        "env",
        "host_log",
        |mut caller: Caller<'_, PluginState>, ptr: u32, len: u32| {
            let memory = match caller.get_export("memory") {
                Some(Extern::Memory(m)) => m,
                _ => return,
            };
            let mut buf = vec![0u8; len as usize];
            if memory.read(&caller, ptr as usize, &mut buf).is_err() {
                return;
            }
            let Ok(message) = String::from_utf8(buf) else {
                return;
            };
            let plugin_id = caller.data().plugin_id.clone();
            if let Ok(mut logs) = caller.data_mut().logs.lock() {
                logs.push(format!("[plugin:{plugin_id}] {message}"));
            }
        },
    )?;
    Ok(())
}
