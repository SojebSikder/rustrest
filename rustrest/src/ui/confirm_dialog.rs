use crate::message::Message;
use crate::ui::modal::{card, muted_text_color};
use iced::widget::{Space, button, column, row, text};
use iced::{Alignment, Font, Length, Theme};

/// generic yes/no confirmation dialog. `on_confirm` is dispatched (and the dialog closed)
/// when the user accepts; cancelling just clears the dialog.
#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub on_confirm: Box<Message>,
}

pub fn view_confirm_dialog(state: &ConfirmDialogState) -> iced::Element<'static, Message> {
    let title = text(state.title.clone()).size(18).font(Font {
        weight: iced::font::Weight::Bold,
        ..Font::DEFAULT
    });

    let body_text = text(state.message.clone())
        .size(14)
        .style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        });

    let cancel_btn = button(text("Cancel").size(13))
        .on_press(Message::ConfirmDialogCancelled)
        .padding([7, 16])
        .style(button::secondary);

    let confirm_btn = button(text(state.confirm_label.clone()).size(13))
        .on_press(Message::ConfirmDialogAccepted)
        .padding([7, 18])
        .style(button::danger);

    let footer = row![Space::new().width(Length::Fill), cancel_btn, confirm_btn]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    let body = column![title, body_text, footer,].spacing(18).padding(24);

    card(body, 420.0)
}
