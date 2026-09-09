use iced::widget::container;
use iced::{Border, Color, Element, Length, Shadow, Theme, Vector};

/// wraps `body` in the dark, bordered "floating card" style shared by all
/// modal dialogs (save-request chooser, commit dialog, confirm dialog).
pub fn card<'a, Message: 'a>(
    body: impl Into<Element<'a, Message>>,
    width: f32,
) -> Element<'a, Message> {
    container(body)
        .width(Length::Fixed(width))
        .style(|_theme: &Theme| container::Style {
            background: Some(Color::from_rgb(0.13, 0.13, 0.15).into()),
            border: Border {
                color: Color::from_rgb(0.25, 0.25, 0.28),
                width: 1.0,
                radius: 10.0.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                offset: Vector::new(0.0, 6.0),
                blur_radius: 24.0,
            },
            ..Default::default()
        })
        .into()
}
