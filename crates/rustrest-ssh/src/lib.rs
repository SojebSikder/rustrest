//! SSH transport: connect, authenticate, and open shell/exec channels.

mod channel;
mod client;
mod config;
mod error;

pub use channel::{ExecChannel, ShellChannel};
pub use client::SshSession;
pub use config::{AuthMethod, SshConfig};
pub use error::SshError;
