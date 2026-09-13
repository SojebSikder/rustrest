#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error(transparent)]
    Ssh(#[from] russh::Error),
    #[error(transparent)]
    Keys(#[from] russh::keys::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("authentication failed for this host")]
    AuthFailed,
    #[error(
        "host key for {host}:{port} does not match the one on record — refusing to connect (possible man-in-the-middle)"
    )]
    HostKeyMismatch { host: String, port: u16 },
    #[error("no SSH agent is available on this system")]
    NoAgent,
    #[error("upload of {path} failed: {reason}")]
    UploadFailed { path: String, reason: String },
}
