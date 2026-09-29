//! The colors Rustrest's own views paint with, beyond what iced's `Palette` covers.
//! Every field is named after the Zed theme key it's read from (see `from_style`),
//! and every one has a fallback derived from the iced palette so a theme only has to set the
//! handful of keys it cares about - and so iced's built-in themes (which set none)
//! keep looking the way they always have.

use super::schema::{Appearance, StyleMap, color};
use iced::Color;
use iced::theme::{Palette, palette::Extended};

#[derive(Debug, Clone, PartialEq)]
pub struct ThemeColors {
    pub appearance: Appearance,

    /// "editor.background" - the main content area (request/response panes).
    pub editor_background: Color,
    /// "editor.foreground"
    pub editor_foreground: Color,
    /// "surface.background" - cards, modals, the status bar.
    pub surface_background: Color,
    /// "elevated_surface.background" - popups and toasts.
    pub elevated_surface_background: Color,
    /// "panel.background" - the sidebar and bottom panels.
    pub panel_background: Color,
    /// "status_bar.background"
    pub status_bar_background: Color,
    /// "tab_bar.background"
    pub tab_bar_background: Color,
    /// "title_bar.background" - the menu bar.
    pub title_bar_background: Color,
    /// "tab.active_background" - `None` keeps the default accent-filled tab.
    pub tab_active_background: Option<Color>,
    /// "tab.inactive_background"
    pub tab_inactive_background: Color,

    /// "border"
    pub border: Color,
    /// "border.variant" - subtle dividers.
    pub border_variant: Color,
    /// "element.hover" - hovered rows, drag handles.
    pub element_hover: Color,
    /// "element.selected"
    pub element_selected: Color,

    /// "text"
    pub text: Color,
    /// "text.muted" - secondary labels, hints, timestamps.
    pub text_muted: Color,
    /// "text.placeholder"
    pub text_placeholder: Color,
    /// "text.accent" - links, progress, focused things.
    pub text_accent: Color,

    /// "success" - 2xx responses, passing tests.
    pub success: Color,
    /// "error" - 4xx/5xx responses, failing tests, errors.
    pub error: Color,
    /// "warning"
    pub warning: Color,
    /// "info"
    pub info: Color,
    /// "hint"
    pub hint: Color,

    /// "created" - git: added.
    pub created: Color,
    /// "modified" - git: modified, unsaved-changes dot.
    pub modified: Color,
    /// "deleted" - git: deleted.
    pub deleted: Color,
    /// "renamed" - git: renamed.
    pub renamed: Color,
    /// "conflict" - git: conflicted.
    pub conflict: Color,
    /// "ignored" - git: untracked.
    pub ignored: Color,

    /// "terminal.background"
    pub terminal_background: Color,
    /// "terminal.foreground"
    pub terminal_foreground: Color,
    /// "players[0].selection" - terminal/editor selection.
    pub selection: Color,
    /// "terminal.ansi.{black,red,...,bright_white}"
    pub terminal_ansi: [Color; 16],
}

