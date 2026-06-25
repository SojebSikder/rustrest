pub mod components;
pub mod messages;
mod state;
pub mod types;
mod views;

pub use state::Tab;

use iced::widget::{column, container};
use iced::{Element, Length};
pub use messages::TabMessage;

impl Tab {
    pub fn view<Message>(
        &self,
        wrap_msg: impl Fn(TabMessage) -> Message + Copy + 'static,
        on_send: Message,
    ) -> Element<'_, Message>
    where
        Message: Clone + 'static,
    {
        let request_bar = views::request::render_request_bar(self, wrap_msg, on_send);
        let configuration_pane = views::request::render_configuration_pane(self, wrap_msg);
        let response_content = views::response::render_response_pane(self, wrap_msg);

        column![
            request_bar,
            configuration_pane,
            container(response_content)
                .width(Length::Fill)
                .padding(15)
                .style(container::bordered_box)
        ]
        .spacing(18)
        .into()
    }
}
