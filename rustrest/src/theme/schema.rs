//! On-disk theme format. Deliberately the same as Zed's theme family JSON
//! (https://zed.dev/schema/themes/v0.2.0.json), so a Zed theme file (or a
//! whole Zed theme extension) can be dropped into Rustrest unchanged:
//!
//! ```json
//! {
//!   "$schema": "https://zed.dev/schema/themes/v0.2.0.json",
//!   "name": "My Theme",
//!   "author": "me",
//!   "themes": [
//!     { "name": "My Theme Dark", "appearance": "dark", "style": { "editor.background": "#1e1e2eff", ... } }
//!   ]
//! }
//! ```
//!
//! `style` is kept as a raw JSON map rather than a fixed struct: Zed themes
//! carry hundreds of keys (most of them `null`), and Rustrest only reads the
//! ones it has a use for - see `colors::ThemeColors`.

use iced::Color;
use serde::{Deserialize, Serialize};

pub type StyleMap = serde_json::Map<String, serde_json::Value>;

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeFamilyContent {
    #[serde(default)]
    pub author: Option<String>,
    pub themes: Vec<ThemeContent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeContent {
    pub name: String,
    pub appearance: Appearance,
    #[serde(default)]
    pub style: StyleMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
}

impl Appearance {
    pub fn is_dark(self) -> bool {
        self == Appearance::Dark
    }
}

pub fn parse_family(json: &str) -> Result<ThemeFamilyContent, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

/// parses `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (the `#` is optional).
pub fn parse_color(value: &str) -> Option<Color> {
    let hex = value.trim().trim_start_matches('#');
    let nibble = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|v| v * 17);
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let (r, g, b, a) = match hex.len() {
        3 => (nibble(0)?, nibble(1)?, nibble(2)?, 255),
        4 => (nibble(0)?, nibble(1)?, nibble(2)?, nibble(3)?),
        6 => (byte(0)?, byte(2)?, byte(4)?, 255),
        8 => (byte(0)?, byte(2)?, byte(4)?, byte(6)?),
        _ => return None,
    };
    Some(Color::from_rgba8(r, g, b, a as f32 / 255.0))
}

/// looks up a color-valued key; missing, `null` and unparsable values are
/// all treated as "not set".
pub fn color(style: &StyleMap, key: &str) -> Option<Color> {
    style.get(key)?.as_str().and_then(parse_color)
}

/// merges `overrides` over `style`, one level deep - except `syntax`, whose
/// per-token entries are merged individually so an override can restyle a
/// single token without having to restate the whole syntax table.
pub fn merge(style: &StyleMap, overrides: &StyleMap) -> StyleMap {
    let mut merged = style.clone();
    for (key, value) in overrides {
        match (key.as_str(), merged.get_mut(key), value) {
            ("syntax", Some(serde_json::Value::Object(base)), serde_json::Value::Object(over)) => {
                for (token, token_style) in over {
                    base.insert(token.clone(), token_style.clone());
                }
            }
            _ => {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_hex_form() {
        assert_eq!(parse_color("#fff"), Some(Color::WHITE));
        assert_eq!(parse_color("ffffffff"), Some(Color::WHITE));
        assert_eq!(parse_color("#000000"), Some(Color::BLACK));
        let half = parse_color("#ff000080").unwrap();
        assert!((half.a - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(parse_color("#12"), None);
        assert_eq!(parse_color("#gggggg"), None);
    }

    #[test]
    fn parses_a_zed_theme_family() {
        let family = parse_family(
            r##"{
                "$schema": "https://zed.dev/schema/themes/v0.2.0.json",
                "name": "Test",
                "author": "me",
                "themes": [{
                    "name": "Test Dark",
                    "appearance": "dark",
                    "style": {
                        "editor.background": "#101010ff",
                        "text": null,
                        "players": [],
                        "syntax": { "keyword": { "color": "#ff0000ff", "font_style": null, "font_weight": 700 } }
                    }
                }]
            }"##,
        )
        .unwrap();
        assert_eq!(family.themes.len(), 1);
        let style = &family.themes[0].style;
        assert!(color(style, "editor.background").is_some());
        assert!(color(style, "text").is_none());
    }

    #[test]
    fn merge_keeps_untouched_syntax_tokens() {
        let base: StyleMap = serde_json::from_str(
            r##"{ "text": "#fff", "syntax": { "keyword": {"color": "#f00"}, "string": {"color": "#0f0"} } }"##,
        )
        .unwrap();
        let over: StyleMap =
            serde_json::from_str(r##"{ "syntax": { "keyword": {"color": "#00f"} } }"##).unwrap();
        let merged = merge(&base, &over);
        let syntax = merged["syntax"].as_object().unwrap();
        assert_eq!(syntax["keyword"]["color"], "#00f");
        assert_eq!(syntax["string"]["color"], "#0f0");
        assert_eq!(merged["text"], "#fff");
    }
}
