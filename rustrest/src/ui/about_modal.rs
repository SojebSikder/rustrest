use crate::message::Message;
use crate::ui::modal::{card, muted_text_color};
use crate::ui::tokens::{
    SPACE_LG, SPACE_MD, SPACE_XL, TEXT_BASE, TEXT_BODY, TEXT_TITLE, primary_button,
    secondary_button, semibold,
};
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
    let title = column![
        text(format!("About {APP_NAME}"))
            .size(TEXT_TITLE)
            .font(semibold()),
        text("API Testing Platform")
            .size(TEXT_BODY)
            .style(|theme: &Theme| text::Style {
                color: Some(muted_text_color(theme)),
            }),
    ]
    .spacing(2);

    let info = text_editor(&state.content)
        .on_action(Message::AboutModalAction)
        .font(Font::MONOSPACE)
        .size(TEXT_BASE)
        .padding(SPACE_LG);

    let copy_btn = button(text("Copy").size(TEXT_BASE))
        .on_press(Message::CopyToClipboard(state.content.text()))
        .padding([8, 16])
        .style(primary_button);

    let close_btn = button(text("Close").size(TEXT_BASE))
        .on_press(Message::CloseAboutModal)
        .padding([8, 16])
        .style(secondary_button);

    let body = column![
        title,
        info,
        container(row![copy_btn, close_btn].spacing(SPACE_MD))
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(SPACE_XL)
    .padding(24);

    card(body, 480.0)
}
