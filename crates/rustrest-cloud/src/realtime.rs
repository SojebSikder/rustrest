//! realtime change hints over the cloud websocket.

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::Connector;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::api::{CloudClient, CloudError};
use crate::wire::Notification;

#[derive(Debug, Clone, PartialEq)]
pub enum RealtimeEvent {
    /// the socket is up; anything sent while it was down has to be fetched
    Connected,
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
    /// an environment of a subscribed team was created or changed; `rev`
    /// is its new rev
    EnvironmentChanged {
        team_id: String,
        environment_id: String,
        rev: i64,
    },
    /// an environment (or, without an id, every environment) of the team was deleted
    EnvironmentDeleted {
        team_id: String,
        environment_id: Option<String>,
    },
    /// server refused these subscriptions (no access / not found)
    Denied(Vec<String>),
    /// a new notification for the signed-in user
    Notification(Box<Notification>),
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
    #[serde(default)]
    team_id: String,
    #[serde(default)]
    environment_id: String,
    #[serde(default)]
    rev: i64,
    #[serde(default)]
    notification: Option<Notification>,
}

/// the server sends unset ids as the nil uuid
fn present(id: String) -> Option<String> {
    (!id.is_empty() && id != "00000000-0000-0000-0000-000000000000").then_some(id)
}

/// connects, subscribes to `collection_ids` and to the environments of
/// `team_ids`, and forwards events (starting with `Connected`) until the
/// socket closes. the returned receiver ends when the connection does.
pub async fn connect(
    client: &CloudClient,
    collection_ids: Vec<String>,
    team_ids: Vec<String>,
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

    // explicit rustls config: rustls is built with both ring and aws-lc-rs, so
    // tungstenite's default `ClientConfig::builder()` panics on wss://
    let connector = Connector::Rustls(rustrest_core::http::client_config());
    let (socket, _) =
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
            .await
            .map_err(|e| CloudError::Network(e.to_string()))?;
    let (mut write, mut read) = socket.split();

    // notifications need no subscription, so the socket may have none
    if !collection_ids.is_empty() || !team_ids.is_empty() {
        let subscribe = serde_json::json!({
            "type": "subscribe",
            "collection_ids": collection_ids,
            "team_ids": team_ids,
        });
        write
            .send(Message::Text(subscribe.to_string()))
            .await
            .map_err(|e| CloudError::Network(e.to_string()))?;
    }

    let (tx, rx) = mpsc::channel(64);
    let _ = tx.send(RealtimeEvent::Connected).await;
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
                "environment.changed" => RealtimeEvent::EnvironmentChanged {
                    team_id: msg.team_id,
                    environment_id: msg.environment_id,
                    rev: msg.rev,
                },
                "environment.deleted" => RealtimeEvent::EnvironmentDeleted {
                    team_id: msg.team_id,
                    environment_id: present(msg.environment_id),
                },
                "notification.created" => match msg.notification {
                    Some(notification) => RealtimeEvent::Notification(Box::new(notification)),
                    None => continue,
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
