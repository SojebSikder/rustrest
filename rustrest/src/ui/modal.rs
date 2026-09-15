use crate::message::Message;
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::widget::container;
use iced::{
    Border, Color, Element, Event, Length, Rectangle, Renderer, Shadow, Size, Theme, Vector, mouse,
};

pub fn card<'a>(body: impl Into<Element<'a, Message>>, width: f32) -> Element<'a, Message> {
    let styled = container(body)
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
        });

    ClickSwallow::new(styled).into()
}

/// A wrapper around an element that captures all mouse events within its bounds,
/// so that clicks inside it do not bubble up to the app-wide listener for closing the modal.
/// it's used to prevent clicks inside the modal from closing it.
struct ClickSwallow<'a> {
    content: Element<'a, Message>,
}

impl<'a> ClickSwallow<'a> {
    fn new(content: impl Into<Element<'a, Message>>) -> Self {
        Self {
            content: content.into(),
        }
    }
}

impl<'a> iced::advanced::Widget<Message, Theme, Renderer> for ClickSwallow<'a> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let should_capture = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                | Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        ) && cursor.is_over(layout.bounds());

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );

        if should_capture {
            shell.capture_event();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<ClickSwallow<'a>> for Element<'a, Message> {
    fn from(widget: ClickSwallow<'a>) -> Self {
        Element::new(widget)
    }
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
