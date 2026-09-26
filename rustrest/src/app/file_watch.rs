//! Watches local collection files/folders for changes made by other programs
//! (an editor, `git pull`, ...) and reloads them.
//! a collection with no unsaved edits reloads silently; one with unsaved edits
//! asks before its changes are discarded.
//!
//! rustrest's own saves also trigger watch events. They're told apart by
//! content rather than timing: every collection remembers a fingerprint of
//! what it last read from or wrote to disk, and a reload only happens when
//! what's on disk now differs from that.

use super::{CollectionSubTab, Rustrest, TabState, WorkspaceContent};
use crate::collection::collection::{PostmanCollection, PostmanRequestNode};
use crate::collection_adapter::{create_tab_from_request, workspace_content_for_request};
use crate::message::Message;
use crate::ui::confirm_dialog::ConfirmDialogState;
use crate::ui::toast::toast::ToastStatus;
use crate::workspace::CollectionSource;
use iced::Task;
use iced::futures::{SinkExt, StreamExt, stream::BoxStream};
use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, new_debouncer};
use rustrest_core::collection::tree_ops::{carry_over_request_ids, find_request};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// how long to wait for a burst of file events (a multi-file save, a
/// `git checkout`) to settle before reloading.
const DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct FileWatchState {
    /// collection id: fingerprint of the contents rustrest last read from
    /// or wrote to that collection's file/folder.
    baselines: HashMap<usize, u64>,
}

impl FileWatchState {
    pub fn clear(&mut self) {
        self.baselines.clear();
    }
}

fn fingerprint(collection: &PostmanCollection) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    serde_json::to_string(collection)
        .unwrap_or_default()
        .hash(&mut hasher);

    hasher.finish()
}

/// remembers `collection` as what's currently on disk for `col_id`,
/// call right after loading it from disk, or right before/after writing it.
pub fn record_baseline(app: &mut Rustrest, col_id: usize, collection: &PostmanCollection) {
    app.file_watch
        .baselines
        .insert(col_id, fingerprint(collection));
}

/// same as `record_baseline`, for the collection's current in-memory state
/// (i.e. call when it's being saved).
pub fn record_current_as_baseline(app: &mut Rustrest, col_id: usize) {
    if let Some(fp) = app
        .collections
        .iter()
        .find(|c| c.id == col_id)
        .map(fingerprint)
    {
        app.file_watch.baselines.insert(col_id, fp);
    }
}

/// what to watch: every local collection's folder (recursively) or file.
/// the result keys the watcher subscription, so it's rebuilt whenever a
/// collection is opened, closed or moved.
pub fn watch_targets(app: &Rustrest) -> Vec<(PathBuf, bool)> {
    let mut targets: Vec<(PathBuf, bool)> = app
        .collections
        .iter()
        .filter(|c| c.remote_dir.is_none())
        .filter_map(|c| match (&c.storage_dir, &c.file_path) {
            (Some(dir), _) => Some((dir.clone(), true)),
            (None, Some(file)) => Some((file.clone(), false)),
            (None, None) => None,
        })
        .collect();
    targets.sort();
    targets.dedup();
    targets
}

pub fn watch_stream(targets: &Vec<(PathBuf, bool)>) -> BoxStream<'static, Message> {
    let targets = targets.clone();
    iced::stream::channel(100, async move |mut output| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<PathBuf>>();
        let debouncer = new_debouncer(DEBOUNCE, move |result: DebounceEventResult| {
            if let Ok(events) = result {
                let _ = tx.send(events.into_iter().map(|e| e.path).collect());
            }
        });
        let mut debouncer = match debouncer {
            Ok(debouncer) => debouncer,
            Err(err) => {
                eprintln!("Failed to start collection file watcher: {err}");
                return;
            }
        };

        for (path, is_dir) in &targets {
            // editors often save by writing a temp file and renaming it over
            // the original, which a watch on the file itself would lose track
            // of, so single-file collections watch their parent directory
            let (watch_path, mode) = if *is_dir {
                (path.as_path(), RecursiveMode::Recursive)
            } else {
                let parent = path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                (parent, RecursiveMode::NonRecursive)
            };
            if let Err(err) = debouncer.watcher().watch(watch_path, mode) {
                eprintln!("Failed to watch {watch_path:?}: {err}");
            }
        }

        while let Some(paths) = rx.recv().await {
            if output
                .send(Message::CollectionFilesChanged(paths))
                .await
                .is_err()
            {
                break;
            }
        }
    })
    .boxed()
}

