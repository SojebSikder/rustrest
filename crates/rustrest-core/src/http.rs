mod multipart;
mod stream;
mod timed_client;
mod tls;

pub use multipart::guess_mime;
pub use stream::{StreamingResponse, open_stream};
pub use tls::client_config;

use crate::common::{BodyType, FormDataRow};
use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PhaseTimings {
    pub prepare: Duration,
    pub socket_initialization: Duration,
    pub dns_lookup: Duration,
    pub tcp_handshake: Duration,
    pub ssl_handshake: Duration,
    pub waiting: Duration,
    pub download: Duration,
    pub process: Duration,
}

impl PhaseTimings {
    pub fn total(&self) -> Duration {
        self.prepare
            + self.socket_initialization
            + self.dns_lookup
            + self.tcp_handshake
            + self.ssl_handshake
            + self.waiting
            + self.download
            + self.process
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    pub headers: HashMap<String, String>,
    pub elapsed: Duration,
    pub test_results: Vec<TestResult>,
    pub timings: PhaseTimings,
    pub request_size: u64,
    pub response_size: u64,
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::Custom(custom) => write!(f, "{}", custom.to_uppercase()),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Describes one HTTP request to send
pub struct RequestSpec {
    url: String,
    method: HttpMethod,
    body_type: BodyType,
    raw_body: String,
    form_data: Vec<FormDataRow>,
    binary_file_path: Option<String>,
    headers: Vec<(String, String)>,
    cookies: Vec<(String, String)>,
    timeout: Duration,
}

impl RequestSpec {
    pub fn new(url: impl Into<String>, method: HttpMethod) -> Self {
        Self {
            url: url.into(),
            method,
            body_type: BodyType::None,
            raw_body: String::new(),
            form_data: Vec::new(),
            binary_file_path: None,
            headers: Vec::new(),
            cookies: Vec::new(),
            timeout: Duration::from_secs(30),
        }
    }

    pub fn body_type(mut self, body_type: BodyType) -> Self {
        self.body_type = body_type;
        self
    }

    pub fn raw_body(mut self, raw_body: impl Into<String>) -> Self {
        self.raw_body = raw_body.into();
        self
    }

    pub fn form_data(mut self, form_data: Vec<FormDataRow>) -> Self {
        self.form_data = form_data;
        self
    }

    pub fn binary_file_path(mut self, path: Option<String>) -> Self {
        self.binary_file_path = path;
        self
    }

    pub fn headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.headers = headers;
        self
    }

    pub fn cookies(mut self, cookies: Vec<(String, String)>) -> Self {
        self.cookies = cookies;
        self
    }

    #[allow(dead_code)]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

fn map_method(method: &HttpMethod) -> Result<http::Method, String> {
    Ok(match method {
        HttpMethod::GET => http::Method::GET,
        HttpMethod::POST => http::Method::POST,
        HttpMethod::PUT => http::Method::PUT,
        HttpMethod::DELETE => http::Method::DELETE,
        HttpMethod::PATCH => http::Method::PATCH,
        HttpMethod::HEAD => http::Method::HEAD,
        HttpMethod::OPTIONS => http::Method::OPTIONS,
        HttpMethod::Custom(custom_str) => {
            let upper = custom_str.trim().to_uppercase();
            http::Method::from_bytes(upper.as_bytes())
                .map_err(|_| format!("Invalid custom HTTP method: '{}'", custom_str))?
        }
    })
}

/// Merges request headers with the assembled Cookie header into one ordered
/// header list, dropping blank keys.
fn collect_headers(
    headers: Vec<(String, String)>,
    cookie_header: Option<String>,
) -> Vec<(String, String)> {
    let mut result: Vec<(String, String)> = headers
        .into_iter()
        .filter(|(key, _)| !key.trim().is_empty())
        .map(|(key, val)| (key.trim().to_string(), val))
        .collect();

    if let Some(cookie) = cookie_header {
        result.push(("Cookie".to_string(), cookie));
    }

    result
}

fn build_cookie_header(cookies: Vec<(String, String)>) -> Option<String> {
    let formatted: String = cookies
        .into_iter()
        .filter(|(key, _)| !key.trim().is_empty())
        .map(|(key, val)| format!("{}={}", key.trim(), val.trim()))
        .collect::<Vec<String>>()
        .join("; ");

    if formatted.is_empty() {
        None
    } else {
        Some(formatted)
    }
}

/// Reads the binary body file from disk, if a path was given and it exists.
async fn build_binary_body(binary_file_path: &Option<String>) -> Result<Option<Vec<u8>>, String> {
    if let Some(path_str) = binary_file_path {
        let path = Path::new(path_str);
        if path.exists() {
            let file_bytes = tokio::fs::read(path)
                .await
                .map_err(|e| format!("Binary File Read Failure: {}", e))?;
            return Ok(Some(file_bytes));
        }
    }
    Ok(None)
}

/// Resolves the request body appropriate to `body_type`, plus any header the
/// body encoding itself requires (namely the multipart boundary).
async fn build_body(
    method: &HttpMethod,
    body_type: BodyType,
    raw_body: String,
    form_data: Vec<FormDataRow>,
    binary_file_path: Option<String>,
) -> Result<(Vec<u8>, Option<(String, String)>), String> {
    // no-op for GET/HEAD requests
    if *method == HttpMethod::GET || *method == HttpMethod::HEAD {
        return Ok((Vec::new(), None));
    }

    match body_type {
        BodyType::FormData => match multipart::encode(form_data).await? {
            Some((boundary, bytes)) => Ok((
                bytes,
                Some((
                    "Content-Type".to_string(),
                    format!("multipart/form-data; boundary={}", boundary),
                )),
            )),
            None => Ok((Vec::new(), None)),
        },
        BodyType::Binary => {
            let bytes = build_binary_body(&binary_file_path)
                .await?
                .unwrap_or_default();
            Ok((bytes, None))
        }
        // fallback text states (raw JSON, URLencoded forms, etc.)
        _ => {
            if raw_body.trim().is_empty() {
                Ok((Vec::new(), None))
            } else {
                Ok((raw_body.into_bytes(), None))
            }
        }
    }
}

/// Reconstructs an approximate wire size for the response (status line +
/// headers + body), since the low-level HTTP/1.1 parser doesn't retain the
/// original bytes off the socket.
fn estimate_response_size(status: u16, headers: &http::HeaderMap, body_len: usize) -> u64 {
    let reason = http::StatusCode::from_u16(status)
        .ok()
        .and_then(|s| s.canonical_reason())
        .unwrap_or("");
    let status_line_len = format!("HTTP/1.1 {} {}\r\n", status, reason).len();
    let headers_len: usize = headers
        .iter()
        .map(|(k, v)| k.as_str().len() + 2 + v.to_str().map(str::len).unwrap_or(0) + 2)
        .sum();

    (status_line_len + headers_len + 2 + body_len) as u64
}

fn shape_response(
    outcome: timed_client::ExecOutcome,
    prepare: Duration,
    process_start: Instant,
) -> HttpResponse {
    let mut headers = HashMap::new();
    for (key, value) in outcome.headers.iter() {
        if let Ok(val_str) = value.to_str() {
            headers.insert(key.to_string(), val_str.to_string());
        }
    }

    let response_size =
        estimate_response_size(outcome.status, &outcome.headers, outcome.body.len());
    let body_text = String::from_utf8_lossy(&outcome.body).into_owned();

    let finalized_body = if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body_text)
    {
        serde_json::to_string_pretty(&json_val).unwrap_or(body_text)
    } else {
        body_text
    };

    let mut timings = outcome.timings;
    timings.prepare = prepare;
    // measured last, since "process" must cover all header/body shaping above
    timings.process = process_start.elapsed();

    HttpResponse {
        status: outcome.status,
        body: finalized_body,
        headers,
        elapsed: timings.total(),
        test_results: Vec::new(),
        timings,
        request_size: outcome.request_size,
        response_size,
    }
}

struct PreparedRequest {
    url: Url,
    method: http::Method,
    headers: Vec<(String, String)>,
    body: Bytes,
    timeout: Duration,
    prepare: Duration,
}

/// Everything about a `RequestSpec` that can be resolved before touching
/// the network: URL/method parsing, merging in the cookie/auth headers, and
/// building the body bytes.
async fn prepare(spec: RequestSpec) -> Result<PreparedRequest, String> {
    let prepare_start = Instant::now();

    let url = Url::parse(&spec.url).map_err(|e| format!("Invalid URL pattern: {}", e))?;
    let method = map_method(&spec.method)?;

    let cookie_header = build_cookie_header(spec.cookies);
    let mut header_list = collect_headers(spec.headers, cookie_header);

    let (body_bytes, extra_header) = build_body(
        &spec.method,
        spec.body_type,
        spec.raw_body,
        spec.form_data,
        spec.binary_file_path,
    )
    .await?;

    // the body encoding's own header (the multipart boundary) always wins: a
    // user-set `Content-Type`, e.g. the default `application/json` on a new
    // tab, would otherwise hide the boundary and the server couldn't parse it.
    if let Some((key, val)) = extra_header {
        header_list.retain(|(k, _)| !k.eq_ignore_ascii_case(&key));
        header_list.push((key, val));
    }

    Ok(PreparedRequest {
        url,
        method,
        headers: header_list,
        body: Bytes::from(body_bytes),
        timeout: spec.timeout,
        prepare: prepare_start.elapsed(),
    })
}

pub async fn send_request(
    spec: RequestSpec,
    cancel_token: CancellationToken,
) -> Result<HttpResponse, String> {
    let prepared = prepare(spec).await?;

    let outcome = match tokio::time::timeout(
        prepared.timeout,
        timed_client::execute_request(
            prepared.method,
            prepared.url,
            prepared.headers,
            prepared.body,
            &cancel_token,
        ),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => return Err("Request timed out".to_string()),
    };

    let process_start = Instant::now();
    Ok(shape_response(outcome, prepared.prepare, process_start))
}

/// A response whose headers have arrived; the body hasn't been touched yet.
pub enum SendOutcome {
    /// A normal response, fully read and shaped exactly like `send_request`
    /// would.
    Complete(HttpResponse),
    /// The response is `text/event-stream` - status/headers are resolved,
    /// and `body` is left open for the caller to read event chunks from
    /// live instead of buffering the whole (potentially endless) stream.
    EventStream {
        status: u16,
        headers: HashMap<String, String>,
        body: EventStreamBody,
    },
}

pub struct EventStreamBody(Incoming);

impl EventStreamBody {
    /// Pulls the next non-empty body chunk, or `None` once the server has
    /// closed the stream.
    pub async fn next_chunk(&mut self) -> Option<Result<Bytes, String>> {
        loop {
            match self.0.frame().await {
                Some(Ok(frame)) => match frame.into_data() {
                    Ok(data) if !data.is_empty() => return Some(Ok(data)),
                    Ok(_) => continue,
                    Err(_) => continue,
                },
                Some(Err(e)) => return Some(Err(format!("Stream read error: {}", e))),
                None => return None,
            }
        }
    }
}

/// Sends a request exactly like `send_request`, except that a
/// `text/event-stream` response is detected from its `Content-Type` and
/// handed back still-open for live streaming, instead of being buffered in
/// full (which would never finish for a long-lived SSE feed).
pub async fn send_request_auto(
    spec: RequestSpec,
    cancel_token: CancellationToken,
) -> Result<SendOutcome, String> {
    let prepared = prepare(spec).await?;

    let final_hop = match tokio::time::timeout(
        prepared.timeout,
        timed_client::execute_request_head(
            prepared.method,
            prepared.url,
            prepared.headers,
            prepared.body,
            &cancel_token,
        ),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => return Err("Request timed out".to_string()),
    };

    let is_event_stream = final_hop
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| {
            s.trim_start()
                .to_ascii_lowercase()
                .starts_with("text/event-stream")
        });

