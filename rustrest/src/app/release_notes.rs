//! Help > View Release Notes: opens (or focuses) a tab showing the running
//! version's GitHub release notes, rendered locally as markdown.

use super::{Rustrest, Tab, TabState, WorkspaceContent};
use crate::APP_VERSION;
use crate::message::Message;
use crate::ui::release_notes::ReleaseNotesState;
use iced::Task;

pub fn view_release_notes(app: &mut Rustrest) -> Task<Message> {
    let existing = app
        .tabs
        .iter()
        .position(|t| matches!(t.content, WorkspaceContent::ReleaseNotes(_)));

    let snap = match existing {
        Some(idx) => {
            app.active_tab_index = idx;
            app.tabs[idx].content = WorkspaceContent::ReleaseNotes(ReleaseNotesState::Loading);
            Task::none()
        }
        None => {
            let mut tab = Tab::new(app.next_tab_id);
            tab.name = "Release Notes".to_string();
            app.next_tab_id += 1;
            app.tabs.push(TabState {
                tab,
                content: WorkspaceContent::ReleaseNotes(ReleaseNotesState::Loading),
                is_editing_name: false,
            });
            app.active_tab_index = app.tabs.len() - 1;
            iced::widget::operation::snap_to_end(crate::ui::workspace::tab_bar_scroll_id())
        }
    };

    let fetch = Task::perform(
        crate::updater::fetch_release_notes(APP_VERSION),
        Message::ReleaseNotesLoaded,
    );
    Task::batch([snap, fetch])
}

/// fills the release-notes tab, if the user hasn't closed it
pub fn release_notes_loaded(app: &mut Rustrest, result: Result<String, String>) -> Task<Message> {
    if let Some(tab_state) = app
        .tabs
        .iter_mut()
        .find(|t| matches!(t.content, WorkspaceContent::ReleaseNotes(_)))
    {
        tab_state.content = WorkspaceContent::ReleaseNotes(ReleaseNotesState::from_result(result));
    }
    Task::none()
}
