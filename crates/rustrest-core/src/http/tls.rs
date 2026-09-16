use rustls::pki_types::ServerName;
use std::sync::{Arc, OnceLock};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, client::TlsStream};

fn client_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let root_store = rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .expect("ring provider supports rustls' default TLS protocol versions")
            .with_root_certificates(root_store)
            .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

/// Performs a TLS handshake over an already-connected TCP stream
pub(crate) async fn connect(host: &str, tcp: TcpStream) -> Result<TlsStream<TcpStream>, String> {
    let connector = TlsConnector::from(client_config());
    let server_name = ServerName::try_from(host.to_string())
        .map_err(|_| format!("Invalid TLS server name: '{}'", host))?;
    connector
        .connect(server_name, tcp)
        .await
        .map_err(|e| format!("SSL Handshake Error: {}", e))
}
