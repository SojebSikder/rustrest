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
use iced::{Element, Length, Size};
use iced::{Event, Subscription, event};
use message::Message;

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
    let sidebar = ui::sidebar::render_sidebar(app);
    let workbench = ui::workspace::render_workbench(app);
    let toast_layer = app.toast_manager.view(|id| Message::DismissToast(id));

    let base_layout = row![sidebar, workbench]
        .spacing(15)
        .padding(15)
        .width(Length::Fill)
        .height(Length::Fill);

    stack![base_layout, toast_layer].into()
}
