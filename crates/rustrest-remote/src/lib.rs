pub mod collection_sync;
mod error;
mod rpc;
mod session;

pub use error::RemoteError;
pub use rpc::RpcClient;
pub use rustrest_remote_protocol::{GitCommandOutput, RemoteEntry, Request, Response};
pub use rustrest_ssh::{
    AuthMethod, RemoteArch, RemoteOs, RemotePlatform, ShellChannel, SshConfig, SshError,
};
pub use rustrest_terminal::{RemoteCommand, RemoteTerminalFeed};
pub use session::{RemoteSession, bridge_shell_to_terminal};
