//! Cross-cutting UI chrome that can float above the main workbench: toasts,
//! the top menu bar, the command palette, the generic confirm dialog, the
//! right-click/paste context menu, the response-timing and about modals, and
//! the self-update flow.

use super::Rustrest;
use crate::message::{Message, SidebarItemKey};
use crate::ui::confirm_dialog::ConfirmDialogState;
use crate::ui::context_menu::{ContextMenu, FieldTarget, apply_field_paste};
use crate::ui::menu::menu::DropdownMenuState;
use crate::ui::menu::menu_message::MenuMessage;
use crate::ui::toast::toast::{ToastManager, ToastStatus};
use crate::updater::{self, UpdateInfo};
use iced::Task;

#[derive(Default)]
pub struct OverlaysState {
    pub toast_manager: ToastManager,
    pub menu_state: DropdownMenuState,
    pub available_update: Option<UpdateInfo>,
    pub update_toast_id: Option<usize>,
    pub download_toast_id: Option<usize>,
    pub response_timing_modal: Option<crate::ui::response_timing_modal::ResponseTimingModalState>,
    pub about_modal: Option<crate::ui::about_modal::AboutModalState>,
    pub confirm_dialog: Option<ConfirmDialogState>,
    pub active_context_menu: Option<ContextMenu>,
    pub context_menu_position: iced::Point,
    pub command_palette: Option<rustrest_command_palette::PaletteState>,
}

/// if `key` is one of 2+ currently multi-selected sidebar rows, returns the
/// batch `MultiSelection` context menu instead of the single-item one.
fn context_menu_for_sidebar_item(
    app: &Rustrest,
    key: SidebarItemKey,
    default: ContextMenu,
) -> ContextMenu {
    if app.sidebar.selected_sidebar_items.len() > 1
        && app.sidebar.selected_sidebar_items.contains(&key)
    {
        ContextMenu::MultiSelection(app.sidebar.selected_sidebar_items.iter().cloned().collect())
    } else {
        default
    }
}

pub fn show_confirm_dialog(app: &mut Rustrest, state: ConfirmDialogState) -> Task<Message> {
    app.overlays.confirm_dialog = Some(state);
    Task::none()
}

pub fn confirm_dialog_accepted(app: &mut Rustrest) -> Task<Message> {
    if let Some(state) = app.overlays.confirm_dialog.take() {
        return Task::done(*state.on_confirm);
    }
    Task::none()
}

pub fn confirm_dialog_cancelled(app: &mut Rustrest) -> Task<Message> {
    app.overlays.confirm_dialog = None;
    Task::none()
}

pub fn show_collection_context_menu(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    let key = SidebarItemKey::Collection(col_id);
    app.overlays.active_context_menu = Some(context_menu_for_sidebar_item(
        app,
        key,
        ContextMenu::Collection(col_id),
    ));
    app.overlays.context_menu_position = app.cursor_position;
    Task::none()
}

pub fn show_git_actions_menu(app: &mut Rustrest, col_id: usize) -> Task<Message> {
    app.overlays.active_context_menu = Some(ContextMenu::GitActions(col_id));
    app.overlays.context_menu_position = app.cursor_position;
    Task::none()
}

pub fn show_new_tab_menu(app: &mut Rustrest) -> Task<Message> {
    app.overlays.active_context_menu = Some(ContextMenu::NewTab);
    app.overlays.context_menu_position = app.cursor_position;
    Task::none()
}

pub fn show_folder_context_menu(
    app: &mut Rustrest,
    collection_id: usize,
    folder_path: Vec<String>,
) -> Task<Message> {
    let key = SidebarItemKey::Folder {
        collection_id,
        path: folder_path.clone(),
    };
    app.overlays.active_context_menu = Some(context_menu_for_sidebar_item(
        app,
        key,
        ContextMenu::Folder {
            col_id: collection_id,
            path: folder_path,
        },
    ));
    app.overlays.context_menu_position = app.cursor_position;
    Task::none()
}

