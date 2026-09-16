//! Low-level guest-side wasm glue used by the [`crate::export_plugin!`]
//! macro. Plugin authors should never need to call anything in this module
//! directly - it implements the JSON-over-linear-memory ABI the host
//! (`rustrest-plugin-host`) speaks on the other side of the call.
//!
//! Wire format: the host sends one JSON envelope `{"fn": "...", "payload":...}`
//! into a guest-allocated buffer, calls `rustrest_call(ptr, len)`, and
//! reads a JSON `{"ok": ...}` / `{"err": "..."}` response out of the
//! guest-allocated buffer described by the packed `(ptr, len)` return value.
//! The host frees both buffers by calling `rustrest_dealloc` with the
//! `(ptr, len)` pairs it was given.

use crate::hooks::{RequestContext, ResponseContext};
use crate::plugin::Plugin;
use crate::process::ProcessStream;
use crate::ui::UiEvent;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "fn")]
    fn_name: String,
    #[serde(default)]
    payload: serde_json::Value,
}

fn encode_ok<T: Serialize>(value: &T) -> Vec<u8> {
    match serde_json::to_vec(&serde_json::json!({ "ok": value })) {
        Ok(bytes) => bytes,
        Err(e) => encode_err(&format!("failed to serialize plugin response: {e}")),
    }
}

fn encode_err(message: &str) -> Vec<u8> {
    // infallible: a JSON object with a single string field always serializes.
    serde_json::json!({ "err": message })
        .to_string()
        .into_bytes()
}

fn decode_payload<T: for<'de> Deserialize<'de>>(payload: serde_json::Value) -> Result<T, Vec<u8>> {
    serde_json::from_value(payload).map_err(|e| encode_err(&format!("bad payload: {e}")))
}

fn route<T: Plugin>(plugin: &mut T, input: &[u8]) -> Vec<u8> {
    let envelope: Envelope = match serde_json::from_slice(input) {
        Ok(e) => e,
        Err(e) => return encode_err(&format!("invalid call envelope: {e}")),
    };

    match envelope.fn_name.as_str() {
        "on_pre_request" => match decode_payload::<RequestContext>(envelope.payload) {
            Ok(ctx) => encode_ok(&plugin.on_pre_request(ctx)),
            Err(bytes) => bytes,
        },
        "on_post_response" => match decode_payload::<ResponseContext>(envelope.payload) {
            Ok(ctx) => encode_ok(&plugin.on_post_response(ctx)),
            Err(bytes) => bytes,
        },
        "on_command" => match decode_payload::<String>(envelope.payload) {
            Ok(id) => match plugin.on_command(&id) {
                Ok(msg) => encode_ok(&msg),
                Err(e) => encode_err(&e),
            },
            Err(bytes) => bytes,
        },
        "render_panel" => match decode_payload::<String>(envelope.payload) {
            Ok(id) => encode_ok(&plugin.render_panel(&id)),
            Err(bytes) => bytes,
        },
        "on_panel_event" => match decode_payload::<(String, UiEvent)>(envelope.payload) {
            Ok((id, event)) => encode_ok(&plugin.on_panel_event(&id, event)),
            Err(bytes) => bytes,
        },
        "import" => match decode_payload::<(String, Vec<u8>)>(envelope.payload) {
            Ok((format_id, bytes)) => match plugin.import(&format_id, bytes) {
                Ok(v) => encode_ok(&v),
                Err(e) => encode_err(&e),
            },
            Err(bytes) => bytes,
        },
        "export" => match decode_payload::<(String, serde_json::Value)>(envelope.payload) {
            Ok((format_id, collection)) => match plugin.export(&format_id, collection) {
                Ok(v) => encode_ok(&v),
                Err(e) => encode_err(&e),
            },
            Err(bytes) => bytes,
        },
        "on_process_output" => {
            match decode_payload::<(u32, ProcessStream, Vec<u8>)>(envelope.payload) {
                Ok((handle, stream, chunk)) => {
                    encode_ok(&plugin.on_process_output(handle, stream, chunk))
                }
                Err(bytes) => bytes,
            }
        }
        "on_process_exit" => match decode_payload::<(u32, Option<i32>)>(envelope.payload) {
            Ok((handle, code)) => encode_ok(&plugin.on_process_exit(handle, code)),
            Err(bytes) => bytes,
        },
        other => encode_err(&format!("unknown plugin call: {other}")),
    }
}

fn pack(ptr: u32, len: u32) -> u64 {
    ((ptr as u64) << 32) | (len as u64)
}

fn write_result(bytes: Vec<u8>) -> u64 {
    let len = bytes.len() as u32;
    let ptr = alloc(len);
    if len > 0 {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        }
    }
    pack(ptr, len)
}

/// exported as `rustrest_alloc`: allocates `len` bytes the host can write
/// into, later freed by the host via `rustrest_dealloc`.
pub fn alloc(len: u32) -> u32 {
    if len == 0 {
        return 1; // non-null placeholder; the host never reads a zero-length buffer.
    }
    let boxed: Box<[u8]> = vec![0u8; len as usize].into_boxed_slice();
    Box::into_raw(boxed) as *mut u8 as u32
}

/// exported as `rustrest_dealloc`: frees a buffer previously returned by
/// `alloc` (or by `write_result`), given back the same `(ptr, len)` pair.
pub fn dealloc(ptr: u32, len: u32) {
    if len == 0 {
        return;
    }
    unsafe {
        let slice_ptr = std::slice::from_raw_parts_mut(ptr as *mut u8, len as usize) as *mut [u8];
        drop(Box::from_raw(slice_ptr));
    }
}

/// exported as `rustrest_call`: reads a JSON envelope from guest memory at
/// `(ptr, len)`, dispatches it to `plugin`, and returns a packed
/// `(ptr, len)` pointing at a freshly allocated JSON response.
pub fn dispatch<T: Plugin>(plugin: &mut T, ptr: u32, len: u32) -> u64 {
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    write_result(route(plugin, input))
}
