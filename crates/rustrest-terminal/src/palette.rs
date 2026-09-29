use crate::{Rgb, TerminalPalette};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};

pub(crate) const BASIC16: [Rgb; 16] = [
    Rgb { r: 0, g: 0, b: 0 },   // Black
    Rgb { r: 205, g: 0, b: 0 }, // Red
    Rgb { r: 0, g: 205, b: 0 }, // Green
    Rgb {
        r: 205,
        g: 205,
        b: 0,
    }, // Yellow
    Rgb { r: 0, g: 0, b: 238 }, // Blue
    Rgb {
        r: 205,
        g: 0,
        b: 205,
    }, // Magenta
    Rgb {
        r: 0,
        g: 205,
        b: 205,
    }, // Cyan
    Rgb {
        r: 229,
        g: 229,
        b: 229,
    }, // White
    Rgb {
        r: 127,
        g: 127,
        b: 127,
    }, // BrightBlack
    Rgb { r: 255, g: 0, b: 0 }, // BrightRed
    Rgb { r: 0, g: 255, b: 0 }, // BrightGreen
    Rgb {
        r: 255,
        g: 255,
        b: 0,
    }, // BrightYellow
    Rgb {
        r: 92,
        g: 92,
        b: 255,
    }, // BrightBlue
    Rgb {
        r: 255,
        g: 0,
        b: 255,
    }, // BrightMagenta
    Rgb {
        r: 0,
        g: 255,
        b: 255,
    }, // BrightCyan
    Rgb {
        r: 255,
        g: 255,
        b: 255,
    }, // BrightWhite
];

pub fn dim(c: Rgb) -> Rgb {
    Rgb {
        r: (c.r as f32 * 0.66) as u8,
        g: (c.g as f32 * 0.66) as u8,
        b: (c.b as f32 * 0.66) as u8,
    }
}

fn indexed_default(index: u8, palette: &TerminalPalette) -> Rgb {
    match index {
        0..=15 => palette.ansi[index as usize],
        16..=231 => {
            let i = index - 16;
            let component = |c: u8| if c == 0 { 0 } else { c * 40 + 55 };
            Rgb {
                r: component(i / 36),
                g: component((i / 6) % 6),
                b: component(i % 6),
            }
        }
        232..=255 => {
            let level = (index - 232) * 10 + 8;
            Rgb {
                r: level,
                g: level,
                b: level,
            }
        }
    }
}

fn named_default(named: NamedColor, palette: &TerminalPalette) -> Rgb {
    let (fg, bg) = (palette.foreground, palette.background);
    match named as usize {
        v @ 0..=15 => palette.ansi[v],
        256 => fg,                                   // Foreground
        257 => bg,                                   // Background
        258 => fg,                                   // Cursor
        v @ 259..=266 => dim(palette.ansi[v - 259]), // DimBlack..DimWhite
        267 => fg,                                   // BrightForeground
        268 => dim(fg),                              // DimForeground
        _ => fg,
    }
}

/// resolves a cell's color, honoring any palette overrides the running
/// program has set via OSC sequences before falling back to `palette`.
pub fn resolve(color: AnsiColor, overrides: &Colors, palette: &TerminalPalette) -> Rgb {
    match color {
        AnsiColor::Spec(rgb) => Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
        AnsiColor::Named(named) => overrides[named]
            .map(|rgb| Rgb {
                r: rgb.r,
                g: rgb.g,
                b: rgb.b,
            })
            .unwrap_or_else(|| named_default(named, palette)),
        AnsiColor::Indexed(index) => overrides[index as usize]
            .map(|rgb| Rgb {
                r: rgb.r,
                g: rgb.g,
                b: rgb.b,
            })
            .unwrap_or_else(|| indexed_default(index, palette)),
    }
}