pub fn show_request_context_menu(
    app: &mut Rustrest,
    collection_id: usize,
    folder_path: Vec<String>,
    request_id: usize,
) -> Task<Message> {
    let key = SidebarItemKey::Request {
        collection_id,
        parent_path: folder_path.clone(),
        request_id,
    };
    app.overlays.active_context_menu = Some(context_menu_for_sidebar_item(
        app,
        key,
        ContextMenu::Request {
            col_id: collection_id,
            folder_path,
            req_id: request_id,
        },
    ));
    app.overlays.context_menu_position = app.cursor_position;
    Task::none()
}

pub fn show_plugin_text_context_menu(app: &mut Rustrest, text: String) -> Task<Message> {
    app.overlays.active_context_menu = Some(ContextMenu::PluginText(text));
    app.overlays.context_menu_position = app.cursor_position;
    Task::none()
}

pub fn close_context_menu(app: &mut Rustrest) -> Task<Message> {
    app.overlays.active_context_menu = None;
    Task::none()
}

pub fn show_text_field_context_menu(
    app: &mut Rustrest,
    target: FieldTarget,
    current_value: String,
) -> Task<Message> {
    app.overlays.active_context_menu = Some(ContextMenu::TextField {
        target,
        current_value,
    });
    app.overlays.context_menu_position = app.cursor_position;
    Task::none()
}

pub fn copy_to_clipboard(app: &mut Rustrest, text: String) -> Task<Message> {
    app.overlays.active_context_menu = None;
    iced::clipboard::write(text)
}

pub fn paste_into_field(app: &mut Rustrest, target: FieldTarget) -> Task<Message> {
    app.overlays.active_context_menu = None;
    iced::clipboard::read()
        .map(move |clipboard_text| Message::TextFieldPasteResolved(target.clone(), clipboard_text))
}

pub fn cut_from_field(app: &mut Rustrest, target: FieldTarget, text: String) -> Task<Message> {
    app.overlays.active_context_menu = None;
    let delete = crate::ui::context_menu::docs_editor_action(
        &target,
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Delete),
    )
    .map(|message| super::update(app, message))
    .unwrap_or_else(Task::none);
    Task::batch([iced::clipboard::write(text), delete])
}

pub fn text_field_paste_resolved(
    app: &mut Rustrest,
    target: FieldTarget,
    clipboard_text: Option<String>,
) -> Task<Message> {
    if let Some(text) = clipboard_text {
        apply_field_paste(app, target, text);
    }
    Task::none()
}

pub fn close_response_timing_modal(app: &mut Rustrest) -> Task<Message> {
    app.overlays.response_timing_modal = None;
    Task::none()
}

pub fn show_about_modal(app: &mut Rustrest) -> Task<Message> {
    app.overlays.about_modal = Some(crate::ui::about_modal::AboutModalState::new());
    Task::none()
}

pub fn close_about_modal(app: &mut Rustrest) -> Task<Message> {
    app.overlays.about_modal = None;
    Task::none()
}

/// selection/navigation only - the about text is read-only
pub fn about_modal_action(
    app: &mut Rustrest,
    action: iced::widget::text_editor::Action,
) -> Task<Message> {
    if let Some(modal) = app.overlays.about_modal.as_mut()
        && !action.is_edit()
    {
        modal.content.perform(action);
    }
    Task::none()
}

pub fn show_toast(app: &mut Rustrest, msg: String, status: ToastStatus) -> Task<Message> {
    crate::ui::toast::toast::show_and_schedule(
        &mut app.overlays.toast_manager,
        msg,
        status,
        crate::ui::toast::toast::TOAST_DURATION,
    )
}

pub fn dismiss_toast(app: &mut Rustrest, id: usize) -> Task<Message> {
    app.overlays.toast_manager.dismiss(id);
    Task::none()
}

