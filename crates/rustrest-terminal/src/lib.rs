//! Reusable multi-instance terminal engine built directly on
//! `alacritty_terminal` (VT100/grid emulation) and its native PTY backend
//! (ConPTY on Windows). Deliberately has no GUI-framework dependency: a host
//! app's own widget layer calls [`TerminalSession::snapshot`] to get a plain
//! grid of cells to paint, and [`TerminalSession::write`]/[`TerminalManager::resize`]
//! to feed it input - so this crate stays reusable across any renderer.

mod palette;

use alacritty_terminal::event::{Event, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, Options as PtyOptions, Shell};
use alacritty_terminal::vte::ansi::CursorShape as AnsiCursorShape;
use std::collections::HashMap;
use std::sync::Arc;

const DEFAULT_FG: Rgb = Rgb {
    r: 230,
    g: 230,
    b: 230,
};
const DEFAULT_BG: Rgb = Rgb {
    r: 18,
    g: 18,
    b: 18,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalCell {
    pub c: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub style: CellStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Underline,
    Beam,
    HollowBlock,
    Hidden,
}

/// a plain-data snapshot of everything needed to paint one frame; owns no
/// alacritty types, so a host widget never needs `alacritty_terminal` itself.
#[derive(Clone, Debug)]
pub struct TerminalGrid {
    pub columns: usize,
    pub rows: usize,
    /// row-major, length `columns * rows`.
    pub cells: Vec<TerminalCell>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub cursor_shape: CursorShape,
}

/// what a terminal session wants the host UI to do in response to backend activity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalNotice {
    /// new output landed in the grid; redraw.
    Dirty,
    /// the shell process exited; the session should be closed.
    Exited,
}

#[derive(Clone)]
struct EventProxy {
    id: u64,
    on_notice: Arc<dyn Fn(u64, TerminalNotice) + Send + Sync>,
}

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        let notice = match event {
            Event::Wakeup | Event::Bell | Event::Title(_) | Event::ResetTitle => {
                TerminalNotice::Dirty
            }
            Event::Exit | Event::ChildExit(_) => TerminalNotice::Exited,
            _ => return,
        };
        (self.on_notice)(self.id, notice);
    }
}

/// a live shell session: a PTY paired with Alacritty's grid/VT100 state,
/// mutated in the background by the I/O thread `EventLoop::spawn` owns.
pub struct TerminalSession {
    term: Arc<FairMutex<Term<EventProxy>>>,
    notifier: Notifier,
    columns: usize,
    rows: usize,
}

impl TerminalSession {
    /// writes raw bytes to the shell (already-encoded key input, pastes, etc).
    pub fn write(&self, bytes: Vec<u8>) {
        self.notifier.notify(bytes);
    }

    pub fn size(&self) -> (usize, usize) {
        (self.columns, self.rows)
    }

