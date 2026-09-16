//! Host-side management of processes a plugin spawns via the
//! `ExternalProcess` capability (`process_spawn`/`process_write`/
//! `process_kill`/`process_try_wait`). One [`ProcessTable`] per loaded
//! plugin, shared (via `Arc<Mutex<_>>`) between the plugin's `PluginState`
//! (so guest calls can reach it) and a couple of plain OS threads per
//! spawned child that feed output back without blocking the wasmtime call
//! path. `PluginManager::pump_processes` drains buffered events on a timer
//! and delivers them into the guest via the normal synchronous call
//! mechanism - no async wasm runtime needed.

use rustrest_plugin_api::ProcessStream;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub enum ProcessEvent {
    Output(u32, ProcessStream, Vec<u8>),
    Exit(u32, Option<i32>),
}

struct SpawnedProcess {
    child: Child,
    stdin: Option<ChildStdin>,
}

#[derive(Default)]
pub struct ProcessTable {
    next_handle: u32,
    processes: HashMap<u32, SpawnedProcess>,
    events: Vec<ProcessEvent>,
}

impl ProcessTable {
    pub fn spawn(
        shared: &Arc<Mutex<ProcessTable>>,
        program: &str,
        args: &[String],
        cwd: Option<&str>,
        env: &[(String, String)],
    ) -> Result<u32, String> {
        let mut cmd = Command::new(program);
        cmd.args(args);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn '{program}': {e}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let handle = {
            let mut table = shared.lock().expect("process table poisoned");
            table.next_handle += 1;
            let handle = table.next_handle;
            table
                .processes
                .insert(handle, SpawnedProcess { child, stdin });
            handle
        };

        if let Some(stdout) = stdout {
            spawn_reader(shared.clone(), handle, stdout, ProcessStream::Stdout);
        }
        if let Some(stderr) = stderr {
            spawn_reader(shared.clone(), handle, stderr, ProcessStream::Stderr);
        }
        spawn_watcher(shared.clone(), handle);

        Ok(handle)
    }

    pub fn write(
        shared: &Arc<Mutex<ProcessTable>>,
        handle: u32,
        bytes: &[u8],
    ) -> Result<(), String> {
        let mut table = shared.lock().expect("process table poisoned");
        let proc = table
            .processes
            .get_mut(&handle)
            .ok_or_else(|| "no such process".to_string())?;
        let stdin = proc
            .stdin
            .as_mut()
            .ok_or_else(|| "process has no stdin".to_string())?;
        stdin.write_all(bytes).map_err(|e| e.to_string())
    }

    pub fn kill(shared: &Arc<Mutex<ProcessTable>>, handle: u32) -> Result<(), String> {
        let mut table = shared.lock().expect("process table poisoned");
        let proc = table
            .processes
            .get_mut(&handle)
            .ok_or_else(|| "no such process".to_string())?;
        proc.child.kill().map_err(|e| e.to_string())
    }

    pub fn try_wait(shared: &Arc<Mutex<ProcessTable>>, handle: u32) -> Result<Option<i32>, String> {
        let mut table = shared.lock().expect("process table poisoned");
        let proc = table
            .processes
            .get_mut(&handle)
            .ok_or_else(|| "no such process".to_string())?;
        proc.child
            .try_wait()
            .map(|status| status.map(|s| s.code().unwrap_or(-1)))
            .map_err(|e| e.to_string())
    }

    /// drains buffered output/exit events; called by the pump on a timer.
    pub fn drain_events(shared: &Arc<Mutex<ProcessTable>>) -> Vec<ProcessEvent> {
        let mut table = shared.lock().expect("process table poisoned");
        std::mem::take(&mut table.events)
    }

    /// kills every process still tracked, e.g. when the owning plugin is
    /// disabled or uninstalled so a background tool doesn't outlive it.
    pub fn kill_all(shared: &Arc<Mutex<ProcessTable>>) {
        let mut table = shared.lock().expect("process table poisoned");
        for proc in table.processes.values_mut() {
            let _ = proc.child.kill();
        }
    }

    fn push_event(shared: &Arc<Mutex<ProcessTable>>, event: ProcessEvent) {
        shared
            .lock()
            .expect("process table poisoned")
            .events
            .push(event);
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    shared: Arc<Mutex<ProcessTable>>,
    handle: u32,
    mut reader: R,
    stream: ProcessStream,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => ProcessTable::push_event(
                    &shared,
                    ProcessEvent::Output(handle, stream, buf[..n].to_vec()),
                ),
                Err(_) => break,
            }
        }
    });
}

fn spawn_watcher(shared: Arc<Mutex<ProcessTable>>, handle: u32) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let exited = {
                let mut table = shared.lock().expect("process table poisoned");
                match table.processes.get_mut(&handle) {
                    Some(proc) => proc.child.try_wait().ok().flatten(),
                    None => return,
                }
            };
            if let Some(status) = exited {
                let mut table = shared.lock().expect("process table poisoned");
                table.processes.remove(&handle);
                table.events.push(ProcessEvent::Exit(handle, status.code()));
                return;
            }
        }
    });
}
