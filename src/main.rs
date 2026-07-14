#![windows_subsystem = "windows"]

mod app;
mod collection;
mod http_client;
mod message;
mod tab;
mod ui;
mod utils;
use app::Rustrest;
use iced::widget::{row, stack};
use iced::{Element, Length, Padding, Size};
use iced::{Event, Subscription, event};
use message::Message;

use crate::ui::menu::menu::{DropdownItem, MenuGroup, render_menu_bar, render_menu_overlay};
use crate::ui::menu::menu_message::MenuMessage;

const APP_NAME: &str = "Rustrest";
const APP_VERSION: &str = "0.1.0";

pub fn main() -> iced::Result {
    iced::application(app::init, app::update, view)
        .title(|_: &Rustrest| format!("{} - API Testing Platform", APP_NAME))
        .subscription(subscription)
        .window(iced::window::Settings {
            size: Size::new(1250.0, 850.0),
            ..Default::default()
        })
        .run()
}

pub fn subscription(app: &Rustrest) -> Subscription<Message> {
    // listen for mouse events only when a context menu is active
    if app.active_context_menu.is_some() {
        event::listen().map(|event| match event {
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                Message::CloseContextMenu
            }
            _ => Message::None,
        })
    } else {
        Subscription::none()
    }
}

fn view(app: &Rustrest) -> Element<'_, Message> {
    let menu_structure = vec![
        MenuGroup::new(
            "File",
            vec![
                DropdownItem::new("New Collection", MenuMessage::FileNew),
                DropdownItem::new("Import Collection", MenuMessage::FileOpen),
                DropdownItem::new("Exit", MenuMessage::FileExit),
            ],
        ),
        MenuGroup::new(
            "Help",
            vec![DropdownItem::new("About", MenuMessage::HelpAbout)],
        ),
    ];

    let menu_strip = render_menu_bar(&menu_structure).map(Message::MenuInteraction);

    let sidebar = ui::sidebar::render_sidebar(app);
    let workbench = ui::workspace::render_workbench(app);
    let toast_layer = app.toast_manager.view(|id| Message::DismissToast(id));

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

    let mut main_interface_stack = stack![base_layout, menu_strip]
        .width(Length::Fill)
        .height(Length::Fill);

    if let Some(overlay) = render_menu_overlay(&app.menu_state, &menu_structure) {
        main_interface_stack = main_interface_stack.push(overlay.map(Message::MenuInteraction));
    }

    stack![main_interface_stack, toast_layer].into()
}
