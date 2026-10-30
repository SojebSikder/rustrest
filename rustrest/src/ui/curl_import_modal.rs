//! "Import cURL" modal: paste a cURL command, get a new request tab.

use crate::message::Message;
use crate::ui::modal::{card, muted_text_color};
use iced::widget::{button, column, row, text, text_editor};
use iced::{Alignment, Element, Font, Length, Theme};

#[derive(Debug, Clone)]
pub enum CurlImportMessage {
    EditorAction(text_editor::Action),
    Import,
    Close,
}

#[derive(Debug, Default)]
pub struct CurlImportModalState {
    pub command: text_editor::Content,
    /// why the last Import attempt failed, shown under the editor.
    pub error: Option<String>,
}

pub fn editor_id() -> iced::widget::Id {
    iced::widget::Id::new("curl-import-editor")
}

pub fn view_curl_import_modal(state: &CurlImportModalState) -> Element<'_, Message> {
    let wrap = Message::CurlImport;

    let title = column![
        text("Import cURL").size(18).font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::DEFAULT
        }),
        text("Paste a cURL command to open it as a new request.")
            .size(12)
            .style(|theme: &Theme| text::Style {
                color: Some(muted_text_color(theme)),
            }),
    ]
    .spacing(4);

    let editor = text_editor(&state.command)
        .id(editor_id())
        .placeholder("curl https://api.example.com/users -H 'Accept: application/json'")
        .on_action(move |action| wrap(CurlImportMessage::EditorAction(action)))
        .font(Font::MONOSPACE)
        .size(13)
        .padding(12)
        .height(Length::Fixed(260.0));

    let mut body = column![title, editor].spacing(16).padding(24);
    if let Some(error) = &state.error {
        body = body.push(text(error.clone()).size(13).style(|_: &Theme| text::Style {
            color: Some(crate::theme::colors().error),
        }));
    }

    let has_input = !state.command.text().trim().is_empty();
    let import_btn = button(text("Import").size(14))
        .on_press_maybe(has_input.then_some(wrap(CurlImportMessage::Import)))
        .padding([8, 16])
        .style(button::primary);
    let cancel_btn = button(text("Cancel").size(14))
        .on_press(wrap(CurlImportMessage::Close))
        .padding([8, 16])
        .style(button::secondary);

    body = body.push(
        row![iced::widget::space::horizontal(), cancel_btn, import_btn]
            .spacing(8)
            .align_y(Alignment::Center),
    );

    card(body, 640.0)
}
