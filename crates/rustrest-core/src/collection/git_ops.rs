use std::path::{Path, PathBuf};
use tokio::process::Command;

const DEFAULT_GITIGNORE: &str = "\
# OS cruft
.DS_Store
Thumbs.db
ehthumbs.db
desktop.ini
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitChangeKind {
    Untracked,
    Modified,
    Added,
    Deleted,
    Renamed,
    Conflicted,
}

#[derive(Debug, Clone)]
pub struct GitFileEntry {
    pub path: PathBuf,
    pub status: GitChangeKind,
}

#[derive(Debug, Clone)]
pub struct GitStatusSnapshot {
    pub branch: Option<String>,
    pub files: Vec<GitFileEntry>,
}

/// a remote-sync operation kind, used to label in-progress UI state and
/// route the result of a background push/pull/fetch task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitRemoteOp {
    Push,
    Pull,
    Fetch,
}

impl GitRemoteOp {
    pub fn label(self) -> &'static str {
        match self {
            GitRemoteOp::Push => "Push",
            GitRemoteOp::Pull => "Pull",
            GitRemoteOp::Fetch => "Fetch",
        }
    }
}

/// true if `path` looks like a git working tree (has a `.git` entry).
pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

async fn run_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command.current_dir(root).args(args);

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "git not found on PATH - install Git to use this feature (git-scm.com)".to_string()
        } else {
            format!("Failed to run git: {e}")
        }
    })?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// like `run_git`, but for commands with output (progress, "Already up to date.", etc.)
async fn run_git_report(root: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command.current_dir(root).args(args);

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "git not found on PATH - install Git to use this feature (git-scm.com)".to_string()
        } else {
            format!("Failed to run git: {e}")
        }
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let combined = match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("{stdout}\n{stderr}"),
    };

    if !output.status.success() {
        return Err(combined);
    }
    Ok(combined)
}

/// runs `git init` in `path`, no-op if it's already a repo.
pub async fn git_init(path: &Path) -> Result<(), String> {
    if is_git_repo(path) {
        return Ok(());
    }
    run_git(path, &["init"]).await.map(|_| ())
}

/// writes a small default `.gitignore` if one doesn't already exist.
pub fn write_default_gitignore_if_missing(path: &Path) -> Result<(), String> {
    let gitignore_path = path.join(".gitignore");
    if gitignore_path.exists() {
        return Ok(());
    }
    std::fs::write(&gitignore_path, DEFAULT_GITIGNORE)
        .map_err(|e| format!("Failed to write {gitignore_path:?}: {e}"))
}

/// parses a `git status --porcelain=v1` two-letter status code into a `GitChangeKind`.
fn parse_status_code(code: &str) -> GitChangeKind {
    if code == "??" {
        return GitChangeKind::Untracked;
    }
    if code.contains('U') || code == "AA" || code == "DD" {
        return GitChangeKind::Conflicted;
    }
    // prefer the index (staged) column, fall back to worktree column
    let bytes = code.as_bytes();
    let primary = if bytes.first().copied().unwrap_or(b' ') != b' ' {
        bytes[0]
    } else {
        bytes.get(1).copied().unwrap_or(b' ')
    };
    match primary {
        b'A' => GitChangeKind::Added,
        b'D' => GitChangeKind::Deleted,
        b'R' => GitChangeKind::Renamed,
        b'C' => GitChangeKind::Added,
        _ => GitChangeKind::Modified,
    }
}

/// git quotes paths containing spaces or other special characters as a
/// C-style quoted string (e.g. `"Get Posts.json"`, with `\NNN` octal byte
/// escapes for anything outside 7-bit ASCII). Undoes that so the returned
/// path matches the real filename on disk.
fn unquote_path(s: &str) -> String {
    if !(s.len() >= 2 && s.starts_with('"') && s.ends_with('"')) {
        return s.to_string();
    }
    let inner = &s[1..s.len() - 1];
    let bytes = inner.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'\\' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            break;
        }
        match bytes[i] {
            b'n' => {
                out.push(b'\n');
                i += 1;
            }
            b't' => {
                out.push(b'\t');
                i += 1;
            }
            b'\\' => {
                out.push(b'\\');
                i += 1;
            }
            b'"' => {
                out.push(b'"');
                i += 1;
            }
            d @ b'0'..=b'7' => {
                let mut val = (d - b'0') as u32;
                i += 1;
                for _ in 0..2 {
                    if i < bytes.len() && (b'0'..=b'7').contains(&bytes[i]) {
                        val = val * 8 + (bytes[i] - b'0') as u32;
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(val as u8);
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// runs `git status --porcelain=v1 -b` and parses the result.
pub async fn git_status(path: &Path) -> Result<GitStatusSnapshot, String> {
    let raw = run_git(
        path,
        &["status", "--porcelain=v1", "-b", "--untracked-files=all"],
    )
    .await?;
    let mut lines = raw.lines();

    let branch = lines.next().and_then(|header| {
        // "## main...origin/main" or "## HEAD (no branch)" or "## No commits yet on main"
        let rest = header.strip_prefix("## ")?;
        if let Some(name) = rest.strip_prefix("No commits yet on ") {
            return Some(name.to_string());
        }
        let name = rest.split("...").next().unwrap_or(rest);
        if name.starts_with("HEAD") {
            None
        } else {
            Some(name.to_string())
        }
    });

    let mut files = Vec::new();
    for line in lines {
        if line.len() < 3 {
            continue;
        }
        let code = &line[0..2];
        let rest = line[3..].trim();
        // renames look like "old -> new"; use the new path.
        let file_path = rest.rsplit_once(" -> ").map_or(rest, |(_, new)| new);
        files.push(GitFileEntry {
            path: PathBuf::from(unquote_path(file_path)),
            status: parse_status_code(code),
        });
    }

    Ok(GitStatusSnapshot { branch, files })
}

/// returns a diff-style preview for a single file, relative to `root`.
/// tracked files diff against HEAD; untracked files are shown as a
/// synthetic "new file" preview of their raw content.
pub async fn git_diff_file(root: &Path, file: &Path) -> Result<String, String> {
    let status = git_status(root).await?;
    let is_untracked = status
        .files
        .iter()
        .any(|f| f.path == file && f.status == GitChangeKind::Untracked);

    if is_untracked {
        let full_path = root.join(file);
        if full_path.is_dir() {
            return Err(format!(
                "{} is a directory, not a file - refresh the git status to see its contents individually",
                file.display()
            ));
        }
        let content = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| format!("Failed to read {full_path:?}: {e}"))?;
        let body: String = content.lines().map(|l| format!("+{l}\n")).collect();
        return Ok(format!("new file: {}\n{body}", file.display()));
    }

    let file_arg = file.to_string_lossy().replace('\\', "/");
    run_git(root, &["diff", "HEAD", "--", &file_arg]).await
}

/// stages all changes and commits them with `message`.
pub async fn git_commit_all(root: &Path, message: &str) -> Result<(), String> {
    run_git(root, &["add", "-A"]).await?;
    run_git(root, &["commit", "-m", message]).await.map(|_| ())
}

/// pushes the current branch to its remote, returning a human-readable
/// status line for a toast/notification.
pub async fn git_push(root: &Path) -> Result<String, String> {
    run_git_report(root, &["push"]).await
}

/// pulls (fetch + merge) from the current branch's remote.
pub async fn git_pull(root: &Path) -> Result<String, String> {
    run_git_report(root, &["pull"]).await
}

/// fetches from the remote without touching the working tree.
pub async fn git_fetch(root: &Path) -> Result<String, String> {
    run_git_report(root, &["fetch", "--prune"]).await
}
