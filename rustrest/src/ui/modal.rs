use iced::widget::container;
use iced::{Border, Color, Element, Length, Shadow, Theme, Vector};

pub fn card<'a, Message: 'a>(
    body: impl Into<Element<'a, Message>>,
    width: f32,
) -> Element<'a, Message> {
    container(body)
        .width(Length::Fixed(width))
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                text_color: Some(palette.background.base.text),
                background: Some(palette.background.weak.color.into()),
                border: Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                    offset: Vector::new(0.0, 6.0),
                    blur_radius: 24.0,
                },
                ..Default::default()
            }
        })
        .into()
}

pub fn muted_text_color(theme: &Theme) -> Color {
    let text = theme.extended_palette().background.base.text;
    Color {
        a: text.a * 0.6,
        ..text
    }
}

pub fn danger_text_color(theme: &Theme) -> Color {
    theme.extended_palette().danger.base.color
}
