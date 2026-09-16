//! Host side of the guest→host call channel. Links a single wasm
//! import, `host_call`, that every guest→host request goes through

use crate::error::PluginError;
use crate::process::ProcessTable;
use crate::state::PluginState;
use rustrest_plugin_api::CommandOutput;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wasmtime::{Caller, Extern, Linker, Memory};

const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024; // 200 MB
const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 30_000;
const MAX_COMMAND_TIMEOUT_MS: u64 = 5 * 60_000;

/// (program, args, cwd, env) - the payload shape for both `run_command` and
/// `process_spawn`.
type SpawnPayload = (String, Vec<String>, Option<String>, Vec<(String, String)>);

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "fn")]
    fn_name: String,
    #[serde(default)]
    payload: serde_json::Value,
}

pub fn link_host_functions(linker: &mut Linker<PluginState>) -> Result<(), PluginError> {
    linker.func_wrap(
        "env",
        "host_call",
        |mut caller: Caller<'_, PluginState>, ptr: u32, len: u32| -> u64 {
            let memory = match caller.get_export("memory") {
                Some(Extern::Memory(m)) => m,
                _ => return 0,
            };

            let mut buf = vec![0u8; len as usize];
            if memory.read(&caller, ptr as usize, &mut buf).is_err() {
                return 0;
            }

            let envelope: Envelope = match serde_json::from_slice(&buf) {
                Ok(e) => e,
                Err(e) => {
                    let bytes = encode_err(&format!("invalid call envelope: {e}"));
                    return write_response(&mut caller, memory, bytes);
                }
            };

            let plugin_id = caller.data().plugin_id.clone();
            let storage_dir = caller.data().storage_dir.clone();
            let external_process_allowed = caller.data().external_process_allowed;
            let processes = caller.data().processes.clone();
            let logs = caller.data().logs.clone();

            let response = execute(
                &envelope.fn_name,
                envelope.payload,
                &plugin_id,
                &storage_dir,
                external_process_allowed,
                &processes,
                &logs,
            );

            write_response(&mut caller, memory, response)
        },
    )?;
    Ok(())
}

fn write_response(caller: &mut Caller<'_, PluginState>, memory: Memory, bytes: Vec<u8>) -> u64 {
    let Some(alloc_fn) = caller
        .get_export("rustrest_alloc")
        .and_then(Extern::into_func)
    else {
        return 0;
    };
    let Ok(alloc_fn) = alloc_fn.typed::<u32, u32>(&caller) else {
        return 0;
    };
    let len = bytes.len() as u32;
    let Ok(ptr) = alloc_fn.call(&mut *caller, len) else {
        return 0;
    };
    if len > 0 && memory.write(&mut *caller, ptr as usize, &bytes).is_err() {
        return 0;
    }
    ((ptr as u64) << 32) | (len as u64)
}

fn encode_ok<T: Serialize>(value: &T) -> Vec<u8> {
    match serde_json::to_vec(&serde_json::json!({ "ok": value })) {
        Ok(bytes) => bytes,
        Err(e) => encode_err(&format!("failed to serialize host response: {e}")),
    }
}

fn encode_err(message: &str) -> Vec<u8> {
    serde_json::json!({ "err": message })
        .to_string()
        .into_bytes()
}

