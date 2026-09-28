//! realtime change hints over the cloud websocket. events carry no item
//! data: on `CollectionChanged`, run a sync for that collection. a dropped
//! connection only delays updates, so callers can simply reconnect.

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::api::{CloudClient, CloudError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeEvent {
    /// `seq` is the collection's new change seq, nothing to do if we're
    /// already there. `actor_id` lets a client ignore its own echoes.
    CollectionChanged {
        collection_id: String,
        seq: i64,
        actor_id: String,
    },
    CollectionDeleted {
        collection_id: String,
    },
    /// server refused these subscriptions (no access / not found)
    Denied(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct ServerMessage {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    collection_id: String,
    #[serde(default)]
    actor_id: String,
    #[serde(default)]
    seq: i64,
    #[serde(default)]
    denied: Vec<String>,
}

/// connects, subscribes to `collection_ids` and forwards events until the
/// socket closes. the returned receiver ends when the connection does.
pub async fn connect(
    client: &CloudClient,
    collection_ids: Vec<String>,
) -> Result<mpsc::Receiver<RealtimeEvent>, CloudError> {
    let token = client.fresh_access_token().await?;
    let mut request = client
        .ws_url()
        .into_client_request()
        .map_err(|e| CloudError::Network(e.to_string()))?;
    request.headers_mut().insert(
        http::header::AUTHORIZATION,
        format!("Bearer {token}")
            .parse()
            .map_err(|_| CloudError::Unauthorized)?,
    );

    let (socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|e| CloudError::Network(e.to_string()))?;
    let (mut write, mut read) = socket.split();

    let subscribe = serde_json::json!({ "type": "subscribe", "collection_ids": collection_ids });
    write
        .send(Message::Text(subscribe.to_string()))
        .await
        .map_err(|e| CloudError::Network(e.to_string()))?;

    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        while let Some(Ok(msg)) = read.next().await {
            let text = match msg {
                Message::Text(text) => text,
                Message::Ping(payload) => {
                    let _ = write.send(Message::Pong(payload)).await;
                    continue;
                }
                Message::Close(_) => break,
                _ => continue,
            };
            let Ok(msg) = serde_json::from_str::<ServerMessage>(&text) else {
                continue;
            };
            let event = match msg.kind.as_str() {
                "collection.changed" => RealtimeEvent::CollectionChanged {
                    collection_id: msg.collection_id,
                    seq: msg.seq,
                    actor_id: msg.actor_id,
                },
                "collection.deleted" => RealtimeEvent::CollectionDeleted {
                    collection_id: msg.collection_id,
                },
                "subscribed" if !msg.denied.is_empty() => RealtimeEvent::Denied(msg.denied),
                _ => continue,
            };
            if tx.send(event).await.is_err() {
                break; // receiver dropped
            }
        }
    });
    Ok(rx)
}
