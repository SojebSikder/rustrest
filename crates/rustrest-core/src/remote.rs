use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SshAuthMethod {
    Password,
    PrivateKey { path: PathBuf },
    Agent,
}

/// a saved SSH connection profile for remote development, persisted
/// alongside a workspace's environments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshProfile {
    pub id: usize,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
}
