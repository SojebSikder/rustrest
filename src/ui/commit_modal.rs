use crate::collection::git_ops::GitFileEntry;
use crate::message::Message;
use crate::ui::context_menu::FieldTarget;
use crate::ui::git_panel::status_badge;
use crate::ui::modal::card;
use crate::ui::multiline_input::multiline_input;
use iced::widget::{button, column, container, row, scrollable, text, text_editor};
use iced::{Alignment, Color, Element, Font, Length};

#[derive(Debug, Clone)]
pub struct CommitModalState {
    pub collection_id: usize,
    pub collection_name: String,
    pub message: text_editor::Content,
    pub files: Vec<GitFileEntry>,
}

pub fn view_commit_modal(state: &CommitModalState) -> Element<'_, Message> {
    let title = text(format!("Commit changes - {}", state.collection_name))
        .size(18)
        .font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::DEFAULT
        });

    let summary = text(format!("{} file(s) changed", state.files.len()))
        .size(12)
        .color(Color::from_rgb(0.55, 0.55, 0.6));

    let mut file_list = column![].spacing(2);
    for entry in &state.files {
        let path_str = entry.path.display().to_string();
        file_list = file_list.push(
            row![
                status_badge(entry.status),
                text(path_str).size(13).font(Font::MONOSPACE)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }
    let files_pane =
        scrollable(container(file_list).width(Length::Fill)).height(Length::Fixed(140.0));

    let message_label = text("Commit message")
        .size(13)
        .color(Color::from_rgb(0.55, 0.55, 0.6));
    let message_input = multiline_input(
        "e.g. Update login request",
        &state.message,
        10,
        200.0,
        Message::CommitMessageChanged,
        Message::ShowTextFieldContextMenu(FieldTarget::CommitMessage, state.message.text()),
    );

    let cancel_btn = button(text("Cancel").size(14))
        .on_press(Message::CommitCancelled)
        .padding([8, 16])
        .style(button::secondary);

    let commit_btn = button(text("Commit").size(14))
        .on_press_maybe((!state.message.text().trim().is_empty()).then_some(Message::CommitConfirmed))
        .padding([8, 16])
        .style(button::primary);

    let footer = row![cancel_btn, commit_btn].spacing(10).width(Length::Fill);

    let body = column![
        title,
        summary,
        files_pane,
        column![message_label, message_input].spacing(6),
        container(footer)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(16)
    .padding(24);

    card(body, 460.0)
}
