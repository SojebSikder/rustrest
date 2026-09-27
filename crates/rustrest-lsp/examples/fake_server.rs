//! simple mtiny LSP server for testing.
//! - diagnostics: a warning on the first `bad` in the document
//! - completion: `alpha`, `beta`, plus `at:<line>:<character>` echoing the request
//! - hover: `hover <line>:<character> cfg=<workspace/configuration answer>`
//! - a document containing `crash` makes it exit with code 3

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};

fn read_message(input: &mut impl BufRead) -> Option<Value> {
    let mut len = 0;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length: ") {
            len = v.parse().ok()?;
        }
    }
    let mut body = vec![0; len];
    input.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn send(message: Value) {
    let body = message.to_string();
    let mut out = std::io::stdout().lock();
    write!(out, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    out.flush().unwrap();
}

fn utf16_position(text: &str, byte: usize) -> Value {
    let before = &text[..byte];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = if std::env::var("FAKE_LSP_UTF8").is_ok() {
        byte - line_start
    } else {
        text[line_start..byte].encode_utf16().count()
    };
    json!({ "line": line, "character": column })
}

fn publish(uri: &str, version: &Value, text: &str) {
    let diagnostics: Vec<Value> = text
        .find("bad")
        .map(|i| {
            json!({
                "range": { "start": utf16_position(text, i), "end": utf16_position(text, i + 3) },
                "severity": 2,
                "message": "bad word",
                "source": "fake"
            })
        })
        .into_iter()
        .collect();
    send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": { "uri": uri, "version": version, "diagnostics": diagnostics }
    }));
}

fn main() {
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut docs: HashMap<String, String> = HashMap::new();
    let mut config = Value::Null;
    eprintln!("fake server starting");

    while let Some(message) = read_message(&mut input) {
        let method = message["method"].as_str().unwrap_or_default();
        let id = message.get("id").cloned();
        let params = &message["params"];
        match method {
            "initialize" => {
                send(json!({
                    "jsonrpc": "2.0", "id": "cfg", "method": "workspace/configuration",
                    "params": { "items": [{ "section": "fake" }] }
                }));
                let encoding = if std::env::var("FAKE_LSP_UTF8").is_ok() {
                    "utf-8"
                } else {
                    "utf-16"
                };
                send(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "capabilities": { "positionEncoding": encoding } }
                }));
            }
            "textDocument/didOpen" => {
                let doc = &params["textDocument"];
                let uri = doc["uri"].as_str().unwrap().to_string();
                let text = doc["text"].as_str().unwrap().to_string();
                publish(&uri, &doc["version"], &text);
                docs.insert(uri, text);
            }
            "textDocument/didChange" => {
                let uri = params["textDocument"]["uri"].as_str().unwrap().to_string();
                let text = params["contentChanges"][0]["text"]
                    .as_str()
                    .unwrap()
                    .to_string();
                if text.contains("crash") {
                    std::process::exit(3);
                }
                publish(&uri, &params["textDocument"]["version"], &text);
                docs.insert(uri, text);
            }
            "textDocument/completion" => {
                let p = &params["position"];
                send(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "isIncomplete": false, "items": [
                        { "label": "alpha", "kind": 3, "detail": "()" },
                        { "label": "beta", "kind": 10, "documentation": { "kind": "markdown", "value": "B" } },
                        { "label": format!("at:{}:{}", p["line"], p["character"]) }
                    ]}
                }));
            }
            "textDocument/hover" => {
                let p = &params["position"];
                send(json!({
                    "jsonrpc": "2.0", "id": id,
                    "result": { "contents": { "kind": "plaintext",
                        "value": format!("hover {}:{} cfg={config}", p["line"], p["character"]) } }
                }));
            }
            "shutdown" => send(json!({ "jsonrpc": "2.0", "id": id, "result": null })),
            "exit" => return,
            _ if method.is_empty() && id == Some(json!("cfg")) => {
                config = message["result"].clone();
            }
            _ => {}
        }
    }
}
