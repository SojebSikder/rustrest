//! Git status/diff panel, remote sync (push/pull/fetch), and the commit
//! modal for a git-backed collection - either disk-backed (`storage_dir`) or
//! backed by a directory on a remote SSH host (`remote_dir`), in which case
//! `git` runs on the remote host via the same RPC channel used for remote
//! file browsing (see [`GitTarget`]).

use super::Rustrest;
use crate::collection::git_ops::{self, GitRemoteOp};
use crate::message::Message;
use crate::ui::commit_modal::CommitModalState;
use crate::ui::remote::join_remote_path;
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use rustrest_remote::RemoteSession;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Default)]
pub struct GitState {
    pub git_status_cache:
        HashMap<usize, Result<crate::collection::git_ops::GitStatusSnapshot, String>>,
    pub git_selected_file: Option<PathBuf>,
    pub git_diff_cache: Option<(PathBuf, String)>,
    pub git_remote_op_running: HashMap<usize, GitRemoteOp>,
    pub commit_modal: Option<CommitModalState>,
}

/// where a collection's git-backed directory actually lives: either a local
/// disk path, or a directory on a remote SSH host reached through its
/// already-connected [`RemoteSession`].
enum GitTarget {
    Local(PathBuf),
    Remote {
        session: Arc<RemoteSession>,
        root: String,
    },
}

/// resolves the git target for `col_id`. `Err` means the collection is
/// remote-backed but not currently connected; `Ok(None)` means it isn't
/// git-backed at all.
fn git_target(app: &Rustrest, col_id: usize) -> Result<Option<GitTarget>, String> {
    let Some(collection) = app.collections.iter().find(|c| c.id == col_id) else {
        return Ok(None);
    };
    if let Some(dir) = &collection.storage_dir {
        return Ok(Some(GitTarget::Local(dir.clone())));
    }
    if let Some(remote) = &collection.remote_dir {
        return match app.remote.remote_sessions.get(&remote.profile_id).cloned() {
            Some(session) => Ok(Some(GitTarget::Remote {
                session,
                root: remote.root.clone(),
            })),
            None => Err(
                "Not connected to the remote host - reconnect to use Git for this collection"
                    .to_string(),
            ),
        };
    }
    Ok(None)
}

async fn remote_git_status(
    session: &RemoteSession,
    root: &str,
) -> Result<git_ops::GitStatusSnapshot, String> {
    let output = session
        .run_git(root, git_ops::STATUS_ARGS)
        .await
        .map_err(|e| e.to_string())?;
    if !output.success {
        return Err(output.stderr.trim().to_string());
    }
    Ok(git_ops::parse_status_output(&output.stdout))
}

async fn remote_git_diff_file(
    session: &RemoteSession,
    root: &str,
    file: &Path,
) -> Result<String, String> {
    let status = remote_git_status(session, root).await?;
    let is_untracked = status
        .files
        .iter()
        .any(|f| f.path == file && f.status == git_ops::GitChangeKind::Untracked);

    if is_untracked {
        let remote_path = join_remote_path(root, &file.to_string_lossy());
        let bytes = session
            .read_file(&remote_path)
            .await
            .map_err(|e| e.to_string())?;
        let content = String::from_utf8_lossy(&bytes).into_owned();
        return Ok(git_ops::synthetic_new_file_diff(file, &content));
    }

    let file_arg = file.to_string_lossy().replace('\\', "/");
    let output = session
        .run_git(root, &["diff", "HEAD", "--", &file_arg])
        .await
        .map_err(|e| e.to_string())?;
    if !output.success {
        return Err(output.stderr.trim().to_string());
    }
    Ok(output.stdout)
}

async fn remote_git_commit_all(
    session: &RemoteSession,
    root: &str,
    message: &str,
) -> Result<(), String> {
    let add = session
        .run_git(root, &["add", "-A"])
        .await
        .map_err(|e| e.to_string())?;
    if !add.success {
        return Err(add.stderr.trim().to_string());
    }
    let commit = session
        .run_git(root, &["commit", "-m", message])
        .await
        .map_err(|e| e.to_string())?;
    if !commit.success {
        return Err(commit.stderr.trim().to_string());
    }
    Ok(())
}

