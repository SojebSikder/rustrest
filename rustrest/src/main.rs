#![windows_subsystem = "windows"]

mod app;
mod collection;
mod collection_adapter;
mod http_client;
mod message;
mod remote_agent;
mod script_engine;
mod session;
mod shortcuts;
mod ui;
mod updater;
mod utils;
mod workspace;

use crate::ui::command_palette::view as view_command_palette;
use crate::ui::commit_modal::view_commit_modal;
use crate::ui::confirm_dialog::view_confirm_dialog;
use crate::ui::console_panel::{render_console_bar, render_console_panel};
use crate::ui::env_editor::render_env_editor;
use crate::ui::menu::menu::{
    DropdownItem, DropdownMessage, MenuGroup, render_menu_bar, render_menu_overlay,
};
use crate::ui::menu::menu_message::MenuMessage;
use crate::ui::remote::{view_remote_config_window, view_remote_connect_modal};
use crate::ui::resize_handle::{DividerOrientation, resize_handle};
use crate::ui::save_request_model::save_request_model::view_save_request_modal;
use app::Rustrest;
use iced::futures::{SinkExt, StreamExt, stream::BoxStream};
use iced::keyboard::Key;
use iced::keyboard::key::Named;
use iced::widget::{column, container, row, stack};
use iced::window;
use iced::{Alignment, Element, Length, Padding};
use iced::{Event, Subscription, event};
use message::{Message, ResizeKind};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedReceiver;

const APP_NAME: &str = "Rustrest";
const APP_VERSION: &str = "0.1.8";

const APP_ICON: &[u8] = include_bytes!("../../assets/images/logo-transparent.png");

pub fn main() -> iced::Result {
    // a daemon (rather than a single-window `application`) so a second,
    // independent OS window can be opened at runtime for the "Remote
    // Development over SSH" configuration screen; `app::init` opens the
    // main window itself since a daemon doesn't open one automatically.
    iced::daemon(app::init, app::update, view)
        .title(title)
        .subscription(subscription)
        .run()
}

fn title(app: &Rustrest, window_id: window::Id) -> String {
    if Some(window_id) == app.remote_config_window_id {
        "Remote Development over SSH".to_string()
    } else {
        format!("{} - API Testing Platform", APP_NAME)
    }
}

pub fn subscription(app: &Rustrest) -> Subscription<Message> {
    let context_menu_sub = if app.active_context_menu.is_some() {
        event::listen_with(|event, _status, _window| match event {
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::CloseContextMenu)
            }
            _ => None,
        })
    } else {
        Subscription::none()
    };

    let menu_bar_sub = if app.menu_state.open_index.is_some() {
        event::listen_with(|event, _status, _window| match event {
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::MenuInteraction(DropdownMessage::Close))
            }
            _ => None,
        })
    } else {
        Subscription::none()
    };

    // all Ctrl/Cmd-style keyboard shortcuts are registered in `shortcuts::bindings()`
    let keyboard_shortcuts = shortcuts::subscription();

    // periodically every 5 seconds autosave of the in-progress session (draft tabs, active tab, etc.),
    let autosave =
        iced::time::every(std::time::Duration::from_secs(5)).map(|_| Message::AutosaveTick);

    // catch the native window close button so we can flush the session
    // before the process actually exits (for the main window), or just
    // close that one window (for the secondary remote-config window),
    // instead of letting iced close/exit immediately.
    let close_requested = event::listen_with(|event, _status, window_id| match event {
        Event::Window(iced::window::Event::CloseRequested) => {
            Some(Message::WindowCloseRequested(window_id))
        }
        _ => None,
    });

    // while the command palette is open, Up/Down move the selection and
    // Escape closes it; typing and Enter are handled by its text input directly.
    let command_palette_sub = if app.command_palette.is_some() {
        event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: Key::Named(Named::ArrowUp),
                ..
            }) => Some(Message::CommandPaletteMoveSelection(-1)),
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: Key::Named(Named::ArrowDown),
                ..
            }) => Some(Message::CommandPaletteMoveSelection(1)),
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: Key::Named(Named::Escape),
                ..
            }) => Some(Message::CommandPaletteClosed),
            _ => None,
        })
    } else {
        Subscription::none()
    };

    // tracks the cursor position so context menus can be anchored where they
    // were triggered
    let cursor_tracker = event::listen_with(|event, _status, _window| match event {
        Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(Message::CursorMoved(position))
        }
        _ => None,
    });

    // if a tab name is mid-rename and the user clicks anywhere else, commit
    // and close the rename UI instead of leaving it open until Enter is hit
    let tab_rename_sub = if app.tabs.iter().any(|t| t.is_editing_name) {
        event::listen_with(|event, _status, _window| match event {
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::TabRenameBlur)
            }
            _ => None,
        })
    } else {
        Subscription::none()
    };

    // while a panel divider is being dragged, release the drag on mouse-up
    let resize_drag_sub = if app.resize_drag.is_some() {
        event::listen_with(|event, _status, _window| match event {
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::ResizeDragEnded)
            }
            _ => None,
        })
    } else {
        Subscription::none()
    };

    // while a tab is being dragged to reorder it, release the drag on mouse-up
    let tab_drag_sub = if app.dragging_tab_index.is_some() {
        event::listen_with(|event, _status, _window| match event {
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::TabDragEnded)
            }
            _ => None,
        })
    } else {
        Subscription::none()
    };

    let terminal_events_data = TerminalEventsData {
        receiver: app.terminal_event_rx.clone(),
    };
    let terminal_sub = Subscription::run_with(terminal_events_data, terminal_events_stream);

    Subscription::batch([
        context_menu_sub,
        menu_bar_sub,
        keyboard_shortcuts,
        autosave,
        close_requested,
        cursor_tracker,
        tab_rename_sub,
        resize_drag_sub,
        tab_drag_sub,
        terminal_sub,
        command_palette_sub,
    ])
}

