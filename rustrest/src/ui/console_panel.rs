use crate::message::Message;
use crate::ui::tokens::{
    RADIUS_SM, SPACE_MD, TEXT_BASE, TEXT_BODY, TEXT_SMALL, icon_button, semibold,
};
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Font, Length};

pub fn render_console_clear_bar<'a>() -> Element<'a, Message> {
    row![
        text("Console Output")
            .size(TEXT_BODY)
            .font(semibold())
            .color(crate::theme::colors().text_muted),
        Space::new().width(Length::Fill),
        button(text("Clear").size(TEXT_SMALL))
            .style(icon_button)
            .padding([2, 6])
            .on_press(Message::ClearConsoleLogs),
    ]
    .align_y(Alignment::Center)
    .padding(iced::Padding {
        top: 0.0,
        right: 4.0,
        bottom: 4.0,
        left: 4.0,
    })
    .into()
}

/// render console log panel from a list of log lines.
pub fn render_console_panel<'a, Message>(logs: &'a [String]) -> Element<'a, Message>
where
    Message: Clone + 'static,
{
    if logs.is_empty() {
        return container(
            text("No console output. Use console.log(...) in your pre-request or post-response scripts to see output here.")
                .size(TEXT_BASE)
                .color(crate::theme::colors().text_muted),
        )
        .padding(12)
        .into();
    }

    let mut log_list = column![].spacing(2);

    for line in logs {
        let (level, message, level_color) = classify_log_line(line);

        let entry = container(
            row![
                container(
                    text(level)
                        .size(TEXT_SMALL)
                        .font(Font::MONOSPACE)
                        .color(iced::Color::WHITE)
                )
                .padding([2, 6])
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(level_color)),
                    border: iced::Border {
                        radius: RADIUS_SM.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                text(message)
                    .font(Font::MONOSPACE)
                    .size(TEXT_BASE)
                    .color(text_color_for(level)),
            ]
            .spacing(SPACE_MD)
            .align_y(Alignment::Center),
        )
        .padding([6, 8])
        .width(Length::Fill)
        .style(container::transparent);

        log_list = log_list.push(entry);
    }

    scrollable(container(log_list).width(Length::Fill))
        .height(Length::Fill)
        .into()
}

/// splits a "[level] message" line into (LEVEL, message, badge color).
fn classify_log_line(line: &str) -> (&'static str, &str, iced::Color) {
    let colors = crate::theme::colors();
    let warn_color = colors.warning;
    let error_color = colors.error;
    let info_color = colors.info;
    let log_color = colors.hint;

    if let Some(rest) = line.strip_prefix("[log] ") {
        ("LOG", rest, log_color)
    } else if let Some(rest) = line.strip_prefix("[info] ") {
        ("INFO", rest, info_color)
    } else if let Some(rest) = line.strip_prefix("[warn] ") {
        ("WARN", rest, warn_color)
    } else if let Some(rest) = line.strip_prefix("[error] ") {
        ("ERROR", rest, error_color)
    } else {
        ("LOG", line, log_color)
    }
}

fn text_color_for(level: &str) -> iced::Color {
    let colors = crate::theme::colors();
    match level {
        "WARN" => colors.warning,
        "ERROR" => colors.error,
        "INFO" => colors.info,
        _ => colors.hint,
    }
}
