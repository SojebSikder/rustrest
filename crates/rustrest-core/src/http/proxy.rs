//! Proxy support, configured app-wide

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::RwLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use url::Url;

/// The protocol spoken to the proxy itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyProtocol {
    #[default]
    Http,
    /// SOCKS5, target host names resolved locally
    Socks5,
    /// SOCKS5, target host names resolved by the proxy
    Socks5h,
    /// SOCKS4, target host names resolved locally (IPv4 only)
    Socks4,
    /// SOCKS4, target host names resolved by the proxy
    Socks4a,
}

impl ProxyProtocol {
    pub const ALL: [ProxyProtocol; 5] = [
        ProxyProtocol::Http,
        ProxyProtocol::Socks5,
        ProxyProtocol::Socks5h,
        ProxyProtocol::Socks4,
        ProxyProtocol::Socks4a,
    ];

    /// the URL scheme for this protocol (`socks5h`, ...), as in `ALL_PROXY`
    pub fn scheme(self) -> &'static str {
        match self {
            ProxyProtocol::Http => "http",
            ProxyProtocol::Socks5 => "socks5",
            ProxyProtocol::Socks5h => "socks5h",
            ProxyProtocol::Socks4 => "socks4",
            ProxyProtocol::Socks4a => "socks4a",
        }
    }

    fn from_scheme(scheme: &str) -> Option<Self> {
        match scheme.to_ascii_lowercase().as_str() {
            "socks" => Some(ProxyProtocol::Socks5),
            other => Self::ALL.into_iter().find(|p| p.scheme() == other),
        }
    }

    fn default_port(self) -> u16 {
        match self {
            ProxyProtocol::Http => 80,
            _ => 1080,
        }
    }
}

impl fmt::Display for ProxyProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ProxyProtocol::Http => "HTTP",
            ProxyProtocol::Socks5 => "SOCKS5",
            ProxyProtocol::Socks5h => "SOCKS5H",
            ProxyProtocol::Socks4 => "SOCKS4",
            ProxyProtocol::Socks4a => "SOCKS4A",
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// route requests through `host:port`
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub protocol: ProxyProtocol,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub auth_enabled: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    /// comma-separated hosts that skip the proxy: `localhost, *.internal`
    #[serde(default = "default_bypass")]
    pub bypass: String,
    /// when no custom proxy is enabled, fall back to `HTTP(S)_PROXY` /
    /// `NO_PROXY` from the environment
    #[serde(default)]
    pub use_system: bool,
}

fn default_port() -> u16 {
    8080
}

fn default_bypass() -> String {
    "localhost, 127.0.0.1".to_string()
}

static PROXY: RwLock<Option<ProxyConfig>> = RwLock::new(None);

/// Replaces the proxy used by every request sent from now on.
pub fn set_proxy_config(config: ProxyConfig) {
    if let Ok(mut slot) = PROXY.write() {
        *slot = Some(config);
    }
}

/// A proxy that applies to one particular target URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Proxy {
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
    /// username and password, when the proxy needs auth
    pub credentials: Option<(String, String)>,
}

impl Proxy {
    /// Whether a plain-http request is handed to the proxy as-is
    /// rather than tunnelled - true only for HTTP proxies.
    pub fn forwards_plain_http(&self) -> bool {
        self.protocol == ProxyProtocol::Http
    }

    /// `Proxy-Authorization` value for an HTTP proxy
    pub fn auth_header(&self) -> Option<String> {
        self.credentials
            .as_ref()
            .map(|(user, pass)| basic_auth(user, pass))
    }

    /// Turns `stream`, already connected to the proxy, into a raw tunnel to `host:port`.
    pub async fn open_tunnel(
        &self,
        stream: &mut TcpStream,
        host: &str,
        port: u16,
    ) -> Result<(), String> {
        let creds = self
            .credentials
            .as_ref()
            .map(|(u, p)| (u.as_str(), p.as_str()));

        match self.protocol {
            ProxyProtocol::Http => {
                connect_tunnel(stream, host, port, self.auth_header().as_deref()).await
            }
            ProxyProtocol::Socks5 => super::socks::socks5(stream, host, port, false, creds).await,
            ProxyProtocol::Socks5h => super::socks::socks5(stream, host, port, true, creds).await,
            ProxyProtocol::Socks4 | ProxyProtocol::Socks4a => {
                let user_id = creds.map(|(u, _)| u).unwrap_or("");
                let remote_dns = self.protocol == ProxyProtocol::Socks4a;
                super::socks::socks4(stream, host, port, remote_dns, user_id).await
            }
        }
    }
}

/// The proxy `url` should go through, if any.
pub(crate) fn for_url(url: &Url) -> Option<Proxy> {
    let config = PROXY.read().ok()?.clone()?;
    resolve(&config, url, |name| std::env::var(name).ok())
}

/// Opens a TCP connection to `url`'s host and port, through a proxy tunnel
/// (HTTP `CONNECT` or SOCKS) when a proxy applies.
pub async fn connect_tcp(url: &Url) -> Result<TcpStream, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "Invalid URL: missing host".to_string())?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| format!("Invalid URL: no port for scheme '{}'", url.scheme()))?;
    let proxy = for_url(url);
    let (dial_host, dial_port) = match &proxy {
        Some(p) => (p.host.as_str(), p.port),
        None => (host, port),
    };

    let mut stream =
        TcpStream::connect((dial_host, dial_port))
            .await
            .map_err(|e| match &proxy {
                Some(_) => format!(
                    "Proxy Connection Error ({}:{}): {}",
                    dial_host, dial_port, e
                ),
                None => format!("TCP Connection Error ({}:{}): {}", host, port, e),
            })?;
    let _ = stream.set_nodelay(true);
    if let Some(p) = &proxy {
        p.open_tunnel(&mut stream, host, port).await?;
    }
    Ok(stream)
}

