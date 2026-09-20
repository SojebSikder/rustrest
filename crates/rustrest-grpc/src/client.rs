//! Connecting to a gRPC endpoint and invoking a method dynamically.

use crate::codec::DynamicCodec;
use http::uri::PathAndQuery;
use prost_reflect::{DynamicMessage, MessageDescriptor};
use tokio::sync::mpsc::Sender;
use tonic::transport::{Channel, ClientTlsConfig, Uri};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Unary,
    ClientStreaming,
    ServerStreaming,
    Bidi,
}

impl MethodKind {
    pub fn from_flags(client_streaming: bool, server_streaming: bool) -> Self {
        match (client_streaming, server_streaming) {
            (false, false) => MethodKind::Unary,
            (true, false) => MethodKind::ClientStreaming,
            (false, true) => MethodKind::ServerStreaming,
            (true, true) => MethodKind::Bidi,
        }
    }
}

pub async fn connect(endpoint: &str, use_tls: bool) -> Result<Channel, String> {
    let uri = build_uri(endpoint, use_tls)?;
    let mut builder = Channel::builder(uri);
    if use_tls {
        let tls = ClientTlsConfig::new().with_webpki_roots();
        builder = builder
            .tls_config(tls)
            .map_err(|e| format!("TLS config error: {}", error_chain(&e)))?;
    }
    builder
        .connect()
        .await
        .map_err(|e| format!("Connection to '{endpoint}' failed: {}", error_chain(&e)))
}

/// Normalizes `endpoint` (typically a bare `host:port`) into a URI with an
/// explicit scheme matching `use_tls`. Tonic decides which connector to use
/// (plain TCP vs TLS) from the URI scheme, so a scheme-less "host:port"
/// string and a mismatched TLS checkbox is a common way to get an opaque
/// "transport error" - e.g. connecting in plaintext to a TLS-only endpoint
/// like `grpc.postman-echo.com`.
fn build_uri(endpoint: &str, use_tls: bool) -> Result<Uri, String> {
    let trimmed = endpoint.trim();
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        let scheme = if use_tls { "https" } else { "http" };
        format!("{scheme}://{trimmed}")
    };
    with_scheme
        .parse()
        .map_err(|e| format!("Invalid endpoint '{endpoint}': {e}"))
}

/// Renders an error together with its full `source()` chain - tonic's
/// `transport::Error` Display is often just a generic top-level summary
/// (e.g. "transport error") with the actually useful detail (TLS failure,
/// connection refused, DNS failure, ...) nested underneath.
fn error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        message.push_str(" -> ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

fn attach_metadata<T>(
    mut req: tonic::Request<T>,
    metadata: &[(String, String)],
) -> tonic::Request<T> {
    for (key, value) in metadata {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        if let (Ok(name), Ok(val)) = (
            tonic::metadata::MetadataKey::from_bytes(key.to_lowercase().as_bytes()),
            tonic::metadata::MetadataValue::try_from(value.as_str()),
        ) {
            req.metadata_mut().insert(name, val);
        }
    }
    req
}

/// Invokes one RPC, forwarding every response message (one, for
/// unary/client-streaming; possibly many, for server-streaming/bidi) into
/// `out` as it arrives. `requests` holds every message to send; for
/// client-streaming/bidi calls they're all queued up front (this client
/// doesn't support interactively composing further messages mid-call).
pub async fn invoke(
    channel: Channel,
    path: PathAndQuery,
    response_desc: MessageDescriptor,
    kind: MethodKind,
    requests: Vec<DynamicMessage>,
    metadata: Vec<(String, String)>,
    out: Sender<Result<DynamicMessage, String>>,
) {
    let mut client = tonic::client::Grpc::new(channel);
    if let Err(e) = client.ready().await {
        let _ = out
            .send(Err(format!("Channel not ready: {}", error_chain(&e))))
            .await;
        return;
    }

    let single_request = || {
        requests
            .clone()
            .into_iter()
            .next()
            .ok_or_else(|| "No request message provided".to_string())
    };

    let result: Result<(), String> = match kind {
        MethodKind::Unary => match single_request() {
            Ok(msg) => {
                let req = attach_metadata(tonic::Request::new(msg), &metadata);
                let codec = DynamicCodec { response_desc };
                match client.unary(req, path, codec).await {
                    Ok(resp) => {
                        let _ = out.send(Ok(resp.into_inner())).await;
                        Ok(())
                    }
                    Err(status) => Err(status.to_string()),
                }
            }
            Err(e) => Err(e),
        },
        MethodKind::ServerStreaming => match single_request() {
            Ok(msg) => {
                let req = attach_metadata(tonic::Request::new(msg), &metadata);
                let codec = DynamicCodec { response_desc };
                match client.server_streaming(req, path, codec).await {
                    Ok(resp) => {
                        drain_stream(resp.into_inner(), &out).await;
                        Ok(())
                    }
                    Err(status) => Err(status.to_string()),
                }
            }
            Err(e) => Err(e),
        },
        MethodKind::ClientStreaming => {
            let req = attach_metadata(tonic::Request::new(tokio_stream::iter(requests)), &metadata);
            let codec = DynamicCodec { response_desc };
            match client.client_streaming(req, path, codec).await {
                Ok(resp) => {
                    let _ = out.send(Ok(resp.into_inner())).await;
                    Ok(())
                }
                Err(status) => Err(status.to_string()),
            }
        }
        MethodKind::Bidi => {
            let req = attach_metadata(tonic::Request::new(tokio_stream::iter(requests)), &metadata);
            let codec = DynamicCodec { response_desc };
            match client.streaming(req, path, codec).await {
                Ok(resp) => {
                    drain_stream(resp.into_inner(), &out).await;
                    Ok(())
                }
                Err(status) => Err(status.to_string()),
            }
        }
    };

    if let Err(e) = result {
        let _ = out.send(Err(e)).await;
    }
}

async fn drain_stream(
    mut stream: tonic::Streaming<DynamicMessage>,
    out: &Sender<Result<DynamicMessage, String>>,
) {
    loop {
        match stream.message().await {
            Ok(Some(msg)) => {
                let _ = out.send(Ok(msg)).await;
            }
            Ok(None) => break,
            Err(status) => {
                let _ = out.send(Err(status.to_string())).await;
                break;
            }
        }
    }
}
