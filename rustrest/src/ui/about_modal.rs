use crate::message::Message;
use crate::ui::modal::{card, muted_text_color};
use crate::{APP_NAME, APP_VERSION};
use iced::widget::{button, column, container, row, text, text_editor};
use iced::{Element, Font, Length, Theme};

pub struct AboutModalState {
    /// read-only editor so the info can be selected and copied
    pub content: text_editor::Content,
}

impl AboutModalState {
    pub fn new() -> Self {
        Self {
            content: text_editor::Content::with_text(&about_info()),
        }
    }
}

/// the build/environment details shown in Help > About
pub fn about_info() -> String {
    format!(
        "{APP_NAME} {APP_VERSION}\n\
         Commit: {}\n\
         Plugin API: {} (manifest schema v{})\n\
         Platform: {}\n\
         Architecture: {}",
        env!("RUSTREST_COMMIT_SHA"),
        rustrest_plugin_host::PLUGIN_API_VERSION,
        rustrest_plugin_host::PLUGIN_SCHEMA_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

pub fn view_about_modal(state: &AboutModalState) -> Element<'_, Message> {
    let title = text(format!("About {APP_NAME}")).size(18).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });

    let subtitle = text("API Testing Platform")
        .size(12)
        .style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        });

    let info = text_editor(&state.content)
        .on_action(Message::AboutModalAction)
        .font(Font::MONOSPACE)
        .size(13)
        .padding(12);

    let copy_btn = button(text("Copy").size(14))
        .on_press(Message::CopyToClipboard(state.content.text()))
        .padding([8, 16])
        .style(button::primary);

    let close_btn = button(text("Close").size(14))
        .on_press(Message::CloseAboutModal)
        .padding([8, 16])
        .style(button::secondary);

    let body = column![
        title,
        subtitle,
        info,
        container(row![copy_btn, close_btn].spacing(8))
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(16)
    .padding(24);

    card(body, 480.0)
}
