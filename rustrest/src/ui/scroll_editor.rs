//! Horizontal and vertical scrolling for a non-wrapping `text_editor`.
//!
//! The editor is laid out at its full content size and moved under a
//! viewport, so it never scrolls internally and where each line is drawn
//! follows from the layout alone.

use iced::advanced::layout::{self, Layout};
use iced::advanced::text::{self as adv_text, Paragraph as _, Renderer as _};
use iced::advanced::widget::{Operation, Tree, operation, tree};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::widget::text_editor;
use iced::{
    Border, Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector,
    alignment, keyboard, mouse,
};

/// pixels scrolled horizontally per wheel "line", matching iced's scrollable.
const WHEEL_LINE_PX: f32 = 60.0;
/// text lines scrolled vertically per wheel "line", matching `text_editor`.
const WHEEL_TEXT_LINES: f32 = 4.0;
/// room kept between the caret and the viewport edge when following it.
const CARET_MARGIN: f32 = 16.0;
const SCROLLBAR_THICKNESS: f32 = 4.0;
/// thicker than the drawn bar so it's easy to grab.
const SCROLLBAR_HIT_THICKNESS: f32 = 10.0;

/// wraps `editor` (a `text_editor`, possibly behind wrappers) built with
/// `Wrapping::None` and `editor_padding` padding on every side.
pub fn scroll_editor<'a, Message: 'a>(
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

    Element::new(ScrollEditor {
        content: editor.into(),
        longest_line,
        line_count: content.line_count().max(1),
        caret: (position.line, position.column),
        caret_prefix,
        padding: editor_padding,
    })
}

struct ScrollEditor<'a, Message> {
    content: Element<'a, Message>,
    longest_line: String,
    line_count: usize,
    caret: (usize, usize),
    caret_prefix: String,
    padding: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

impl Axis {
    /// (start, length) of `rect` along this axis.
    fn span(self, rect: Rectangle) -> (f32, f32) {
        match self {
            Axis::X => (rect.x, rect.width),
            Axis::Y => (rect.y, rect.height),
        }
    }

    fn coord(self, point: Point) -> f32 {
        match self {
            Axis::X => point.x,
            Axis::Y => point.y,
        }
    }
}

#[derive(Default)]
struct Scroll {
    offset: f32,
    max_offset: f32,
    content: f32,
}

#[derive(Default)]
struct State {
    x: Scroll,
    y: Scroll,
    line_height: f32,
    /// cached (line, width) so an unchanged widest line isn't re-shaped every layout.
    measured_line: Option<(String, f32)>,
    /// the caret position last followed, so the offsets only jump to it when
    /// it moves and otherwise stay wherever the user scrolled.
    followed_caret: Option<(usize, usize)>,
    /// the scrollbar being dragged, and where along its thumb it was grabbed.
    drag: Option<(Axis, f32)>,
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
    fn scroll(&self, axis: Axis) -> &Scroll {
        match axis {
            Axis::X => &self.x,
            Axis::Y => &self.y,
        }
    }

    fn scroll_mut(&mut self, axis: Axis) -> &mut Scroll {
        match axis {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
        }
    }

    fn scrollable(&self, axis: Axis) -> bool {
        self.scroll(axis).max_offset > 0.0
    }

    fn set_offset<Message>(&mut self, axis: Axis, offset: f32, shell: &mut Shell<'_, Message>) {
        let scroll = self.scroll_mut(axis);
        let offset = offset.clamp(0.0, scroll.max_offset);
        if offset != scroll.offset {
            scroll.offset = offset;
            shell.invalidate_layout();
            shell.request_redraw();
        }
    }

    /// the hit area of `axis`'s scrollbar: along the bottom for X, the right for Y.
    fn track(&self, axis: Axis, bounds: Rectangle) -> Rectangle {
        // leave the corner to the other bar when both show
        let corner = |other: Axis| {
            if self.scrollable(other) {
                SCROLLBAR_HIT_THICKNESS
            } else {
                0.0
            }
        };
        match axis {
            Axis::X => Rectangle {
                x: bounds.x + 2.0,
                y: bounds.y + bounds.height - SCROLLBAR_HIT_THICKNESS,
                width: (bounds.width - 4.0 - corner(Axis::Y)).max(0.0),
                height: SCROLLBAR_HIT_THICKNESS,
            },
            Axis::Y => Rectangle {
                x: bounds.x + bounds.width - SCROLLBAR_HIT_THICKNESS,
                y: bounds.y + 2.0,
                width: SCROLLBAR_HIT_THICKNESS,
                height: (bounds.height - 4.0 - corner(Axis::X)).max(0.0),
            },
        }
    }

