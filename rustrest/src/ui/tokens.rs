//! Shared visual tokens: type scale, spacing steps, corner radii and the
//! handful of reusable widget styles the whole UI is built from, so the many
//! hand-styled widgets across the app stay consistent instead of each picking
//! their own magic numbers.
//!
//! Everything here reads its colors from the active theme
//! ([`crate::theme::colors`]), so a Zed theme keeps controlling the palette;
//! these tokens only fix the *shape* of the UI (sizes, spacing, radii) and the
//! way theme colors are applied.

use iced::widget::{button, container, text_input};
use iced::{Border, Color};

// ── Type scale ────────────────────────────────────────────────────────────
// Six sizes cover the whole UI. Reach for the nearest one rather than a new
// literal so labels stay on the same rhythm.

/// Captions and tags: method badges, `git`/`ssh` tags, uppercase section labels.
pub const TEXT_CAPTION: f32 = 10.0;
/// Small print: secondary metadata, keyboard-shortcut hints, timestamps.
pub const TEXT_SMALL: f32 = 11.0;
/// Body: list rows, controls, tabs, table cells.
pub const TEXT_BODY: f32 = 12.0;
/// Base: default reading size - menu items, descriptions, field values.
pub const TEXT_BASE: f32 = 13.0;
/// Subtitle: empty-state headlines and dialog titles.
pub const TEXT_SUBTITLE: f32 = 15.0;
/// Title: page and hero headings.
pub const TEXT_TITLE: f32 = 18.0;

// ── Spacing scale ─────────────────────────────────────────────────────────
// A 4px-based rhythm. Use these instead of ad-hoc gaps.

/// 4px - gaps inside a single logical control.
pub const SPACE_XS: f32 = 4.0;
/// 6px - gaps between tightly related controls.
pub const SPACE_SM: f32 = 6.0;
/// 8px - default gap between controls.
pub const SPACE_MD: f32 = 8.0;
/// 12px - gap between groups of controls.
pub const SPACE_LG: f32 = 12.0;
/// 16px - padding inside a surface.
pub const SPACE_XL: f32 = 16.0;

// ── Corner radii ──────────────────────────────────────────────────────────
// Only the three "steps" of corner radius the UI actually uses live here;
// anything that is deliberately a different shape (the window outline, pill
// badges) keeps its own value at the call site.

/// Corner radius for small elements: badges, menu rows, list rows, chips.
pub const RADIUS_SM: f32 = 4.0;
/// Corner radius for standard controls: tabs, buttons, inputs, cards, dropdowns.
pub const RADIUS_MD: f32 = 6.0;
/// Corner radius for large floating surfaces: modals and popovers.
pub const RADIUS_LG: f32 = 10.0;

/// The UI's single semibold weight, used wherever a label needs emphasis
/// (the open menu group, section headings) instead of ad-hoc `Weight::Bold`.
pub fn semibold() -> iced::Font {
    iced::Font {
        weight: iced::font::Weight::Semibold,
        ..iced::Font::DEFAULT
    }
}

/// `color` at `alpha`, multiplied into any alpha it already carries - the one
/// way the UI derives translucent tints (selected rows, active tabs, badges).
pub fn tint(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha,
        ..color
    }
}

/// overwrites a button style's corner radius, so every button shape in a given
/// role stays on the same step no matter what iced's base style picked.
fn with_radius(mut style: button::Style, radius: f32) -> button::Style {
    style.border.radius = radius.into();
    style
}

// ── Buttons ───────────────────────────────────────────────────────────────
// One shape (rounded, `RADIUS_MD`) in four roles. Use these in place of
// `button::primary` / `button::danger` directly so radii never drift.

/// The primary call to action ("Send", "Save"): accent fill, on-accent text.
pub fn primary_button(theme: &iced::Theme, status: button::Status) -> button::Style {
    with_radius(button::primary(theme, status), RADIUS_MD)
}

/// A confirming action ("New Request"): success fill.
pub fn success_button(theme: &iced::Theme, status: button::Status) -> button::Style {
    with_radius(button::success(theme, status), RADIUS_MD)
}

/// A destructive action ("Cancel request", deletes): danger fill.
pub fn danger_button(theme: &iced::Theme, status: button::Status) -> button::Style {
    with_radius(button::danger(theme, status), RADIUS_MD)
}

/// A quiet secondary action ("Generate code", "Back to Live"): neutral fill.
pub fn secondary_button(theme: &iced::Theme, status: button::Status) -> button::Style {
    with_radius(button::secondary(theme, status), RADIUS_MD)
}

