use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use rustrest_remote_protocol::{Request, Response};
use rustrest_ssh::{RemotePlatform, SshConfig, SshSession};
use rustrest_terminal::RemoteCommand;

use crate::error::RemoteError;
use crate::rpc::RpcClient;

/// A connected SSH host with the remote-agent RPC channel already wired up.
pub struct RemoteSession {
    ssh: Arc<SshSession>,
    rpc: RpcClient,
}

impl std::fmt::Debug for RemoteSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteSession").finish_non_exhaustive()
    }
}

impl RemoteSession {
    /// connects over SSH, detects the remote host's OS/architecture, and
    /// makes sure a matching `rustrest-remote-agent` binary tagged
    /// `app_version` is present on the remote (skipping upload entirely if
    /// it's already there from a previous connection) before running it to
    /// establish the RPC channel used by the file operations below.
    ///
    /// `resolve_agent_binary` is only called on a cache miss, with the detected
    /// platform, and must return the bytes of an agent binary built for it.
    pub async fn connect<F, Fut>(
        config: &SshConfig,
        known_hosts_path: &Path,
        app_version: &str,
        resolve_agent_binary: F,
    ) -> Result<Self, RemoteError>
    where
        F: FnOnce(RemotePlatform) -> Fut,
        Fut: Future<Output = Result<Vec<u8>, RemoteError>>,
    {
        let ssh = SshSession::connect(config, known_hosts_path).await?;
        let platform = rustrest_ssh::detect_remote_platform(&ssh).await?;
        let (remote_dir, remote_path) = platform.agent_paths(app_version)?;

        if !ssh.remote_file_exists(&remote_path, platform.os).await? {
            let agent_binary = resolve_agent_binary(platform.clone()).await?;
            ssh.ensure_remote_dir(&remote_dir, platform.os).await?;
            ssh.upload_executable(&remote_path, &agent_binary, platform.os)
                .await?;
        }

        let exec = ssh.open_exec(&remote_path).await?;
        let rpc = RpcClient::spawn(exec.into_stream());

        Ok(Self {
            ssh: Arc::new(ssh),
            rpc,
        })
    }

    pub async fn list_dir(
        &self,
        path: &str,
    ) -> Result<Vec<rustrest_remote_protocol::RemoteEntry>, RemoteError> {
        match self.rpc.call(Request::ListDir(path.to_string())).await? {
            Response::DirListing(entries) => Ok(entries),
            Response::Error(message) => Err(RemoteError::Remote(message)),
            _ => Err(RemoteError::Remote(
                "unexpected response to ListDir".to_string(),
            )),
        }
    }

    pub async fn read_file(&self, path: &str) -> Result<Vec<u8>, RemoteError> {
        match self.rpc.call(Request::ReadFile(path.to_string())).await? {
            Response::FileContent(bytes) => Ok(bytes),
            Response::Error(message) => Err(RemoteError::Remote(message)),
            _ => Err(RemoteError::Remote(
                "unexpected response to ReadFile".to_string(),
            )),
        }
    }

    pub async fn write_file(&self, path: &str, contents: Vec<u8>) -> Result<(), RemoteError> {
        self.expect_ok(Request::WriteFile(path.to_string(), contents))
            .await
    }

    pub async fn create_dir(&self, path: &str) -> Result<(), RemoteError> {
        self.expect_ok(Request::CreateDir(path.to_string())).await
    }

    pub async fn delete(&self, path: &str) -> Result<(), RemoteError> {
        self.expect_ok(Request::Delete(path.to_string())).await
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), RemoteError> {
        self.expect_ok(Request::Rename(from.to_string(), to.to_string()))
            .await
    }

    /// runs `git <args>` with its working directory set to `cwd` on the
    /// remote host.
    pub async fn run_git(
        &self,
        cwd: &str,
        args: &[&str],
    ) -> Result<rustrest_remote_protocol::GitCommandOutput, RemoteError> {
        let request = Request::RunGit {
            cwd: cwd.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        };
        match self.rpc.call(request).await? {
            Response::GitOutput(output) => Ok(output),
            Response::Error(message) => Err(RemoteError::Remote(message)),
            _ => Err(RemoteError::Remote(
                "unexpected response to RunGit".to_string(),
            )),
        }
    }

    async fn expect_ok(&self, request: Request) -> Result<(), RemoteError> {
        match self.rpc.call(request).await? {
            Response::Ok => Ok(()),
            Response::Error(message) => Err(RemoteError::Remote(message)),
            _ => Err(RemoteError::Remote("unexpected response".to_string())),
        }
    }

    /// opens a plain remote shell
    pub async fn open_shell(
        &self,
        columns: u32,
        rows: u32,
    ) -> Result<rustrest_ssh::ShellChannel, RemoteError> {
        Ok(self.ssh.open_shell(columns, rows).await?)
    }
}

/// wires an already-open remote shell into the pieces returned by
/// `TerminalManager::spawn_remote`, spawning the background task that feeds
/// shell output into the grid and forwards writes/resizes back to the shell.
/// Runs until the shell exits or the terminal session is closed (which drops
/// `commands`).
pub fn bridge_shell_to_terminal(
    mut shell: rustrest_ssh::ShellChannel,
    mut feed: rustrest_terminal::RemoteTerminalFeed,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<RemoteCommand>,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                data = shell.read() => {
                    match data {
                        Some(bytes) => feed.feed(&bytes),
                        None => break,
                    }
                }
                command = commands.recv() => {
                    match command {
                        Some(RemoteCommand::Write(bytes)) => {
                            let _ = shell.write(&bytes).await;
                        }
                        Some(RemoteCommand::Resize(columns, rows)) => {
                            let _ = shell.resize(columns as u32, rows as u32).await;
                        }
                        None => break,
                    }
                }
            }
        }
        let _ = shell.close().await;
    });
}
