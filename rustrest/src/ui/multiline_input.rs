use crate::ui::context_menu::with_context_menu;
use iced::Element;
use iced::widget::text_editor;

/// A reusable multi-line text input, built on `text_editor`.
pub fn multiline_input<'a, Message>(
    placeholder: &'a str,
    content: &'a text_editor::Content,
    padding: u16,
    max_height: f32,
    on_action: impl Fn(text_editor::Action) -> Message + 'a,
    on_context_menu: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    with_context_menu(
        text_editor(content)
            .placeholder(placeholder)
            .padding(padding)
            .max_height(max_height)
            .on_action(on_action),
        on_context_menu,
    )
}
