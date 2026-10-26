//! SOCKS4/4a/5/5h client handshakes, turning a TCP stream connected to a
//! SOCKS proxy into a raw tunnel to the target.

use std::net::{IpAddr, Ipv4Addr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn io_err(e: std::io::Error) -> String {
    format!("SOCKS Proxy Error: {}", e)
}

/// How the target address is handed to the proxy.
enum Target {
    Ip(IpAddr),
    /// resolved by the proxy (SOCKS4a / SOCKS5h)
    Domain(String),
}

/// An IP literal is always sent as-is. otherwise the name goes to the proxy
/// when `remote_dns`, else it's looked up here first.
async fn target(host: &str, port: u16, remote_dns: bool, v4_only: bool) -> Result<Target, String> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(Target::Ip(ip));
    }
    if remote_dns {
        return Ok(Target::Domain(host.to_string()));
    }
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("DNS Lookup Error: {}", e))?;

    addrs
        .map(|a| a.ip())
        .find(|ip| !v4_only || ip.is_ipv4())
        .map(Target::Ip)
        .ok_or_else(|| {
            let kind = if v4_only { "IPv4 " } else { "" };
            format!(
                "DNS Lookup Error: no {}addresses found for host '{}'",
                kind, host
            )
        })
}

/// SOCKS5, with username/password auth when `credentials` is set.
pub(crate) async fn socks5(
    stream: &mut TcpStream,
    host: &str,
    port: u16,
    remote_dns: bool,
    credentials: Option<(&str, &str)>,
) -> Result<(), String> {
    let target = target(host, port, remote_dns, false).await?;

    let greeting: &[u8] = match credentials {
        Some(_) => &[5, 2, 0x00, 0x02],
        None => &[5, 1, 0x00],
    };
    stream.write_all(greeting).await.map_err(io_err)?;
    let mut choice = [0u8; 2];
    stream.read_exact(&mut choice).await.map_err(io_err)?;
    if choice[0] != 5 {
        return Err("SOCKS Proxy Error: server is not a SOCKS5 proxy".to_string());
    }

    match (choice[1], credentials) {
        (0x00, _) => {} // no auth required
        (0x02, Some((user, pass))) => {
            // username/password auth
            if user.len() > 255 || pass.len() > 255 {
                return Err("SOCKS Proxy Error: username/password over 255 bytes".to_string());
            }
            let mut auth = vec![1, user.len() as u8];
            auth.extend_from_slice(user.as_bytes());
            auth.push(pass.len() as u8);
            auth.extend_from_slice(pass.as_bytes());
            stream.write_all(&auth).await.map_err(io_err)?;

            let mut status = [0u8; 2];
            stream.read_exact(&mut status).await.map_err(io_err)?;
            if status[1] != 0 {
                return Err(
                    "SOCKS Proxy Authentication Failed (check the proxy username/password)"
                        .to_string(),
                );
            }
        }
        (0xFF, None) | (0x02, None) => {
            // auth required
            return Err("SOCKS Proxy Authentication Required (enable proxy auth)".to_string());
        }
        _ => {
            return Err("SOCKS Proxy Error: no acceptable authentication method".to_string());
        }
    }

    let mut req = vec![5, 1, 0];
    match &target {
        Target::Ip(IpAddr::V4(ip)) => {
            req.push(1);
            req.extend_from_slice(&ip.octets());
        }
        Target::Ip(IpAddr::V6(ip)) => {
            req.push(4);
            req.extend_from_slice(&ip.octets());
        }
        Target::Domain(name) => {
            if name.len() > 255 {
                return Err("SOCKS Proxy Error: host name over 255 bytes".to_string());
            }
            req.push(3);
            req.push(name.len() as u8);
            req.extend_from_slice(name.as_bytes());
        }
    }
    req.extend_from_slice(&port.to_be_bytes());
    stream.write_all(&req).await.map_err(io_err)?;

    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await.map_err(io_err)?;
    if head[1] != 0 {
        let reason = match head[1] {
            1 => "general failure",
            2 => "connection not allowed by ruleset",
            3 => "network unreachable",
            4 => "host unreachable",
            5 => "connection refused",
            6 => "TTL expired",
            7 => "command not supported",
            8 => "address type not supported",
            _ => "unknown error",
        };
        return Err(format!("SOCKS Proxy CONNECT failed: {}", reason));
    }
    // skip the bound address the reply carries
    let addr_len = match head[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await.map_err(io_err)?;
            len[0] as usize
        }
        _ => return Err("SOCKS Proxy Error: malformed reply".to_string()),
    };
    let mut rest = vec![0u8; addr_len + 2];
    stream.read_exact(&mut rest).await.map_err(io_err)?;
    Ok(())
}

/// SOCKS4, or SOCKS4a when `remote_dns`; `user_id` is the username, if any
/// (SOCKS4 has no passwords).
pub(crate) async fn socks4(
    stream: &mut TcpStream,
    host: &str,
    port: u16,
    remote_dns: bool,
    user_id: &str,
) -> Result<(), String> {
    let target = target(host, port, remote_dns, true).await?;

    let mut req = vec![4, 1];
    req.extend_from_slice(&port.to_be_bytes());
    match &target {
        Target::Ip(IpAddr::V4(ip)) => req.extend_from_slice(&ip.octets()),
        Target::Ip(IpAddr::V6(_)) => {
            return Err("SOCKS4 Proxy Error: IPv6 targets need SOCKS5".to_string());
        }
        // 0.0.0.x (x != 0) tells a SOCKS4a server a host name follows
        Target::Domain(_) => req.extend_from_slice(&Ipv4Addr::new(0, 0, 0, 1).octets()),
    }

    req.extend_from_slice(user_id.as_bytes());
    req.push(0);
    if let Target::Domain(name) = &target {
        req.extend_from_slice(name.as_bytes());
        req.push(0);
    }
    stream.write_all(&req).await.map_err(io_err)?;

    let mut reply = [0u8; 8];
    stream.read_exact(&mut reply).await.map_err(io_err)?;

    match reply[1] {
        0x5A => Ok(()), // CONNECT succeeded
        0x5B => Err("SOCKS4 Proxy CONNECT failed: request rejected".to_string()), // CONNECT request was rejected
        0x5C | 0x5D => Err("SOCKS4 Proxy CONNECT failed: identd check failed".to_string()), // identd check failed
        other => Err(format!(
            "SOCKS4 Proxy Error: unexpected reply code {:#x}",
            other
        )),
    }
}
