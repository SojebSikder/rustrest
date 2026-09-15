use iced::widget::{row, text};
use iced::{Alignment, Element};

/// braille "dots" animation frames, cycled by `tick` to form a spinning
/// indicator using only a text glyph.
const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner_glyph(tick: u64) -> &'static str {
    FRAMES[(tick as usize) % FRAMES.len()]
}

/// a lightweight animated spinner glyph
pub fn spinner<'a, Message: 'a>(tick: u64) -> Element<'a, Message> {
    text(spinner_glyph(tick)).size(13).into()
}

/// spinner glyph followed by a label, e.g. "Connecting..." or "Loading...".
pub fn spinner_with_label<'a, Message: 'a>(
    tick: u64,
    label: impl Into<String>,
) -> Element<'a, Message> {
    row![spinner(tick), text(label.into()).size(12)]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}
