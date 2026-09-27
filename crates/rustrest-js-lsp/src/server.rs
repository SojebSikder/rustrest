//! LSP message loop.

use std::{
    collections::HashMap,
    error::Error,
    panic::{AssertUnwindSafe, catch_unwind},
};

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic, DiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, Hover,
    HoverContents, HoverParams, HoverProviderCapability, InitializeResult, MarkupContent,
    MarkupKind, PositionEncodingKind, PublishDiagnosticsParams, Range, ServerCapabilities,
    ServerInfo, SignatureHelpOptions, SignatureHelpParams, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use serde_json::Value;

use crate::{
    completion,
    diagnostics::Severity,
    document::{DocKind, Document},
    hover,
    position::Encoding,
};

type BoxError = Box<dyn Error + Send + Sync>;

/// Picks UTF-8 when the client lists it in `general.positionEncodings`.
pub fn negotiate_encoding(init_params: &Value) -> Encoding {
    let supports_utf8 = init_params
        .pointer("/capabilities/general/positionEncodings")
        .and_then(Value::as_array)
        .is_some_and(|list| list.iter().any(|e| e.as_str() == Some("utf-8")));
    if supports_utf8 {
        Encoding::Utf8
    } else {
        Encoding::Utf16
    }
}

/// Capabilities advertised in the `initialize` response.
pub fn capabilities(enc: Encoding) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(match enc {
            Encoding::Utf8 => PositionEncodingKind::UTF8,
            Encoding::Utf16 => PositionEncodingKind::UTF16,
        }),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_string()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

struct Server {
    conn: Connection,
    enc: Encoding,
    docs: HashMap<Uri, Document>,
}

/// Runs the server until `exit`. Returns `Ok(true)` when `shutdown` preceded
/// `exit` (exit code 0), `Ok(false)` otherwise.
pub fn run(conn: Connection) -> Result<bool, BoxError> {
    let Some(enc) = initialize(&conn)? else {
        return Ok(false);
    };
    let mut server = Server {
        conn,
        enc,
        docs: HashMap::new(),
    };
    server.main_loop()
}

/// Waits for `initialize` and answers it; `None` if `exit` came first.
fn initialize(conn: &Connection) -> Result<Option<Encoding>, BoxError> {
    loop {
        match conn.receiver.recv()? {
            Message::Request(req) if req.method == "initialize" => {
                let enc = negotiate_encoding(&req.params);
                let result = InitializeResult {
                    capabilities: capabilities(enc),
                    server_info: Some(ServerInfo {
                        name: env!("CARGO_PKG_NAME").to_string(),
                        version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    }),
                };
                conn.sender.send(Response::new_ok(req.id, result).into())?;
                return Ok(Some(enc));
            }
            Message::Request(req) => {
                conn.sender.send(
                    Response::new_err(
                        req.id,
                        ErrorCode::ServerNotInitialized as i32,
                        "server not initialized".to_string(),
                    )
                    .into(),
                )?;
            }
            Message::Notification(n) if n.method == "exit" => return Ok(None),
            _ => {}
        }
    }
}

impl Server {
    fn main_loop(&mut self) -> Result<bool, BoxError> {
        let mut shutdown = false;
        while let Ok(msg) = self.conn.receiver.recv() {
            match msg {
                Message::Request(req) => {
                    if req.method == "shutdown" {
                        shutdown = true;
                        self.respond(Response::new_ok(req.id, ()))?;
                        continue;
                    }
                    if shutdown {
                        self.respond(Response::new_err(
                            req.id,
                            ErrorCode::InvalidRequest as i32,
                            "server is shutting down".to_string(),
                        ))?;
                        continue;
                    }
                    let id = req.id.clone();
                    let resp = match catch_unwind(AssertUnwindSafe(|| self.handle_request(req))) {
                        Ok(resp) => resp,
                        Err(_) => {
                            eprintln!("rustrest-js-lsp: request handler panicked");
                            Response::new_err(
                                id,
                                ErrorCode::InternalError as i32,
                                "internal error".to_string(),
                            )
                        }
                    };
                    self.respond(resp)?;
                }
                Message::Notification(n) => {
                    if n.method == "exit" {
                        return Ok(shutdown);
                    }
                    let method = n.method.clone();
                    if catch_unwind(AssertUnwindSafe(|| self.handle_notification(n))).is_err() {
                        eprintln!("rustrest-js-lsp: {method} handler panicked");
                    }
                }
                Message::Response(_) => {}
            }
        }
        Ok(shutdown)
    }

