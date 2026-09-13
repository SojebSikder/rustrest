#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error(transparent)]
    Ssh(#[from] rustrest_ssh::SshError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("remote agent reported an error: {0}")]
    Remote(String),
    #[error("the remote agent connection is no longer available")]
    Disconnected,
}
