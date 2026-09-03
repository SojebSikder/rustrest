#![windows_subsystem = "windows"]

mod app;
mod collection;
mod collection_adapter;
mod http_client;
mod message;
mod script_engine;
mod session;
mod ui;
mod updater;
mod utils;

use crate::ui::env_editor::render_env_editor;
use crate::ui::menu::menu::{
    DropdownItem, DropdownMessage, MenuGroup, render_menu_bar, render_menu_overlay,
};
use crate::ui::menu::menu_message::MenuMessage;
use crate::ui::save_request_model::save_request_model::view_save_request_modal;
use app::Rustrest;
use iced::widget::{container, row, stack};
use iced::{Alignment, Element, Length, Padding, Size};
use iced::{Event, Subscription, event};
use message::Message;

const APP_NAME: &str = "Rustrest";
const APP_VERSION: &str = "0.1.2";

pub fn main() -> iced::Result {
    iced::application(app::init, app::update, view)
        .title(|_: &Rustrest| format!("{} - API Testing Platform", APP_NAME))
        .subscription(subscription)
        .exit_on_close_request(false)
        .window(iced::window::Settings {
            size: Size::new(1250.0, 850.0),
            ..Default::default()
        })
        .run()
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

    // listen for save shortcut (Ctrl+S)
    let save_shortcut = event::listen_with(|event, _status, _window| match event {
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Character(ref c),
            modifiers,
            ..
        }) if modifiers.command() && c.as_str() == "s" => Some(Message::SaveActiveRequestShortcut),
        _ => None,
    });

    // periodically every 5 seconds autosave of the in-progress session (draft tabs, active tab, etc.),
    let autosave =
        iced::time::every(std::time::Duration::from_secs(5)).map(|_| Message::AutosaveTick);

    // catch the native window close button so we can flush the session
    // before the process actually exits, instead of letting iced exit immediately
    let close_requested = event::listen_with(|event, _status, _window| match event {
        Event::Window(iced::window::Event::CloseRequested) => Some(Message::AppExit),
        _ => None,
    });

    // tracks the cursor position so context menus can be anchored where they
    // were triggered
    let cursor_tracker = event::listen_with(|event, _status, _window| match event {
        Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
            Some(Message::CursorMoved(position))
        }
        _ => None,
    });

    Subscription::batch([
        context_menu_sub,
        menu_bar_sub,
        save_shortcut,
        autosave,
        close_requested,
        cursor_tracker,
    ])
}

fn view(app: &Rustrest) -> Element<'_, Message> {
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

    let sidebar = ui::sidebar::render_sidebar(app);
    let workbench = ui::workspace::render_workbench(app);
    // let toast_layer = app.toast_manager.view(|id| Message::DismissToast(id));
    let toast_layer = app
        .toast_manager
        .view(Message::DismissToast, Message::ToastActionPressed);

    let base_layout = row![sidebar, workbench]
        .spacing(15)
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

    stack![main_interface_stack, toast_layer].into()
}
