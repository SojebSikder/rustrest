use crate::ui::context_menu::with_context_menu;
use crate::ui::resize_handle::{DividerOrientation, resize_handle};
use iced::Element;
use iced::widget::{column, text_editor};

/// A reusable multi-line text input
pub fn multiline_input<'a, Message>(
    placeholder: &'a str,
    content: &'a text_editor::Content,
    padding: u16,
    height: f32,
    on_action: impl Fn(text_editor::Action) -> Message + 'a,
    on_context_menu: Message,
    on_resize_start: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    column![
        with_context_menu(
            text_editor(content)
                .placeholder(placeholder)
                .padding(padding)
                .height(iced::Length::Fixed(height))
                .on_action(on_action),
            on_context_menu,
        ),
        resize_handle(DividerOrientation::Horizontal, on_resize_start),
    ]
    .into()
}
