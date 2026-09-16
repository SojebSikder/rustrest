//! Template/example native Rustrest plugin. Demonstrates the capabilities
//! most plugins will use: a request hook, a command, a sidebar panel, and
//! (via the `ExternalProcess` capability) spawning and driving a persistent
//! external process - the same shape a plugin would use to download and run
//! something LSP-like (e.g. `rust-analyzer`), just with a network-free stand-in
//! so this demo works offline. Build with:
//!
//! ```text
//! rustup target add wasm32-unknown-unknown
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! then copy both `target/wasm32-unknown-unknown/release/rustrest_plugin_example.wasm`
//! and `plugin.toml` into `<plugins-dir>/example/` (the containing directory
//! name must match the `id` declared in `plugin.toml`, here `"example"`).

use rustrest_plugin_api::{Plugin, Process, ProcessStream, RequestContext, UiEvent, UiNode};

#[derive(Default)]
struct ExamplePlugin {
    clicks: u32,
    echo_process: Option<Process>,
    input_value: String,
    output_log: Vec<String>,
}

/// picks the external command this demo spawns and talks to over
/// stdin/stdout. Any program that echoes its stdin back to stdout works; the guest
/// is always wasm32 regardless of host OS, so which one exists is a runtime
/// question answered via `which`, not a `cfg(unix)`/`cfg(windows)` guess.
fn echo_program() -> (String, Vec<String>) {
    if rustrest_plugin_api::process::which("cat").is_some() {
        ("cat".to_string(), Vec::new())
    } else {
        ("findstr".to_string(), vec!["/R".to_string(), "^".to_string()])
    }
}

impl Plugin for ExamplePlugin {
    fn on_pre_request(&mut self, mut ctx: RequestContext) -> RequestContext {
        rustrest_plugin_api::log(&format!("injecting header into request to {}", ctx.url));
        ctx.headers
            .push(("X-Example-Plugin".to_string(), "1".to_string()));
        ctx
    }

    fn on_command(&mut self, command_id: &str) -> Result<Option<String>, String> {
        match command_id {
            "say-hello" => {
                rustrest_plugin_api::log("say-hello command invoked");
                Ok(Some("Hello from the example plugin!".to_string()))
            }
            other => Err(format!("unknown command: {other}")),
        }
    }

    fn render_panel(&mut self, _panel_id: &str) -> UiNode {
        let mut children = vec![
            UiNode::Label("Example plugin panel".to_string()),
            UiNode::Label(format!("Button clicked {} time(s)", self.clicks)),
            UiNode::Button {
                id: "clicked".to_string(),
                label: "Click me".to_string(),
            },
        ];

        if self.echo_process.is_some() {
            children.push(UiNode::Row(vec![
                UiNode::TextInput {
                    id: "stdin-input".to_string(),
                    value: self.input_value.clone(),
                    placeholder: "text to echo".to_string(),
                },
                UiNode::Button {
                    id: "send".to_string(),
                    label: "Send".to_string(),
                },
                UiNode::Button {
                    id: "kill".to_string(),
                    label: "Stop process".to_string(),
                },
            ]));
            children.push(UiNode::List(self.output_log.clone()));
        } else {
            children.push(UiNode::Button {
                id: "spawn".to_string(),
                label: "Spawn echo process".to_string(),
            });
        }

        UiNode::Column(children)
    }

    fn on_panel_event(&mut self, panel_id: &str, event: UiEvent) -> Option<UiNode> {
        match event {
            UiEvent::Clicked(id) if id == "clicked" => {
                self.clicks += 1;
                Some(self.render_panel(panel_id))
            }
            UiEvent::Clicked(id) if id == "spawn" => {
                let (program, args) = echo_program();
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                match Process::spawn(&program, &args, None, &[]) {
                    Ok(process) => {
                        self.output_log
                            .push(format!("spawned '{program}' (handle {})", process.handle()));
                        self.echo_process = Some(process);
                    }
                    Err(e) => self.output_log.push(format!("spawn failed: {e}")),
                }
                Some(self.render_panel(panel_id))
            }
            UiEvent::Changed(id, value) if id == "stdin-input" => {
                self.input_value = value;
                None
            }
            UiEvent::Clicked(id) if id == "send" => {
                if let Some(process) = &self.echo_process {
                    let mut line = self.input_value.clone();
                    line.push('\n');
                    if let Err(e) = process.write(line.as_bytes()) {
                        self.output_log.push(format!("write failed: {e}"));
                    }
                }
                self.input_value.clear();
                Some(self.render_panel(panel_id))
            }
            UiEvent::Clicked(id) if id == "kill" => {
                if let Some(process) = self.echo_process.take()
                    && let Err(e) = process.kill()
                {
                    self.output_log.push(format!("kill failed: {e}"));
                }
                Some(self.render_panel(panel_id))
            }
            _ => None,
        }
    }

    fn on_process_output(&mut self, _handle: u32, stream: ProcessStream, chunk: Vec<u8>) {
        let text = String::from_utf8_lossy(&chunk);
        for line in text.lines() {
            let prefix = match stream {
                ProcessStream::Stdout => "out",
                ProcessStream::Stderr => "err",
            };
            self.output_log.push(format!("[{prefix}] {line}"));
        }
    }

    fn on_process_exit(&mut self, _handle: u32, code: Option<i32>) {
        self.output_log.push(format!("process exited: {code:?}"));
        self.echo_process = None;
    }
}

rustrest_plugin_api::export_plugin!(ExamplePlugin);
