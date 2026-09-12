use crate::message::Message;
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Font, Length};

/// render the bottom bar header strip: title + log count, expand/collapse toggle, and a clear button.
pub fn render_console_bar<'a>(logs: &'a [String], collapsed: bool) -> Element<'a, Message> {
    let toggle_icon = if collapsed { "▸" } else { "▾" };
    let label = if logs.is_empty() {
        "Console".to_string()
    } else {
        format!("Console ({})", logs.len())
    };

    row![
        button(text(format!("{} {}", toggle_icon, label)).size(13))
            .style(button::text)
            .padding([4, 6])
            .on_press(Message::ToggleConsolePanel),
        Space::new().width(Length::Fill),
        button(text("Clear").size(12))
            .style(button::text)
            .padding([4, 6])
            .on_press(Message::ClearConsoleLogs),
    ]
    .align_y(Alignment::Center)
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
                .color(iced::Color::from_rgb(0.5, 0.5, 0.5)),
        )
        .padding(10)
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
    let warn_color = iced::Color::from_rgb(0.85, 0.55, 0.10); // yellow
    let error_color = iced::Color::from_rgb(0.87, 0.22, 0.22); // red
    let info_color = iced::Color::from_rgb(0.20, 0.45, 0.85); // blue
    let log_color = iced::Color::from_rgb(0.45, 0.45, 0.45); // gray

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
    match level {
        "WARN" => iced::Color::from_rgb(0.75, 0.5, 0.05), // yellow
        "ERROR" => iced::Color::from_rgb(0.75, 0.15, 0.15), // red
        "INFO" => iced::Color::from_rgb(0.20, 0.45, 0.85), // blue
        _ => iced::Color::from_rgb(0.45, 0.45, 0.45),     // gray
    }
}
