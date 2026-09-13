//! Custom `iced` widget that paints a [`rustrest_terminal::TerminalGrid`]
//! snapshot and forwards keyboard/resize input back to its session.

use crate::message::Message;
use iced::advanced::clipboard::Kind as ClipboardKind;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::text::{Alignment, LineHeight, Shaping};
use iced::advanced::widget::operation::{self, Focusable};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell};
use iced::alignment::Vertical;
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Modifiers};
use iced::widget::canvas::{Frame, Path, Text};
use iced::{Color, Element, Event, Font, Length, Point, Rectangle, Renderer, Size, Theme, mouse};
use rustrest_terminal::{CursorShape, TerminalSession};

const FONT_SIZE: f32 = 14.0;
const CELL_WIDTH: f32 = FONT_SIZE * 0.6;
const CELL_HEIGHT: f32 = FONT_SIZE * 1.3;
const BACKGROUND: Color = Color::from_rgb(18.0 / 255.0, 18.0 / 255.0, 18.0 / 255.0);
const DEFAULT_FG: Color = Color::from_rgb(230.0 / 255.0, 230.0 / 255.0, 230.0 / 255.0);
const SELECTION_BG: Color = Color::from_rgba(80.0 / 255.0, 130.0 / 255.0, 220.0 / 255.0, 0.45);
/// wheel notches translate to this many terminal lines each.
const SCROLL_LINES_PER_NOTCH: f32 = 3.0;

pub struct TerminalView<'a> {
    session: &'a TerminalSession,
    terminal_id: u64,
    widget_id: iced::widget::Id,
}

impl<'a> TerminalView<'a> {
    pub fn show(
        session: &'a TerminalSession,
        terminal_id: u64,
        widget_id: iced::widget::Id,
    ) -> Element<'a, Message> {
        iced::widget::container(Self {
            session,
            terminal_id,
            widget_id,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| iced::widget::container::Style {
            background: Some(iced::Background::Color(BACKGROUND)),
            ..Default::default()
        })
        .into()
    }
}

