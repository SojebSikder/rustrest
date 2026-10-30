use rustls::DigitallySignedStruct;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, client::TlsStream};

static VERIFY_CERTIFICATES: AtomicBool = AtomicBool::new(true);

/// Turns server certificate verification on or off for every user request
/// (HTTP and WebSocket) sent from now on.
pub fn set_ssl_verification(enabled: bool) {
    VERIFY_CERTIFICATES.store(enabled, Ordering::Relaxed);
}

/// for TLS setup, always verifying the server certificate
pub fn client_config() -> Arc<rustls::ClientConfig> {
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

/// TLS setup for user requests: like `client_config`, unless SSL
/// verification is turned off in settings, in which case any certificate
/// (self-signed, expired, wrong host) is accepted.
pub fn request_client_config() -> Arc<rustls::ClientConfig> {
    if VERIFY_CERTIFICATES.load(Ordering::Relaxed) {
        return client_config();
    }
    static INSECURE: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    INSECURE
        .get_or_init(|| {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = rustls::ClientConfig::builder_with_provider(provider.clone())
                .with_safe_default_protocol_versions()
                .expect("ring provider supports rustls' default TLS protocol versions")
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate(provider)))
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

/// Skips certificate chain/hostname checks, but still checks the handshake
/// signatures so the connection itself is sound.
#[derive(Debug)]
struct AcceptAnyCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Performs a TLS handshake over an already-connected TCP stream
pub(crate) async fn connect(host: &str, tcp: TcpStream) -> Result<TlsStream<TcpStream>, String> {
    let connector = TlsConnector::from(request_client_config());
    let server_name = ServerName::try_from(host.to_string())
        .map_err(|_| format!("Invalid TLS server name: '{}'", host))?;

    connector.connect(server_name, tcp).await.map_err(|e| {
        let hint = if e.to_string().contains("certificate") {
            " (turn off SSL certificate verification in Settings > General to allow it)"
        } else {
            ""
        };
        format!("SSL Handshake Error: {}{}", e, hint)
    })
}
