use crate::message::Message;
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Font, Length};

pub fn render_console_clear_bar<'a>() -> Element<'a, Message> {
    row![
        text("Console Output")
            .size(12)
            .font(Font {
                weight: iced::font::Weight::Bold,
                ..Font::DEFAULT
            })
            .color(crate::theme::colors().text_muted),
        Space::new().width(Length::Fill),
        button(text("Clear").size(11))
            .style(button::text)
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
                .size(13)
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
                        .size(11)
                        .font(Font::MONOSPACE)
                        .color(iced::Color::WHITE)
                )
                .padding([2, 6])
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(level_color)),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                text(message)
                    .font(Font::MONOSPACE)
                    .size(13)
                    .color(text_color_for(level)),
            ]
            .spacing(10)
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
