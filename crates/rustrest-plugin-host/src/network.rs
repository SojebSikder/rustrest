//! Host-side management of outbound HTTP requests a plugin starts via the
//! `ExternalProcess` capability's `http_request` host call.

use rustrest_plugin_api::HttpResponseData;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_RESPONSE_BYTES: u64 = 20 * 1024 * 1024; // 20 MB
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub enum NetworkEvent {
    /// one line of the response body, delivered as soon as it's read off
    /// the socket (before the response is complete) - lets a plugin render
    /// a streamed reply (e.g. an SSE/NDJSON chat completion) incrementally
    /// instead of waiting for the whole body.
    Chunk(u32, Vec<u8>),
    Response(u32, Result<HttpResponseData, String>),
}

#[derive(Default)]
pub struct NetworkTable {
    next_handle: u32,
    events: Vec<NetworkEvent>,
}

impl NetworkTable {
    /// starts `method url` with `headers`/`body` on a background thread,
    /// returning a handle immediately. `https://` only.
    pub fn spawn_request(
        shared: &Arc<Mutex<NetworkTable>>,
        method: String,
        url: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> Result<u32, String> {
        if !url.starts_with("https://") {
            return Err("only https:// urls are allowed".to_string());
        }

        let handle = {
            let mut table = shared.lock().expect("network table poisoned");
            table.next_handle += 1;
            table.next_handle
        };

        let shared = shared.clone();
        std::thread::spawn(move || {
            let result = run_request(handle, &shared, &method, &url, &headers, body.as_deref());
            let mut table = shared.lock().expect("network table poisoned");
            table.events.push(NetworkEvent::Response(handle, result));
        });

        Ok(handle)
    }

    /// drains buffered response events; called by the pump on a timer.
    pub fn drain_events(shared: &Arc<Mutex<NetworkTable>>) -> Vec<NetworkEvent> {
        let mut table = shared.lock().expect("network table poisoned");
        std::mem::take(&mut table.events)
    }
}

fn run_request(
    handle: u32,
    shared: &Arc<Mutex<NetworkTable>>,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<HttpResponseData, String> {
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;

    let mut request = client.request(method, url);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    if let Some(bytes) = body {
        request = request.body(bytes.to_vec());
    }

    let response = request.send().map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let response_headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();

    if response
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES)
    {
        return Err("response exceeds maximum allowed size".to_string());
    }

    // read line-by-line rather than all at once - a streamed reply (SSE or
    // NDJSON, as chat-completion APIs use) flushes one line per event, so
    // this lets the caller push a `Chunk` per line as it arrives instead of
    // blocking until the whole body has been received. Harmless for a
    // non-streamed body: it just reads out as one final "line".
    let mut reader = BufReader::new(response);
    let mut full_body = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        full_body.extend_from_slice(&line);
        if full_body.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("response exceeds maximum allowed size".to_string());
        }
        let mut table = shared.lock().expect("network table poisoned");
        table.events.push(NetworkEvent::Chunk(handle, line.clone()));
    }

    Ok(HttpResponseData {
        status,
        headers: response_headers,
        body: full_body,
    })
}
