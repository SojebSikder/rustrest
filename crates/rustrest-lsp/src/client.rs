//! one running language server: the child process, JSON-RPC plumbing over
//! its stdio, and full-text document sync. Reading/writing happens on
//! plain threads; the app drains results with [`LanguageServer::poll`] on
//! its tick, so nothing here ever blocks the UI.

use crate::codec::{Decoder, encode};
use crate::convert;
use crate::position::{self, Encoding};
use crate::{Event, Position, RequestId, ServerCommand};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Duration;

enum Incoming {
    Message(Value),
    Stderr(String),
    Closed,
}

enum Pending {
    Completion,
    Hover,
}

struct Document {
    version: i32,
    text: String,
}

pub struct LanguageServer {
    name: String,
    child: Child,
    writer: Sender<Vec<u8>>,
    incoming: Receiver<Incoming>,
    next_id: RequestId,
    init_id: RequestId,
    initialized: bool,
    encoding: Encoding,
    /// notifications sent before the initialize response arrived
    queued: Vec<Value>,
    docs: HashMap<String, Document>,
    pending: HashMap<RequestId, Pending>,
    initialization_options: Option<Value>,
    exited: bool,
}

impl LanguageServer {
    /// spawns the server and sends `initialize`. `name` prefixes its log
    /// lines; `initialization_options` is also the answer to any
    /// `workspace/configuration` request.
    pub fn start(
        name: &str,
        command: &ServerCommand,
        initialization_options: Option<Value>,
    ) -> io::Result<Self> {
        let mut cmd = Command::new(&command.command);
        cmd.args(&command.args)
            .envs(command.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn()?;
        let mut stdin = child.stdin.take().expect("stdin is piped");
        let mut stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        let (writer, to_write) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            for bytes in to_write {
                if stdin
                    .write_all(&bytes)
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        });

        let (tx, incoming) = mpsc::channel();
        let stdout_tx = tx.clone();
        std::thread::spawn(move || {
            let mut decoder = Decoder::default();
            let mut buf = [0u8; 8192];
            loop {
                match stdout.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        for message in decoder.push(&buf[..n]) {
                            let message = match message {
                                Ok(value) => Incoming::Message(value),
                                Err(e) => Incoming::Stderr(format!("unreadable message: {e}")),
                            };
                            if stdout_tx.send(message).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
            let _ = stdout_tx.send(Incoming::Closed);
        });
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).split(b'\n') {
                let Ok(line) = line else { break };
                let line = String::from_utf8_lossy(&line).trim_end().to_string();
                if tx.send(Incoming::Stderr(line)).is_err() {
                    break;
                }
            }
        });

