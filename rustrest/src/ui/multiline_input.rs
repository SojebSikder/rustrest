use crate::ui::context_menu::with_context_menu;
use crate::ui::resize_handle::{DividerOrientation, resize_handle};
use crate::ui::scroll_editor::scroll_editor;
use crate::ui::var_assist::var_editor;
use iced::Element;
use iced::widget::{column, text, text_editor};
use std::rc::Rc;

/// A reusable multi-line text input. Long lines don't wrap. the editor
/// scrolls horizontally and vertically instead. Offers `{{variable}}` completion and
/// shows a variable's value on hover.
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
    build(
        placeholder,
        content,
        padding,
        height,
        on_action,
        on_context_menu,
        on_resize_start,
        true,
    )
}

/// a [`multiline_input`] for text that isn't sent with requests, so
/// without `{{variable}}` assistance.
pub fn plain_multiline_input<'a, Message>(
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
    build(
        placeholder,
        content,
        padding,
        height,
        on_action,
        on_context_menu,
        on_resize_start,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn build<'a, Message>(
    placeholder: &'a str,
    content: &'a text_editor::Content,
    padding: u16,
    height: f32,
    on_action: impl Fn(text_editor::Action) -> Message + 'a,
    on_context_menu: Message,
    on_resize_start: Message,
    var_assist: bool,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    let on_action: Rc<dyn Fn(text_editor::Action) -> Message + 'a> = Rc::new(on_action);
    let forward = on_action.clone();
    let editor = with_context_menu(
        text_editor(content)
            .placeholder(placeholder)
            .padding(padding)
            .wrapping(text::Wrapping::None)
            .height(iced::Length::Fixed(height))
            .on_action(move |action| forward(action)),
        on_context_menu,
    );
    let editor = if var_assist {
        var_editor(editor, content, padding, on_action)
    } else {
        editor
    };

    column![
        scroll_editor(editor, content, f32::from(padding)),
        resize_handle(DividerOrientation::Horizontal, on_resize_start),
    ]
    .into()
}