/// wraps the shared PTY-notice receiver so `Subscription::run_with` can
/// identify a single, app-wide instance of the stream below (there is only
/// ever one, regardless of how many terminal tabs are open).
struct TerminalEventsData {
    receiver: Arc<Mutex<UnboundedReceiver<(u64, rustrest_terminal::TerminalNotice)>>>,
}

impl Hash for TerminalEventsData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "rustrest-terminal-events".hash(state);
    }
}

fn terminal_events_stream(data: &TerminalEventsData) -> BoxStream<'static, Message> {
    let receiver = data.receiver.clone();
    iced::stream::channel(100, async move |mut output| {
        loop {
            let next = {
                let mut rx = receiver.lock().await;
                rx.recv().await
            };
            match next {
                Some((id, notice)) => {
                    let _ = output.send(Message::TerminalNotice(id, notice)).await;
                }
                None => break,
            }
        }
    })
    .boxed()
}

fn view(app: &Rustrest, window_id: window::Id) -> Element<'_, Message> {
    if Some(window_id) == app.remote_config_window_id {
        return view_remote_config_window(app);
    }

    let menu_structure = vec![
        MenuGroup::new(
            "File",
            vec![
                DropdownItem::new("New Collection", MenuMessage::FileNew),
                DropdownItem::new("Import Collection", MenuMessage::FileOpen),
                DropdownItem::new("Import Git Folder...", MenuMessage::FileOpenGitFolder),
                DropdownItem::new("Exit", MenuMessage::FileExit),
            ],
        ),
        MenuGroup::new(
            "Help",
            vec![
                DropdownItem::new("Check for Updates", MenuMessage::CheckForUpdate),
                DropdownItem::new("About", MenuMessage::HelpAbout),
            ],
        ),
    ];

    let menu_strip = render_menu_bar(&menu_structure).map(Message::MenuInteraction);

    let workspace_selector = ui::sidebar::render_workspace_selector(app);
    let top_bar = container(row![workspace_selector].align_y(Alignment::Center))
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            left: 0.0,
            right: 0.0,
            bottom: 10.0,
        });

    let sidebar = ui::sidebar::render_sidebar(app);
    let sidebar_resize_handle = resize_handle(
        DividerOrientation::Vertical,
        Message::ResizeDragStarted(ResizeKind::Sidebar),
    );
    let workbench = ui::workspace::render_workbench(app);

    let toast_layer = app
        .toast_manager
        .view(Message::DismissToast, Message::ToastActionPressed);

    let console_bar = render_console_bar(&app.console_logs, app.console_collapsed);

    let mut workbench_column = column![workbench, console_bar]
        .spacing(10)
        .width(Length::Fill)
        .height(Length::Fill);

    if !app.console_collapsed {
        let console_resize_handle = resize_handle(
            DividerOrientation::Horizontal,
            Message::ResizeDragStarted(ResizeKind::ConsolePanel),
        );
        let console_content = container(render_console_panel(&app.console_logs))
            .height(Length::Fixed(app.console_panel_height))
            .width(Length::Fill)
            .padding(10)
            .style(container::bordered_box);

        workbench_column = workbench_column
            .push(console_resize_handle)
            .push(console_content);
    }

    let content_row = row![sidebar, sidebar_resize_handle, workbench_column]
        .spacing(10)
        .width(Length::Fill)
        .height(Length::Fill);

    let base_layout = column![top_bar, content_row]
        .padding(Padding {
            top: 44.0,
            left: 15.0,
            bottom: 15.0,
            right: 15.0,
        })
        .width(Length::Fill)
        .height(Length::Fill);

    let mut main_interface_stack = stack![base_layout];

    // environment editor modal overlay
    if let Some(env_modal) = render_env_editor(app) {
        let env_overlay = container(env_modal)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);

        main_interface_stack = main_interface_stack.push(env_overlay);
    }

    // save-request collection chooser modal overlay
    if let Some(save_request_modal) = view_save_request_modal(app) {
        let save_request_overlay = container(save_request_modal)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(save_request_overlay);
    }

    // commit-changes modal overlay
    if let Some(commit_modal) = app.commit_modal.as_ref() {
        let commit_overlay = container(view_commit_modal(commit_modal))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(commit_overlay);
    }

    // generic confirm-dialog overlay
    if let Some(confirm_dialog) = app.confirm_dialog.as_ref() {
        let confirm_overlay = container(view_confirm_dialog(confirm_dialog))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(confirm_overlay);
    }

    // remote-connect (password/passphrase) modal overlay
    if let Some(pending) = app.remote_connect_pending.as_ref() {
        let remote_connect_overlay = container(view_remote_connect_modal(pending))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(remote_connect_overlay);
    }

    // menu bar layer
    main_interface_stack = main_interface_stack.push(menu_strip);

    // dropdown menu overlay
    if let Some(overlay) = render_menu_overlay(&app.menu_state, &menu_structure) {
        main_interface_stack = main_interface_stack.push(overlay.map(Message::MenuInteraction));
    }

    // sidebar item context menu overlay
    if let Some(overlay) = ui::sidebar::render_context_menu_overlay(app) {
        main_interface_stack = main_interface_stack.push(overlay);
    }

    // command palette overlay (Ctrl+Shift+P)
    if let Some(palette_state) = app.command_palette.as_ref() {
        let palette_overlay = container(view_command_palette(palette_state))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(palette_overlay);
    }

    stack![main_interface_stack, toast_layer].into()
}