    /// (start, length) of the thumb along `track`.
    fn thumb(&self, axis: Axis, track: Rectangle, viewport: f32) -> (f32, f32) {
        let scroll = self.scroll(axis);
        let (track_start, track_len) = axis.span(track);
        let len = (track_len * viewport / scroll.content.max(1.0))
            .clamp(20.0_f32.min(track_len), track_len);
        let progress = if scroll.max_offset > 0.0 {
            scroll.offset / scroll.max_offset
        } else {
            0.0
        };
        (track_start + (track_len - len) * progress, len)
    }

    fn offset_for_thumb(
        &self,
        axis: Axis,
        track: Rectangle,
        viewport: f32,
        thumb_start: f32,
    ) -> f32 {
        let (_, len) = self.thumb(axis, track, viewport);
        let (track_start, track_len) = axis.span(track);
        let free = (track_len - len).max(1.0);
        ((thumb_start - track_start) / free) * self.scroll(axis).max_offset
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

const AXES: [Axis; 2] = [Axis::X, Axis::Y];

impl<'a, Message> iced::advanced::Widget<Message, Theme, Renderer> for ScrollEditor<'a, Message> {
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
        let line_height = adv_text::LineHeight::default()
            .to_absolute(renderer.default_size())
            .0;
        state.line_height = line_height;
        let text_height = self.line_count as f32 * line_height + self.padding * 2.0;

        let max_width = limits.max().width;
        let viewport_width = if max_width.is_finite() {
            max_width
        } else {
            text_width
        };
        let own_height = limits
            .height(self.content.as_widget().size().height)
            .max()
            .height;
        let viewport_height = if own_height.is_finite() {
            own_height
        } else {
            text_height
        };

        let content_width = text_width.max(viewport_width);
        let content_height = text_height.max(viewport_height);
        state.x.content = content_width;
        state.x.max_offset = content_width - viewport_width;
        state.y.content = content_height;
        state.y.max_offset = content_height - viewport_height;

        if state.followed_caret != Some(self.caret) {
            state.followed_caret = Some(self.caret);

            let caret_x = self.padding + measure(renderer, &self.caret_prefix);
            let visible_start = state.x.offset + self.padding;
            let visible_end = state.x.offset + viewport_width - self.padding;
            if caret_x < visible_start {
                state.x.offset = caret_x - self.padding - CARET_MARGIN;
            } else if caret_x > visible_end {
                state.x.offset = caret_x - viewport_width + self.padding + CARET_MARGIN;
            }

            let caret_top = self.padding + self.caret.0 as f32 * line_height;
            let caret_bottom = caret_top + line_height;
            if caret_top < state.y.offset + self.padding {
                state.y.offset = caret_top - self.padding;
            } else if caret_bottom > state.y.offset + viewport_height - self.padding {
                state.y.offset = caret_bottom - viewport_height + self.padding;
            }
        }
        state.x.offset = state.x.offset.clamp(0.0, state.x.max_offset);
        state.y.offset = state.y.offset.clamp(0.0, state.y.max_offset);

        let content_size = Size::new(content_width, content_height);
        let child_limits = layout::Limits::new(content_size, content_size);
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &child_limits)
            .move_to(Point::new(-state.x.offset, -state.y.offset));

        layout::Node::with_children(Size::new(viewport_width, viewport_height), vec![child])
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
        let viewport_len = |axis: Axis| axis.span(bounds).1;

        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if (state.scrollable(Axis::X) || state.scrollable(Axis::Y))
                    && cursor.is_over(bounds) =>
            {
                let shift = state.modifiers.shift();
                let (dx, dy) = match *delta {
                    // macOS already swaps the axes while shift is held
                    mouse::ScrollDelta::Lines { y, .. } if shift && !cfg!(target_os = "macos") => {
                        (y * WHEEL_LINE_PX, 0.0)
                    }
                    mouse::ScrollDelta::Lines { x, y } => {
                        (x * WHEEL_LINE_PX, y * WHEEL_TEXT_LINES * state.line_height)
                    }
                    mouse::ScrollDelta::Pixels { x, y } if shift && x == 0.0 => (y, 0.0),
                    mouse::ScrollDelta::Pixels { x, y } => (x, y),
                };
                let mut scrolled = false;
                for (axis, delta) in [(Axis::X, dx), (Axis::Y, dy)] {
                    if delta != 0.0 && state.scrollable(axis) {
                        let offset = state.scroll(axis).offset - delta;
                        state.set_offset(axis, offset, shell);
                        scrolled = true;
                    }
                }
                if scrolled {
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                for axis in AXES {
                    if !state.scrollable(axis) {
                        continue;
                    }
                    let track = state.track(axis, bounds);
                    let Some(position) = cursor.position_over(track) else {
                        continue;
                    };
                    let at = axis.coord(position);
                    let (thumb_start, thumb_len) = state.thumb(axis, track, viewport_len(axis));
                    let grab = if (thumb_start..=thumb_start + thumb_len).contains(&at) {
                        at - thumb_start
                    } else {
                        // clicking the track jumps the thumb's center there
                        let grab = thumb_len / 2.0;
                        let offset =
                            state.offset_for_thumb(axis, track, viewport_len(axis), at - grab);
                        state.set_offset(axis, offset, shell);
                        grab
                    };
                    state.drag = Some((axis, grab));
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some((axis, grab)) = state.drag {
                    let track = state.track(axis, bounds);
                    let offset = state.offset_for_thumb(
                        axis,
                        track,
                        viewport_len(axis),
                        axis.coord(*position) - grab,
                    );
                    state.set_offset(axis, offset, shell);
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.drag.take().is_some() =>
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

        if !state.scrollable(Axis::X) && !state.scrollable(Axis::Y) {
            return;
        }

        // the editor's own border is cut off on whichever sides are scrolled
        // out of view, so redraw those edges on ours
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
        let mut edges = Vec::new();
        if state.x.offset > 0.0 {
            edges.push(Rectangle {
                width: border.width,
                ..bounds
            });
        }
        if state.x.offset < state.x.max_offset {
            edges.push(Rectangle {
                x: bounds.x + bounds.width - border.width,
                width: border.width,
                ..bounds
            });
        }
        if state.y.offset > 0.0 {
            edges.push(Rectangle {
                height: border.width,
                ..bounds
            });
        }
        if state.y.offset < state.y.max_offset {
            edges.push(Rectangle {
                y: bounds.y + bounds.height - border.width,
                height: border.width,
                ..bounds
            });
        }
        for edge in edges {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: edge,
                    ..renderer::Quad::default()
                },
                border.color,
            );
        }

        for axis in AXES {
            if !state.scrollable(axis) {
                continue;
            }
            let track = state.track(axis, bounds);
            let (thumb_start, thumb_len) = state.thumb(axis, track, axis.span(bounds).1);
            let hovered =
                cursor.is_over(track) || state.drag.is_some_and(|(dragged, _)| dragged == axis);
            let mut color = crate::theme::colors().border;
            if !hovered {
                color = Color {
                    a: color.a * 0.7,
                    ..color
                };
            }
            let thumb = match axis {
                Axis::X => Rectangle {
                    x: thumb_start,
                    y: track.y + track.height - SCROLLBAR_THICKNESS - 2.0,
                    width: thumb_len,
                    height: SCROLLBAR_THICKNESS,
                },
                Axis::Y => Rectangle {
                    x: track.x + track.width - SCROLLBAR_THICKNESS - 2.0,
                    y: thumb_start,
                    width: SCROLLBAR_THICKNESS,
                    height: thumb_len,
                },
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: thumb,
                    border: Border {
                        radius: (SCROLLBAR_THICKNESS / 2.0).into(),
                        ..Border::default()
                    },
                    ..renderer::Quad::default()
                },
                color,
            );
        }
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
        let over_bar = AXES
            .into_iter()
            .any(|axis| state.scrollable(axis) && cursor.is_over(state.track(axis, bounds)));
        if state.drag.is_some() || over_bar {
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
        .expect("ScrollEditor always lays out its one child")
}
