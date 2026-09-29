use crate::message::Message;
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, tree};
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
                background: Some(crate::theme::colors().elevated_surface_background.into()),
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

#[derive(Default)]
struct SwallowState {
    /// current left button press started inside the card, or outside it
    /// while one of its overlays was open (which just dismisses the overlay).
    press_inside: bool,
    /// the content has had an overlay open since the last press this widget
    /// saw - so a press may have gone to that overlay instead.
    overlay_seen: bool,
    /// the content's overlay is open right now.
    overlay_open: bool,
}

impl<'a> iced::advanced::Widget<Message, Theme, Renderer> for ClickSwallow<'a> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SwallowState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SwallowState::default())
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
        let over_card = cursor.is_over(layout.bounds());
        let state = tree.state.downcast_mut::<SwallowState>();
        let should_capture = match event {
            // only reached for presses the overlay (if any) didn't capture
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.press_inside = over_card || state.overlay_open;
                state.overlay_seen = false;
                over_card
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let from_inside = std::mem::take(&mut state.press_inside)
                    || std::mem::take(&mut state.overlay_seen);
                from_inside || over_card
            }
            _ => false,
        };

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
        let Tree {
            state, children, ..
        } = tree;
        let overlay = self.content.as_widget_mut().overlay(
            &mut children[0],
            layout,
            renderer,
            viewport,
            translation,
        );
        let state = state.downcast_mut::<SwallowState>();
        state.overlay_open = overlay.is_some();
        state.overlay_seen |= state.overlay_open;
        overlay
    }
}

impl<'a> From<ClickSwallow<'a>> for Element<'a, Message> {
    fn from(widget: ClickSwallow<'a>) -> Self {
        Element::new(widget)
    }
}

/// the active theme's `text.muted` (for iced's built-in themes, the base text color at 60% opacity).
pub fn muted_text_color(_theme: &Theme) -> Color {
    crate::theme::colors().text_muted
}

pub fn danger_text_color(theme: &Theme) -> Color {
    theme.extended_palette().danger.base.color
}
