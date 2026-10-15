//! Horizontal scrolling for a non-wrapping `text_editor`.

use iced::advanced::layout::{self, Layout};
use iced::advanced::text::{self as adv_text, Paragraph as _, Renderer as _};
use iced::advanced::widget::{Operation, Tree, operation, tree};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::widget::text_editor;
use iced::{
    Border, Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector,
    alignment, keyboard, mouse,
};

/// pixels scrolled per wheel "line", matching iced's scrollable.
const WHEEL_LINE_PX: f32 = 60.0;
/// room kept between the caret and the viewport edge when following it.
const CARET_MARGIN: f32 = 16.0;
const SCROLLBAR_HEIGHT: f32 = 4.0;
/// taller than the drawn bar so it's easy to grab.
const SCROLLBAR_HIT_HEIGHT: f32 = 10.0;

/// wraps `editor` (a `text_editor`, possibly behind a context-menu wrapper)
/// built with `Wrapping::None` and `editor_padding` horizontal padding.
pub fn hscroll_editor<'a, Message: 'a>(
    editor: impl Into<Element<'a, Message>>,
    content: &text_editor::Content,
    editor_padding: f32,
) -> Element<'a, Message> {
    // widest line by char count; exact for monospace, close enough otherwise
    let longest_line = content
        .lines()
        .max_by_key(|line| line.text.chars().count())
        .map(|line| line.text.into_owned())
        .unwrap_or_default();

    let position = content.cursor().position;
    let caret_prefix = content
        .line(position.line)
        .map(|line| {
            let column = position.column.min(line.text.len());
            line.text.get(..column).unwrap_or(&line.text).to_string()
        })
        .unwrap_or_default();

    Element::new(HScrollEditor {
        content: editor.into(),
        longest_line,
        caret: (position.line, position.column),
        caret_prefix,
        padding: editor_padding,
    })
}

struct HScrollEditor<'a, Message> {
    content: Element<'a, Message>,
    longest_line: String,
    caret: (usize, usize),
    caret_prefix: String,
    padding: f32,
}

#[derive(Default)]
struct State {
    offset: f32,
    max_offset: f32,
    content_width: f32,
    /// cached (line, width) so an unchanged widest line isn't re-shaped every layout.
    measured_line: Option<(String, f32)>,
    /// the caret position last followed, so the offset only jumps to it when
    /// it moves and otherwise stays wherever the user scrolled.
    followed_caret: Option<(usize, usize)>,
    /// x of the press point within the scrollbar thumb, while dragging it.
    drag_grab: Option<f32>,
    modifiers: keyboard::Modifiers,
    focused: bool,
}

fn measure(renderer: &Renderer, text: &str) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let paragraph = <Renderer as adv_text::Renderer>::Paragraph::with_text(adv_text::Text {
        content: text,
        bounds: Size::INFINITE,
        size: renderer.default_size(),
        line_height: adv_text::LineHeight::default(),
        font: renderer.default_font(),
        align_x: adv_text::Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: adv_text::Shaping::Advanced,
        wrapping: adv_text::Wrapping::None,
    });
    paragraph.min_width()
}

impl State {
    fn set_offset<Message>(&mut self, offset: f32, shell: &mut Shell<'_, Message>) {
        let offset = offset.clamp(0.0, self.max_offset);
        if offset != self.offset {
            self.offset = offset;
            shell.invalidate_layout();
            shell.request_redraw();
        }
    }

    fn track(&self, bounds: Rectangle) -> Rectangle {
        Rectangle {
            x: bounds.x + 2.0,
            y: bounds.y + bounds.height - SCROLLBAR_HIT_HEIGHT,
            width: (bounds.width - 4.0).max(0.0),
            height: SCROLLBAR_HIT_HEIGHT,
        }
    }