/// `path` relative to `root`, when it's inside it.
fn relative_to(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(rel) = path.strip_prefix(root) {
        return Some(rel.to_path_buf());
    }

    let root = std::fs::canonicalize(root).ok()?;
    path.strip_prefix(&root).ok().map(Path::to_path_buf)
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// whether a changed `path` affects the collection stored at `dir`,
/// changes under dot directories (`.git`, ...) never do, as they aren't part of the format.
fn touches_dir_collection(path: &Path, dir: &Path) -> bool {
    relative_to(path, dir).is_some_and(|rel| {
        !rel.as_os_str().is_empty()
            && !rel
                .components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
    })
}

/// rereads every collection a batch of changed paths belongs to.
pub fn files_changed(app: &mut Rustrest, paths: Vec<PathBuf>) -> Task<Message> {
    let tasks: Vec<Task<Message>> = app
        .collections
        .iter()
        .filter(|c| c.remote_dir.is_none())
        .filter_map(|c| {
            let source = match (&c.storage_dir, &c.file_path) {
                (Some(dir), _) if paths.iter().any(|p| touches_dir_collection(p, dir)) => {
                    CollectionSource::Dir(dir.clone())
                }
                (None, Some(file)) if paths.iter().any(|p| same_file(p, file)) => {
                    CollectionSource::File(file.clone())
                }
                _ => return None,
            };

            let col_id = c.id;
            Some(Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || super::load_collection_from_source(&source))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                move |result| Message::CollectionDiskLoaded(col_id, result.map(Box::new)),
            ))
        })
        .collect();
    Task::batch(tasks)
}

pub fn disk_loaded(
    app: &mut Rustrest,
    col_id: usize,
    result: Result<Box<PostmanCollection>, String>,
) -> Task<Message> {
    // usually a file caught mid-write; the write's own event will follow
    let Ok(mut loaded) = result else {
        return Task::none();
    };
    let Some(col) = app.collections.iter().find(|c| c.id == col_id) else {
        return Task::none();
    };

    // the collection was moved/re-saved elsewhere while this was loading
    let same_source = match (&loaded.storage_dir, &loaded.file_path) {
        (Some(dir), _) => col.storage_dir.as_ref() == Some(dir),
        (None, Some(file)) => col.storage_dir.is_none() && col.file_path.as_ref() == Some(file),
        (None, None) => false,
    };
    if !same_source || col.remote_dir.is_some() {
        return Task::none();
    }
    loaded.file_path = col.file_path.clone();
    loaded.storage_dir = col.storage_dir.clone();

    let on_disk = fingerprint(&loaded);
    match app.file_watch.baselines.get(&col_id) {
        // our own save, or a write that didn't change anything
        Some(baseline) if *baseline == on_disk => return Task::none(),
        None if fingerprint(col) == on_disk => {
            app.file_watch.baselines.insert(col_id, on_disk);
            return Task::none();
        }
        _ => {}
    }

    if !crate::ui::unsaved::collection_is_unsaved(app, col) {
        replace_collection(app, col_id, *loaded);
        return Task::none();
    }

    // conflict: keep the in-memory edits unless the user opts to reload.
    // remembering this disk state as seen means it won't prompt again for
    // it, and a later save simply overwrites it.
    let name = col.info.name.clone();
    app.file_watch.baselines.insert(col_id, on_disk);

    Task::done(Message::ShowConfirmDialog(ConfirmDialogState {
        title: "Collection changed on disk".to_string(),
        message: format!(
            "\"{name}\" was changed by another program, but has unsaved changes here. \
             Reload it and discard your changes? Cancel keeps your changes; saving \
             will overwrite the file."
        ),
        confirm_label: "Reload".to_string(),
        on_confirm: Box::new(Message::ReplaceCollectionConfirmed(col_id, loaded)),
    }))
}

/// swaps in a freshly loaded copy of collection `col_id`, keeping request ids and open tabs,
/// wherever the same request still exists, and refreshing those tabs from the new contents.
/// tabs for requests that no longer exist are closed.
pub fn replace_collection(app: &mut Rustrest, col_id: usize, mut new: PostmanCollection) {
    let Some(pos) = app.collections.iter().position(|c| c.id == col_id) else {
        return;
    };
    new.id = col_id;

    carry_over_request_ids(
        &app.collections[pos].item,
        &mut new.item,
        &mut app.next_request_id,
    );

    record_baseline(app, col_id, &new);
    app.collections[pos] = new;
    app.git.git_status_cache.remove(&col_id);
    refresh_open_tabs(app, pos);
}

fn refresh_open_tabs(app: &mut Rustrest, collection_pos: usize) {
    let col = &app.collections[collection_pos];
    let col_id = col.id;
    let mut stale_tabs = Vec::new();

    for (idx, tab_state) in app.tabs.iter_mut().enumerate() {
        match &mut tab_state.content {
            WorkspaceContent::HttpRequest
            | WorkspaceContent::WebSocket(_)
            | WorkspaceContent::GraphQl(_)
            | WorkspaceContent::Grpc(_)
                if tab_state.tab.collection_id == Some(col_id) =>
            {
                let Some(req_id) = tab_state.tab.request_id else {
                    continue;
                };
                match find_request(&col.item, req_id) {
                    Some(node) => refresh_request_tab(tab_state, node, col_id),
                    None => stale_tabs.push(idx),
                }
            }
            WorkspaceContent::CollectionRoot {
                collection_id,
                collection_name,
                active_sub_tab,
                docs,
                settings,
            } if *collection_id == col_id => {
                *collection_name = col.info.name.clone();
                tab_state.tab.name = col.info.name.clone();
                if settings.is_some()
                    || matches!(
                        active_sub_tab,
                        CollectionSubTab::Authorization | CollectionSubTab::Scripts
                    )
                {
                    *settings = Some(Box::new(
                        crate::ui::collection_settings::CollectionSettingsState::from_collection(
                            col,
                        ),
                    ));
                }
                if docs.is_some() {
                    *docs = Some(Box::new(crate::ui::docs_view::DocsState::new(
                        col,
                        rustrest_core::docs::DocsTarget::Collection,
                    )));
                }
            }
            _ => {}
        }
    }

    for idx in stale_tabs.into_iter().rev() {
        app.tabs.remove(idx);
        if idx < app.active_tab_index {
            app.active_tab_index -= 1;
        }
    }
    if app.active_tab_index >= app.tabs.len() {
        app.active_tab_index = app.tabs.len().saturating_sub(1);
    }
}

