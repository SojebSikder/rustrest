use iced::widget::{Space, container, mouse_area};
use iced::{Element, Length, mouse};

pub enum DividerOrientation {
    Vertical,
    Horizontal,
}

pub fn resize_handle<'a, Message: Clone + 'a>(
    orientation: DividerOrientation,
    on_press: Message,
) -> Element<'a, Message> {
    let (width, height, interaction) = match orientation {
        DividerOrientation::Vertical => (
            Length::Fixed(6.0),
            Length::Fill,
            mouse::Interaction::ResizingHorizontally,
        ),
        DividerOrientation::Horizontal => (
            Length::Fill,
            Length::Fixed(6.0),
            mouse::Interaction::ResizingVertically,
        ),
    };

    mouse_area(
        container(Space::new())
            .width(width)
            .height(height)
            .style(|_theme| container::Style {
                background: Some(crate::theme::colors().border_variant.into()),
                ..Default::default()
            }),
    )
    .interaction(interaction)
    .on_press(on_press)
    .into()
}
