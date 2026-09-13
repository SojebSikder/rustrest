//! detecting the remote host's OS/CPU architecture so the caller can pick a
//! matching `rustrest-remote-agent` binary, probed over the SSH session itself

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::client::SshSession;
use crate::error::SshError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteOs {
    Linux,
    MacOs,
    Windows,
}

impl std::fmt::Display for RemoteOs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Windows => "windows",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteArch {
    X86_64,
    Aarch64,
}

impl std::fmt::Display for RemoteArch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
        })
    }
}

/// the remote host's OS/arch, and (for Windows, which has no `/tmp`) its
/// `%TEMP%` directory picked up as part of the same probe.
#[derive(Debug, Clone)]
pub struct RemotePlatform {
    pub os: RemoteOs,
    pub arch: RemoteArch,
    pub windows_temp_dir: Option<String>,
}

impl RemotePlatform {
    /// the cargo-dist target triple this app publishes `rustrest-remote-agent`
    /// release archives under for this platform, if any.
    pub fn target_triple(&self) -> Result<&'static str, SshError> {
        match (self.os, self.arch) {
            (RemoteOs::Linux, RemoteArch::X86_64) => Ok("x86_64-unknown-linux-gnu"),
            (RemoteOs::Linux, RemoteArch::Aarch64) => Ok("aarch64-unknown-linux-gnu"),
            (RemoteOs::MacOs, RemoteArch::X86_64) => Ok("x86_64-apple-darwin"),
            (RemoteOs::MacOs, RemoteArch::Aarch64) => Ok("aarch64-apple-darwin"),
            (RemoteOs::Windows, RemoteArch::X86_64) => Ok("x86_64-pc-windows-msvc"),
            (RemoteOs::Windows, RemoteArch::Aarch64) => Err(SshError::UnsupportedPlatform {
                os: self.os.to_string(),
                arch: self.arch.to_string(),
            }),
        }
    }

    /// the directory to cache versioned agent binaries under, and the full
    /// path of the binary for `app_version`, on this remote host.
    pub fn agent_paths(&self, app_version: &str) -> Result<(String, String), SshError> {
        let target = self.target_triple()?;
        match self.os {
            RemoteOs::Windows => {
                let temp = self
                    .windows_temp_dir
                    .as_deref()
                    .unwrap_or(r"C:\Windows\Temp")
                    .trim_end_matches('\\');
                let dir = format!(r"{temp}\.rustrest\agents\{app_version}");
                let path = format!(r"{dir}\rustrest-remote-agent-{target}.exe");
                Ok((dir, path))
            }
            RemoteOs::Linux | RemoteOs::MacOs => {
                let dir = format!("/tmp/.rustrest/agents/{app_version}");
                let path = format!("{dir}/rustrest-remote-agent-{target}");
                Ok((dir, path))
            }
        }
    }
}

/// probes the remote host over `session` to determine its OS/architecture:
/// tries `uname` first (covers Linux, macOS, and any other POSIX-ish
/// remote), then falls back to Windows `cmd.exe` environment variables if
/// `uname` isn't a recognized command, the default exec shell for Windows' OpenSSH server.
pub async fn detect_remote_platform(session: &SshSession) -> Result<RemotePlatform, SshError> {
    if let Some(platform) = probe_uname(session).await? {
        return Ok(platform);
    }
    probe_windows(session).await
}

/// runs a read-only probe `command` and returns everything it wrote to
/// stdout, there's nothing to send on stdin, so it's closed immediately.
async fn run_probe(session: &SshSession, command: &str) -> Result<String, SshError> {
    let exec = session.open_exec(command).await?;
    let mut stream = exec.into_stream();
    stream.shutdown().await?;
    let mut output = String::new();
    // a probe failing to parse (wrong shell, unexpected banner, etc.) isn't
    // fatal here - the caller falls back to the next probe.
    let _ = stream.read_to_string(&mut output).await;
    Ok(output)
}

async fn probe_uname(session: &SshSession) -> Result<Option<RemotePlatform>, SshError> {
    let output = run_probe(session, "uname -s && uname -m").await?;
    Ok(parse_uname_output(&output))
}

async fn probe_windows(session: &SshSession) -> Result<RemotePlatform, SshError> {
    let output = run_probe(session, "echo %PROCESSOR_ARCHITECTURE%& echo %TEMP%").await?;
    parse_windows_probe(&output)
}

