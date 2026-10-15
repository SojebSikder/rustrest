//! Shared visual tokens: corner radii and color helpers, so the many
//! hand-styled widgets across the UI stay consistent instead of each picking
//! their own magic numbers.
//!
//! Only the three "steps" of corner radius the UI actually uses live here;
//! anything that is deliberately a different shape (the window outline, pill
//! badges) keeps its own value at the call site.

use iced::widget::{button, container};
use iced::{Border, Color};

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

/// Style for a pill-style sub-tab button (the Request/Response/Collection
/// sub-tab strips): the active tab is filled with a tint of the accent color
/// and outlined, the rest are plain text that light up on hover.
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
            match status {
                button::Status::Hovered => button::Style {
                    background: Some(colors.element_hover.into()),
                    text_color: colors.text,
                    border: Border {
                        radius: RADIUS_MD.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                _ => button::Style {
                    text_color: colors.text_muted,
                    ..button::text(theme, status)
                },
            }
        }
    }
}

/// iced's secondary button with the UI's standard corner radius, so metadata
/// buttons (timing, size, save, back) match every other control.
pub fn secondary_button(theme: &iced::Theme, status: button::Status) -> button::Style {
    button::Style {
        border: Border {
            radius: RADIUS_MD.into(),
            ..button::secondary(theme, status).border
        },
        ..button::secondary(theme, status)
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
