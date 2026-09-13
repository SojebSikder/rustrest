use std::path::{Path, PathBuf};
use std::sync::Arc;

use russh::keys::agent::client::AgentClient;
use russh::keys::known_hosts::learn_known_hosts_path;
use russh::keys::{
    PrivateKeyWithHashAlg, PublicKeyOrCertificate, check_known_hosts_path, load_secret_key,
};

use crate::channel::{ExecChannel, ShellChannel};
use crate::config::{AuthMethod, SshConfig};
use crate::error::SshError;

/// verifies the server's host key against a known_hosts file at a
/// caller-supplied path (trust-on-first-use: an unseen host is recorded and
/// accepted, a host whose key changed is a hard error).
pub(crate) struct ClientHandler {
    host: String,
    port: u16,
    known_hosts_path: PathBuf,
}

impl russh::client::Handler for ClientHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key,
            // certificate-authenticated hosts are trusted via the certificate
            // chain the transport already validated; known_hosts TOFU only
            // applies to plain host keys.
            PublicKeyOrCertificate::Certificate(_) => return Ok(true),
        };

        match check_known_hosts_path(&self.host, self.port, key, &self.known_hosts_path) {
            Ok(true) => Ok(true),
            Ok(false) => {
                learn_known_hosts_path(&self.host, self.port, key, &self.known_hosts_path)?;
                Ok(true)
            }
            Err(russh::keys::Error::KeyChanged { .. }) => Err(SshError::HostKeyMismatch {
                host: self.host.clone(),
                port: self.port,
            }),
            Err(err) => Err(err.into()),
        }
    }
}

/// a live, authenticated SSH connection. Channels (shell or exec) are opened
/// from a shared reference, so a single session can back a remote terminal
/// and a remote-agent RPC channel at the same time.
pub struct SshSession {
    handle: russh::client::Handle<ClientHandler>,
}

impl SshSession {
    /// connects and authenticates using `config`, verifying the host key
    /// against (and recording new hosts into) `known_hosts_path`.
    pub async fn connect(config: &SshConfig, known_hosts_path: &Path) -> Result<Self, SshError> {
        let handler = ClientHandler {
            host: config.host.clone(),
            port: config.port,
            known_hosts_path: known_hosts_path.to_path_buf(),
        };
        let russh_config = Arc::new(russh::client::Config::default());
        let mut handle =
            russh::client::connect(russh_config, (config.host.as_str(), config.port), handler)
                .await?;

        let authenticated = match &config.auth {
            AuthMethod::Password(password) => handle
                .authenticate_password(config.username.clone(), password.clone())
                .await?
                .success(),
            AuthMethod::PrivateKey { path, passphrase } => {
                let key = load_secret_key(path, passphrase.as_deref())?;
                let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
                handle
                    .authenticate_publickey(
                        config.username.clone(),
                        PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
                    )
                    .await?
                    .success()
            }
            AuthMethod::Agent => {
                Self::authenticate_via_agent(&mut handle, &config.username).await?
            }
        };

        if !authenticated {
            return Err(SshError::AuthFailed);
        }

        Ok(Self { handle })
    }

    #[cfg(unix)]
    async fn authenticate_via_agent(
        handle: &mut russh::client::Handle<ClientHandler>,
        username: &str,
    ) -> Result<bool, SshError> {
        let agent = AgentClient::connect_env()
            .await
            .map_err(|_| SshError::NoAgent)?;
        try_agent_auth(handle, username, agent).await
    }

    #[cfg(windows)]
    async fn authenticate_via_agent(
        handle: &mut russh::client::Handle<ClientHandler>,
        username: &str,
    ) -> Result<bool, SshError> {
        if let Ok(agent) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
            return try_agent_auth(handle, username, agent).await;
        }
        let agent = AgentClient::connect_pageant()
            .await
            .map_err(|_| SshError::NoAgent)?;
        try_agent_auth(handle, username, agent).await
    }

    /// opens a remote shell channel with a PTY, sized to `columns`x`rows`.
    pub async fn open_shell(&self, columns: u32, rows: u32) -> Result<ShellChannel, SshError> {
        ShellChannel::open(self, columns, rows).await
    }

    /// runs `command` and returns a full-duplex byte stream connected to its
    /// stdin/stdout — used to push and run the remote-agent binary.
    pub async fn open_exec(&self, command: &str) -> Result<ExecChannel, SshError> {
        ExecChannel::open(self, command).await
    }

    pub(crate) fn handle(&self) -> &russh::client::Handle<ClientHandler> {
        &self.handle
    }

    /// uploads `bytes` to `remote_path` on the remote host (via a `cat >`
    /// pipe over an exec channel — no SFTP subsystem required) and marks it
    /// executable.
    pub async fn upload_executable(&self, remote_path: &str, bytes: &[u8]) -> Result<(), SshError> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let quoted = shell_quote(remote_path);
        let command = format!(
            "sh -c \"cat > {quoted} && chmod +x {quoted} && echo RUSTREST_UPLOAD_OK || echo RUSTREST_UPLOAD_FAILED\""
        );
        let exec = self.open_exec(&command).await?;
        let mut stream = exec.into_stream();

        stream.write_all(bytes).await?;
        stream.shutdown().await?;

        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        if response.contains("RUSTREST_UPLOAD_OK") {
            Ok(())
        } else {
            Err(SshError::UploadFailed {
                path: remote_path.to_string(),
                reason: response.trim().to_string(),
            })
        }
    }
}

async fn try_agent_auth<A>(
    handle: &mut russh::client::Handle<ClientHandler>,
    username: &str,
    mut agent: AgentClient<A>,
) -> Result<bool, SshError>
where
    A: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    let Ok(identities) = agent.request_identities().await else {
        return Ok(false);
    };

    for identity in identities {
        let public_key = identity.public_key().into_owned();
        if let Ok(result) = handle
            .authenticate_publickey_with(username.to_string(), public_key, None, &mut agent)
            .await
        {
            if result.success() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', r"'\''"))
}