    if is_event_stream {
        let mut headers = HashMap::new();
        for (key, value) in final_hop.headers.iter() {
            if let Ok(val_str) = value.to_str() {
                headers.insert(key.to_string(), val_str.to_string());
            }
        }
        return Ok(SendOutcome::EventStream {
            status: final_hop.status,
            headers,
            body: EventStreamBody(final_hop.body),
        });
    }

    let download_start = Instant::now();
    let body_bytes = tokio::select! {
        result = final_hop.body.collect() => {
            result.map_err(|e| format!("Payload Parsing Error: {}", e))?.to_bytes()
        }
        _ = cancel_token.cancelled() => return Err("Request cancelled by user.".to_string()),
    };

    let mut timings = final_hop.timings;
    timings.download = download_start.elapsed();

    let outcome = timed_client::ExecOutcome {
        status: final_hop.status,
        headers: final_hop.headers,
        body: body_bytes,
        timings,
        request_size: final_hop.request_size,
    };

    let process_start = Instant::now();
    Ok(SendOutcome::Complete(shape_response(
        outcome,
        prepared.prepare,
        process_start,
    )))
}

#[cfg(test)]
mod prepare_tests {
    use super::*;
    use crate::common::FormDataType;

    #[tokio::test]
    async fn multipart_content_type_replaces_a_user_set_one() {
        let spec = RequestSpec::new("http://localhost/upload", HttpMethod::POST)
            .body_type(BodyType::FormData)
            .form_data(vec![FormDataRow::new("data", "{}", FormDataType::Text)])
            .headers(vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("Accept".to_string(), "*/*".to_string()),
            ]);

        let prepared = prepare(spec).await.unwrap();
        let content_types: Vec<&str> = prepared
            .headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str())
            .collect();

        assert_eq!(content_types.len(), 1);
        assert!(content_types[0].starts_with("multipart/form-data; boundary="));
        assert!(prepared.headers.iter().any(|(k, _)| k == "Accept"));
    }
}
