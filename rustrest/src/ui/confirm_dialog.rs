use crate::message::Message;
use crate::ui::modal::{card, muted_text_color};
use crate::ui::tokens::{
    SPACE_MD, SPACE_XL, TEXT_BASE, TEXT_TITLE, danger_button, secondary_button, semibold,
};
use iced::widget::{Space, button, column, row, text};
use iced::{Alignment, Length, Theme};

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
    let title = text(state.title.clone()).size(TEXT_TITLE).font(semibold());

    let body_text = text(state.message.clone())
        .size(TEXT_BASE)
        .style(|theme: &Theme| text::Style {
            color: Some(muted_text_color(theme)),
        });

    let cancel_btn = button(text("Cancel").size(TEXT_BASE))
        .on_press(Message::ConfirmDialogCancelled)
        .padding([7, 16])
        .style(secondary_button);

    let confirm_btn = button(text(state.confirm_label.clone()).size(TEXT_BASE))
        .on_press(Message::ConfirmDialogAccepted)
        .padding([7, 18])
        .style(danger_button);

    let footer = row![Space::new().width(Length::Fill), cancel_btn, confirm_btn]
        .spacing(SPACE_MD)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    let body = column![title, body_text, footer,]
        .spacing(SPACE_XL)
        .padding(24);

    card(body, 420.0)
}
