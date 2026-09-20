//! GraphQL client: queries/mutations run as a plain HTTP POST through
//! `rustrest_core::http::send_request` (GraphQL-over-HTTP is just JSON), and
//! subscriptions run the `graphql-transport-ws` protocol over
//! `rustrest_ws`'s session engine.

pub mod schema;

use rustrest_core::BodyType;
use rustrest_core::http::{HttpMethod, RequestSpec, send_request};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub use schema::{OperationKind, Schema, SchemaField, SelectionNode, find_node_mut};

/// This is the introspection query used to discover the schema. it's needed
/// to know what fields are available on each type.
pub const INTROSPECTION_QUERY: &str = include_str!("introspection_query.graphql");

pub struct GraphQlRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query: String,
    pub variables: Option<Value>,
    pub operation_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GraphQlResponse {
    pub status: u16,
    pub body: String,
    pub elapsed: std::time::Duration,
}

/// Runs a query or mutation over HTTP. Subscriptions must use
/// [`run_subscription`] instead.
pub async fn execute(
    req: GraphQlRequest,
    cancel_token: CancellationToken,
) -> Result<GraphQlResponse, String> {
    let payload = json!({
        "query": req.query,
        "variables": req.variables.unwrap_or(Value::Null),
        "operationName": req.operation_name,
    });
    let body = serde_json::to_string(&payload).map_err(|e| e.to_string())?;

    let mut headers = req.headers;
    if !headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    {
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
    }

    let spec = RequestSpec::new(req.url, HttpMethod::POST)
        .body_type(BodyType::Raw)
        .raw_body(body)
        .headers(headers);

    let resp = send_request(spec, cancel_token).await?;
    Ok(GraphQlResponse {
        status: resp.status,
        body: resp.body,
        elapsed: resp.elapsed,
    })
}

/// Fetches the schema via the standard introspection query.
pub async fn introspect(
    url: String,
    headers: Vec<(String, String)>,
    cancel_token: CancellationToken,
) -> Result<GraphQlResponse, String> {
    execute(
        GraphQlRequest {
            url,
            headers,
            query: INTROSPECTION_QUERY.to_string(),
            variables: None,
            operation_name: None,
        },
        cancel_token,
    )
    .await
}

#[derive(Debug, Clone)]
pub enum SubscriptionEvent {
    Connected,
    Data(Value),
    Error(String),
    Complete,
}

pub enum SubscriptionCommand {
    Stop,
}

const SUBSCRIPTION_ID: &str = "1";

/// Drives one `graphql-transport-ws` subscription over a `rustrest_ws`
/// session it owns internally. Runs until the server completes/errors the
/// subscription, the connection drops, or [`SubscriptionCommand::Stop`] (or
/// dropping `commands`) is received.
pub async fn run_subscription(
    url: String,
    mut headers: Vec<(String, String)>,
    query: String,
    variables: Option<Value>,
    operation_name: Option<String>,
    mut commands: mpsc::UnboundedReceiver<SubscriptionCommand>,
    out: mpsc::Sender<SubscriptionEvent>,
) {
    headers.push((
        "Sec-WebSocket-Protocol".to_string(),
        "graphql-transport-ws".to_string(),
    ));

    let (ws_out_tx, ws_out_rx) = mpsc::unbounded_channel();
    let (ws_evt_tx, mut ws_evt_rx) = mpsc::channel(64);
    tokio::spawn(rustrest_ws::run_session(url, headers, ws_out_rx, ws_evt_tx));

    let mut subscribed = false;

    loop {
        tokio::select! {
            evt = ws_evt_rx.recv() => {
                match evt {
                    Some(rustrest_ws::WsEvent::Connected) => {
                        let init = json!({"type": "connection_init"});
                        let _ = ws_out_tx.send(rustrest_ws::WsOutbound::Text(init.to_string()));
                    }
                    Some(rustrest_ws::WsEvent::Message(rustrest_ws::WsMessageKind::Text(text))) => {
                        let Ok(msg) = serde_json::from_str::<Value>(&text) else { continue };
                        match msg.get("type").and_then(Value::as_str) {
                            Some("connection_ack") => {
                                let subscribe = json!({
                                    "id": SUBSCRIPTION_ID,
                                    "type": "subscribe",
                                    "payload": {
                                        "query": query,
                                        "variables": variables.clone().unwrap_or(Value::Null),
                                        "operationName": operation_name,
                                    }
                                });
                                let _ = ws_out_tx.send(rustrest_ws::WsOutbound::Text(subscribe.to_string()));
                                subscribed = true;
                                let _ = out.send(SubscriptionEvent::Connected).await;
                            }
                            Some("next") => {
                                if let Some(payload) = msg.get("payload") {
                                    let _ = out.send(SubscriptionEvent::Data(payload.clone())).await;
                                }
                            }
                            Some("error") => {
                                let reason = msg.get("payload").map(|p| p.to_string()).unwrap_or_default();
                                let _ = out.send(SubscriptionEvent::Error(reason)).await;
                            }
                            Some("complete") => {
                                let _ = out.send(SubscriptionEvent::Complete).await;
                                break;
                            }
                            _ => {}
                        }
                    }
                    Some(rustrest_ws::WsEvent::Error(e)) => {
                        let _ = out.send(SubscriptionEvent::Error(e)).await;
                        break;
                    }
                    Some(rustrest_ws::WsEvent::Closed(reason)) => {
                        let _ = out.send(SubscriptionEvent::Error(
                            reason.unwrap_or_else(|| "connection closed".to_string()),
                        )).await;
                        break;
                    }
                    Some(rustrest_ws::WsEvent::Message(rustrest_ws::WsMessageKind::Binary(_))) => {}
                    None => break,
                }
            }
            cmd = commands.recv() => {
                if subscribed {
                    let complete = json!({"id": SUBSCRIPTION_ID, "type": "complete"});
                    let _ = ws_out_tx.send(rustrest_ws::WsOutbound::Text(complete.to_string()));
                }
                let _ = ws_out_tx.send(rustrest_ws::WsOutbound::Close);
                let _ = cmd;
                break;
            }
        }
    }
}