    /// builds a plain-data snapshot of the currently visible grid for painting.
    pub fn snapshot(&self) -> TerminalGrid {
        let term = self.term.lock();
        let content = term.renderable_content();
        let columns = self.columns;
        let rows = self.rows;

        let mut cells = vec![
            TerminalCell {
                c: ' ',
                fg: DEFAULT_FG,
                bg: DEFAULT_BG,
                style: CellStyle::default(),
            };
            columns * rows
        ];

        let colors = content.colors;
        let cursor = content.cursor;
        for indexed in content.display_iter {
            let row = indexed.point.line.0;
            let col = indexed.point.column.0;
            if row < 0 || row as usize >= rows || col >= columns {
                continue;
            }

            let mut fg = palette::resolve(indexed.fg, colors, DEFAULT_FG, DEFAULT_BG);
            let mut bg = palette::resolve(indexed.bg, colors, DEFAULT_FG, DEFAULT_BG);
            if indexed.cell.flags.contains(CellFlags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if indexed
                .cell
                .flags
                .intersects(CellFlags::DIM | CellFlags::DIM_BOLD)
            {
                fg = palette::dim(fg);
            }

            cells[row as usize * columns + col] = TerminalCell {
                c: indexed.cell.c,
                fg,
                bg,
                style: CellStyle {
                    bold: indexed
                        .cell
                        .flags
                        .intersects(CellFlags::BOLD | CellFlags::DIM_BOLD),
                    italic: indexed.cell.flags.contains(CellFlags::ITALIC),
                    underline: indexed.cell.flags.intersects(CellFlags::ALL_UNDERLINES),
                    strikeout: indexed.cell.flags.contains(CellFlags::STRIKEOUT),
                },
            };
        }

        let cursor_shape = if cursor.point.line.0 >= 0 {
            match cursor.shape {
                AnsiCursorShape::Block => CursorShape::Block,
                AnsiCursorShape::Underline => CursorShape::Underline,
                AnsiCursorShape::Beam => CursorShape::Beam,
                AnsiCursorShape::HollowBlock => CursorShape::HollowBlock,
                AnsiCursorShape::Hidden => CursorShape::Hidden,
            }
        } else {
            CursorShape::Hidden
        };

        TerminalGrid {
            columns,
            rows,
            cells,
            cursor_row: cursor.point.line.0.max(0) as usize,
            cursor_col: cursor.point.column.0,
            cursor_shape,
        }
    }
}

/// owns one PTY-backed, Alacritty-driven [`TerminalSession`] per open
/// terminal tab, keyed by an id that's independent of tab position so tabs
/// can be reordered or closed without disturbing the others.
pub struct TerminalManager {
    sessions: HashMap<u64, TerminalSession>,
    next_id: u64,
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            next_id: 0,
        }
    }

    fn shell() -> Shell {
        #[cfg(windows)]
        {
            Shell::new("powershell.exe".to_string(), Vec::new())
        }
        #[cfg(not(windows))]
        {
            let program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            Shell::new(program, Vec::new())
        }
    }

    /// spawns a new shell session over a fresh PTY sized to `columns` x `rows`.
    /// `on_notice` is invoked (from the session's background I/O thread)
    /// whenever the grid changes or the shell exits.
    pub fn spawn(
        &mut self,
        columns: usize,
        rows: usize,
        on_notice: impl Fn(u64, TerminalNotice) + Send + Sync + 'static,
    ) -> Result<u64, String> {
        let id = self.next_id;

        let window_size = WindowSize {
            num_lines: rows as u16,
            num_cols: columns as u16,
            cell_width: 8,
            cell_height: 16,
        };
        let pty_options = PtyOptions {
            shell: Some(Self::shell()),
            ..Default::default()
        };
        let pty = tty::new(&pty_options, window_size, id).map_err(|e| e.to_string())?;

        let event_proxy = EventProxy {
            id,
            on_notice: Arc::new(on_notice),
        };
        let term_size = TermSize::new(columns, rows);
        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &term_size,
            event_proxy.clone(),
        )));

        let event_loop = EventLoop::new(term.clone(), event_proxy, pty, false, false)
            .map_err(|e| e.to_string())?;
        let notifier = Notifier(event_loop.channel());
        let _ = event_loop.spawn();

        self.sessions.insert(
            id,
            TerminalSession {
                term,
                notifier,
                columns,
                rows,
            },
        );
        self.next_id += 1;
        Ok(id)
    }

    /// tears down the session: asks its I/O thread to shut down, which kills
    /// the underlying shell process.
    pub fn close(&mut self, id: u64) {
        if let Some(session) = self.sessions.remove(&id) {
            let _ = session
                .notifier
                .0
                .send(alacritty_terminal::event_loop::Msg::Shutdown);
        }
    }

    pub fn get(&self, id: u64) -> Option<&TerminalSession> {
        self.sessions.get(&id)
    }

    /// resizes both the visible grid and the underlying PTY; a no-op if the
    /// cell dimensions haven't actually changed.
    pub fn resize(
        &mut self,
        id: u64,
        columns: usize,
        rows: usize,
        cell_width: u16,
        cell_height: u16,
    ) {
        let Some(session) = self.sessions.get_mut(&id) else {
            return;
        };
        if session.columns == columns && session.rows == rows {
            return;
        }
        session.columns = columns;
        session.rows = rows;

        let term_size = TermSize::new(columns, rows);
        session.term.lock().resize(term_size);
        session.notifier.on_resize(WindowSize {
            num_lines: rows as u16,
            num_cols: columns as u16,
            cell_width,
            cell_height,
        });
    }
}