/// Style for a pill-style sub-tab button (the Request/Response/Collection
/// sub-tab strips): the active tab is filled with a tint of the accent color
/// and outlined, the rest are plain text that light up on hover.
///
/// Every state keeps a 1px border of the same width - only its color changes -
/// so the label never shifts when the active tab moves.
pub fn subtab_style(active: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |theme: &iced::Theme, status| {
        let colors = crate::theme::colors();
        if active {
            button::Style {
                background: Some(tint(colors.text_accent, 0.12).into()),
                text_color: colors.text_accent,
                border: Border {
                    radius: RADIUS_MD.into(),
                    width: 1.0,
                    color: tint(colors.text_accent, 0.35),
                },
                ..Default::default()
            }
        } else {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: hovered.then(|| colors.element_hover.into()),
                text_color: if hovered {
                    colors.text
                } else {
                    colors.text_muted
                },
                border: Border {
                    radius: RADIUS_MD.into(),
                    width: 1.0,
                    color: Color::TRANSPARENT,
                },
                ..button::text(theme, status)
            }
        }
    }
}

/// Style for a flat, icon-only button (a row's remove "✕", a tab's close "×",
/// a collapse arrow): muted at rest with a soft neutral wash on hover/press.
pub fn icon_button(theme: &iced::Theme, status: button::Status) -> button::Style {
    let colors = crate::theme::colors();
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: if hovered {
            Some(colors.element_hover.into())
        } else {
            None
        },
        text_color: if hovered {
            colors.text
        } else {
            colors.text_muted
        },
        border: Border {
            radius: RADIUS_SM.into(),
            ..Default::default()
        },
        ..button::text(theme, status)
    }
}

// ── Surfaces ──────────────────────────────────────────────────────────────

/// A recessed surface holding an editor, table or code block: the editor
/// background inside a hairline border. Replaces the raw `bordered_box`.
pub fn editor_surface(_theme: &iced::Theme) -> container::Style {
    let colors = crate::theme::colors();
    container::Style {
        background: Some(colors.editor_background.into()),
        text_color: Some(colors.text),
        border: Border {
            color: colors.border_variant,
            width: 1.0,
            radius: RADIUS_MD.into(),
        },
        ..Default::default()
    }
}

/// A subtle raised card (metadata chips, list items): surface background with a
/// faint border, the look used for badges and callouts.
pub fn card_surface(_theme: &iced::Theme) -> container::Style {
    let colors = crate::theme::colors();
    container::Style {
        background: Some(colors.surface_background.into()),
        text_color: Some(colors.text),
        border: Border {
            color: colors.border_variant,
            width: 1.0,
            radius: RADIUS_MD.into(),
        },
        ..Default::default()
    }
}

/// A filled, outlined chip in a tint of `color` - the shared look for status
/// badges and callouts.
pub fn chip(color: Color, background_alpha: f32, border_alpha: f32) -> container::Style {
    container::Style {
        background: Some(tint(color, background_alpha).into()),
        border: Border {
            color: tint(color, border_alpha),
            width: 1.0,
            radius: RADIUS_MD.into(),
        },
        ..Default::default()
    }
}

/// Style for a sidebar/tree row: filled with the selection color when selected,
/// a soft neutral wash on hover, plain otherwise.
pub fn list_row(selected: bool, hovered: bool) -> impl Fn(&iced::Theme) -> container::Style {
    move |theme: &iced::Theme| {
        if selected {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(palette.primary.weak.color.into()),
                text_color: Some(palette.primary.weak.text),
                border: Border {
                    radius: RADIUS_SM.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        } else if hovered {
            container::Style {
                background: Some(crate::theme::colors().element_hover.into()),
                border: Border {
                    radius: RADIUS_SM.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        } else {
            container::Style::default()
        }
    }
}

// ── Text fields ───────────────────────────────────────────────────────────

/// The shared look for text inputs and search fields: editor background,
/// hairline border, accent ring on focus.
pub fn input_style(_theme: &iced::Theme, status: text_input::Status) -> text_input::Style {
    let colors = crate::theme::colors();
    let base = text_input::Style {
        background: colors.editor_background.into(),
        border: Border {
            radius: RADIUS_MD.into(),
            width: 1.0,
            color: colors.border_variant,
        },
        icon: colors.text_muted,
        placeholder: colors.text_placeholder,
        value: colors.text,
        selection: colors.selection,
    };

    match status {
        text_input::Status::Active => base,
        text_input::Status::Hovered => text_input::Style {
            border: Border {
                color: colors.border,
                ..base.border
            },
            ..base
        },
        text_input::Status::Focused { .. } => text_input::Style {
            border: Border {
                color: colors.text_accent,
                ..base.border
            },
            ..base
        },
        text_input::Status::Disabled => text_input::Style {
            background: colors.surface_background.into(),
            value: colors.text_placeholder,
            ..base
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_keeps_rgb_and_scales_alpha() {
        let color = Color::from_rgba(0.2, 0.4, 0.6, 0.5);
        let tinted = tint(color, 0.5);
        assert_eq!((tinted.r, tinted.g, tinted.b), (0.2, 0.4, 0.6));
        assert_eq!(tinted.a, 0.25);
    }

    #[test]
    fn tint_of_an_opaque_color_uses_the_given_alpha() {
        assert_eq!(tint(Color::WHITE, 0.12).a, 0.12);
    }
}
