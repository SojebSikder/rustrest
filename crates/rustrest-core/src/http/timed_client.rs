use super::PhaseTimings;
use bytes::Bytes;
use http::{HeaderMap, Method, Request};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use url::Url;

const MAX_REDIRECTS: u8 = 10;

pub(crate) struct ExecOutcome {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub timings: PhaseTimings,
    pub request_size: u64,
}

/// The result of following every redirect hop, with the final hop's
/// response headers resolved but its body left unread - the caller decides
/// whether to buffer it whole (a normal response) or stream it chunk by
/// chunk (an event-stream response), without redoing any of the connection
/// work above.
pub(crate) struct FinalHop {
    pub status: u16,
    pub headers: HeaderMap,
    /// every phase except `download`/`process`, which depend on what the
    /// caller does with `body`.
    pub timings: PhaseTimings,
    pub request_size: u64,
    pub body: Incoming,
}

enum MaybeTlsStream {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl AsyncRead for MaybeTlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            MaybeTlsStream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybeTlsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            MaybeTlsStream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_flush(cx),
            MaybeTlsStream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            MaybeTlsStream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}

/// One hop's response, with headers resolved but the body left unread.
struct HopHead {
    status: u16,
    headers: HeaderMap,
    location: Option<String>,
    socket_init: Duration,
    dns: Duration,
    tcp: Duration,
    tls: Duration,
    ttfb: Duration,
    request_bytes: u64,
    body: Incoming,
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
}

async fn execute_hop_head(
    url: &Url,
    method: &Method,
    headers: &[(String, String)],
    body: Bytes,
) -> Result<HopHead, String> {
    let hop_start = Instant::now();
    let scheme_https = url.scheme() == "https";
    let host = url
        .host_str()
        .ok_or_else(|| "Invalid URL pattern: missing host".to_string())?
        .to_string();
    let port = url
        .port_or_known_default()
        .unwrap_or(if scheme_https { 443 } else { 80 });
    let socket_init = hop_start.elapsed();

    let dns_start = Instant::now();
    let mut addrs = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|e| format!("DNS Lookup Error: {}", e))?;
    let addr = addrs
        .next()
        .ok_or_else(|| format!("DNS Lookup Error: no addresses found for host '{}'", host))?;
    let dns = dns_start.elapsed();

    let tcp_start = Instant::now();
    let tcp_stream = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("TCP Handshake Error: {}", e))?;
    let _ = tcp_stream.set_nodelay(true);
    let tcp = tcp_start.elapsed();

    let (stream, tls) = if scheme_https {
        let tls_start = Instant::now();
        let tls_stream = super::tls::connect(&host, tcp_stream).await?;
        (
            MaybeTlsStream::Tls(Box::new(tls_stream)),
            tls_start.elapsed(),
        )
    } else {
        (MaybeTlsStream::Plain(tcp_stream), Duration::ZERO)
    };

    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .map_err(|e| format!("HTTP Handshake Error: {}", e))?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let path_and_query = match url.query() {
        Some(q) => format!("{}?{}", url.path(), q),
        None => url.path().to_string(),
    };

    let host_header_value = if (scheme_https && port == 443) || (!scheme_https && port == 80) {
        host.clone()
    } else {
        format!("{}:{}", host, port)
    };

    let mut sent_headers: Vec<(String, String)> =
        vec![("Host".to_string(), host_header_value.clone())];
    let mut req_builder = Request::builder()
        .method(method.clone())
        .uri(path_and_query.clone())
        .header("Host", host_header_value);

    for (key, val) in headers {
        let key = key.trim();
        if key.is_empty() || key.eq_ignore_ascii_case("host") {
            continue;
        }
        req_builder = req_builder.header(key, val.as_str());
        sent_headers.push((key.to_string(), val.clone()));
    }

    if !has_header(headers, "content-length") && !body.is_empty() {
        let content_length = body.len().to_string();
        req_builder = req_builder.header("Content-Length", content_length.clone());
        sent_headers.push(("Content-Length".to_string(), content_length));
    }

    req_builder = req_builder.header("Connection", "close");
    sent_headers.push(("Connection".to_string(), "close".to_string()));

    let request_line_len = format!("{} {} HTTP/1.1\r\n", method, path_and_query).len();
    let headers_len: usize = sent_headers
        .iter()
        .map(|(k, v)| k.len() + 2 + v.len() + 2)
        .sum();
    let request_bytes = (request_line_len + headers_len + 2 + body.len()) as u64;

    let req = req_builder
        .body(Full::new(body))
        .map_err(|e| format!("Failed to build request: {}", e))?;

    let ttfb_start = Instant::now();
    let response = sender
        .send_request(req)
        .await
        .map_err(|e| format!("Network Dispatch Error: {}", e))?;
    let ttfb = ttfb_start.elapsed();

    let status = response.status().as_u16();
    let resp_headers = response.headers().clone();
    let location = resp_headers
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    Ok(HopHead {
        status,
        headers: resp_headers,
        location,
        socket_init,
        dns,
        tcp,
        tls,
        ttfb,
        request_bytes,
        body: response.into_body(),
    })
}