    /// (x, width) of the thumb within `track`.
    fn thumb(&self, track: Rectangle, viewport_width: f32) -> (f32, f32) {
        let width = (track.width * viewport_width / self.content_width.max(1.0))
            .clamp(20.0_f32.min(track.width), track.width);
        let progress = if self.max_offset > 0.0 {
            self.offset / self.max_offset
        } else {
            0.0
        };
        (track.x + (track.width - width) * progress, width)
    }

    fn offset_for_thumb_x(&self, track: Rectangle, viewport_width: f32, thumb_x: f32) -> f32 {
        let (_, width) = self.thumb(track, viewport_width);
        let free = (track.width - width).max(1.0);
        ((thumb_x - track.x) / free) * self.max_offset
    }
}

/// reports whether any focusable widget below it is focused.
struct IsFocused(bool);

impl Operation for IsFocused {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(
        &mut self,
        _id: Option<&iced::advanced::widget::Id>,
        _bounds: Rectangle,
        state: &mut dyn operation::Focusable,
    ) {
        self.0 |= state.is_focused();
    }
}

impl<'a, Message> iced::advanced::Widget<Message, Theme, Renderer> for HScrollEditor<'a, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, self.content.as_widget().size().height)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
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
        let state = tree.state.downcast_mut::<State>();

        let line_width = match &state.measured_line {
            Some((line, width)) if *line == self.longest_line => *width,
            _ => {
                let width = measure(renderer, &self.longest_line);
                state.measured_line = Some((self.longest_line.clone(), width));
                width
            }
        };
        // +2 leaves room for the caret after the last glyph
        let text_width = line_width + self.padding * 2.0 + 2.0;

        let max_width = limits.max().width;
        let viewport_width = if max_width.is_finite() {
            max_width
        } else {
            text_width
        };
        let content_width = text_width.max(viewport_width);
        state.content_width = content_width;
        state.max_offset = content_width - viewport_width;

        if state.followed_caret != Some(self.caret) {
            state.followed_caret = Some(self.caret);
            let caret_x = self.padding + measure(renderer, &self.caret_prefix);
            let visible_start = state.offset + self.padding;
            let visible_end = state.offset + viewport_width - self.padding;
            if caret_x < visible_start {
                state.offset = caret_x - self.padding - CARET_MARGIN;
            } else if caret_x > visible_end {
                state.offset = caret_x - viewport_width + self.padding + CARET_MARGIN;
            }
        }
        state.offset = state.offset.clamp(0.0, state.max_offset);

        let child_limits = layout::Limits::new(
            Size::new(content_width, limits.min().height),
            Size::new(content_width, limits.max().height),
        );
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &child_limits)
            .move_to(Point::new(-state.offset, 0.0));

        let size = Size::new(viewport_width, child.size().height);
        layout::Node::with_children(size, vec![child])
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            child_layout(layout),
            renderer,
            operation,
        );
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
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();

        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if state.max_offset > 0.0 && cursor.is_over(bounds) =>
            {
                let shift = state.modifiers.shift();
                let dx = match *delta {
                    // macOS already swaps the axes while shift is held
                    mouse::ScrollDelta::Lines { y, .. } if shift && !cfg!(target_os = "macos") => {
                        y * WHEEL_LINE_PX
                    }
                    mouse::ScrollDelta::Lines { x, .. } => x * WHEEL_LINE_PX,
                    mouse::ScrollDelta::Pixels { x, y } if shift && x == 0.0 => y,
                    mouse::ScrollDelta::Pixels { x, .. } => x,
                };
                if dx != 0.0 {
                    state.set_offset(state.offset - dx, shell);
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if state.max_offset > 0.0 =>
            {
                let track = state.track(bounds);
                if let Some(position) = cursor.position_over(track) {
                    let (thumb_x, thumb_width) = state.thumb(track, bounds.width);
                    let grab = if (thumb_x..=thumb_x + thumb_width).contains(&position.x) {
                        position.x - thumb_x
                    } else {
                        // clicking the track jumps the thumb's center there
                        let grab = thumb_width / 2.0;
                        let offset =
                            state.offset_for_thumb_x(track, bounds.width, position.x - grab);
                        state.set_offset(offset, shell);
                        grab
                    };
                    state.drag_grab = Some(grab);
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(grab) = state.drag_grab {
                    let track = state.track(bounds);
                    let offset = state.offset_for_thumb_x(track, bounds.width, position.x - grab);
                    state.set_offset(offset, shell);
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.drag_grab.take().is_some() =>
            {
                shell.capture_event();
                return;
            }
            _ => {}
        }

        // the child extends past our bounds while scrolled, so don't let it
        // claim presses that land on whatever is drawn beside us there.
        let child_cursor = match event {
            Event::Mouse(mouse::Event::ButtonPressed(_)) if !cursor.is_over(bounds) => {
                mouse::Cursor::Unavailable
            }
            _ => cursor,
        };

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child_layout(layout),
            child_cursor,
            renderer,
            clipboard,
            shell,
            &bounds.intersection(viewport).unwrap_or(bounds),
        );

        let mut is_focused = IsFocused(false);
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            child_layout(layout),
            renderer,
            &mut is_focused,
        );
        let state = tree.state.downcast_mut::<State>();
        if state.focused != is_focused.0 {
            state.focused = is_focused.0;
            shell.request_redraw();
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
        use iced::advanced::Renderer as _;

        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        let child_cursor = if cursor.is_over(bounds) {
            cursor
        } else {
            mouse::Cursor::Unavailable
        };

        renderer.with_layer(clip, |renderer| {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                child_layout(layout),
                child_cursor,
                &clip,
            );
        });

        if state.max_offset <= 0.0 {
            return;
        }

        // the editor's own border is cut off on whichever side is scrolled
        // out of view, so redraw that edge on ours
        let status = if state.focused {
            text_editor::Status::Focused {
                is_hovered: cursor.is_over(bounds),
            }
        } else if cursor.is_over(bounds) {
            text_editor::Status::Hovered
        } else {
            text_editor::Status::Active
        };
        let border = text_editor::default(theme, status).border;
        let edge = |x: f32| renderer::Quad {
            bounds: Rectangle {
                x,
                y: bounds.y,
                width: border.width,
                height: bounds.height,
            },
            ..renderer::Quad::default()
        };
        if state.offset > 0.0 {
            renderer.fill_quad(edge(bounds.x), border.color);
        }
        if state.offset < state.max_offset {
            renderer.fill_quad(edge(bounds.x + bounds.width - border.width), border.color);
        }

        let track = state.track(bounds);
        let (thumb_x, thumb_width) = state.thumb(track, bounds.width);
        let hovered = cursor.is_over(track) || state.drag_grab.is_some();
        let mut color = crate::theme::colors().border;
        if !hovered {
            color = Color {
                a: color.a * 0.7,
                ..color
            };
        }
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: thumb_x,
                    y: track.y + track.height - SCROLLBAR_HEIGHT - 2.0,
                    width: thumb_width,
                    height: SCROLLBAR_HEIGHT,
                },
                border: Border {
                    radius: (SCROLLBAR_HEIGHT / 2.0).into(),
                    ..Border::default()
                },
                ..renderer::Quad::default()
            },
            color,
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
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        if state.drag_grab.is_some()
            || (state.max_offset > 0.0 && cursor.is_over(state.track(bounds)))
        {
            return mouse::Interaction::Idle;
        }
        if !cursor.is_over(bounds) {
            return mouse::Interaction::None;
        }
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            child_layout(layout),
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
            child_layout(layout),
            renderer,
            viewport,
            translation,
        )
    }
}

fn child_layout(layout: Layout<'_>) -> Layout<'_> {
    layout
        .children()
        .next()
        .expect("HScrollEditor always lays out its one child")
}
