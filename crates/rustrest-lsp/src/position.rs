//! column conversion between the app's UTF-8 byte offsets and the server's
//! negotiated position encoding.

use crate::{Position, Range};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    #[default]
    Utf16,
    Utf8,
}

impl Encoding {
    /// from the server's `capabilities.positionEncoding` (LSP default: utf-16).
    pub fn from_server(value: Option<&str>) -> Self {
        match value {
            Some("utf-8") => Encoding::Utf8,
            _ => Encoding::Utf16,
        }
    }
}

/// the `line`th line of `text` (0-based), without its line terminator.
pub fn line_of(text: &str, line: u32) -> &str {
    let l = text.split('\n').nth(line as usize).unwrap_or("");
    l.strip_suffix('\r').unwrap_or(l)
}

/// UTF-8 byte column -> UTF-16 code units. Clamps to the line end and
/// rounds down to a char boundary.
pub fn utf8_to_utf16(line: &str, byte_col: u32) -> u32 {
    let mut idx = (byte_col as usize).min(line.len());
    while !line.is_char_boundary(idx) {
        idx -= 1;
    }
    line[..idx].encode_utf16().count() as u32
}

/// UTF-16 code-unit column -> UTF-8 byte column. Clamps to the line end; a
/// column inside a surrogate pair maps to the start of that char.
pub fn utf16_to_utf8(line: &str, utf16_col: u32) -> u32 {
    let mut units = 0u32;
    for (i, c) in line.char_indices() {
        let next = units + c.len_utf16() as u32;
        if next > utf16_col {
            return i as u32;
        }
        units = next;
    }
    line.len() as u32
}

/// app position -> LSP `Position` JSON.
pub fn to_lsp(text: &str, pos: Position, enc: Encoding) -> Value {
    let character = match enc {
        Encoding::Utf8 => pos.column,
        Encoding::Utf16 => utf8_to_utf16(line_of(text, pos.line), pos.column),
    };
    json!({ "line": pos.line, "character": character })
}

/// LSP `Position` JSON -> app position (missing fields read as 0).
pub fn from_lsp(text: &str, value: &Value, enc: Encoding) -> Position {
    let line = value.get("line").and_then(Value::as_u64).unwrap_or(0) as u32;
    let character = value.get("character").and_then(Value::as_u64).unwrap_or(0) as u32;
    let column = match enc {
        Encoding::Utf8 => character,
        Encoding::Utf16 => utf16_to_utf8(line_of(text, line), character),
    };
    Position { line, column }
}

/// LSP `Range` JSON -> app range.
pub fn range_from_lsp(text: &str, value: &Value, enc: Encoding) -> Range {
    Range {
        start: from_lsp(text, &value["start"], enc),
        end: from_lsp(text, &value["end"], enc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines() {
        let text = "a\r\nbc\n\nd";
        assert_eq!(line_of(text, 0), "a");
        assert_eq!(line_of(text, 1), "bc");
        assert_eq!(line_of(text, 2), "");
        assert_eq!(line_of(text, 3), "d");
        assert_eq!(line_of(text, 9), "");
    }

    #[test]
    fn utf16_conversion() {
        // é = 2 bytes / 1 unit, 😀 = 4 bytes / 2 units
        let line = "é😀x";
        assert_eq!(utf8_to_utf16(line, 0), 0);
        assert_eq!(utf8_to_utf16(line, 2), 1);
        assert_eq!(utf8_to_utf16(line, 6), 3);
        assert_eq!(utf8_to_utf16(line, 7), 4);
        assert_eq!(utf8_to_utf16(line, 99), 4);
        assert_eq!(utf8_to_utf16(line, 3), 1); // mid-char rounds down

        assert_eq!(utf16_to_utf8(line, 0), 0);
        assert_eq!(utf16_to_utf8(line, 1), 2);
        assert_eq!(utf16_to_utf8(line, 2), 2); // inside the surrogate pair
        assert_eq!(utf16_to_utf8(line, 3), 6);
        assert_eq!(utf16_to_utf8(line, 4), 7);
        assert_eq!(utf16_to_utf8(line, 99), 7);
    }

    #[test]
    fn position_json() {
        let text = "x\nconst é = pm.";
        let pos = Position {
            line: 1,
            column: 14,
        };
        let lsp = to_lsp(text, pos, Encoding::Utf16);
        assert_eq!(lsp, json!({"line": 1, "character": 13}));
        assert_eq!(from_lsp(text, &lsp, Encoding::Utf16), pos);
        assert_eq!(
            to_lsp(text, pos, Encoding::Utf8),
            json!({"line": 1, "character": 14})
        );
        assert_eq!(Encoding::from_server(Some("utf-8")), Encoding::Utf8);
        assert_eq!(Encoding::from_server(None), Encoding::Utf16);
    }
}
