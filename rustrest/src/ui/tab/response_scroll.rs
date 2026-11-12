//! Mirror of the response body editor's vertical scroll position.

use std::cell::Cell;

/// fixed so the fold gutter's rows line up with the editor's lines
pub const LINE_HEIGHT: f32 = crate::ui::tokens::CODE_TEXT.line_height;
/// editor's padding on every side
pub const PADDING: f32 = 5.0;

#[derive(Debug, Clone, Default)]
pub struct EditorScroll {
    /// pixels from the top of the text to the top of the visible area
    offset: Cell<f32>,
    /// height of the editor's text area (inside its padding) at the last
    /// view, or 0 before the editor has been shown
    viewport: Cell<f32>,
}

impl EditorScroll {
    /// records the editor's current height. called from the view, which is the only place that knows it
    pub fn set_editor_height(&self, height: f32) {
        self.viewport.set((height - 2.0 * PADDING).max(0.0));
    }

    /// the current offset, clamped to a text of `lines` lines
    pub fn offset(&self, lines: usize) -> f32 {
        self.clamp(lines);
        self.offset.get()
    }

    /// new content starts at the top
    pub fn reset(&self) {
        self.offset.set(0.0);
    }

    /// `text_editor::Action::Scroll { lines }`
    pub fn scroll_lines(&self, lines: i32, total_lines: usize) {
        self.offset
            .set(self.offset.get() + lines as f32 * LINE_HEIGHT);
        self.clamp(total_lines);
    }

    /// the cursor moved to `line`: scroll it into view the way cosmic-text
    /// does, aligning it to whichever edge it was beyond
    pub fn follow_cursor(&self, line: usize, total_lines: usize) {
        let viewport = self.viewport.get();
        let top = line as f32 * LINE_HEIGHT;
        let bottom = top + LINE_HEIGHT;
        let offset = self.offset.get();
        if top < offset {
            self.offset.set(top);
        } else if viewport > 0.0 && bottom > offset + viewport {
            self.offset.set(bottom - viewport);
        }
        self.clamp(total_lines);
    }

    fn clamp(&self, lines: usize) {
        let viewport = self.viewport.get();
        let mut offset = self.offset.get().max(0.0);
        // until the editor has been laid out its height is unknown and so
        // is the furthest it can scroll
        if viewport > 0.0 {
            let max = (lines as f32 * LINE_HEIGHT - viewport).max(0.0);
            offset = offset.min(max);
        }
        self.offset.set(offset);
    }
}
