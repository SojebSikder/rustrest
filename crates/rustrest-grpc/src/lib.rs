//! Dynamic gRPC client: no generated Rust code for the target service.
//! Methods are discovered either via server reflection ([`ProtoSource::Reflection`])
//! or by compiling user-supplied `.proto` files ([`ProtoSource::Files`]),
//! then invoked by converting JSON request bodies to/from
//! `prost_reflect::DynamicMessage` on the fly.

mod client;
mod codec;
mod descriptor;
mod discovery;
mod skeleton;

pub use client::MethodKind;
pub use prost_reflect::{DescriptorPool, MessageDescriptor};

use serde_json::Value;
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub enum ProtoSource {
    Reflection,
    Files(Vec<PathBuf>),
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    /// e.g. `/package.Service/Method`
    pub full_path: String,
    pub kind: MethodKind,
    pub input: MessageDescriptor,
    pub output: MessageDescriptor,
}

#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub methods: Vec<MethodInfo>,
}

#[derive(Debug, Clone)]
pub struct GrpcTarget {
    pub pool: DescriptorPool,
    pub services: Vec<ServiceInfo>,
}

/// Discovers the services/methods a target exposes, either by asking it
/// (reflection) or by reading `.proto` files the user picked.
pub async fn discover(
    endpoint: String,
    use_tls: bool,
    source: ProtoSource,
) -> Result<GrpcTarget, String> {
    let pool = match source {
        ProtoSource::Reflection => {
            let channel = client::connect(&endpoint, use_tls).await?;
            let (pool, _service_names) = discovery::discover_via_reflection(channel).await?;
            pool
        }
        ProtoSource::Files(files) => descriptor::compile_proto_files(&files)?,
    };

    let services = pool
        .services()
        .map(|svc| {
            let methods = svc
                .methods()
                .map(|m| MethodInfo {
                    name: m.name().to_string(),
                    full_path: format!("/{}/{}", svc.full_name(), m.name()),
                    kind: MethodKind::from_flags(m.is_client_streaming(), m.is_server_streaming()),
                    input: m.input(),
                    output: m.output(),
                })
                .collect();
            ServiceInfo {
                name: svc.full_name().to_string(),
                methods,
            }
        })
        .collect();

    Ok(GrpcTarget { pool, services })
}

/// A JSON skeleton for `desc`, to pre-fill the request editor.
pub fn json_skeleton(desc: &MessageDescriptor) -> Value {
    skeleton::json_skeleton(desc)
}

/// Invokes one RPC. `request_json` is parsed as one JSON object per request
/// message to send (a single object for unary/server-streaming; a JSON
/// array of objects for client-streaming/bidi). Every response message is
/// converted to a pretty-printed JSON string and pushed into `out` as it
/// arrives.
pub async fn invoke(
    endpoint: String,
    use_tls: bool,
    method: MethodInfo,
    request_json: String,
    metadata: Vec<(String, String)>,
    out: Sender<Result<String, String>>,
) {
    let requests = match parse_requests(&method, &request_json) {
        Ok(reqs) => reqs,
        Err(e) => {
            let _ = out.send(Err(e)).await;
            return;
        }
    };

    let channel = match client::connect(&endpoint, use_tls).await {
        Ok(c) => c,
        Err(e) => {
            let _ = out.send(Err(e)).await;
            return;
        }
    };

    let path: http::uri::PathAndQuery = match method.full_path.parse() {
        Ok(p) => p,
        Err(e) => {
            let _ = out
                .send(Err(format!(
                    "Invalid method path '{}': {e}",
                    method.full_path
                )))
                .await;
            return;
        }
    };

    // bounded so a fast server-streaming/bidi response can't queue an
    // unbounded backlog here if the UI is momentarily behind
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let output_desc = method.output.clone();
    tokio::spawn(client::invoke(
        channel,
        path,
        output_desc,
        method.kind,
        requests,
        metadata,
        tx,
    ));

    while let Some(result) = rx.recv().await {
        let mapped = result.and_then(|msg| {
            serde_json::to_string_pretty(&msg)
                .map_err(|e| format!("Failed to encode response as JSON: {e}"))
        });
        if out.send(mapped).await.is_err() {
            break;
        }
    }
}

fn parse_requests(
    method: &MethodInfo,
    request_json: &str,
) -> Result<Vec<prost_reflect::DynamicMessage>, String> {
    let trimmed = request_json.trim();
    let raw_messages: Vec<Value> =
        if method.kind == MethodKind::ClientStreaming || method.kind == MethodKind::Bidi {
            if trimmed.is_empty() {
                Vec::new()
            } else {
                let parsed: Value =
                    serde_json::from_str(trimmed).map_err(|e| format!("Invalid JSON: {e}"))?;
                match parsed {
                    Value::Array(items) => items,
                    single => vec![single],
                }
            }
        } else if trimmed.is_empty() {
            vec![Value::Object(Default::default())]
        } else {
            vec![serde_json::from_str(trimmed).map_err(|e| format!("Invalid JSON: {e}"))?]
        };

    raw_messages
        .into_iter()
        .map(|v| {
            prost_reflect::DynamicMessage::deserialize(method.input.clone(), v)
                .map_err(|e| format!("Request does not match the method's input message: {e}"))
        })
        .collect()
}
