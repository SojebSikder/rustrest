use std::path::PathBuf;

/// authenticate to the remote host.
#[derive(Clone, Debug)]
pub enum AuthMethod {
    Password(String),
    PrivateKey {
        path: PathBuf,
        passphrase: Option<String>,
    },
    /// authenticate via a running SSH agent (ssh-agent on Unix, the Windows
    /// OpenSSH agent service, or Pageant).
    Agent,
}

#[derive(Clone, Debug)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
}
