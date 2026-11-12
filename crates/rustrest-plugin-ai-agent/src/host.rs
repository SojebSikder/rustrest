//! Every call this plugin makes into the Rustrest host, in one place.

use serde::de::DeserializeOwned;
use serde::Serialize;

#[cfg(target_arch = "wasm32")]
pub(crate) use rustrest_plugin_api::{
    http_request, pick_files, storage_delete, storage_read, storage_write,
};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{http_request, pick_files, storage_delete, storage_read, storage_write};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use rustrest_plugin_api::HttpRequestSpec;

    const UNAVAILABLE: &str = "host calls are only available inside Rustrest";

    pub(crate) fn http_request(_spec: HttpRequestSpec) -> Result<u32, String> {
        Err(UNAVAILABLE.to_string())
    }

    pub(crate) fn pick_files() -> Result<u32, String> {
        Err(UNAVAILABLE.to_string())
    }

    pub(crate) fn storage_read(_filename: &str) -> Result<Option<Vec<u8>>, String> {
        Err(UNAVAILABLE.to_string())
    }

    pub(crate) fn storage_write(_filename: &str, _bytes: &[u8]) -> Result<(), String> {
        Err(UNAVAILABLE.to_string())
    }

    pub(crate) fn storage_delete(_filename: &str) -> Result<(), String> {
        Err(UNAVAILABLE.to_string())
    }
}

/// Reads and decodes a JSON file from the plugin's storage. `None` if the file
/// is missing, unreadable, or doesn't decode.
pub(crate) fn read_json<T: DeserializeOwned>(filename: &str) -> Option<T> {
    let bytes = storage_read(filename).ok().flatten()?;
    serde_json::from_slice(&bytes).ok()
}

/// Encodes `value` as JSON and writes it to the plugin's storage.
pub(crate) fn write_json<T: Serialize>(filename: &str, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    storage_write(filename, &bytes)
}

/// Deletes a file from the plugin's storage, ignoring failures (the file may already be gone).
pub(crate) fn delete_file(filename: &str) {
    let _ = storage_delete(filename);
}