/// rebuilds a request tab's editable state from `node`, keeping what isn't
/// stored in the collection (the last response, which sub-tabs are shown, an
/// in-flight request). protocol tabs with a live connection only pick up the
/// new name, so reloading never drops a connection.
fn refresh_request_tab(tab_state: &mut TabState, node: &PostmanRequestNode, col_id: usize) {
    use std::mem::swap;

    let is_live = match &tab_state.content {
        WorkspaceContent::WebSocket(s) => s.connected || s.connecting,
        WorkspaceContent::GraphQl(s) => s.subscribed || s.is_loading,
        WorkspaceContent::Grpc(s) => s.invoking || s.discovering,
        _ => false,
    };
    if is_live {
        tab_state.tab.name = node.name.clone();
        return;
    }

    let mut fresh = create_tab_from_request(tab_state.tab.id, node, Some(col_id));
    let old = &mut tab_state.tab;

    swap(&mut fresh.response, &mut old.response);
    swap(
        &mut fresh.response_body_editor,
        &mut old.response_body_editor,
    );
    swap(&mut fresh.response_view, &mut old.response_view);
    swap(&mut fresh.active_sub_tab, &mut old.active_sub_tab);
    swap(&mut fresh.active_response_tab, &mut old.active_response_tab);
    swap(&mut fresh.script_tab, &mut old.script_tab);
    swap(&mut fresh.is_loading, &mut old.is_loading);
    swap(&mut fresh.cancel_token, &mut old.cancel_token);
    swap(&mut fresh.sse_active, &mut old.sse_active);
    swap(&mut fresh.sse_log, &mut old.sse_log);

    fresh.viewing_saved_response = old
        .viewing_saved_response
        .filter(|&i| i < fresh.saved_responses.len());
    *old = fresh;

    let mut content = workspace_content_for_request(node);
    match (&mut content, &mut tab_state.content) {
        (WorkspaceContent::WebSocket(new), WorkspaceContent::WebSocket(old)) => {
            swap(&mut new.log, &mut old.log);
        }
        (WorkspaceContent::GraphQl(new), WorkspaceContent::GraphQl(old)) => {
            swap(&mut new.response, &mut old.response);
            swap(&mut new.schema, &mut old.schema);
            swap(&mut new.subscription_log, &mut old.subscription_log);
        }
        (WorkspaceContent::Grpc(new), WorkspaceContent::Grpc(old)) => {
            swap(&mut new.target, &mut old.target);
            swap(&mut new.log, &mut old.log);
        }
        _ => {}
    }
    tab_state.content = content;
}

/// `ReplaceCollectionConfirmed`: the user chose to reload a collection
/// (after a git/remote re-import, or a conflicting change on disk).
pub fn replace_confirmed(
    app: &mut Rustrest,
    col_id: usize,
    new: Box<PostmanCollection>,
) -> Task<Message> {
    replace_collection(app, col_id, *new);
    Task::done(Message::ShowToast(
        "Collection reloaded from disk".to_string(),
        ToastStatus::Success,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn watcher_reports_external_edits_to_a_collection_file() {
        let dir = std::env::temp_dir().join(format!("rustrest-watch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("c.postman_collection.json");
        std::fs::write(&file, "{}").unwrap();

        let mut stream = watch_stream(&vec![(file.clone(), false)]);
        // the stream only sets up its watcher once polled, so edit the file
        // from another task while waiting on it
        let edited = file.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            std::fs::write(&edited, "{\"changed\":true}").unwrap();
        });

        let event = tokio::time::timeout(Duration::from_secs(5), stream.next()).await;
        let _ = std::fs::remove_dir_all(&dir);
        match event {
            Ok(Some(Message::CollectionFilesChanged(paths))) => {
                assert!(
                    paths
                        .iter()
                        .any(|p| same_file(p, &file) || p.ends_with("c.postman_collection.json"))
                );
            }
            _ => panic!("no file change event received"),
        }
    }

    #[test]
    fn ignores_dot_directories_inside_a_dir_collection() {
        let root = Path::new("/cols/api");
        assert!(touches_dir_collection(&root.join("users/get.json"), root));
        assert!(!touches_dir_collection(&root.join(".git/index"), root));
        assert!(!touches_dir_collection(root, root));
        assert!(!touches_dir_collection(
            Path::new("/elsewhere/x.json"),
            root
        ));
    }
}