    fn respond(&self, resp: Response) -> Result<(), BoxError> {
        self.conn.sender.send(resp.into())?;
        Ok(())
    }

    fn handle_request(&self, req: Request) -> Response {
        let result = match req.method.as_str() {
            "textDocument/completion" => {
                parse::<CompletionParams>(&req).map(|p| self.completion(&p.text_document_position))
            }
            "textDocument/hover" => {
                parse::<HoverParams>(&req).map(|p| self.hover(&p.text_document_position_params))
            }
            "textDocument/signatureHelp" => parse::<SignatureHelpParams>(&req)
                .map(|p| self.signature_help(&p.text_document_position_params)),
            _ => {
                return Response::new_err(
                    req.id,
                    ErrorCode::MethodNotFound as i32,
                    format!("unhandled method {}", req.method),
                );
            }
        };
        match result {
            Ok(value) => Response::new_ok(req.id, value),
            Err(e) => {
                eprintln!("rustrest-js-lsp: invalid params for {}: {e}", req.method);
                Response::new_err(req.id, ErrorCode::InvalidParams as i32, e)
            }
        }
    }

    /// Document and byte offset of a text-document position.
    fn locate(&self, pos: &TextDocumentPositionParams) -> Option<(&Document, usize)> {
        let doc = self.docs.get(&pos.text_document.uri)?;
        let offset = doc.index.offset(&doc.text, pos.position, self.enc);
        Some((doc, offset))
    }

    fn completion(&self, pos: &TextDocumentPositionParams) -> Value {
        let items = self
            .locate(pos)
            .map(|(doc, offset)| {
                completion::complete(&doc.text, &doc.masks, offset, doc.kind, &doc.locals)
            })
            .unwrap_or_default();
        to_value(CompletionResponse::Array(items))
    }

    fn hover(&self, pos: &TextDocumentPositionParams) -> Value {
        let hover = self.locate(pos).and_then(|(doc, offset)| {
            hover::hover(&doc.text, &doc.masks, offset, &doc.locals).map(|value| Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::PlainText,
                    value,
                }),
                range: None,
            })
        });
        to_value(hover)
    }

    fn signature_help(&self, pos: &TextDocumentPositionParams) -> Value {
        let help = self.locate(pos).and_then(|(doc, offset)| {
            hover::signature_help(&doc.text, &doc.masks, offset, &doc.locals)
        });
        to_value(help)
    }

    fn handle_notification(&mut self, n: Notification) {
        match n.method.as_str() {
            "textDocument/didOpen" => {
                if let Some(p) = parse_notification::<DidOpenTextDocumentParams>(&n) {
                    let doc = p.text_document;
                    self.open(doc.uri, doc.text, doc.version);
                }
            }
            "textDocument/didChange" => {
                if let Some(p) = parse_notification::<DidChangeTextDocumentParams>(&n)
                    && let Some(change) = p.content_changes.into_iter().last()
                {
                    self.open(p.text_document.uri, change.text, p.text_document.version);
                }
            }
            "textDocument/didClose" => {
                if let Some(p) = parse_notification::<DidCloseTextDocumentParams>(&n) {
                    let uri = p.text_document.uri;
                    let version = self.docs.remove(&uri).map(|d| d.version);
                    self.publish(uri, Vec::new(), version);
                }
            }
            _ => {}
        }
    }

    fn open(&mut self, uri: Uri, text: String, version: i32) {
        let kind = DocKind::from_uri(uri.as_str());
        let doc = Document::new(text, version, kind);
        let diagnostics = doc
            .diagnostics
            .iter()
            .map(|d| Diagnostic {
                range: Range::new(
                    doc.index.position(&doc.text, d.start, self.enc),
                    doc.index.position(&doc.text, d.end, self.enc),
                ),
                severity: Some(match d.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                }),
                source: Some("rustrest-js".to_string()),
                message: d.message.clone(),
                ..Default::default()
            })
            .collect();
        self.docs.insert(uri.clone(), doc);
        self.publish(uri, diagnostics, Some(version));
    }

    fn publish(&self, uri: Uri, diagnostics: Vec<Diagnostic>, version: Option<i32>) {
        let params = PublishDiagnosticsParams {
            uri,
            diagnostics,
            version,
        };
        let note = Notification::new("textDocument/publishDiagnostics".to_string(), params);
        if let Err(e) = self.conn.sender.send(note.into()) {
            eprintln!("rustrest-js-lsp: failed to publish diagnostics: {e}");
        }
    }
}