fn is_redirect_status(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Follows redirects until a non-redirect response comes back, leaving its body unread.
async fn run_hops(
    method: Method,
    start_url: Url,
    mut headers: Vec<(String, String)>,
    mut body: Bytes,
    cancel_token: &CancellationToken,
) -> Result<FinalHop, String> {
    let mut url = start_url;
    let mut current_method = method;
    let mut timings = PhaseTimings::default();
    let mut redirects = 0u8;

    loop {
        let hop = tokio::select! {
            res = execute_hop_head(&url, &current_method, &headers, body.clone()) => res?,
            _ = cancel_token.cancelled() => return Err("Request cancelled by user.".to_string()),
        };

        timings.socket_initialization += hop.socket_init;
        timings.dns_lookup += hop.dns;
        timings.tcp_handshake += hop.tcp;
        timings.ssl_handshake += hop.tls;
        timings.waiting += hop.ttfb;

        if is_redirect_status(hop.status) {
            if let Some(location) = hop.location {
                if redirects >= MAX_REDIRECTS {
                    return Err("Too many redirects".to_string());
                }
                redirects += 1;

                // this hop's body is a redirect page we're discarding; drop
                // it without reading (the connection is per-hop here)
                drop(hop.body);

                url = url
                    .join(&location)
                    .map_err(|e| format!("Invalid redirect location: {}", e))?;

                let downgrade_to_get = match hop.status {
                    303 => current_method != Method::HEAD,
                    301 | 302 => current_method == Method::POST,
                    _ => false,
                };
                if downgrade_to_get {
                    current_method = Method::GET;
                    body = Bytes::new();
                    headers.retain(|(k, _)| {
                        !k.eq_ignore_ascii_case("content-type")
                            && !k.eq_ignore_ascii_case("content-length")
                    });
                }
                continue;
            }
        }

        return Ok(FinalHop {
            status: hop.status,
            headers: hop.headers,
            timings,
            request_size: hop.request_bytes,
            body: hop.body,
        });
    }
}

pub(crate) async fn execute_request(
    method: Method,
    start_url: Url,
    headers: Vec<(String, String)>,
    body: Bytes,
    cancel_token: &CancellationToken,
) -> Result<ExecOutcome, String> {
    let final_hop = run_hops(method, start_url, headers, body, cancel_token).await?;

    let download_start = Instant::now();
    let body_bytes = tokio::select! {
        result = final_hop.body.collect() => {
            result.map_err(|e| format!("Payload Parsing Error: {}", e))?.to_bytes()
        }
        _ = cancel_token.cancelled() => return Err("Request cancelled by user.".to_string()),
    };

    let mut timings = final_hop.timings;
    timings.download = download_start.elapsed();

    Ok(ExecOutcome {
        status: final_hop.status,
        headers: final_hop.headers,
        body: body_bytes,
        timings,
        request_size: final_hop.request_size,
    })
}

/// Like [`execute_request`], but stops short of reading the final body -
/// the caller (which knows whether the response is an event stream from its
/// `Content-Type`) decides whether to buffer it or stream it.
pub(crate) async fn execute_request_head(
    method: Method,
    start_url: Url,
    headers: Vec<(String, String)>,
    body: Bytes,
    cancel_token: &CancellationToken,
) -> Result<FinalHop, String> {
    run_hops(method, start_url, headers, body, cancel_token).await
}