async fn remote_git_report(
    session: &RemoteSession,
    root: &str,
    args: &[&str],
) -> Result<String, String> {
    let output = session
        .run_git(root, args)
        .await
        .map_err(|e| e.to_string())?;
    let combined = git_ops::combine_output(&output.stdout, &output.stderr);
    if !output.success {
        return Err(combined);
    }
    Ok(combined)
}

pub fn start_remote_op(app: &mut Rustrest, col_id: usize, op: GitRemoteOp) -> Task<Message> {
    let target = match git_target(app, col_id) {
        Ok(Some(target)) => target,
        Ok(None) => return Task::none(),
        Err(e) => return Task::done(Message::GitRemoteOpResult(col_id, op, Err(e))),
    };

    app.git.git_remote_op_running.insert(col_id, op);

    let args: &'static [&'static str] = match op {
        GitRemoteOp::Push => &["push"],
        GitRemoteOp::Pull => &["pull"],
        GitRemoteOp::Fetch => &["fetch", "--prune"],
    };

    Task::perform(
        async move {
            match target {
                GitTarget::Local(dir) => match op {
                    GitRemoteOp::Push => crate::collection::git_ops::git_push(&dir).await,
                    GitRemoteOp::Pull => crate::collection::git_ops::git_pull(&dir).await,
                    GitRemoteOp::Fetch => crate::collection::git_ops::git_fetch(&dir).await,
                },
                GitTarget::Remote { session, root } => {
                    remote_git_report(&session, &root, args).await
                }
            }
        },
        move |result| Message::GitRemoteOpResult(col_id, op, result),
    )
}

pub fn status_requested(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    let target = match git_target(app, col_id) {
        Ok(Some(target)) => target,
        Ok(None) => return Task::none(),
        Err(e) => return Task::done(Message::GitStatusLoaded(col_id, Err(e))),
    };

    Task::perform(
        async move {
            match target {
                GitTarget::Local(dir) => crate::collection::git_ops::git_status(&dir).await,
                GitTarget::Remote { session, root } => remote_git_status(&session, &root).await,
            }
        },
        move |result| Message::GitStatusLoaded(col_id, result),
    )
}

pub fn status_loaded(
    app: &mut Rustrest,
    col_id: usize,
    result: Result<crate::collection::git_ops::GitStatusSnapshot, String>,
) -> Task<Message> {
    app.git.git_status_cache.insert(col_id, result);
    Task::none()
}

pub fn diff_requested(app: &mut Rustrest, col_id: usize, file: PathBuf) -> Task<Message> {
    app.git.git_selected_file = Some(file.clone());
    let target = match git_target(app, col_id) {
        Ok(Some(target)) => target,
        Ok(None) => return Task::none(),
        Err(e) => return Task::done(Message::GitDiffLoaded(col_id, file, Err(e))),
    };

    let file_for_result = file.clone();
    Task::perform(
        async move {
            match target {
                GitTarget::Local(dir) => {
                    crate::collection::git_ops::git_diff_file(&dir, &file).await
                }
                GitTarget::Remote { session, root } => {
                    remote_git_diff_file(&session, &root, &file).await
                }
            }
        },
        move |result| Message::GitDiffLoaded(col_id, file_for_result, result),
    )
}

pub fn diff_loaded(
    app: &mut Rustrest,
    file: PathBuf,
    result: Result<String, String>,
) -> Task<Message> {
    if app.git.git_selected_file.as_ref() == Some(&file) {
        let content = match result {
            Ok(diff) if diff.trim().is_empty() => "(no textual differences)".to_string(),
            Ok(diff) => diff,
            Err(e) => format!("Failed to load diff: {e}"),
        };
        app.git.git_diff_cache = Some((file, content));
    }
    Task::none()
}

pub fn remote_op_result(
    app: &mut Rustrest,
    col_id: usize,
    op: GitRemoteOp,
    result: Result<String, String>,
) -> Task<Message> {
    app.git.git_remote_op_running.remove(&col_id);
    let label = op.label();
    match result {
        Ok(output) => {
            let toast_msg = if output.is_empty() {
                format!("{label} completed")
            } else {
                output
            };
            Task::batch([
                Task::done(Message::ShowToast(toast_msg, ToastStatus::Success)),
                Task::done(Message::GitStatusRequested(col_id)),
            ])
        }
        Err(e) => Task::done(Message::ShowToast(
            format!("{label} failed: {e}"),
            ToastStatus::Error,
        )),
    }
}

