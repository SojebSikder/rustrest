//! Host side of the JSON-over-linear-memory ABI a plugin's
//! `rustrest_plugin_api::export_plugin!` macro implements. See
//! `rustrest-plugin-api::runtime` for the guest side and the wire format.

use crate::error::PluginError;
use crate::state::PluginState;
use serde::Serialize;
use serde::de::DeserializeOwned;
use wasmtime::{Instance, Memory, Store, TypedFunc};

/// fuel budget for a single plugin call, guarding against a runaway or
/// malicious plugin hanging the UI thread. Cheap calls (a command, a panel
/// render) use a tiny fraction of this; it mainly exists to bound infinite
/// loops.
const CALL_FUEL: u64 = 200_000_000;

pub struct CallHandles {
    alloc: TypedFunc<u32, u32>,
    dealloc: TypedFunc<(u32, u32), ()>,
    call: TypedFunc<(u32, u32), u64>,
    memory: Memory,
}

impl CallHandles {
    pub fn resolve(
        store: &mut Store<PluginState>,
        instance: &Instance,
        plugin_id: &str,
    ) -> Result<Self, PluginError> {
        let alloc = instance.get_typed_func::<u32, u32>(&mut *store, "rustrest_alloc")?;
        let dealloc = instance.get_typed_func::<(u32, u32), ()>(&mut *store, "rustrest_dealloc")?;
        let call = instance.get_typed_func::<(u32, u32), u64>(&mut *store, "rustrest_call")?;
        let memory = instance
            .get_memory(&mut *store, "memory")
            .ok_or_else(|| PluginError::NoMemory(plugin_id.to_string()))?;
        Ok(Self {
            alloc,
            dealloc,
            call,
            memory,
        })
    }

    /// serializes `payload` under `fn_name`, sends it to the guest, and
    /// deserializes the `Out` the plugin returned. Returns `PluginError::Plugin`
    /// if the plugin itself reported an error rather than trapping.
    pub fn call_json<In: Serialize, Out: DeserializeOwned>(
        &self,
        store: &mut Store<PluginState>,
        fn_name: &str,
        payload: In,
    ) -> Result<Out, PluginError> {
        store.set_fuel(CALL_FUEL)?;

        let envelope = serde_json::json!({ "fn": fn_name, "payload": payload });
        let bytes = serde_json::to_vec(&envelope)?;

        let in_ptr = self.alloc.call(&mut *store, bytes.len() as u32)?;
        self.memory
            .write(&mut *store, in_ptr as usize, &bytes)
            .map_err(|e| PluginError::Memory(e.to_string()))?;

        let packed = self.call.call(&mut *store, (in_ptr, bytes.len() as u32))?;
        self.dealloc
            .call(&mut *store, (in_ptr, bytes.len() as u32))?;

        let out_ptr = (packed >> 32) as u32;
        let out_len = (packed & 0xffff_ffff) as u32;
        let mut buf = vec![0u8; out_len as usize];
        self.memory
            .read(&mut *store, out_ptr as usize, &mut buf)
            .map_err(|e| PluginError::Memory(e.to_string()))?;
        self.dealloc.call(&mut *store, (out_ptr, out_len))?;

        let value: serde_json::Value = serde_json::from_slice(&buf)?;
        if let Some(err) = value.get("err").and_then(|v| v.as_str()) {
            return Err(PluginError::Plugin(err.to_string()));
        }
        let ok = value.get("ok").cloned().unwrap_or(serde_json::Value::Null);
        Ok(serde_json::from_value(ok)?)
    }
}