/// Whether connections to `url` currently go through a proxy.
pub fn uses_proxy(url: &Url) -> bool {
    for_url(url).is_some()
}

fn resolve(config: &ProxyConfig, url: &Url, env: impl Fn(&str) -> Option<String>) -> Option<Proxy> {
    let target_host = url.host_str()?;

    if config.enabled && !config.host.trim().is_empty() {
        if is_bypassed(&config.bypass, target_host) {
            return None;
        }
        let credentials = (config.auth_enabled && !config.username.is_empty())
            .then(|| (config.username.clone(), config.password.clone()));
        return Some(Proxy {
            protocol: config.protocol,
            host: config.host.trim().to_string(),
            port: config.port,
            credentials,
        });
    }

    if !config.use_system {
        return None;
    }
    let env_any = |names: &[&str]| names.iter().find_map(|n| env(n).filter(|v| !v.is_empty()));
    let raw = if matches!(url.scheme(), "https" | "wss") {
        env_any(&["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"])
    } else {
        env_any(&["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"])
    }?;
    if let Some(no_proxy) = env_any(&["NO_PROXY", "no_proxy"]) {
        if is_bypassed(&no_proxy, target_host) {
            return None;
        }
    }
    parse_proxy_url(&raw)
}

/// Parses `scheme://user:pass@host:port`, where the scheme is `http` (the
/// default when omitted), `socks`/`socks5`, `socks5h`, `socks4` or `socks4a`.
fn parse_proxy_url(raw: &str) -> Option<Proxy> {
    let raw = raw.trim();
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("http://{}", raw)
    };
    let parsed = Url::parse(&with_scheme).ok()?;
    let protocol = ProxyProtocol::from_scheme(parsed.scheme())?;

    let username = percent_encoding::percent_decode_str(parsed.username())
        .decode_utf8_lossy()
        .into_owned();
    let password = parsed
        .password()
        .map(|p| {
            percent_encoding::percent_decode_str(p)
                .decode_utf8_lossy()
                .into_owned()
        })
        .unwrap_or_default();

    Some(Proxy {
        protocol,
        host: parsed.host_str()?.to_string(),
        port: parsed.port().unwrap_or(protocol.default_port()),
        credentials: (!username.is_empty()).then_some((username, password)),
    })
}

fn basic_auth(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username, password))
    )
}

/// `list` is comma-separated: `*` matches everything, `*.example.com` /
/// `.example.com` match the domain and its subdomains, anything else is an
/// exact host (a trailing `:port` is ignored).
fn is_bypassed(list: &str, host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    list.split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .any(|entry| {
            if entry == "*" {
                return true;
            }
            let entry = match entry.rsplit_once(':') {
                Some((h, port)) if !h.contains(':') && port.chars().all(|c| c.is_ascii_digit()) => {
                    h
                }
                _ => entry,
            };
            match entry.strip_prefix("*.").or_else(|| entry.strip_prefix('.')) {
                Some(suffix) => {
                    host.eq_ignore_ascii_case(suffix)
                        || host
                            .to_ascii_lowercase()
                            .ends_with(&format!(".{}", suffix.to_ascii_lowercase()))
                }
                None => host.eq_ignore_ascii_case(entry),
            }
        })
}

/// Sends `CONNECT host:port` over a stream already connected to the proxy
/// and waits for the 2xx that turns it into a raw tunnel to the target.
pub(crate) async fn connect_tunnel(
    stream: &mut TcpStream,
    host: &str,
    port: u16,
    auth: Option<&str>,
) -> Result<(), String> {
    // `host` comes from `Url::host_str`, so an IPv6 literal is already bracketed
    let authority = format!("{}:{}", host, port);
    let mut req = format!("CONNECT {0} HTTP/1.1\r\nHost: {0}\r\n", authority);
    if let Some(auth) = auth {
        req.push_str(&format!("Proxy-Authorization: {}\r\n", auth));
    }
    req.push_str("\r\n");
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| format!("Proxy Error: {}", e))?;

    // one byte at a time, so nothing past the header block (the start of
    // the tunnelled TLS stream) is consumed here
    let mut head = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        let n = stream
            .read(&mut byte)
            .await
            .map_err(|e| format!("Proxy Error: {}", e))?;
        if n == 0 {
            return Err("Proxy Error: connection closed during CONNECT".to_string());
        }
        if head.len() >= 16 * 1024 {
            return Err("Proxy Error: CONNECT response headers too large".to_string());
        }
        head.push(byte[0]);
    }

    let head = String::from_utf8_lossy(&head);
    let status_line = head.lines().next().unwrap_or_default().trim();
    match status_line.split_whitespace().nth(1) {
        Some(code) if code.starts_with('2') => Ok(()),
        Some("407") => Err(format!(
            "Proxy Authentication Required: {} (check the proxy username/password)",
            status_line
        )),
        _ => Err(format!("Proxy CONNECT failed: {}", status_line)),
    }
}
