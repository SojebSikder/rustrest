//! Git status/diff panel, remote sync (push/pull/fetch), and the commit
//! modal for a git-backed (`storage_dir`-having) collection.

use super::Rustrest;
use crate::collection::git_ops::GitRemoteOp;
use crate::message::Message;
use crate::ui::commit_modal::CommitModalState;
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct GitState {
    pub git_status_cache:
        HashMap<usize, Result<crate::collection::git_ops::GitStatusSnapshot, String>>,
    pub git_selected_file: Option<PathBuf>,
    pub git_diff_cache: Option<(PathBuf, String)>,
    pub git_remote_op_running: HashMap<usize, GitRemoteOp>,
    pub commit_modal: Option<CommitModalState>,
}

pub fn start_remote_op(app: &mut Rustrest, col_id: usize, op: GitRemoteOp) -> Task<Message> {
    let dir = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .and_then(|c| c.storage_dir.clone());

    let Some(dir) = dir else {
        return Task::none();
    };

    app.git.git_remote_op_running.insert(col_id, op);

    Task::perform(
        async move {
            match op {
                GitRemoteOp::Push => crate::collection::git_ops::git_push(&dir).await,
                GitRemoteOp::Pull => crate::collection::git_ops::git_pull(&dir).await,
                GitRemoteOp::Fetch => crate::collection::git_ops::git_fetch(&dir).await,
            }
        },
        move |result| Message::GitRemoteOpResult(col_id, op, result),
    )
}

pub fn status_requested(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    let dir = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .and_then(|c| c.storage_dir.clone());

    match dir {
        Some(dir) => Task::perform(
            async move { crate::collection::git_ops::git_status(&dir).await },
            move |result| Message::GitStatusLoaded(col_id, result),
        ),
        None => Task::none(),
    }
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
    let dir = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .and_then(|c| c.storage_dir.clone());

    match dir {
        Some(dir) => {
            let file_for_result = file.clone();
            Task::perform(
                async move { crate::collection::git_ops::git_diff_file(&dir, &file).await },
                move |result| Message::GitDiffLoaded(col_id, file_for_result, result),
            )
        }
        None => Task::none(),
    }
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
    let Some(collection) = app.collections.iter().find(|c| c.id == col_id) else {
        return Task::none();
    };
    let Some(dir) = collection.storage_dir.clone() else {
        return Task::none();
    };
    let collection_name = collection.info.name.clone();

    Task::perform(
        async move { crate::collection::git_ops::git_status(&dir).await },
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
    let Some(collection) = app.collections.iter().find(|c| c.id == modal.collection_id) else {
        return Task::none();
    };
    let Some(dir) = collection.storage_dir.clone() else {
        return Task::none();
    };
    let col_id = modal.collection_id;
    let message = modal.message.text();
    if let Some(modal) = app.git.commit_modal.as_mut() {
        modal.committing = true;
    }

    Task::perform(
        async move { crate::collection::git_ops::git_commit_all(&dir, &message).await },
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
