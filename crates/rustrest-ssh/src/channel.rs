use russh::ChannelMsg;

use crate::client::SshSession;
use crate::error::SshError;

/// a remote shell with a PTY attached - the transport a remote terminal
/// session reads from and writes to.
pub struct ShellChannel {
    channel: russh::Channel<russh::client::Msg>,
}

impl std::fmt::Debug for ShellChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellChannel").finish_non_exhaustive()
    }
}

impl ShellChannel {
    pub(crate) async fn open(
        session: &SshSession,
        columns: u32,
        rows: u32,
    ) -> Result<Self, SshError> {
        let channel = session.handle().channel_open_session().await?;
        channel
            .request_pty(false, "xterm-256color", columns, rows, 0, 0, &[])
            .await?;
        channel.request_shell(true).await?;
        Ok(Self { channel })
    }

    pub async fn write(&self, bytes: &[u8]) -> Result<(), SshError> {
        self.channel.data_bytes(bytes.to_vec()).await?;
        Ok(())
    }

    pub async fn resize(&self, columns: u32, rows: u32) -> Result<(), SshError> {
        self.channel.window_change(columns, rows, 0, 0).await?;
        Ok(())
    }

    /// waits for the next chunk of shell output, or `None` once the shell
    /// has exited / the channel closed.
    pub async fn read(&mut self) -> Option<Vec<u8>> {
        loop {
            match self.channel.wait().await? {
                ChannelMsg::Data { data } => return Some(data.to_vec()),
                ChannelMsg::Close | ChannelMsg::Eof => return None,
                _ => continue,
            }
        }
    }

    pub async fn close(&self) -> Result<(), SshError> {
        self.channel.eof().await?;
        self.channel.close().await?;
        Ok(())
    }
}

/// a one-shot command execution channel, exposed as a full-duplex byte
/// stream. Used both to push the remote-agent binary onto the host (piping
/// bytes into a `cat >` command) and to run it (framing an RPC protocol over
/// its stdin/stdout).
pub struct ExecChannel {
    channel: russh::Channel<russh::client::Msg>,
}

impl ExecChannel {
    pub(crate) async fn open(session: &SshSession, command: &str) -> Result<Self, SshError> {
        let channel = session.handle().channel_open_session().await?;
        channel.exec(true, command).await?;
        Ok(Self { channel })
    }

    /// consumes this channel, returning a full-duplex `AsyncRead + AsyncWrite`
    /// stream connected to the remote command's stdin/stdout.
    pub fn into_stream(
        self,
    ) -> impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static {
        self.channel.into_stream()
    }
}
