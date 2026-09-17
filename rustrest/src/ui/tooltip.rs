use iced::Element;
use iced::widget::{container, text, tooltip};

/// wraps `content` so hovering it shows `label` in a small floating tooltip -
/// e.g. naming what an icon-only button does, like the right-panel rail.
pub fn with_tooltip<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    label: impl Into<String>,
    position: tooltip::Position,
) -> Element<'a, Message>
where
    Message: 'a,
{
    tooltip(
        content,
        container(text(label.into()).size(12))
            .padding(6)
            .style(container::rounded_box),
        position,
    )
    .gap(6)
    .into()
}
