use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Font, Length};

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
