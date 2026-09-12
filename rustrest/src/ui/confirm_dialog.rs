use crate::message::Message;
use crate::ui::modal::card;
use iced::widget::{button, column, container, row, text};
use iced::{Color, Font, Length};

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
        .color(Color::from_rgb(0.8, 0.8, 0.82));

    let cancel_btn = button(text("Cancel").size(14))
        .on_press(Message::ConfirmDialogCancelled)
        .padding([8, 16])
        .style(button::secondary);

    let confirm_btn = button(text(state.confirm_label.clone()).size(14))
        .on_press(Message::ConfirmDialogAccepted)
        .padding([8, 16])
        .style(button::danger);

    let footer = row![cancel_btn, confirm_btn]
        .spacing(10)
        .width(Length::Fill);

    let body = column![
        title,
        body_text,
        container(footer)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(18)
    .padding(24);

    card(body, 420.0)
}
