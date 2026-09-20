//! Headless binary that runs on the remote host.

use rustrest_remote_protocol::{
    Envelope, GitCommandOutput, Request, Response, read_message, write_message,
};
use tokio::io::{self, AsyncWriteExt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        let envelope: Envelope<Request> = match read_message(&mut stdin).await {
            Ok(envelope) => envelope,
            Err(_) => break,
        };

        let response = handle(envelope.payload).await;
        let reply = Envelope {
            id: envelope.id,
            payload: response,
        };
        if write_message(&mut stdout, &reply).await.is_err() {
            break;
        }
        if stdout.flush().await.is_err() {
            break;
        }
    }
}

async fn handle(request: Request) -> Response {
    match request {
        Request::ListDir(path) => list_dir(&path).await,
        Request::ReadFile(path) => match tokio::fs::read(&path).await {
            Ok(bytes) => Response::FileContent(bytes),
            Err(err) => Response::Error(err.to_string()),
        },
        Request::WriteFile(path, bytes) => match tokio::fs::write(&path, bytes).await {
            Ok(()) => Response::Ok,
            Err(err) => Response::Error(err.to_string()),
        },
        Request::CreateDir(path) => match tokio::fs::create_dir_all(&path).await {
            Ok(()) => Response::Ok,
            Err(err) => Response::Error(err.to_string()),
        },
        Request::Delete(path) => delete(&path).await,
        Request::Rename(from, to) => match tokio::fs::rename(&from, &to).await {
            Ok(()) => Response::Ok,
            Err(err) => Response::Error(err.to_string()),
        },
        Request::RunGit { cwd, args } => run_git(&cwd, &args).await,
    }
}

async fn run_git(cwd: &str, args: &[String]) -> Response {
    let mut command = tokio::process::Command::new("git");
    command.current_dir(cwd).args(args);

    match command.output().await {
        Ok(output) => Response::GitOutput(GitCommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
        }),
        Err(err) => Response::Error(if err.kind() == std::io::ErrorKind::NotFound {
            "git not found on PATH on the remote host, install Git to use this feature".to_string()
        } else {
            format!("Failed to run git: {err}")
        }),
    }
}

async fn list_dir(path: &str) -> Response {
    let mut read_dir = match tokio::fs::read_dir(path).await {
        Ok(read_dir) => read_dir,
        Err(err) => return Response::Error(err.to_string()),
    };

    let mut entries = Vec::new();
    loop {
        let entry = match read_dir.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(err) => return Response::Error(err.to_string()),
        };
        let metadata = match entry.metadata().await {
            Ok(metadata) => metadata,
            Err(err) => return Response::Error(err.to_string()),
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        entries.push(rustrest_remote_protocol::RemoteEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified,
        });
    }

    Response::DirListing(entries)
}

async fn delete(path: &str) -> Response {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(err) => return Response::Error(err.to_string()),
    };

    let result = if metadata.is_dir() {
        tokio::fs::remove_dir_all(path).await
    } else {
        tokio::fs::remove_file(path).await
    };

    match result {
        Ok(()) => Response::Ok,
        Err(err) => Response::Error(err.to_string()),
    }
}
