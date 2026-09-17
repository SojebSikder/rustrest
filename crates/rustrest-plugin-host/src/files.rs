//! Host-side management of the `pick_files` host call: opens a native
//! "choose files" dialog on a background thread (so it never blocks the
//! wasmtime call path or the UI) and reads the chosen files' text
//! contents, delivered back into the guest via `PluginManager::pump_files`.

use rustrest_plugin_api::{PickFilesResult, PickedFile};
use std::sync::{Arc, Mutex};

/// these are meant to be small text/spec files pasted into an LLM prompt,
/// not arbitrary uploads.
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024; // 5 MB

pub enum FileEvent {
    Picked(u32, PickFilesResult),
}

#[derive(Default)]
pub struct FileTable {
    next_handle: u32,
    events: Vec<FileEvent>,
}

impl FileTable {
    /// opens a native multi-file picker on a background thread, returning a
    /// handle immediately.
    pub fn spawn_pick(shared: &Arc<Mutex<FileTable>>) -> u32 {
        let handle = {
            let mut table = shared.lock().expect("file table poisoned");
            table.next_handle += 1;
            table.next_handle
        };

        let shared = shared.clone();
        std::thread::spawn(move || {
            let result = pick_and_read();
            shared
                .lock()
                .expect("file table poisoned")
                .events
                .push(FileEvent::Picked(handle, result));
        });

        handle
    }

    /// drains buffered pick results; called by the pump on a timer.
    pub fn drain_events(shared: &Arc<Mutex<FileTable>>) -> Vec<FileEvent> {
        let mut table = shared.lock().expect("file table poisoned");
        std::mem::take(&mut table.events)
    }
}

/// blocking - only ever called on the background thread `spawn_pick` starts,
/// never on the wasmtime call path.
fn pick_and_read() -> PickFilesResult {
    let mut result = PickFilesResult::default();
    let Some(paths) = rfd::FileDialog::new().pick_files() else {
        return result; // cancelled
    };

    for path in paths {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        match std::fs::read(&path) {
            Ok(bytes) if bytes.len() as u64 > MAX_FILE_BYTES => {
                result.skipped.push(format!("{name}: exceeds maximum allowed size"));
            }
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => result.files.push(PickedFile { name, text }),
                Err(_) => result.skipped.push(format!("{name}: not a text file")),
            },
            Err(e) => result.skipped.push(format!("{name}: {e}")),
        }
    }

    result
}