const ANSI_NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// the terminal's historical xterm-ish palette, used when a theme doesn't
/// set `terminal.ansi.*`.
const DEFAULT_ANSI: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 0, 0),
    (0, 205, 0),
    (205, 205, 0),
    (0, 0, 238),
    (205, 0, 205),
    (0, 205, 205),
    (229, 229, 229),
    (127, 127, 127),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (92, 92, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

fn with_alpha(color: Color, a: f32) -> Color {
    Color {
        a: color.a * a,
        ..color
    }
}

impl ThemeColors {
    /// fallbacks: what Rustrest looked like before themes existed, adapted to the palette where that was already the case.
    pub fn derive(palette: &Palette, extended: &Extended, appearance: Appearance) -> Self {
        let rgb = Color::from_rgb;
        Self {
            appearance,
            editor_background: palette.background,
            editor_foreground: palette.text,
            surface_background: extended.background.weak.color,
            elevated_surface_background: extended.background.weak.color,
            panel_background: palette.background,
            status_bar_background: extended.background.weak.color,
            tab_bar_background: palette.background,
            title_bar_background: palette.background,
            tab_active_background: None,
            tab_inactive_background: Color::from_rgba(0.5, 0.5, 0.5, 0.12),
            border: extended.background.strong.color,
            border_variant: Color::from_rgba(0.5, 0.5, 0.5, 0.25),
            element_hover: Color::from_rgba(0.5, 0.5, 0.5, 0.12),
            element_selected: extended.primary.weak.color,
            text: palette.text,
            text_muted: with_alpha(extended.background.base.text, 0.6),
            text_placeholder: with_alpha(extended.background.base.text, 0.45),
            text_accent: palette.primary,
            success: rgb(0.12, 0.64, 0.35),
            error: rgb(0.87, 0.22, 0.22),
            warning: rgb(0.85, 0.55, 0.10),
            info: rgb(0.20, 0.45, 0.85),
            hint: rgb(0.45, 0.45, 0.45),
            created: rgb(0.25, 0.65, 0.35),
            modified: rgb(0.85, 0.55, 0.10),
            deleted: rgb(0.87, 0.22, 0.22),
            renamed: rgb(0.20, 0.45, 0.85),
            conflict: rgb(0.87, 0.22, 0.22),
            ignored: rgb(0.45, 0.45, 0.45),
            terminal_background: Color::from_rgb8(18, 18, 18),
            terminal_foreground: Color::from_rgb8(230, 230, 230),
            selection: Color::from_rgba8(80, 130, 220, 0.45),
            terminal_ansi: DEFAULT_ANSI.map(|(r, g, b)| Color::from_rgb8(r, g, b)),
        }
    }

    /// `derive`, then every key the theme actually sets.
    pub fn from_style(
        palette: &Palette,
        extended: &Extended,
        appearance: Appearance,
        style: &StyleMap,
    ) -> Self {
        let mut c = Self::derive(palette, extended, appearance);
        let set = |target: &mut Color, keys: &[&str]| {
            if let Some(value) = keys.iter().find_map(|k| color(style, k)) {
                *target = value;
            }
        };

        set(
            &mut c.editor_background,
            &["editor.background", "background"],
        );
        set(&mut c.editor_foreground, &["editor.foreground", "text"]);
        set(&mut c.surface_background, &["surface.background"]);
        set(
            &mut c.elevated_surface_background,
            &["elevated_surface.background", "surface.background"],
        );
        set(&mut c.panel_background, &["panel.background"]);
        set(&mut c.status_bar_background, &["status_bar.background"]);
        set(&mut c.tab_bar_background, &["tab_bar.background"]);
        set(&mut c.title_bar_background, &["title_bar.background"]);
        c.tab_active_background = color(style, "tab.active_background");
        set(&mut c.tab_inactive_background, &["tab.inactive_background"]);
        set(&mut c.border, &["border"]);
        set(&mut c.border_variant, &["border.variant", "border"]);
        set(
            &mut c.element_hover,
            &["element.hover", "ghost_element.hover"],
        );
        set(
            &mut c.element_selected,
            &["element.selected", "ghost_element.selected"],
        );
        set(&mut c.text, &["text"]);
        set(&mut c.text_muted, &["text.muted"]);
        set(&mut c.text_placeholder, &["text.placeholder", "text.muted"]);
        set(&mut c.text_accent, &["text.accent", "icon.accent"]);
        set(&mut c.success, &["success", "created"]);
        set(&mut c.error, &["error", "deleted"]);
        set(&mut c.warning, &["warning", "modified"]);
        set(&mut c.info, &["info", "text.accent"]);
        set(&mut c.hint, &["hint", "text.muted"]);
        set(&mut c.created, &["created", "success"]);
        set(&mut c.modified, &["modified", "warning"]);
        set(&mut c.deleted, &["deleted", "error"]);
        set(&mut c.renamed, &["renamed", "info"]);
        set(&mut c.conflict, &["conflict", "error"]);
        set(&mut c.ignored, &["ignored", "hint", "text.muted"]);
        set(
            &mut c.terminal_background,
            &["terminal.background", "editor.background"],
        );
        set(
            &mut c.terminal_foreground,
            &["terminal.foreground", "editor.foreground", "text"],
        );

        if let Some(selection) = style
            .get("players")
            .and_then(|p| p.as_array())
            .and_then(|players| players.first())
            .and_then(|p| p.get("selection"))
            .and_then(|s| s.as_str())
            .and_then(super::schema::parse_color)
        {
            c.selection = selection;
        }

        for (i, name) in ANSI_NAMES.iter().enumerate() {
            set(&mut c.terminal_ansi[i], &[&format!("terminal.ansi.{name}")]);
            set(
                &mut c.terminal_ansi[i + 8],
                &[&format!("terminal.ansi.bright_{name}")],
            );
        }

        c
    }

    pub fn is_dark(&self) -> bool {
        self.appearance.is_dark()
    }
}

/// iced palette a theme maps onto: the six base colors every iced widget derives its look from.
pub fn palette_from_style(base: Palette, style: &StyleMap) -> Palette {
    let pick = |fallback: Color, keys: &[&str]| {
        keys.iter()
            .find_map(|k| color(style, k))
            .unwrap_or(fallback)
    };
    Palette {
        background: pick(base.background, &["editor.background", "background"]),
        text: pick(base.text, &["text", "editor.foreground"]),
        primary: pick(
            base.primary,
            &["text.accent", "icon.accent", "border.focused"],
        ),
        success: pick(base.success, &["success", "created"]),
        warning: pick(base.warning, &["warning", "modified"]),
        danger: pick(base.danger, &["error", "deleted"]),
    }
}

/// iced's generated extended palette, with the surface/border shades
/// replaced by the theme's own where it sets them - so stock iced widgets
/// (text inputs, pick lists, secondary buttons, ...) pick up the theme's
/// surfaces instead of shades iced guessed from the background.
pub fn extended_from_style(palette: Palette, style: &StyleMap) -> Extended {
    let mut extended = Extended::generate(palette);
    if let Some(c) = color(style, "surface.background") {
        extended.background.weak.color = c;
    }
    if let Some(c) = color(style, "element.background") {
        extended.background.neutral.color = c;
    }
    if let Some(c) = color(style, "border") {
        extended.background.strong.color = c;
    }
    if let Some(c) = color(style, "element.hover") {
        extended.background.weaker.color = c;
    }
    extended
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_style_is_the_derived_fallback() {
        let palette = Palette::DARK;
        let extended = Extended::generate(palette);
        let empty = StyleMap::new();
        assert_eq!(
            ThemeColors::from_style(&palette, &extended, Appearance::Dark, &empty),
            ThemeColors::derive(&palette, &extended, Appearance::Dark)
        );
    }

    #[test]
    fn reads_zed_keys_and_their_fallbacks() {
        let style: StyleMap = serde_json::from_str(
            r##"{
                "background": "#111111ff",
                "created": "#00ff00ff",
                "terminal.ansi.bright_red": "#ff0000ff",
                "players": [{ "cursor": "#fff", "selection": "#12345680" }]
            }"##,
        )
        .unwrap();
        let palette = palette_from_style(Palette::DARK, &style);
        let extended = Extended::generate(palette);
        let c = ThemeColors::from_style(&palette, &extended, Appearance::Dark, &style);
        assert_eq!(c.editor_background, Color::from_rgb8(0x11, 0x11, 0x11));
        // "success" falls back to "created"
        assert_eq!(c.success, Color::from_rgb8(0, 255, 0));
        assert_eq!(c.terminal_ansi[9], Color::from_rgb8(255, 0, 0));
        assert_eq!(c.selection.r, 0x12 as f32 / 255.0);
    }
}
