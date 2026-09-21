#![windows_subsystem = "windows"]

mod app;
mod app_settings;
mod collection;
mod collection_adapter;
mod http_client;
mod message;
mod plugin_gallery;
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
use crate::ui::console_panel::{render_console_clear_bar, render_console_panel};
use crate::ui::env_editor::render_env_editor;
use crate::ui::export_plugin_picker::view_export_plugin_picker;
use crate::ui::menu::menu::{
    DropdownItem, DropdownMessage, MenuGroup, render_menu_bar, render_menu_overlay,
};
use crate::ui::menu::menu_message::MenuMessage;
use crate::ui::remote::{view_remote_config_modal, view_remote_connect_modal};
use crate::ui::resize_handle::{DividerOrientation, resize_handle};
use crate::ui::response_timing_modal::view_response_timing_modal;
use crate::ui::save_request_model::save_request_model::view_save_request_modal;
use crate::ui::settings::view_settings_modal;
use crate::ui::tooltip::with_tooltip;
use app::Rustrest;
use iced::futures::{SinkExt, StreamExt, stream::BoxStream};
use iced::keyboard::Key;
use iced::keyboard::key::Named;
use iced::widget::{Space, button, column, container, row, stack, text, tooltip};
use iced::window;
use iced::{Alignment, Element, Length, Padding};
use iced::{Event, Subscription, event};
use message::{Message, MultilineFieldKind, ResizeKind};
use self_update::cargo_crate_version;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedReceiver;

// #[global_allocator]
// static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const APP_NAME: &str = "Rustrest";
const APP_VERSION: &str = cargo_crate_version!();

const APP_ICON: &[u8] = include_bytes!("../../assets/images/logo-transparent.png");

pub fn main() -> iced::Result {
    // a daemon (rather than a single-window `application`) so `app::init` can
    // open the main window itself and construct `Rustrest` with a real window
    // id up front, since a daemon doesn't open one automatically.
    iced::daemon(app::init, app::update, view)
        .title(title)
        .theme(theme)
        .subscription(subscription)
        .run()
}

fn theme(app: &Rustrest, _window_id: window::Id) -> iced::Theme {
    app.settings.theme.to_iced()
}

fn title(_app: &Rustrest, _window_id: window::Id) -> String {
    format!("{} - API Testing Platform", APP_NAME)
}

/// which overlay (if any) is currently topmost, in the same order they are
/// pushed onto `main_interface_stack` in `view()`.
enum ActiveOverlay {
    EnvEditor,
    SaveRequest,
    Commit,
    ConfirmDialog,
    Settings,
    ExportPicker,
    RemoteConfig,
    RemoteConnect,
    CommandPalette,
    ResponseTiming,
}

macro_rules! outside_click_sub {
    ($message:expr) => {
        event::listen_with(|event, status, _window| match event {
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left))
                if status == iced::event::Status::Ignored =>
            {
                Some($message)
            }
            _ => None,
        })
    };
}

macro_rules! escape_close_sub {
    ($message:expr) => {
        event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: Key::Named(Named::Escape),
                ..
            }) => Some($message),
            _ => None,
        })
    };
}

