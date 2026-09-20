//! WebSocket session engine: owns the socket, forwards inbound
//! frames to the caller and accepts outbound frames over a
//! pair of mpsc channels. Kept transport-agnostic from the UI layer's point
//! of view so the `iced` app only ever sees [`WsEvent`]/[`WsOutbound`].

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc::{Sender, UnboundedReceiver};
use tokio_tungstenite::Connector;
use tokio_tungstenite::tungstenite::Message as WsProtoMessage;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};

#[derive(Debug, Clone)]
pub enum WsMessageKind {
    Text(String),
    Binary(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum WsEvent {
    Connected,
    Message(WsMessageKind),
    Error(String),
    Closed(Option<String>),
}

#[derive(Debug, Clone)]
pub enum WsOutbound {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

/// Connects to `url`, then runs until the connection closes, an error
/// occurs, or `outgoing` is dropped (the caller closed the tab / cancelled).
/// Every event is reported through `events`; the caller is expected to have
/// already spawned this as its own task.
pub async fn run_session(
    url: String,
    headers: Vec<(String, String)>,
    mut outgoing: UnboundedReceiver<WsOutbound>,
    events: Sender<WsEvent>,
) {
    let mut request = match url.into_client_request() {
        Ok(req) => req,
        Err(e) => {
            let _ = events
                .send(WsEvent::Error(format!("Invalid WebSocket URL: {}", e)))
                .await;
            return;
        }
    };

    for (key, value) in &headers {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            request.headers_mut().insert(name, val);
        }
    }

    let connector = Connector::Rustls(rustrest_core::http::client_config());
    let ws_stream = match tokio_tungstenite::connect_async_tls_with_config(
        request,
        None,
        false,
        Some(connector),
    )
    .await
    {
        Ok((stream, _response)) => stream,
        Err(e) => {
            let _ = events
                .send(WsEvent::Error(format!("Connection failed: {}", e)))
                .await;
            return;
        }
    };

    let _ = events.send(WsEvent::Connected).await;
    let (mut write, mut read) = ws_stream.split();

    loop {
        tokio::select! {
            outbound = outgoing.recv() => {
                match outbound {
                    Some(WsOutbound::Text(text)) => {
                        if write.send(WsProtoMessage::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Some(WsOutbound::Binary(bytes)) => {
                        if write.send(WsProtoMessage::Binary(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Some(WsOutbound::Close) => {
                        let _ = write.send(WsProtoMessage::Close(None)).await;
                        break;
                    }
                    None => break, // tab closed / cancelled: drop the connection
                }
            }
            incoming = read.next() => {
                match incoming {
                    Some(Ok(WsProtoMessage::Text(text))) => {
                        let _ = events
                            .send(WsEvent::Message(WsMessageKind::Text(text.as_str().to_owned())))
                            .await;
                    }
                    Some(Ok(WsProtoMessage::Binary(bytes))) => {
                        let _ = events
                            .send(WsEvent::Message(WsMessageKind::Binary(bytes.to_vec())))
                            .await;
                    }
                    Some(Ok(WsProtoMessage::Ping(_))) | Some(Ok(WsProtoMessage::Pong(_))) => {}
                    Some(Ok(WsProtoMessage::Frame(_))) => {}
                    Some(Ok(WsProtoMessage::Close(frame))) => {
                        let _ = events
                            .send(WsEvent::Closed(frame.map(|f| f.reason.to_string())))
                            .await;
                        break;
                    }
                    Some(Err(e)) => {
                        let _ = events.send(WsEvent::Error(e.to_string())).await;
                        break;
                    }
                    None => {
                        let _ = events.send(WsEvent::Closed(None)).await;
                        break;
                    }
                }
            }
        }
    }
}
