use super::{CollectionSubTab, Rustrest, Tab, TabState, WorkspaceContent};
use crate::message::{Message, SidebarDragItem};
use crate::ui::docs_view::{DocsMessage, DocsMode, DocsState};
use crate::ui::toast::toast::ToastStatus;
use iced::Task;
use rustrest_core::docs::{self, DocsTarget};

/// the docs pane shown in tab `idx`: a folder tab, or a collection root tab
/// currently on its Docs sub-tab.
fn docs_state_mut(tabs: &mut [TabState], idx: usize) -> Option<&mut DocsState> {
    match &mut tabs.get_mut(idx)?.content {
        WorkspaceContent::Folder(state) => Some(state),
        WorkspaceContent::CollectionRoot {
            docs: Some(state),
            active_sub_tab: CollectionSubTab::Documentation,
            ..
        } => Some(state),
        _ => None,
    }
}

/// a sidebar folder row was pressed: arm it for dragging and focus its folder tab, opening one if needed.
pub fn folder_clicked(
    app: &mut Rustrest,
    collection_id: usize,
    folder_path: Vec<String>,
) -> Task<Message> {
    let _ = super::sidebar::drag_started(
        app,
        SidebarDragItem::Folder {
            collection_id,
            path: folder_path.clone(),
        },
    );

    let target = DocsTarget::Folder(folder_path);
    if let Some(idx) = app.tabs.iter().position(|t| {
        matches!(&t.content, WorkspaceContent::Folder(s)
            if s.collection_id == collection_id && s.target == target)
    }) {
        app.active_tab_index = idx;
        return Task::none();
    }

    let Some(collection) = app.collections.iter().find(|c| c.id == collection_id) else {
        return Task::none();
    };
    let Some(name) = docs::target_name(collection, &target) else {
        return Task::none();
    };
    let state = DocsState::new(collection, target);

    let mut tab = Tab::new(app.next_tab_id);
    tab.name = name;
    app.next_tab_id += 1;
    app.tabs.push(TabState {
        tab,
        content: WorkspaceContent::Folder(Box::new(state)),
        is_editing_name: false,
    });
    app.active_tab_index = app.tabs.len() - 1;
    iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
}

/// keeps open folder tabs pointing at the right folder after the folder at
/// `old_path` is renamed to `new_name` (which also moves its descendants).
pub fn folder_renamed(
    app: &mut Rustrest,
    collection_id: usize,
    old_path: &[String],
    new_name: &str,
) {
    let Some((_, parent)) = old_path.split_last() else {
        return;
    };
    for tab_state in &mut app.tabs {
        let WorkspaceContent::Folder(state) = &mut tab_state.content else {
            continue;
        };
        let DocsTarget::Folder(path) = &mut state.target else {
            continue;
        };
        if state.collection_id != collection_id || !path.starts_with(old_path) {
            continue;
        }
        path[parent.len()] = new_name.to_string();
        if path.len() == old_path.len() {
            tab_state.tab.name = new_name.to_string();
        }
    }
}

pub fn update(app: &mut Rustrest, message: DocsMessage) -> Task<Message> {
    let idx = app.active_tab_index;
    let Some(state) = docs_state_mut(&mut app.tabs, idx) else {
        return Task::none();
    };
    let collection_id = state.collection_id;

    match message {
        DocsMessage::ShowContextMenu(target, value) => {
            super::overlays::show_text_field_context_menu(app, target, value)
        }
        DocsMessage::EditorAction(action) => {
            if state.doc.perform(action) {
                let text = state.doc.text();
                let target = state.target.clone();
                if let Some(col) = app.collections.iter_mut().find(|c| c.id == collection_id) {
                    docs::set_description(col, &target, &text);
                }
            }
            Task::none()
        }
        DocsMessage::ModeSelected(mode) => {
            state.doc.mode = mode;
            if mode == DocsMode::Generated {
                regenerate(app, idx);
            }
            Task::none()
        }
        DocsMessage::OptionsChanged(options) => {
            state.options = options;
            regenerate(app, idx);
            Task::none()
        }
        DocsMessage::LinkClicked(uri) => link_clicked(uri),
        DocsMessage::CopyMarkdown => {
            let Some(markdown) = regenerate(app, idx) else {
                return Task::none();
            };
            Task::batch([
                iced::clipboard::write(markdown),
                Task::done(Message::ShowToast(
                    "Docs copied to clipboard".to_string(),
                    ToastStatus::Success,
                )),
            ])
        }
        DocsMessage::ExportMarkdown => {
            let target = state.target.clone();
            let Some(markdown) = regenerate(app, idx) else {
                return Task::none();
            };
            let name = app
                .collections
                .iter()
                .find(|c| c.id == collection_id)
                .and_then(|c| docs::target_name(c, &target))
                .unwrap_or_else(|| "docs".to_string());
            export(name, markdown)
        }
    }
}

/// the rendered preview can't navigate, so in-page anchors (table of
/// contents) are ignored and external links are copied instead.
pub fn link_clicked(uri: String) -> Task<Message> {
    if uri.starts_with('#') {
        return Task::none();
    }
    Task::batch([
        iced::clipboard::write(uri.clone()),
        Task::done(Message::ShowToast(
            format!("Link copied: {uri}"),
            ToastStatus::Success,
        )),
    ])
}

/// pulls unsaved edits from open request tabs into the collection, rebuilds
/// the generated docs for tab `idx`, and returns them.
fn regenerate(app: &mut Rustrest, idx: usize) -> Option<String> {
    let collection_id = docs_state_mut(&mut app.tabs, idx)?.collection_id;
    app.sync_collection_tabs(collection_id);
    let collection = app.collections.iter().find(|c| c.id == collection_id)?;
    let state = docs_state_mut(&mut app.tabs, idx)?;
    state.regenerate(collection);
    Some(state.generated_markdown().to_string())
}

fn export(name: String, markdown: String) -> Task<Message> {
    let default_name = format!(
        "{}.md",
        rustrest_core::collection::dir_format::sanitize_name(&name)
    );
    Task::perform(
        async move {
            let handle = rfd::AsyncFileDialog::new()
                .set_title("Export Documentation")
                .set_file_name(&default_name)
                .add_filter("Markdown (*.md)", &["md", "markdown"])
                .save_file()
                .await?;
            let path = handle.path().to_path_buf();
            Some(tokio::fs::write(&path, markdown).await.map(|_| path))
        },
        |result| match result {
            Some(Ok(path)) => {
                Message::ShowToast(format!("Docs exported to {:?}", path), ToastStatus::Success)
            }
            Some(Err(err)) => {
                Message::ShowToast(format!("Export failed: {err}"), ToastStatus::Error)
            }
            None => Message::None,
        },
    )
}
