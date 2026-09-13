use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rustrest_remote_protocol::{Envelope, Request, Response, read_message, write_message};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};

use crate::error::RemoteError;

pub struct RpcClient {
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>>,
    outbound: mpsc::UnboundedSender<Envelope<Request>>,
}

impl RpcClient {
    pub fn spawn<S>(stream: S) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (outbound, mut outbound_rx) = mpsc::unbounded_channel::<Envelope<Request>>();

        tokio::spawn(async move {
            while let Some(envelope) = outbound_rx.recv().await {
                if write_message(&mut writer, &envelope).await.is_err() {
                    break;
                }
            }
        });

        let pending_reader = pending.clone();
        tokio::spawn(async move {
            loop {
                let envelope: Envelope<Response> = match read_message(&mut reader).await {
                    Ok(envelope) => envelope,
                    Err(_) => break,
                };
                if let Some(sender) = pending_reader.lock().unwrap().remove(&envelope.id) {
                    let _ = sender.send(envelope.payload);
                }
            }
            for (_, sender) in pending_reader.lock().unwrap().drain() {
                let _ = sender.send(Response::Error(
                    "remote agent connection closed".to_string(),
                ));
            }
        });

        Self {
            next_id: AtomicU64::new(0),
            pending,
            outbound,
        }
    }

    pub async fn call(&self, request: Request) -> Result<Response, RemoteError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);

        if self
            .outbound
            .send(Envelope {
                id,
                payload: request,
            })
            .is_err()
        {
            self.pending.lock().unwrap().remove(&id);
            return Err(RemoteError::Disconnected);
        }

        rx.await.map_err(|_| RemoteError::Disconnected)
    }
}