#[allow(clippy::too_many_arguments)]
fn execute(
    fn_name: &str,
    payload: serde_json::Value,
    plugin_id: &str,
    storage_dir: &Path,
    external_process_allowed: bool,
    processes: &Arc<Mutex<ProcessTable>>,
    logs: &Arc<Mutex<Vec<String>>>,
) -> Vec<u8> {
    macro_rules! decode {
        () => {
            match serde_json::from_value(payload) {
                Ok(v) => v,
                Err(e) => return encode_err(&format!("bad payload: {e}")),
            }
        };
    }
    macro_rules! require_external_process {
        () => {
            if !external_process_allowed {
                return encode_err("capability 'external-process' not declared in plugin.toml");
            }
        };
    }

    match fn_name {
        "log" => {
            let message: String = decode!();
            logs.lock()
                .expect("plugin log sink poisoned")
                .push(format!("[plugin:{plugin_id}] {message}"));
            encode_ok(&())
        }
        "which" => {
            require_external_process!();
            let name: String = decode!();
            encode_ok(&which(&name))
        }
        "download_file" => {
            require_external_process!();
            let (url, filename): (String, String) = decode!();
            match download_file(storage_dir, &url, &filename) {
                Ok(path) => encode_ok(&path),
                Err(e) => encode_err(&e),
            }
        }
        "make_executable" => {
            require_external_process!();
            let path: String = decode!();
            match make_executable(&path) {
                Ok(()) => encode_ok(&()),
                Err(e) => encode_err(&e),
            }
        }
        "storage_dir" => {
            require_external_process!();
            match std::fs::create_dir_all(storage_dir) {
                Ok(()) => encode_ok(&storage_dir.to_string_lossy().to_string()),
                Err(e) => encode_err(&e.to_string()),
            }
        }
        "run_command" => {
            require_external_process!();
            let (program, args, cwd, timeout_ms): (
                String,
                Vec<String>,
                Option<String>,
                Option<u64>,
            ) = decode!();
            match run_command(&program, &args, cwd.as_deref(), timeout_ms) {
                Ok(out) => encode_ok(&out),
                Err(e) => encode_err(&e),
            }
        }
        "process_spawn" => {
            require_external_process!();
            let (program, args, cwd, env): SpawnPayload = decode!();
            match ProcessTable::spawn(processes, &program, &args, cwd.as_deref(), &env) {
                Ok(handle) => encode_ok(&handle),
                Err(e) => encode_err(&e),
            }
        }
        "process_write" => {
            require_external_process!();
            let (handle, bytes): (u32, Vec<u8>) = decode!();
            match ProcessTable::write(processes, handle, &bytes) {
                Ok(()) => encode_ok(&()),
                Err(e) => encode_err(&e),
            }
        }
        "process_kill" => {
            require_external_process!();
            let handle: u32 = decode!();
            match ProcessTable::kill(processes, handle) {
                Ok(()) => encode_ok(&()),
                Err(e) => encode_err(&e),
            }
        }
        "process_try_wait" => {
            require_external_process!();
            let handle: u32 = decode!();
            match ProcessTable::try_wait(processes, handle) {
                Ok(code) => encode_ok(&code),
                Err(e) => encode_err(&e),
            }
        }
        other => encode_err(&format!("unknown host call: {other}")),
    }
}

fn which(name: &str) -> Option<String> {
    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string())
            .split(';')
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path_var) {
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn download_file(storage_dir: &Path, url: &str, filename: &str) -> Result<String, String> {
    if !url.starts_with("https://") {
        return Err("only https:// urls are allowed".to_string());
    }
    let safe_name: PathBuf = Path::new(filename)
        .file_name()
        .ok_or_else(|| "invalid filename".to_string())?
        .into();
    std::fs::create_dir_all(storage_dir).map_err(|e| e.to_string())?;
    let dest = storage_dir.join(&safe_name);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let mut response = client.get(url).send().map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("download failed: HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_DOWNLOAD_BYTES)
    {
        return Err("download exceeds maximum allowed size".to_string());
    }

    let mut file = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
    let mut written: u64 = 0;
    let mut buf = [0u8; 8192];
    loop {
        let n = response.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > MAX_DOWNLOAD_BYTES {
            drop(file);
            let _ = std::fs::remove_file(&dest);
            return Err("download exceeds maximum allowed size".to_string());
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }

    Ok(dest.to_string_lossy().to_string())
}

fn make_executable(path: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(perms.mode() | 0o111);
        std::fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn run_command(
    program: &str,
    args: &[String],
    cwd: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<CommandOutput, String> {
    let timeout = Duration::from_millis(
        timeout_ms
            .unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS)
            .min(MAX_COMMAND_TIMEOUT_MS),
    );

    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to run '{program}': {e}"))?;
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_handle = stdout_pipe.map(|mut p| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = p.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_handle = stderr_pipe.map(|mut p| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = p.read_to_end(&mut buf);
            buf
        })
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("command timed out after {}ms", timeout.as_millis()));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e.to_string()),
        }
    };

    let stdout = stdout_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let stderr = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    Ok(CommandOutput {
        status: status.code().unwrap_or(-1),
        stdout,
        stderr,
    })
}