pub fn subscription(app: &Rustrest) -> Subscription<Message> {
    let context_menu_sub = if app.overlays.active_context_menu.is_some() {
        event::listen_with(|event, _status, _window| match event {
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::CloseContextMenu)
            }
            _ => None,
        })
    } else {
        Subscription::none()
    };

    let active_overlay = if app.overlays.command_palette.is_some() {
        Some(ActiveOverlay::CommandPalette)
    } else if app.overlays.confirm_dialog.is_some() {
        Some(ActiveOverlay::ConfirmDialog)
    } else if app.remote.remote_connect_pending.is_some() {
        Some(ActiveOverlay::RemoteConnect)
    } else if app.remote.remote_config_open {
        Some(ActiveOverlay::RemoteConfig)
    } else if app.plugins.export_plugin_picker.is_some() {
        Some(ActiveOverlay::ExportPicker)
    } else if app.settings.settings_open {
        Some(ActiveOverlay::Settings)
    } else if app.git.commit_modal.is_some() {
        Some(ActiveOverlay::Commit)
    } else if app.overlays.response_timing_modal.is_some() {
        Some(ActiveOverlay::ResponseTiming)
    } else if app.workbench.save_request_model.is_some() {
        Some(ActiveOverlay::SaveRequest)
    } else if app.env.editing_env_index.is_some() {
        Some(ActiveOverlay::EnvEditor)
    } else {
        None
    };

    let click_outside_sub = if app.settings.close_on_outside_click {
        match active_overlay {
            Some(ActiveOverlay::EnvEditor) => {
                outside_click_sub!(Message::CloseEnvEditorPressed)
            }
            Some(ActiveOverlay::SaveRequest) => {
                outside_click_sub!(Message::CloseSaveRequestModal)
            }
            Some(ActiveOverlay::Commit) => outside_click_sub!(Message::CommitCancelled),
            Some(ActiveOverlay::ResponseTiming) => {
                outside_click_sub!(Message::CloseResponseTimingModal)
            }
            Some(ActiveOverlay::ConfirmDialog) => {
                outside_click_sub!(Message::ConfirmDialogCancelled)
            }
            Some(ActiveOverlay::Settings) => outside_click_sub!(Message::CloseSettingsPressed),
            Some(ActiveOverlay::ExportPicker) => {
                outside_click_sub!(Message::CloseExportPluginPicker)
            }
            Some(ActiveOverlay::RemoteConnect) => {
                outside_click_sub!(Message::RemoteConnectCancelled)
            }
            Some(ActiveOverlay::RemoteConfig) => {
                outside_click_sub!(Message::CloseRemoteConfigPressed)
            }
            Some(ActiveOverlay::CommandPalette) => {
                outside_click_sub!(Message::CommandPaletteClosed)
            }
            None => Subscription::none(),
        }
    } else {
        Subscription::none()
    };

    // Escape clears the sidebar multi-selection, but only when no overlay is
    // open (an open overlay's own Escape handling below takes priority).
    let sidebar_selection_escape_sub = match active_overlay {
        None if !app.sidebar.selected_sidebar_items.is_empty() => {
            escape_close_sub!(Message::ClearSidebarSelection)
        }
        _ => Subscription::none(),
    };

    // Escape closes whichever overlay is topmost, using the same priority
    // as `click_outside_sub` above - independent of `close_on_outside_click`,
    // since that setting only governs the click-outside behavior.
    let escape_close_sub = match active_overlay {
        Some(ActiveOverlay::EnvEditor) => escape_close_sub!(Message::CloseEnvEditorPressed),
        Some(ActiveOverlay::SaveRequest) => escape_close_sub!(Message::CloseSaveRequestModal),
        Some(ActiveOverlay::Commit) => escape_close_sub!(Message::CommitCancelled),
        Some(ActiveOverlay::ResponseTiming) => {
            escape_close_sub!(Message::CloseResponseTimingModal)
        }
        Some(ActiveOverlay::ConfirmDialog) => {
            escape_close_sub!(Message::ConfirmDialogCancelled)
        }
        Some(ActiveOverlay::Settings) => escape_close_sub!(Message::CloseSettingsPressed),
        Some(ActiveOverlay::ExportPicker) => {
            escape_close_sub!(Message::CloseExportPluginPicker)
        }
        Some(ActiveOverlay::RemoteConnect) => {
            escape_close_sub!(Message::RemoteConnectCancelled)
        }
        Some(ActiveOverlay::RemoteConfig) => {
            escape_close_sub!(Message::CloseRemoteConfigPressed)
        }
        Some(ActiveOverlay::CommandPalette) => escape_close_sub!(Message::CommandPaletteClosed),
        None => Subscription::none(),
    };

    let menu_bar_sub = if app.overlays.menu_state.open_index.is_some() {
        event::listen_with(|event, status, _window| match event {
            // Only treat this as an "outside" click if no widget (e.g. a menu
            // header button switching to a different menu) already handled it;
            // otherwise this stray Close would race and clobber a same-click Toggle.
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left))
                if status == iced::event::Status::Ignored =>
            {
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

    // drives the loading-spinner animation; only runs while something is
    // actually showing a spinner, so it's not ticking (and waking the event
    // loop) all the time.
    let spinner_sub = if app.any_spinner_active() {
        iced::time::every(std::time::Duration::from_millis(80)).map(|_| Message::SpinnerTick)
    } else {
        Subscription::none()
    };

    // drains buffered stdout/stderr from any process a plugin spawned via
    // the `ExternalProcess` capability.
    // only runs while some active plugin actually declared that capability,
    // same "don't tick the event loop for nothing" rule as `spinner_sub`.
    let plugin_process_sub = if app.plugins.plugin_manager.installed().iter().any(|p| {
        p.is_active()
            && p.manifest.as_ref().is_some_and(|m| {
                m.capabilities
                    .iter()
                    .any(|c| matches!(c, rustrest_plugin_host::Capability::ExternalProcess))
            })
    }) {
        iced::time::every(std::time::Duration::from_millis(100)).map(|_| Message::PluginProcessTick)
    } else {
        Subscription::none()
    };

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

    // while the command palette is open, Up/Down move the selection; Escape
    // (handled by `escape_close_sub` above) closes it, and typing/Enter are
    // handled by its text input directly.
    let command_palette_sub = if app.overlays.command_palette.is_some() {
        event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: Key::Named(Named::ArrowUp),
                ..
            }) => Some(Message::CommandPaletteMoveSelection(-1)),
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: Key::Named(Named::ArrowDown),
                ..
            }) => Some(Message::CommandPaletteMoveSelection(1)),
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

    // tracks live modifier-key state so the sidebar view can tell a plain
    // click apart from a Ctrl/Cmd-click (toggle selection) or Shift-click
    // (range-select) without threading modifiers through every message.
    let modifiers_tracker = event::listen_with(|event, _status, _window| match event {
        Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::ModifiersChanged(modifiers))
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
    let resize_drag_sub = if app.layout.resize_drag.is_some() {
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
    let tab_drag_sub = if app.workbench.dragging_tab_index.is_some() {
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
        receiver: app.terminal.terminal_event_rx.clone(),
    };
    let terminal_sub = Subscription::run_with(terminal_events_data, terminal_events_stream);

    Subscription::batch([
        context_menu_sub,
        menu_bar_sub,
        click_outside_sub,
        escape_close_sub,
        keyboard_shortcuts,
        autosave,
        spinner_sub,
        plugin_process_sub,
        close_requested,
        cursor_tracker,
        modifiers_tracker,
        sidebar_selection_escape_sub,
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

fn view(app: &Rustrest, _window_id: window::Id) -> Element<'_, Message> {
    let menu_structure = vec![
        {
            let mut items = vec![
                DropdownItem::new("New Collection", MenuMessage::FileNew),
                DropdownItem::new("Import Collection", MenuMessage::FileOpen),
                DropdownItem::new("Import Git Folder...", MenuMessage::FileOpenGitFolder),
            ];
            for (plugin_id, format) in app.plugins.plugin_manager.import_formats() {
                items.push(DropdownItem::new(
                    format!("Import via {}", format.title),
                    MenuMessage::ImportViaPlugin(plugin_id, format.id, format.extensions),
                ));
            }
            items.push(DropdownItem::new("Exit", MenuMessage::FileExit));
            MenuGroup::new("File", items)
        },
        MenuGroup::new(
            "Go",
            vec![
                DropdownItem::new("Command Palette", MenuMessage::CommandPalette)
                    .with_shortcut("Ctrl+Shift+P"),
            ],
        ),
        {
            let mut items = vec![DropdownItem::new(
                "Manage Plugins...",
                MenuMessage::OpenPluginManager,
            )];
            for (plugin_id, item) in app.plugins.plugin_manager.menu_items() {
                items.push(DropdownItem::new(
                    item.label,
                    MenuMessage::Plugin(plugin_id, item.command_id),
                ));
            }
            MenuGroup::new("Plugins", items)
        },
        MenuGroup::new(
            "Settings",
            vec![DropdownItem::new(
                "Preferences...",
                MenuMessage::OpenSettings,
            )],
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
    let top_bar_row = row![workspace_selector, Space::new().width(Length::Fill)]
        .width(Length::Fill)
        .align_y(Alignment::Center);
    let top_bar = container(top_bar_row).width(Length::Fill).padding(Padding {
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

    let toast_layer = app.overlays.toast_manager.view(
        app.spinner_tick,
        Message::DismissToast,
        Message::ToastActionPressed,
    );

    let mut workbench_column = column![workbench]
        .spacing(10)
        .width(Length::Fill)
        .height(Length::Fill);

    if !app.layout.console_collapsed {
        let console_resize_handle = resize_handle(
            DividerOrientation::Horizontal,
            Message::ResizeDragStarted(ResizeKind::ConsolePanel),
        );
        let console_content = container(column![
            render_console_clear_bar(),
            render_console_panel(&app.layout.console_logs)
        ])
        .height(Length::Fixed(app.layout.console_panel_height))
        .width(Length::Fill)
        .padding(10)
        .style(container::bordered_box);

        workbench_column = workbench_column
            .push(console_resize_handle)
            .push(console_content);
    }

    let mut content_row = row![sidebar, sidebar_resize_handle, workbench_column]
        .spacing(10)
        .width(Length::Fill)
        .height(Length::Fill);

    if let Some((plugin_id, panel_id)) = &app.plugins.right_panel_open {
        let right_panel_resize_handle = resize_handle(
            DividerOrientation::Vertical,
            Message::ResizeDragStarted(ResizeKind::RightPanel),
        );
        let right_panel_content = container(ui::right_panel::render_right_panel(
            plugin_id,
            panel_id,
            app.plugins.right_panel_tree.as_ref(),
        ))
        .width(Length::Fixed(app.plugins.right_panel_width))
        .height(Length::Fill)
        .padding(10)
        .style(container::bordered_box);

        content_row = content_row
            .push(right_panel_resize_handle)
            .push(right_panel_content);
    }

    let right_panels = app.plugins.plugin_manager.right_panels();
    if !right_panels.is_empty() {
        content_row = content_row.push(render_right_panel_rail(app, right_panels));
    }

    let status_bar = ui::status_bar::render_status_bar(app);

    let base_layout = column![top_bar, content_row, status_bar]
        .spacing(8)
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
    if let Some(commit_modal) = app.git.commit_modal.as_ref() {
        let commit_overlay = container(view_commit_modal(
            commit_modal,
            app.spinner_tick,
            app.layout
                .multiline_height(MultilineFieldKind::CommitMessage),
            Message::ResizeDragStarted(ResizeKind::MultilineField(
                MultilineFieldKind::CommitMessage,
            )),
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(commit_overlay);
    }

    // response timing modal overlay
    if let Some(timing_modal) = app.overlays.response_timing_modal.as_ref() {
        let timing_overlay = container(view_response_timing_modal(timing_modal))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(timing_overlay);
    }

    // settings modal overlay
    if app.settings.settings_open {
        let settings_overlay = container(view_settings_modal(app))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(settings_overlay);
    }

    // export-via-plugin format picker modal overlay
    if let Some(picker) = view_export_plugin_picker(app) {
        let export_picker_overlay = container(picker)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(export_picker_overlay);
    }

    // remote development (SSH) configuration modal overlay
    if app.remote.remote_config_open {
        let remote_config_overlay = container(view_remote_config_modal(app))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(remote_config_overlay);
    }

    // remote-connect (password/passphrase) modal overlay
    if let Some(pending) = app.remote.remote_connect_pending.as_ref() {
        let remote_connect_overlay = container(view_remote_connect_modal(pending))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(remote_connect_overlay);
    }

    // generic confirm-dialog overlay - rendered last among the blocking
    // modals above so it's always on top, since any of them can trigger one
    // (e.g. plugin uninstall confirmation over the "Manage Plugins" modal).
    if let Some(confirm_dialog) = app.overlays.confirm_dialog.as_ref() {
        let confirm_overlay = container(view_confirm_dialog(confirm_dialog))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(confirm_overlay);
    }

    // menu bar layer
    main_interface_stack = main_interface_stack.push(menu_strip);

    // dropdown menu overlay
    if let Some(overlay) = render_menu_overlay(&app.overlays.menu_state, &menu_structure) {
        main_interface_stack = main_interface_stack.push(overlay.map(Message::MenuInteraction));
    }

    // sidebar item context menu overlay
    if let Some(overlay) = ui::sidebar::render_context_menu_overlay(app) {
        main_interface_stack = main_interface_stack.push(overlay);
    }

    // command palette overlay (Ctrl+Shift+P)
    if let Some(palette_state) = app.overlays.command_palette.as_ref() {
        let palette_overlay = container(view_command_palette(app, palette_state))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        main_interface_stack = main_interface_stack.push(palette_overlay);
    }

    stack![main_interface_stack, toast_layer].into()
}

fn render_right_panel_rail(
    app: &Rustrest,
    panels: Vec<(String, rustrest_plugin_host::PanelDef)>,
) -> Element<'_, Message> {
    let mut rail = column![].spacing(6).width(Length::Shrink);

    for (plugin_id, panel) in panels {
        let is_open =
            app.plugins.right_panel_open.as_ref() == Some(&(plugin_id.clone(), panel.id.clone()));
        let initial = panel
            .title
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string());

        let icon_button = button(text(initial).size(14))
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(32.0))
            .on_press(Message::ToggleRightPanel(plugin_id, panel.id))
            .style(if is_open {
                button::primary
            } else {
                button::secondary
            });

        rail = rail.push(with_tooltip(
            icon_button,
            panel.title,
            tooltip::Position::Left,
        ));
    }

    container(rail).padding(Padding::from([8.0, 0.0])).into()
}