        let mut server = Self {
            name: name.to_string(),
            child,
            writer,
            incoming,
            next_id: 0,
            init_id: 0,
            initialized: false,
            encoding: Encoding::default(),
            queued: Vec::new(),
            docs: HashMap::new(),
            pending: HashMap::new(),
            initialization_options,
            exited: false,
        };
        server.init_id = server.next_request_id();
        server.write(&json!({
            "jsonrpc": "2.0",
            "id": server.init_id,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "clientInfo": { "name": "Rustrest" },
                "rootUri": null,
                "workspaceFolders": null,
                "initializationOptions": server.initialization_options,
                "capabilities": {
                    "general": { "positionEncodings": ["utf-8", "utf-16"] },
                    "workspace": { "configuration": true },
                    "textDocument": {
                        "synchronization": { "didSave": false, "dynamicRegistration": false },
                        "completion": {
                            "completionItem": {
                                "snippetSupport": false,
                                "documentationFormat": ["plaintext", "markdown"]
                            }
                        },
                        "hover": { "contentFormat": ["plaintext", "markdown"] },
                        "publishDiagnostics": { "versionSupport": true }
                    }
                }
            }
        }));
        Ok(server)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_running(&self) -> bool {
        !self.exited
    }

    /// initialized and still running.
    pub fn is_ready(&self) -> bool {
        self.initialized && !self.exited
    }

    /// opens `uri`, or replaces its text if it's already open.
    pub fn sync_document(&mut self, uri: &str, language_id: &str, version: i32, text: &str) {
        match self.docs.get_mut(uri) {
            Some(doc) if doc.version == version => {}
            Some(doc) => {
                doc.version = version;
                doc.text = text.to_string();
                self.notify(
                    "textDocument/didChange",
                    json!({
                        "textDocument": { "uri": uri, "version": version },
                        "contentChanges": [{ "text": text }]
                    }),
                );
            }
            None => {
                self.docs.insert(
                    uri.to_string(),
                    Document {
                        version,
                        text: text.to_string(),
                    },
                );
                self.notify(
                    "textDocument/didOpen",
                    json!({
                        "textDocument": {
                            "uri": uri,
                            "languageId": language_id,
                            "version": version,
                            "text": text
                        }
                    }),
                );
            }
        }
    }

    pub fn close_document(&mut self, uri: &str) {
        if self.docs.remove(uri).is_some() {
            self.notify(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": uri } }),
            );
        }
    }

    pub fn is_open(&self, uri: &str) -> bool {
        self.docs.contains_key(uri)
    }

    /// asks for completions at `position` in an open document; answered by
    /// an [`Event::Completions`]. `None` until the server is ready.
    pub fn completion(&mut self, uri: &str, position: Position) -> Option<RequestId> {
        self.position_request(
            "textDocument/completion",
            uri,
            position,
            Pending::Completion,
        )
    }

    /// hover counterpart of [`Self::completion`], answered by an [`Event::Hover`].
    pub fn hover(&mut self, uri: &str, position: Position) -> Option<RequestId> {
        self.position_request("textDocument/hover", uri, position, Pending::Hover)
    }

    fn position_request(
        &mut self,
        method: &str,
        uri: &str,
        position: Position,
        kind: Pending,
    ) -> Option<RequestId> {
        if !self.is_ready() {
            return None;
        }
        let doc = self.docs.get(uri)?;
        let position = position::to_lsp(&doc.text, position, self.encoding);
        let id = self.next_request_id();
        self.pending.insert(id, kind);
        self.write(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": { "textDocument": { "uri": uri }, "position": position }
        }));
        Some(id)
    }

    /// drains everything the server sent since the last call.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            match self.incoming.try_recv() {
                Ok(Incoming::Message(message)) => self.handle(message, &mut events),
                Ok(Incoming::Stderr(line)) => {
                    events.push(Event::Log(format!("[{}] {line}", self.name)));
                }
                Ok(Incoming::Closed) | Err(TryRecvError::Disconnected) => {
                    self.on_exit(&mut events);
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        events
    }

    fn on_exit(&mut self, events: &mut Vec<Event>) {
        if self.exited {
            return;
        }
        self.exited = true;
        for (request, kind) in self.pending.drain() {
            events.push(match kind {
                Pending::Completion => Event::Completions {
                    request,
                    items: Vec::new(),
                },
                Pending::Hover => Event::Hover {
                    request,
                    contents: None,
                },
            });
        }
        // stdout closing usually means the process is on its way out
        let mut code = None;
        for _ in 0..10 {
            if let Ok(Some(status)) = self.child.try_wait() {
                code = status.code();
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        events.push(Event::Exited(code));
    }

    fn handle(&mut self, message: Value, events: &mut Vec<Event>) {
        let method = message.get("method").and_then(Value::as_str);
        match (method, message.get("id")) {
            (Some(method), Some(id)) => {
                let result = self.answer_server_request(method, &message["params"]);
                self.write(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
            }
            (Some(method), None) => {
                let method = method.to_string();
                self.handle_notification(&method, &message["params"], events);
            }
            (None, Some(id)) => {
                if let Some(id) = id.as_i64() {
                    self.handle_response(id, &message, events);
                }
            }
            (None, None) => {}
        }
    }

    fn answer_server_request(&self, method: &str, params: &Value) -> Value {
        match method {
            "workspace/configuration" => {
                let count = params["items"].as_array().map_or(0, Vec::len);
                let value = self.initialization_options.clone().unwrap_or(Value::Null);
                Value::Array(vec![value; count])
            }
            _ => Value::Null,
        }
    }

    fn handle_notification(&mut self, method: &str, params: &Value, events: &mut Vec<Event>) {
        match method {
            "textDocument/publishDiagnostics" => {
                let Some(uri) = params["uri"].as_str() else {
                    return;
                };
                let text = self.docs.get(uri).map_or("", |d| d.text.as_str());
                events.push(Event::Diagnostics {
                    uri: uri.to_string(),
                    version: params["version"].as_i64().map(|v| v as i32),
                    diagnostics: convert::diagnostics(&params["diagnostics"], text, self.encoding),
                });
            }
            "window/logMessage" | "window/showMessage" => {
                if let Some(message) = params["message"].as_str() {
                    events.push(Event::Log(format!("[{}] {message}", self.name)));
                }
            }
            _ => {}
        }
    }

    fn handle_response(&mut self, id: RequestId, message: &Value, events: &mut Vec<Event>) {
        let error = message.get("error").filter(|e| !e.is_null());
        if id == self.init_id {
            if let Some(error) = error {
                events.push(Event::Log(format!(
                    "[{}] initialize failed: {error}",
                    self.name
                )));
                let _ = self.child.kill();
                return;
            }
            self.encoding = Encoding::from_server(
                message
                    .pointer("/result/capabilities/positionEncoding")
                    .and_then(Value::as_str),
            );
            self.initialized = true;
            self.write(&json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
            for queued in std::mem::take(&mut self.queued) {
                self.write(&queued);
            }
            events.push(Event::Ready);
            return;
        }

        let result = if error.is_some() {
            &Value::Null
        } else {
            &message["result"]
        };
        match self.pending.remove(&id) {
            Some(Pending::Completion) => events.push(Event::Completions {
                request: id,
                items: convert::completions(result),
            }),
            Some(Pending::Hover) => events.push(Event::Hover {
                request: id,
                contents: convert::hover(result),
            }),
            None => {}
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        if self.initialized {
            self.write(&message);
        } else {
            self.queued.push(message);
        }
    }

    fn write(&self, message: &Value) {
        let _ = self.writer.send(encode(message));
    }

    fn next_request_id(&mut self) -> RequestId {
        self.next_id += 1;
        self.next_id
    }
}

impl Drop for LanguageServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