pub fn menu_interaction(
    app: &mut Rustrest,
    dropdown_msg: crate::ui::menu::menu::DropdownMessage<MenuMessage>,
) -> Task<Message> {
    if let Some(menu_action) = app.overlays.menu_state.update(dropdown_msg) {
        match menu_action {
            MenuMessage::FileNew => {
                return super::update(app, Message::CreateNewCollectionPressed);
            }
            MenuMessage::FileOpen => {
                return super::update(app, Message::ImportCollectionPressed);
            }
            MenuMessage::FileOpenGitFolder => {
                return super::update(app, Message::ImportGitCollectionPressed);
            }
            MenuMessage::FileExit => {
                return super::update(app, Message::AppExit);
            }
            MenuMessage::CommandPalette => {
                return super::update(app, Message::ToggleCommandPalette);
            }
            MenuMessage::CheckForUpdate => {
                return super::update(app, Message::CheckForUpdate);
            }
            MenuMessage::HelpAbout => {
                return super::update(app, Message::ShowAboutModal);
            }
            MenuMessage::OpenPluginManager => {
                return super::update(app, Message::OpenPluginManagerPressed);
            }
            MenuMessage::OpenSettings => {
                return super::update(app, Message::OpenSettingsPressed);
            }
            MenuMessage::Plugin(plugin_id, command_id) => {
                return super::update(app, Message::PluginCommand(plugin_id, command_id));
            }
            MenuMessage::ImportViaPlugin(plugin_id, format_id, extensions) => {
                return super::update(
                    app,
                    Message::ImportCollectionViaPluginPressed(plugin_id, format_id, extensions),
                );
            }
        }
    }
    Task::none()
}

pub fn toggle_command_palette(app: &mut Rustrest) -> Task<Message> {
    if app.overlays.command_palette.take().is_some() {
        Task::none()
    } else {
        app.overlays.command_palette = Some(rustrest_command_palette::PaletteState::new());
        iced::widget::operation::focus(crate::ui::command_palette::input_id())
    }
}

pub fn command_palette_query_changed(app: &mut Rustrest, query: String) -> Task<Message> {
    if let Some(state) = app.overlays.command_palette.as_mut() {
        state.query = query;
        state.selected = 0;
    }
    Task::none()
}

pub fn command_palette_move_selection(app: &mut Rustrest, delta: i32) -> Task<Message> {
    if let Some(mut state) = app.overlays.command_palette.take() {
        let len = crate::ui::command_palette::matches_for(app, &state).len();
        state.move_selection(delta, len);
        app.overlays.command_palette = Some(state);
    }
    Task::none()
}

pub fn command_palette_confirm(app: &mut Rustrest) -> Task<Message> {
    let Some(state) = app.overlays.command_palette.take() else {
        return Task::none();
    };
    let matches = crate::ui::command_palette::matches_for(app, &state);
    match matches.get(state.selected) {
        Some(cmd) => {
            let action = cmd.action.clone();
            super::update(app, crate::ui::command_palette::to_message(action))
        }
        None => Task::none(),
    }
}

pub fn command_palette_closed(app: &mut Rustrest) -> Task<Message> {
    app.overlays.command_palette = None;
    Task::none()
}

pub fn command_palette_item_clicked(
    app: &mut Rustrest,
    action: crate::ui::command_palette::AppCommand,
) -> Task<Message> {
    app.overlays.command_palette = None;
    super::update(app, crate::ui::command_palette::to_message(action))
}

pub fn check_for_update(app: &mut Rustrest) -> Task<Message> {
    let (_, toast_task) = crate::ui::toast::toast::show_pending_and_schedule(
        &mut app.overlays.toast_manager,
        "Checking for updates…".to_string(),
        crate::ui::toast::toast::TOAST_DURATION,
    );
    let check_task = iced::Task::perform(
        async {
            tokio::task::spawn_blocking(updater::check_for_update)
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
        },
        Message::UpdateCheckResult,
    );
    iced::Task::batch([toast_task, check_task])
}

pub fn check_for_update_silently() -> Task<Message> {
    iced::Task::perform(
        async {
            tokio::task::spawn_blocking(updater::check_for_update)
                .await
                .unwrap_or_else(|e| Err(e.to_string()))
        },
        Message::SilentUpdateCheckResult,
    )
}

pub fn silent_update_check_result(
    app: &mut Rustrest,
    result: Result<Option<UpdateInfo>, String>,
) -> Task<Message> {
    match result {
        Ok(Some(info)) => super::update(app, Message::UpdateCheckResult(Ok(Some(info)))),
        _ => Task::none(),
    }
}

