//! SDK for the `ExternalProcess` capability: locating, downloading, and
//! running external binaries (e.g. a plugin that fetches and drives a
//! standalone tool). Every function here fails with a plain `String` error
//! if the plugin didn't declare `Capability::ExternalProcess` in its
//! manifest - the host checks this before doing anything.
//!
//! The functions/impls here (everything that actually calls through
//! `hostcall`) only exist for `wasm32` guest builds. `rustrest-plugin-api`
//! is also compiled natively as a plain dependency of `rustrest-plugin-host`
//! (for the shared data types below), and the `host_call` extern this module
//! calls into only resolves inside a wasmtime guest - there's no such symbol
//! to link against natively.

use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use crate::hostcall;

/// resolves `name` against `PATH`, returning the absolute path if found.
#[cfg(target_arch = "wasm32")]
pub fn which(name: &str) -> Option<String> {
    hostcall::call("which", name).ok().flatten()
}

/// downloads `url` into this plugin's private storage directory under
/// `filename`, returning the absolute path it was saved to. `https` only,
/// size-capped host-side.
#[cfg(target_arch = "wasm32")]
pub fn download_file(url: &str, filename: &str) -> Result<String, String> {
    hostcall::call("download_file", (url, filename))
}

/// marks a file executable
#[cfg(target_arch = "wasm32")]
pub fn make_executable(path: &str) -> Result<(), String> {
    hostcall::call("make_executable", path)
}

/// the absolute path of this plugin's private storage directory
#[cfg(target_arch = "wasm32")]
pub fn storage_dir() -> Result<String, String> {
    hostcall::call("storage_dir", ())
}

/// reads a file previously written via [`storage_write`] from this plugin's
/// private storage directory. `filename` must be a bare filename (no path
/// separators). Returns `Ok(None)` if the file doesn't exist.
#[cfg(target_arch = "wasm32")]
pub fn storage_read(filename: &str) -> Result<Option<Vec<u8>>, String> {
    hostcall::call("storage_read", filename)
}

/// writes `bytes` to a file in this plugin's private storage directory,
/// creating or overwriting it. `filename` must be a bare filename (no path
/// separators).
#[cfg(target_arch = "wasm32")]
pub fn storage_write(filename: &str, bytes: &[u8]) -> Result<(), String> {
    hostcall::call("storage_write", (filename, bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// one file a user picked via [`pick_files`], successfully read as UTF-8 text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickedFile {
    pub name: String,
    pub text: String,
}

/// delivered to `Plugin::on_files_picked` once a [`pick_files`] dialog
/// resolves. `files` is empty (not an error) if the user cancelled.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PickFilesResult {
    pub files: Vec<PickedFile>,
    /// one entry per picked file that couldn't be attached (too large, or
    /// not valid UTF-8 text), shaped `"<name>: <reason>"`.
    pub skipped: Vec<String>,
}

/// opens a native "choose files" dialog on a background thread (so this
/// call never blocks) and reads the chosen files' text contents, returning
/// a handle immediately - the result is delivered later via
/// `Plugin::on_files_picked`. Files that aren't valid UTF-8 text, or exceed
/// a host-enforced size cap, are reported in the result's `skipped` list
/// rather than included.
#[cfg(target_arch = "wasm32")]
pub fn pick_files() -> Result<u32, String> {
    hostcall::call("pick_files", ())
}

/// runs `program` to completion (spawn, wait, capture output), host-enforced
/// timeout. For anything long-lived (a server-like process you'll write to
/// / read from repeatedly), use [`Process::spawn`] instead.
#[cfg(target_arch = "wasm32")]
pub fn run_command(
    program: &str,
    args: &[&str],
    cwd: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<CommandOutput, String> {
    hostcall::call("run_command", (program, args, cwd, timeout_ms))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessStream {
    Stdout,
    Stderr,
}

/// a handle to a long-running child process. Output received via `Plugin::on_process_output`/`on_process_exit`,
/// which the host calls whenever new data is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Process {
    handle: u32,
}

#[cfg(target_arch = "wasm32")]
impl Process {
    /// spawns `program`, returning a handle to it. The process is killed
    /// automatically if the plugin is disabled/uninstalled.
    pub fn spawn(
        program: &str,
        args: &[&str],
        cwd: Option<&str>,
        env: &[(&str, &str)],
    ) -> Result<Self, String> {
        let handle = hostcall::call("process_spawn", (program, args, cwd, env))?;
        Ok(Self { handle })
    }

    pub fn handle(&self) -> u32 {
        self.handle
    }

    /// writes to the process's stdin.
    pub fn write(&self, bytes: &[u8]) -> Result<(), String> {
        hostcall::call("process_write", (self.handle, bytes))
    }

    /// sends a kill signal; does not wait for exit (watch for
    /// `on_process_exit`).
    pub fn kill(&self) -> Result<(), String> {
        hostcall::call("process_kill", self.handle)
    }

    /// non-blocking: `Some(code)` once the process has exited, `None` if
    /// it's still running.
    pub fn try_wait(&self) -> Result<Option<i32>, String> {
        hostcall::call("process_try_wait", self.handle)
    }
}
