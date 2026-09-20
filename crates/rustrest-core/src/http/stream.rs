//! stream utilities for HTTP requests. It's received data chunk by chunk as
//! it arrives off the socket.

use bytes::Bytes;
use http::{HeaderMap, Method, Request};
use http_body_util::{BodyExt, Empty};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use url::Url;

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

/// A response whose body hasn't been read yet; call [`next_chunk`] in a loop
/// to pull bytes as they arrive.
pub struct StreamingResponse {
    pub status: u16,
    pub headers: HeaderMap,
    body: Incoming,
}

impl StreamingResponse {
    /// Pulls the next non-empty body chunk, or `None` once the server has
    /// closed the stream.
    pub async fn next_chunk(&mut self) -> Option<Result<Bytes, String>> {
        loop {
            match self.body.frame().await {
                Some(Ok(frame)) => match frame.into_data() {
                    Ok(data) if !data.is_empty() => return Some(Ok(data)),
                    Ok(_) => continue,  // empty data frame, keep polling
                    Err(_) => continue, // trailer frame, keep polling
                },
                Some(Err(e)) => return Some(Err(format!("Stream read error: {}", e))),
                None => return None,
            }
        }
    }
}

/// Opens a connection and sends `method` to `url`, returning as soon as
/// response headers are in
pub async fn open_stream(
    url: &Url,
    method: Method,
    headers: Vec<(String, String)>,
    cancel_token: &CancellationToken,
) -> Result<StreamingResponse, String> {
    let scheme_https = url.scheme() == "https";
    let host = url
        .host_str()
        .ok_or_else(|| "Invalid URL pattern: missing host".to_string())?
        .to_string();
    let port = url
        .port_or_known_default()
        .unwrap_or(if scheme_https { 443 } else { 80 });

    let connect_fut = async {
        let mut addrs = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| format!("DNS Lookup Error: {}", e))?;
        let addr = addrs
            .next()
            .ok_or_else(|| format!("DNS Lookup Error: no addresses found for host '{}'", host))?;
        let tcp_stream = TcpStream::connect(addr)
            .await
            .map_err(|e| format!("TCP Handshake Error: {}", e))?;
        let _ = tcp_stream.set_nodelay(true);

        let stream = if scheme_https {
            MaybeTlsStream::Tls(Box::new(super::tls::connect(&host, tcp_stream).await?))
        } else {
            MaybeTlsStream::Plain(tcp_stream)
        };
        Ok::<_, String>(stream)
    };

    let stream = tokio::select! {
        res = connect_fut => res?,
        _ = cancel_token.cancelled() => return Err("Request cancelled by user.".to_string()),
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

    let mut req_builder = Request::builder()
        .method(method)
        .uri(path_and_query)
        .header("Host", host_header_value);

    for (key, val) in &headers {
        let key = key.trim();
        if key.is_empty() || key.eq_ignore_ascii_case("host") {
            continue;
        }
        req_builder = req_builder.header(key, val.as_str());
    }

    let req = req_builder
        .body(Empty::<Bytes>::new())
        .map_err(|e| format!("Failed to build request: {}", e))?;

    let response = tokio::select! {
        res = sender.send_request(req) => res.map_err(|e| format!("Network Dispatch Error: {}", e))?,
        _ = cancel_token.cancelled() => return Err("Request cancelled by user.".to_string()),
    };

    let status = response.status().as_u16();
    let headers_out = response.headers().clone();
    let body = response.into_body();

    Ok(StreamingResponse {
        status,
        headers: headers_out,
        body,
    })
}
