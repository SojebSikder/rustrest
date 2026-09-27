//! Byte offset <-> LSP position conversion for the negotiated encoding.

use lsp_types::Position;

/// Negotiated position encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// columns are byte offsets within the line
    Utf8,
    /// columns are UTF-16 code units (the LSP default)
    Utf16,
}

/// Line start offsets of a text.
#[derive(Debug, Clone)]
pub struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self {
            starts,
            len: text.len(),
        }
    }

    fn line_end(&self, line: usize) -> usize {
        self.starts
            .get(line + 1)
            .map(|next| next - 1)
            .unwrap_or(self.len)
    }

    /// Converts a byte offset (clamped to the text) into a position.
    pub fn position(&self, text: &str, offset: usize, enc: Encoding) -> Position {
        let offset = floor_char_boundary(text, offset.min(text.len()));
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let start = self.starts[line];
        let character = match enc {
            Encoding::Utf8 => offset - start,
            Encoding::Utf16 => text[start..offset].encode_utf16().count(),
        };
        Position::new(line as u32, character as u32)
    }

    /// Converts a position into a byte offset, clamping out-of-range lines and
    /// columns to the end of the text / line.
    pub fn offset(&self, text: &str, pos: Position, enc: Encoding) -> usize {
        let line = pos.line as usize;
        if line >= self.starts.len() {
            return text.len();
        }
        let start = self.starts[line];
        let end = self.line_end(line).min(text.len());
        let col = pos.character as usize;
        match enc {
            Encoding::Utf8 => floor_char_boundary(text, (start + col).min(end)),
            Encoding::Utf16 => {
                let mut units = 0;
                for (i, ch) in text[start..end].char_indices() {
                    if units >= col {
                        return start + i;
                    }
                    units += ch.len_utf16();
                }
                end
            }
        }
    }
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_vs_utf16_columns() {
        let text = "const s = \"héllo 😀\"; pm.\nx";
        let idx = LineIndex::new(text);
        let end = text.find('\n').unwrap();
        // 'é' is 2 bytes / 1 unit, '😀' is 4 bytes / 2 units
        let p8 = idx.position(text, end, Encoding::Utf8);
        let p16 = idx.position(text, end, Encoding::Utf16);
        assert_eq!(p8, Position::new(0, end as u32));
        assert_eq!(p16, Position::new(0, end as u32 - 1 - 2));
        assert_eq!(idx.offset(text, p8, Encoding::Utf8), end);
        assert_eq!(idx.offset(text, p16, Encoding::Utf16), end);
        // second line
        let last = text.len();
        assert_eq!(
            idx.position(text, last, Encoding::Utf16),
            Position::new(1, 1)
        );
        assert_eq!(idx.offset(text, Position::new(1, 1), Encoding::Utf8), last);
    }

    #[test]
    fn clamps_out_of_range() {
        let text = "ab\ncd";
        let idx = LineIndex::new(text);
        assert_eq!(idx.offset(text, Position::new(0, 99), Encoding::Utf16), 2);
        assert_eq!(idx.offset(text, Position::new(9, 0), Encoding::Utf8), 5);
        // mid-codepoint utf-8 column snaps back to the char start
        let text = "é";
        let idx = LineIndex::new(text);
        assert_eq!(idx.offset(text, Position::new(0, 1), Encoding::Utf8), 0);
        assert_eq!(idx.position(text, 1, Encoding::Utf8), Position::new(0, 0));
    }
}