struct State {
    focused: bool,
    size: Size,
    /// true while the left mouse button is held down after starting a
    /// selection, so drag events keep extending it.
    selecting: bool,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// translates a key press into the byte sequence a shell expects; deliberately
/// covers the keys people actually use in a terminal (typing, history
/// navigation, interrupt/EOF) rather than full xterm/kitty key reporting.
fn encode_key(key: &Key, modifiers: Modifiers, text: Option<&str>) -> Option<Vec<u8>> {
    if modifiers.control() {
        if let Key::Character(c) = key {
            let ch = c.chars().next()?.to_ascii_uppercase();
            if ch.is_ascii_uppercase() {
                return Some(vec![ch as u8 - b'A' + 1]);
            }
        }
    }

    match key {
        Key::Named(Named::Enter) => Some(vec![b'\r']),
        Key::Named(Named::Backspace) => Some(vec![0x7f]),
        Key::Named(Named::Tab) => Some(vec![b'\t']),
        Key::Named(Named::Escape) => Some(vec![0x1b]),
        Key::Named(Named::ArrowUp) => Some(b"\x1b[A".to_vec()),
        Key::Named(Named::ArrowDown) => Some(b"\x1b[B".to_vec()),
        Key::Named(Named::ArrowRight) => Some(b"\x1b[C".to_vec()),
        Key::Named(Named::ArrowLeft) => Some(b"\x1b[D".to_vec()),
        Key::Named(Named::Home) => Some(b"\x1b[H".to_vec()),
        Key::Named(Named::End) => Some(b"\x1b[F".to_vec()),
        Key::Named(Named::PageUp) => Some(b"\x1b[5~".to_vec()),
        Key::Named(Named::PageDown) => Some(b"\x1b[6~".to_vec()),
        Key::Named(Named::Delete) => Some(b"\x1b[3~".to_vec()),
        Key::Named(Named::Space) => Some(vec![b' ']),
        Key::Named(_) => None,
        Key::Character(_) | Key::Unidentified => text.map(|t| t.as_bytes().to_vec()),
    }
}

fn to_iced_color(rgb: rustrest_terminal::Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}

/// converts a widget-local pointer position into a (row, column) cell,
/// clamped to the visible grid so drags outside the bounds still extend the
/// selection towards the nearest edge.
fn cell_at(bounds: Rectangle, position: Point, columns: usize, rows: usize) -> (usize, usize) {
    let col = ((position.x - bounds.x) / CELL_WIDTH).floor().max(0.0) as usize;
    let row = ((position.y - bounds.y) / CELL_HEIGHT).floor().max(0.0) as usize;
    (
        row.min(rows.saturating_sub(1)),
        col.min(columns.saturating_sub(1)),
    )
}

impl<'a> Widget<Message, Theme, Renderer> for TerminalView<'a> {
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            focused: false,
            size: Size::ZERO,
            selecting: false,
        })
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        layout::Node::new(size)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn operation::Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(Some(&self.widget_id), layout.bounds(), state);
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let grid = self.session.snapshot();

        let mut frame = Frame::new(renderer, viewport.size());

        for row in 0..grid.rows {
            let y = bounds.y + row as f32 * CELL_HEIGHT;
            let row_cells = &grid.cells[row * grid.columns..(row + 1) * grid.columns];

            // batch same-background runs into one fill each, skipping the
            // default background (the container already paints it).
            let mut run_start = 0usize;
            while run_start < row_cells.len() {
                let bg = row_cells[run_start].bg;
                let mut run_end = run_start + 1;
                while run_end < row_cells.len() && row_cells[run_end].bg == bg {
                    run_end += 1;
                }
                if to_iced_color(bg) != BACKGROUND {
                    let x = bounds.x + run_start as f32 * CELL_WIDTH;
                    let width = (run_end - run_start) as f32 * CELL_WIDTH;
                    frame.fill(
                        &Path::rectangle(Point::new(x, y), Size::new(width, CELL_HEIGHT)),
                        to_iced_color(bg),
                    );
                }
                run_start = run_end;
            }

            // highlight the selection on top of the background, batched the
            // same way, so text painted afterwards still shows through it.
            let mut run_start = 0usize;
            while run_start < row_cells.len() {
                if !row_cells[run_start].selected {
                    run_start += 1;
                    continue;
                }
                let mut run_end = run_start + 1;
                while run_end < row_cells.len() && row_cells[run_end].selected {
                    run_end += 1;
                }
                let x = bounds.x + run_start as f32 * CELL_WIDTH;
                let width = (run_end - run_start) as f32 * CELL_WIDTH;
                frame.fill(
                    &Path::rectangle(Point::new(x, y), Size::new(width, CELL_HEIGHT)),
                    SELECTION_BG,
                );
                run_start = run_end;
            }

            // batch same-style text runs the same way.
            let mut run_start = 0usize;
            while run_start < row_cells.len() {
                let style = (row_cells[run_start].fg, row_cells[run_start].style);
                let mut run_end = run_start + 1;
                while run_end < row_cells.len()
                    && (row_cells[run_end].fg, row_cells[run_end].style) == style
                {
                    run_end += 1;
                }

                let text: String = row_cells[run_start..run_end].iter().map(|c| c.c).collect();
                if text.trim().is_empty() {
                    run_start = run_end;
                    continue;
                }

                let (fg, cell_style) = style;
                let x = bounds.x + run_start as f32 * CELL_WIDTH;
                let mut font = Font::MONOSPACE;
                if cell_style.bold {
                    font.weight = iced::font::Weight::Bold;
                }
                if cell_style.italic {
                    font.style = iced::font::Style::Italic;
                }

                frame.fill_text(Text {
                    content: text,
                    position: Point::new(x, y + CELL_HEIGHT / 2.0),
                    color: to_iced_color(fg),
                    size: iced::Pixels(FONT_SIZE),
                    font,
                    align_x: Alignment::Left,
                    align_y: Vertical::Center,
                    shaping: Shaping::Basic,
                    line_height: LineHeight::Relative(1.0),
                    ..Text::default()
                });

                if cell_style.underline {
                    let underline_y = y + CELL_HEIGHT - 1.0;
                    let width = (run_end - run_start) as f32 * CELL_WIDTH;
                    frame.stroke(
                        &Path::line(
                            Point::new(x, underline_y),
                            Point::new(x + width, underline_y),
                        ),
                        iced::widget::canvas::Stroke::default()
                            .with_width(1.0)
                            .with_color(to_iced_color(fg)),
                    );
                }

                run_start = run_end;
            }
        }

        // cursor overlay.
        if grid.cursor_shape != CursorShape::Hidden
            && grid.cursor_row < grid.rows
            && grid.cursor_col < grid.columns
        {
            let x = bounds.x + grid.cursor_col as f32 * CELL_WIDTH;
            let y = bounds.y + grid.cursor_row as f32 * CELL_HEIGHT;
            let cursor_color = Color {
                a: 0.55,
                ..DEFAULT_FG
            };

            match grid.cursor_shape {
                CursorShape::Block => {
                    frame.fill(
                        &Path::rectangle(Point::new(x, y), Size::new(CELL_WIDTH, CELL_HEIGHT)),
                        cursor_color,
                    );
                }
                CursorShape::Underline => {
                    frame.fill(
                        &Path::rectangle(
                            Point::new(x, y + CELL_HEIGHT - 2.0),
                            Size::new(CELL_WIDTH, 2.0),
                        ),
                        cursor_color,
                    );
                }
                CursorShape::Beam => {
                    frame.fill(
                        &Path::rectangle(Point::new(x, y), Size::new(2.0, CELL_HEIGHT)),
                        cursor_color,
                    );
                }
                CursorShape::HollowBlock => {
                    frame.stroke(
                        &Path::rectangle(Point::new(x, y), Size::new(CELL_WIDTH, CELL_HEIGHT)),
                        iced::widget::canvas::Stroke::default()
                            .with_width(1.0)
                            .with_color(cursor_color),
                    );
                }
                CursorShape::Hidden => {}
            }
        }

        use iced::advanced::graphics::geometry::Renderer as _;
        renderer.draw_geometry(frame.into_geometry());
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();

        let layout_size = bounds.size();
        if state.size != layout_size {
            state.size = layout_size;
            let columns = ((layout_size.width / CELL_WIDTH).floor() as usize).max(1);
            let rows = ((layout_size.height / CELL_HEIGHT).floor() as usize).max(1);
            shell.publish(Message::TerminalResized(
                self.terminal_id,
                columns,
                rows,
                CELL_WIDTH as u16,
                CELL_HEIGHT as u16,
            ));
        }

        let is_cursor_in_bounds = cursor.position_over(bounds).is_some();
        let grid_size = self.session.size();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.focused = is_cursor_in_bounds;
                if let Some(position) = cursor.position_over(bounds) {
                    let (row, col) = cell_at(bounds, position, grid_size.0, grid_size.1);
                    self.session.start_selection(row, col);
                    state.selecting = true;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.selecting => {
                let (row, col) = cell_at(bounds, *position, grid_size.0, grid_size.1);
                self.session.update_selection(row, col);
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.selecting = false;
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if is_cursor_in_bounds => {
                let lines = match *delta {
                    mouse::ScrollDelta::Lines { y, .. } => (y * SCROLL_LINES_PER_NOTCH).round(),
                    mouse::ScrollDelta::Pixels { y, .. } => (y / CELL_HEIGHT).round(),
                };
                if lines != 0.0 {
                    self.session.scroll(lines as i32);
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) if state.focused => {
                let is_copy = modifiers.control()
                    && (modifiers.shift()
                        && matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("c"))
                        || matches!(key, Key::Named(Named::Insert)));
                let is_paste = (modifiers.control()
                    && modifiers.shift()
                    && matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("v")))
                    || (modifiers.shift() && matches!(key, Key::Named(Named::Insert)));

                if is_copy {
                    if let Some(selected) = self.session.selection_text() {
                        clipboard.write(ClipboardKind::Standard, selected);
                    }
                    shell.capture_event();
                } else if is_paste {
                    if let Some(pasted) = clipboard.read(ClipboardKind::Standard) {
                        let bytes = pasted
                            .replace("\r\n", "\r")
                            .replace('\n', "\r")
                            .into_bytes();
                        shell.publish(Message::TerminalInput(self.terminal_id, bytes));
                    }
                    shell.capture_event();
                } else if let Some(bytes) = encode_key(key, *modifiers, text.as_deref()) {
                    shell.publish(Message::TerminalInput(self.terminal_id, bytes));
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.position_over(layout.bounds()).is_some() {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a> From<TerminalView<'a>> for Element<'a, Message> {
    fn from(widget: TerminalView<'a>) -> Self {
        Self::new(widget)
    }
}
