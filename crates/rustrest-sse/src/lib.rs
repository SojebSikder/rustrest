//! Server-Sent Events client: opens a streaming GET via
//! `rustrest_core::http::open_stream` and parses the `text/event-stream`
//! wire format (blank-line-delimited records of `field: value` lines) as
//! bytes arrive, forwarding each decoded event to the caller.

use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Default)]
pub struct SseMessage {
    pub event: String,
    pub data: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SseEvent {
    Open {
        status: u16,
        headers: Vec<(String, String)>,
    },
    Message(SseMessage),
    Error(String),
    Closed,
}

/// Incremental `text/event-stream` parser: feed it raw bytes as they arrive
/// off any connection, and it hands back every complete event found so far,
/// buffering a partial trailing record until the rest arrives. Kept
/// separate from any particular way of obtaining those bytes, so the same
/// parsing logic can sit on top of a connection this crate opened itself
/// ([`run_session`]) or one an HTTP request already had open.
#[derive(Debug, Default)]
pub struct SseParser {
    buffer: String,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds in the next chunk of raw bytes, returning every event that
    /// chunk completed (zero, one, or more).
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<SseMessage> {
        self.buffer
            .push_str(&String::from_utf8_lossy(bytes).replace("\r\n", "\n"));

        let mut messages = Vec::new();
        while let Some(pos) = self.buffer.find("\n\n") {
            let record: String = self.buffer[..pos].to_string();
            self.buffer.drain(..pos + 2);
            if let Some(msg) = parse_record(&record) {
                messages.push(msg);
            }
        }
        messages
    }
}

/// Runs until the server closes the stream, an error occurs, or
/// `cancel_token` is cancelled.
pub async fn run_session(
    url: String,
    mut headers: Vec<(String, String)>,
    events: UnboundedSender<SseEvent>,
    cancel_token: CancellationToken,
) {
    let parsed_url = match url::Url::parse(&url) {
        Ok(u) => u,
        Err(e) => {
            let _ = events.send(SseEvent::Error(format!("Invalid URL: {}", e)));
            return;
        }
    };

    if !headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("accept"))
    {
        headers.push(("Accept".to_string(), "text/event-stream".to_string()));
    }

    let mut stream = match rustrest_core::http::open_stream(
        &parsed_url,
        http::Method::GET,
        headers,
        &cancel_token,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            let _ = events.send(SseEvent::Error(e));
            return;
        }
    };

    let response_headers = stream
        .headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();
    let _ = events.send(SseEvent::Open {
        status: stream.status,
        headers: response_headers,
    });

    let mut parser = SseParser::new();
    loop {
        let chunk = tokio::select! {
            c = stream.next_chunk() => c,
            _ = cancel_token.cancelled() => {
                let _ = events.send(SseEvent::Closed);
                return;
            }
        };

        match chunk {
            Some(Ok(bytes)) => {
                for msg in parser.feed(&bytes) {
                    let _ = events.send(SseEvent::Message(msg));
                }
            }
            Some(Err(e)) => {
                let _ = events.send(SseEvent::Error(e));
                break;
            }
            None => {
                let _ = events.send(SseEvent::Closed);
                break;
            }
        }
    }
}

/// Parses one blank-line-delimited SSE record into a message, per the
/// `text/event-stream` spec (unlabeled `data:` lines join with `\n`,
/// `:`-prefixed lines are comments, unknown fields are ignored).
fn parse_record(record: &str) -> Option<SseMessage> {
    let mut event = String::new();
    let mut data_lines: Vec<String> = Vec::new();
    let mut id = None;
    let mut saw_field = false;

    for line in record.split('\n') {
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = match line.find(':') {
            Some(idx) => {
                let value = &line[idx + 1..];
                (&line[..idx], value.strip_prefix(' ').unwrap_or(value))
            }
            None => (line, ""),
        };
        saw_field = true;
        match field {
            "event" => event = value.to_string(),
            "data" => data_lines.push(value.to_string()),
            "id" => id = Some(value.to_string()),
            _ => {}
        }
    }

    if !saw_field {
        return None;
    }

    Some(SseMessage {
        event: if event.is_empty() {
            "message".to_string()
        } else {
            event
        },
        data: data_lines.join("\n"),
        id,
    })
}