pub fn update_check_result(
    app: &mut Rustrest,
    result: Result<Option<UpdateInfo>, String>,
) -> Task<Message> {
    match result {
        Ok(Some(info)) => {
            let msg = format!("Update available: v{}", info.version);
            show_update_status_item(app, &info);
            app.overlays.available_update = Some(info);
            let (id, task) = crate::ui::toast::toast::show_with_action_and_schedule(
                &mut app.overlays.toast_manager,
                msg,
                ToastStatus::Info,
                crate::ui::toast::toast::TOAST_DURATION_LONG,
                "Update",
            );
            app.overlays.update_toast_id = Some(id);
            task
        }
        Ok(None) => crate::ui::toast::toast::show_and_schedule(
            &mut app.overlays.toast_manager,
            "You're up to date.".to_string(),
            ToastStatus::Info,
            crate::ui::toast::toast::TOAST_DURATION,
        ),
        Err(e) => crate::ui::toast::toast::show_and_schedule(
            &mut app.overlays.toast_manager,
            format!("Update check failed: {e}"),
            ToastStatus::Error,
            crate::ui::toast::toast::TOAST_DURATION,
        ),
    }
}

pub fn toast_action_pressed(app: &mut Rustrest, id: usize) -> Task<Message> {
    if app.overlays.update_toast_id == Some(id) {
        app.overlays.update_toast_id = None;
        app.overlays.toast_manager.dismiss(id);
        return super::update(app, Message::InstallUpdate);
    }
    Task::none()
}

/// status bar id for the persistent "update available" button
const UPDATE_STATUS_ID: &str = "update-available";

fn show_update_status_item(app: &mut Rustrest, info: &UpdateInfo) {
    app.status_bar.set_with_action(
        UPDATE_STATUS_ID,
        format!("Update to v{}", info.version),
        false,
        Some(Message::InstallUpdate),
    );
}

pub fn install_update(app: &mut Rustrest) -> Task<Message> {
    // already downloading (e.g. clicked both the toast and the status bar)
    if app.overlays.download_toast_id.is_some() {
        return Task::none();
    }
    app.status_bar.clear(UPDATE_STATUS_ID);
    if let Some(id) = app.overlays.update_toast_id.take() {
        app.overlays.toast_manager.dismiss(id);
    }
    let id = crate::ui::toast::toast::show_sticky_pending(
        &mut app.overlays.toast_manager,
        "Downloading update…".to_string(),
    );
    app.overlays.download_toast_id = Some(id);
    app.overlays
        .toast_manager
        .set_download_progress(id, "Downloading update… 0%", 0.0);

    let download = iced::task::sipper::<updater::UpdateProgress, _>(move |mut sender| async move {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = tokio::spawn(async move {
            updater::perform_update_with_progress(move |progress| {
                let _ = tx.send(progress);
            })
            .await
        });

        while let Some(progress) = rx.recv().await {
            sender.send(progress).await;
        }

        handle.await.unwrap_or_else(|e| Err(e.to_string()))
    });

    iced::Task::sip(
        download,
        Message::UpdateInstallProgress,
        Message::UpdateInstallResult,
    )
}

pub fn update_install_progress(
    app: &mut Rustrest,
    progress: updater::UpdateProgress,
) -> Task<Message> {
    let Some(id) = app.overlays.download_toast_id else {
        return Task::none();
    };
    let fraction = if progress.total > 0 {
        progress.downloaded as f32 / progress.total as f32
    } else {
        0.0
    };
    let message = if progress.total > 0 {
        format!("Downloading update… {}%", (fraction * 100.0).round() as u32)
    } else {
        "Downloading update…".to_string()
    };
    app.overlays
        .toast_manager
        .set_download_progress(id, message, fraction);
    Task::none()
}

pub fn update_install_result(app: &mut Rustrest, result: Result<String, String>) -> Task<Message> {
    if let Some(id) = app.overlays.download_toast_id.take() {
        app.overlays.toast_manager.dismiss(id);
    }
    match result {
        Ok(version) => crate::ui::toast::toast::show_sticky(
            &mut app.overlays.toast_manager,
            format!("Updated to v{version}. Please restart the app."),
            ToastStatus::Success,
        ),
        Err(e) => {
            // put the button back so the user can retry
            if let Some(info) = app.overlays.available_update.clone() {
                show_update_status_item(app, &info);
            }
            crate::ui::toast::toast::show_and_schedule(
                &mut app.overlays.toast_manager,
                format!("Update failed: {e}"),
                ToastStatus::Error,
                crate::ui::toast::toast::TOAST_DURATION,
            )
        }
    }
}
