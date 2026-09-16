//! Guest-side half of the guest→host call channel. Mirrors the host→guest
//! channel in `runtime.rs` (JSON envelope, packed `(ptr, len)` response)
//!
//! Plugin authors should never call [`call`] directly - it's the transport
//! `hooks.rs`/`process.rs` build safe, typed wrappers on top of.

use serde::Serialize;
use serde::de::DeserializeOwned;

unsafe extern "C" {
    /// sends a JSON envelope `{"fn": "...", "payload": ...}` (at `ptr`/`len`,
    /// in the guest's own memory) to the host and returns a packed
    /// `(ptr, len)` pointing at a JSON `{"ok": ...}` / `{"err": "..."}`
    /// response the host wrote into a buffer obtained via this module's
    /// exported `rustrest_alloc`.
    fn host_call(ptr: u32, len: u32) -> u64;
}

pub fn call<In: Serialize, Out: DeserializeOwned>(
    fn_name: &str,
    payload: In,
) -> Result<Out, String> {
    let envelope = serde_json::json!({ "fn": fn_name, "payload": payload });
    let bytes = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;

    let packed = unsafe { host_call(bytes.as_ptr() as u32, bytes.len() as u32) };
    let out_ptr = (packed >> 32) as u32;
    let out_len = (packed & 0xffff_ffff) as u32;

    let out_bytes = if out_len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(out_ptr as *const u8, out_len as usize) }.to_vec()
    };
    crate::runtime::dealloc(out_ptr, out_len);

    let value: serde_json::Value = serde_json::from_slice(&out_bytes).map_err(|e| e.to_string())?;
    if let Some(err) = value.get("err").and_then(|v| v.as_str()) {
        return Err(err.to_string());
    }
    let ok = value.get("ok").cloned().unwrap_or(serde_json::Value::Null);
    serde_json::from_value(ok).map_err(|e| e.to_string())
}