fn parse<P: serde::de::DeserializeOwned>(req: &Request) -> Result<P, String> {
    serde_json::from_value(req.params.clone()).map_err(|e| e.to_string())
}

fn parse_notification<P: serde::de::DeserializeOwned>(n: &Notification) -> Option<P> {
    serde_json::from_value(n.params.clone())
        .map_err(|e| eprintln!("rustrest-js-lsp: invalid params for {}: {e}", n.method))
        .ok()
}

fn to_value<T: serde::Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_server::RequestId;
    use lsp_types::{CompletionItem, Position};
    use serde_json::json;

    fn recv(client: &Connection) -> Message {
        client
            .receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("server response")
    }

    #[test]
    fn end_to_end() {
        let (server_conn, client) = Connection::memory();
        let handle = std::thread::spawn(move || run(server_conn).unwrap());

        client
            .sender
            .send(
                Request::new(
                    RequestId::from(1),
                    "initialize".to_string(),
                    json!({
                        "processId": null,
                        "rootUri": null,
                        "capabilities": { "general": { "positionEncodings": ["utf-16", "utf-8"] } }
                    }),
                )
                .into(),
            )
            .unwrap();
        let Message::Response(resp) = recv(&client) else {
            panic!("expected initialize response");
        };
        let result = resp.response_result.unwrap();
        assert_eq!(result["capabilities"]["positionEncoding"], "utf-8");
        assert_eq!(result["capabilities"]["textDocumentSync"], 1);
        assert_eq!(
            result["capabilities"]["completionProvider"]["triggerCharacters"],
            json!(["."])
        );
        client
            .sender
            .send(Notification::new("initialized".to_string(), json!({})).into())
            .unwrap();

        let uri = "rustrest:///scripts/abc/post-response.js";
        let text = "const s = \"héllo 😀\"; pm.";
        client
            .sender
            .send(
                Notification::new(
                    "textDocument/didOpen".to_string(),
                    json!({ "textDocument": {
                        "uri": uri, "languageId": "javascript", "version": 3, "text": text
                    }}),
                )
                .into(),
            )
            .unwrap();
        let Message::Notification(note) = recv(&client) else {
            panic!("expected diagnostics");
        };
        assert_eq!(note.method, "textDocument/publishDiagnostics");
        let diags: PublishDiagnosticsParams = serde_json::from_value(note.params).unwrap();
        assert_eq!(diags.version, Some(3));
        assert!(!diags.diagnostics.is_empty()); // `pm.` alone is a syntax error
        assert_eq!(diags.diagnostics[0].source.as_deref(), Some("rustrest-js"));

        // utf-8 columns are byte offsets
        let col = text.len() as u32;
        client
            .sender
            .send(
                Request::new(
                    RequestId::from(2),
                    "textDocument/completion".to_string(),
                    json!({
                        "textDocument": { "uri": uri },
                        "position": Position::new(0, col),
                    }),
                )
                .into(),
            )
            .unwrap();
        let Message::Response(resp) = recv(&client) else {
            panic!("expected completion response");
        };
        let items: Vec<CompletionItem> =
            serde_json::from_value(resp.response_result.unwrap()).unwrap();
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"test") && labels.contains(&"expect"),
            "{labels:?}"
        );
        assert!(!labels.contains(&"setHeader"));

        client
            .sender
            .send(
                Notification::new(
                    "textDocument/didClose".to_string(),
                    json!({ "textDocument": { "uri": uri } }),
                )
                .into(),
            )
            .unwrap();
        let Message::Notification(note) = recv(&client) else {
            panic!("expected diagnostics");
        };
        let diags: PublishDiagnosticsParams = serde_json::from_value(note.params).unwrap();
        assert!(diags.diagnostics.is_empty());

        client
            .sender
            .send(Request::new(RequestId::from(3), "shutdown".to_string(), json!(null)).into())
            .unwrap();
        let Message::Response(resp) = recv(&client) else {
            panic!("expected shutdown response");
        };
        assert_eq!(resp.id, RequestId::from(3));
        client
            .sender
            .send(Notification::new("exit".to_string(), json!(null)).into())
            .unwrap();
        assert!(handle.join().unwrap(), "clean shutdown");
    }

    #[test]
    fn utf16_by_default() {
        assert_eq!(
            negotiate_encoding(&json!({ "capabilities": {} })),
            Encoding::Utf16
        );
        assert_eq!(negotiate_encoding(&json!(null)), Encoding::Utf16);
    }
}