pub fn commit_changes_pressed(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    app.sync_collection_tabs(col_id);
    let target = match git_target(app, col_id) {
        Ok(Some(target)) => target,
        Ok(None) => return Task::none(),
        Err(e) => {
            return Task::done(Message::ShowToast(e, ToastStatus::Error));
        }
    };
    let Some(collection) = app.collections.iter().find(|c| c.id == col_id) else {
        return Task::none();
    };
    let collection_name = collection.info.name.clone();

    Task::perform(
        async move {
            match target {
                GitTarget::Local(dir) => crate::collection::git_ops::git_status(&dir).await,
                GitTarget::Remote { session, root } => remote_git_status(&session, &root).await,
            }
        },
        move |result| match result {
            Ok(snapshot) => Message::CommitStatusLoaded(col_id, collection_name.clone(), snapshot),
            Err(e) => Message::ShowToast(
                format!("Failed to read git status: {e}"),
                ToastStatus::Error,
            ),
        },
    )
}

pub fn commit_status_loaded(
    app: &mut Rustrest,
    col_id: usize,
    collection_name: String,
    snapshot: crate::collection::git_ops::GitStatusSnapshot,
) -> Task<Message> {
    if snapshot.files.is_empty() {
        app.git.git_status_cache.insert(col_id, Ok(snapshot));
        return Task::done(Message::ShowToast(
            "No changes to commit".to_string(),
            ToastStatus::Info,
        ));
    }
    app.git
        .git_status_cache
        .insert(col_id, Ok(snapshot.clone()));
    app.git.commit_modal = Some(CommitModalState {
        collection_id: col_id,
        collection_name,
        message: iced::widget::text_editor::Content::new(),
        files: snapshot.files,
        committing: false,
    });
    Task::none()
}

pub fn commit_message_changed(
    app: &mut Rustrest,
    action: iced::widget::text_editor::Action,
) -> Task<Message> {
    if let Some(modal) = app.git.commit_modal.as_mut() {
        modal.message.perform(action);
    }
    Task::none()
}

pub fn commit_cancelled(app: &mut Rustrest) -> Task<Message> {
    app.git.commit_modal = None;
    Task::none()
}

pub fn commit_confirmed(app: &mut Rustrest) -> Task<Message> {
    let Some(modal) = app.git.commit_modal.as_ref() else {
        return Task::none();
    };
    let col_id = modal.collection_id;
    let target = match git_target(app, col_id) {
        Ok(Some(target)) => target,
        Ok(None) => return Task::none(),
        Err(e) => return Task::done(Message::CommitResult(col_id, Err(e))),
    };
    let Some(modal) = app.git.commit_modal.as_ref() else {
        return Task::none();
    };
    let message = modal.message.text();
    if let Some(modal) = app.git.commit_modal.as_mut() {
        modal.committing = true;
    }

    Task::perform(
        async move {
            match target {
                GitTarget::Local(dir) => {
                    crate::collection::git_ops::git_commit_all(&dir, &message).await
                }
                GitTarget::Remote { session, root } => {
                    remote_git_commit_all(&session, &root, &message).await
                }
            }
        },
        move |result| Message::CommitResult(col_id, result),
    )
}

pub fn commit_result(
    app: &mut Rustrest,
    col_id: usize,
    result: Result<(), String>,
) -> Task<Message> {
    match result {
        Ok(()) => {
            app.git.commit_modal = None;
            Task::batch([
                Task::done(Message::ShowToast(
                    "Changes committed".to_string(),
                    ToastStatus::Success,
                )),
                Task::done(Message::GitStatusRequested(col_id)),
            ])
        }
        Err(e) => {
            if let Some(modal) = app.git.commit_modal.as_mut() {
                modal.committing = false;
            }
            Task::done(Message::ShowToast(
                format!("Commit failed: {e}"),
                ToastStatus::Error,
            ))
        }
    }
}