fn parse_uname_output(output: &str) -> Option<RemotePlatform> {
    let mut lines = output.lines().map(str::trim);

    let os = match lines.next() {
        Some("Linux") => RemoteOs::Linux,
        Some("Darwin") => RemoteOs::MacOs,
        _ => return None,
    };
    let arch = match lines.next() {
        Some("x86_64") => RemoteArch::X86_64,
        Some("aarch64" | "arm64") => RemoteArch::Aarch64,
        _ => return None,
    };

    Some(RemotePlatform {
        os,
        arch,
        windows_temp_dir: None,
    })
}

fn parse_windows_probe(output: &str) -> Result<RemotePlatform, SshError> {
    let mut lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());

    let arch = match lines.next() {
        Some("AMD64") => RemoteArch::X86_64,
        Some("ARM64") => RemoteArch::Aarch64,
        Some(other) => {
            return Err(SshError::UnsupportedPlatform {
                os: "windows".to_string(),
                arch: other.to_string(),
            });
        }
        None => return Err(SshError::PlatformDetectionFailed),
    };
    let windows_temp_dir = lines.next().map(str::to_string);

    Ok(RemotePlatform {
        os: RemoteOs::Windows,
        arch,
        windows_temp_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_uname_output() {
        let platform = parse_uname_output("Linux\nx86_64\n").expect("should parse");
        assert_eq!(platform.os, RemoteOs::Linux);
        assert_eq!(platform.arch, RemoteArch::X86_64);
        assert_eq!(
            platform.target_triple().unwrap(),
            "x86_64-unknown-linux-gnu"
        );
    }

    #[test]
    fn parses_macos_uname_output_with_apple_silicon_arch_name() {
        let platform = parse_uname_output("Darwin\narm64\n").expect("should parse");
        assert_eq!(platform.os, RemoteOs::MacOs);
        assert_eq!(platform.arch, RemoteArch::Aarch64);
        assert_eq!(platform.target_triple().unwrap(), "aarch64-apple-darwin");
    }

    #[test]
    fn uname_output_from_a_non_posix_shell_does_not_parse() {
        // e.g. cmd.exe's "'uname' is not recognized..." error text
        assert!(
            parse_uname_output("'uname' is not recognized as an internal or external command.")
                .is_none()
        );
    }

    #[test]
    fn parses_windows_probe_output() {
        let platform = parse_windows_probe("AMD64\nC:\\Users\\me\\AppData\\Local\\Temp\n").unwrap();
        assert_eq!(platform.os, RemoteOs::Windows);
        assert_eq!(platform.arch, RemoteArch::X86_64);
        assert_eq!(
            platform.windows_temp_dir.as_deref(),
            Some("C:\\Users\\me\\AppData\\Local\\Temp")
        );
        assert_eq!(platform.target_triple().unwrap(), "x86_64-pc-windows-msvc");
    }

    #[test]
    fn windows_arm64_has_no_published_target() {
        let platform = parse_windows_probe("ARM64\nC:\\Temp\n").unwrap();
        assert!(platform.target_triple().is_err());
    }

    #[test]
    fn agent_paths_are_versioned_and_target_tagged() {
        let linux = RemotePlatform {
            os: RemoteOs::Linux,
            arch: RemoteArch::X86_64,
            windows_temp_dir: None,
        };
        let (dir, path) = linux.agent_paths("1.2.3").unwrap();
        assert_eq!(dir, "/tmp/.rustrest/agents/1.2.3");
        assert_eq!(
            path,
            "/tmp/.rustrest/agents/1.2.3/rustrest-remote-agent-x86_64-unknown-linux-gnu"
        );

        let windows = RemotePlatform {
            os: RemoteOs::Windows,
            arch: RemoteArch::X86_64,
            windows_temp_dir: Some(r"C:\Users\me\AppData\Local\Temp".to_string()),
        };
        let (dir, path) = windows.agent_paths("1.2.3").unwrap();
        assert_eq!(
            dir,
            r"C:\Users\me\AppData\Local\Temp\.rustrest\agents\1.2.3"
        );
        assert_eq!(
            path,
            r"C:\Users\me\AppData\Local\Temp\.rustrest\agents\1.2.3\rustrest-remote-agent-x86_64-pc-windows-msvc.exe"
        );
    }

    #[test]
    fn agent_paths_fall_back_to_a_default_windows_temp_dir() {
        let windows = RemotePlatform {
            os: RemoteOs::Windows,
            arch: RemoteArch::X86_64,
            windows_temp_dir: None,
        };
        let (dir, _path) = windows.agent_paths("1.2.3").unwrap();
        assert_eq!(dir, r"C:\Windows\Temp\.rustrest\agents\1.2.3");
    }
}
