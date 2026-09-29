//! Themes, Zed-style.
//!
//! - Theme files use Zed's theme family JSON format (`schema`), so Zed
//!   themes work as is.
//! - Themes come from (later sources override earlier ones by name): iced's
//!   built-in themes, the ones bundled with Rustrest, installed extensions
//!   (`plugins/<id>/themes/*.json`, like Zed's `extensions/installed/<id>/themes`)
//!   and the user's own `themes/*.json` - see `loader`.
//! - `settings.json` picks one with `"theme": "Name"` or
//!   `"theme": { "mode": "system", "light": "..", "dark": ".." }`, and can
//!   patch any theme with `"theme_overrides": { "Name": { "<key>": .. } }`.

pub mod colors;
pub mod loader;
pub mod schema;
pub mod syntax;

pub use colors::ThemeColors;
pub use schema::{Appearance, StyleMap};
pub use syntax::SyntaxTheme;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, RwLock};

/// where a theme was loaded from.
#[derive(Debug, Clone, PartialEq)]
pub enum ThemeSource {
    /// one of iced's built-in themes (`Light`, `Dark`, `Dracula`, ...).
    Iced,
    /// shipped inside the Rustrest binary.
    Bundled,
    /// from an installed extension, by extension id.
    Extension(String),
    /// from a file in the user's themes folder.
    User(PathBuf),
}

impl ThemeSource {
    pub fn label(&self) -> String {
        match self {
            ThemeSource::Iced | ThemeSource::Bundled => "Built-in".to_string(),
            ThemeSource::Extension(id) => format!("Extension: {id}"),
            ThemeSource::User(_) => "User".to_string(),
        }
    }
}

/// one selectable theme, not yet resolved into colors.
#[derive(Debug, Clone)]
pub struct ThemeEntry {
    pub name: String,
    pub appearance: Appearance,
    pub author: Option<String>,
    pub source: ThemeSource,
    /// the Zed `style` map (empty for iced's built-ins).
    pub style: StyleMap,
    /// the iced theme this entry is (for `ThemeSource::Iced`) - used as-is
    /// when there's nothing to override, and as the base palette otherwise.
    pub iced: Option<iced::Theme>,
}

impl ThemeEntry {
    fn base_palette(&self) -> iced::theme::Palette {
        match &self.iced {
            Some(theme) => theme.palette(),
            None if self.appearance.is_dark() => iced::theme::Palette::DARK,
            None => iced::theme::Palette::LIGHT,
        }
    }

    /// resolves this entry (plus any user overrides) into everything the UI paints with.
    pub fn build(&self, overrides: Option<&StyleMap>) -> ActiveTheme {
        let style = match overrides {
            Some(o) if !o.is_empty() => schema::merge(&self.style, o),
            _ => self.style.clone(),
        };

        let iced = match &self.iced {
            Some(theme) if style.is_empty() => theme.clone(),
            _ => {
                let palette = colors::palette_from_style(self.base_palette(), &style);
                let for_extended = style.clone();
                iced::Theme::custom_with_fn(self.name.clone(), palette, move |p| {
                    colors::extended_from_style(p, &for_extended)
                })
            }
        };

        let palette = iced.palette();
        let colors =
            ThemeColors::from_style(&palette, iced.extended_palette(), self.appearance, &style);
        let syntax =
            SyntaxTheme::from_style(&style, colors.editor_foreground, colors.editor_background)
                .unwrap_or_else(|| SyntaxTheme::fallback(self.appearance.is_dark()));

        ActiveTheme {
            name: self.name.clone(),
            iced,
            colors: Arc::new(colors),
            syntax,
        }
    }
}

/// a fully resolved theme.
#[derive(Debug, Clone)]
pub struct ActiveTheme {
    pub name: String,
    pub iced: iced::Theme,
    pub colors: Arc<ThemeColors>,
    pub syntax: SyntaxTheme,
}

/// every theme currently available, sorted by name.
#[derive(Debug, Clone, Default)]
pub struct ThemeRegistry {
    themes: Vec<ThemeEntry>,
    /// one message per theme file that failed to load.
    pub errors: Vec<String>,
}

impl ThemeRegistry {
    /// adds `entries` in order, each replacing any earlier theme of the same name
    pub fn extend(&mut self, entries: impl IntoIterator<Item = ThemeEntry>) {
        for entry in entries {
            match self.themes.iter_mut().find(|t| t.name == entry.name) {
                Some(existing) => *existing = entry,
                None => self.themes.push(entry),
            }
        }
        self.themes.sort_by_key(|t| t.name.to_lowercase());
    }

    pub fn all(&self) -> &[ThemeEntry] {
        &self.themes
    }

    pub fn get(&self, name: &str) -> Option<&ThemeEntry> {
        self.themes.iter().find(|t| t.name == name)
    }

    pub fn names(&self, appearance: Option<Appearance>) -> Vec<String> {
        self.themes
            .iter()
            .filter(|t| appearance.is_none_or(|a| t.appearance == a))
            .map(|t| t.name.clone())
            .collect()
    }

    /// `name`, or the default theme for `appearance` if it isn't installed
    /// (e.g. its extension was removed).
    pub fn get_or_default(&self, name: &str, appearance: Appearance) -> ThemeEntry {
        self.get(name)
            .or_else(|| self.get(default_theme_name(appearance)))
            .cloned()
            .unwrap_or_else(|| loader::iced_entry(default_iced(appearance)))
    }
}

pub fn default_theme_name(appearance: Appearance) -> &'static str {
    match appearance {
        Appearance::Light => "Light",
        Appearance::Dark => "Dark",
    }
}

fn default_iced(appearance: Appearance) -> iced::Theme {
    match appearance {
        Appearance::Light => iced::Theme::Light,
        Appearance::Dark => iced::Theme::Dark,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    /// follow the OS light/dark setting.
    #[default]
    System,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System];

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
            ThemeMode::System => "System",
        }
    }
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// the `"theme"` setting, in the same two shapes Zed accepts. The old
/// `"Light"`/`"Dark"` values Rustrest used to write are just `Static`
/// selections of iced's themes of those names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThemeSelection {
    Static(String),
    Dynamic {
        #[serde(default)]
        mode: ThemeMode,
        light: String,
        dark: String,
    },
}

impl Default for ThemeSelection {
    fn default() -> Self {
        ThemeSelection::Dynamic {
            mode: ThemeMode::System,
            light: default_theme_name(Appearance::Light).to_string(),
            dark: default_theme_name(Appearance::Dark).to_string(),
        }
    }
}

impl ThemeSelection {
    /// the appearance a `Dynamic` selection currently resolves to.
    fn slot(mode: ThemeMode, system: Appearance) -> Appearance {
        match mode {
            ThemeMode::Light => Appearance::Light,
            ThemeMode::Dark => Appearance::Dark,
            ThemeMode::System => system,
        }
    }

    /// theme name to show, and the appearance to fall back to if it's missing.
    pub fn resolve(&self, system: Appearance) -> (&str, Appearance) {
        match self {
            ThemeSelection::Static(name) => (name, Appearance::Dark),
            ThemeSelection::Dynamic { mode, light, dark } => match Self::slot(*mode, system) {
                Appearance::Light => (light, Appearance::Light),
                Appearance::Dark => (dark, Appearance::Dark),
            },
        }
    }

    pub fn mode(&self) -> Option<ThemeMode> {
        match self {
            ThemeSelection::Static(_) => None,
            ThemeSelection::Dynamic { mode, .. } => Some(*mode),
        }
    }

    /// picks `name`: replaces a static selection, or fills whichever slot of  a dynamic one is currently showing.
    pub fn select(&mut self, name: &str, system: Appearance) {
        match self {
            ThemeSelection::Static(current) => *current = name.to_string(),
            ThemeSelection::Dynamic { mode, light, dark } => match Self::slot(*mode, system) {
                Appearance::Light => *light = name.to_string(),
                Appearance::Dark => *dark = name.to_string(),
            },
        }
    }

    /// sets the light or dark slot directly (the Settings dropdowns).
    pub fn select_for(&mut self, appearance: Appearance, name: &str) {
        if let ThemeSelection::Static(current) = self {
            let current = current.clone();
            *self = ThemeSelection::Dynamic {
                mode: ThemeMode::System,
                light: current.clone(),
                dark: current,
            };
        }
        if let ThemeSelection::Dynamic { light, dark, .. } = self {
            match appearance {
                Appearance::Light => *light = name.to_string(),
                Appearance::Dark => *dark = name.to_string(),
            }
        }
    }

    /// switches to `mode`, turning a static selection into a dynamic one
    /// that keeps the current theme in the slot matching its appearance.
    pub fn set_mode(&mut self, new_mode: ThemeMode, registry: &ThemeRegistry) {
        match self {
            ThemeSelection::Dynamic { mode, .. } => *mode = new_mode,
            ThemeSelection::Static(name) => {
                let appearance = registry
                    .get(name)
                    .map(|t| t.appearance)
                    .unwrap_or(Appearance::Dark);
                let (mut light, mut dark) = (
                    default_theme_name(Appearance::Light).to_string(),
                    default_theme_name(Appearance::Dark).to_string(),
                );
                match appearance {
                    Appearance::Light => light = name.clone(),
                    Appearance::Dark => dark = name.clone(),
                }
                *self = ThemeSelection::Dynamic {
                    mode: new_mode,
                    light,
                    dark,
                };
            }
        }
    }
}

/// `"theme_overrides"`: theme name -> Zed style keys to patch onto it.
pub type ThemeOverrides = HashMap<String, StyleMap>;

struct Published {
    colors: Arc<ThemeColors>,
    syntax: SyntaxTheme,
}

static PUBLISHED: LazyLock<RwLock<Published>> = LazyLock::new(|| {
    let theme = loader::iced_entry(iced::Theme::Dark).build(None);
    RwLock::new(Published {
        colors: theme.colors,
        syntax: theme.syntax,
    })
});

/// makes `theme` the one `colors()`/`syntax()` return.
pub fn publish(theme: &ActiveTheme) {
    let mut published = PUBLISHED.write().unwrap_or_else(|e| e.into_inner());
    published.colors = theme.colors.clone();
    published.syntax = theme.syntax;
}

/// active theme's app-specific colors.
pub fn colors() -> Arc<ThemeColors> {
    PUBLISHED
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .colors
        .clone()
}

/// active theme's syntax highlighting theme.
pub fn syntax() -> SyntaxTheme {
    PUBLISHED.read().unwrap_or_else(|e| e.into_inner()).syntax
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_zed_settings_shapes_deserialize() {
        let legacy: ThemeSelection = serde_json::from_str(r#""Dark""#).unwrap();
        assert_eq!(legacy, ThemeSelection::Static("Dark".into()));

        let dynamic: ThemeSelection = serde_json::from_str(
            r#"{ "mode": "system", "light": "One Light", "dark": "One Dark" }"#,
        )
        .unwrap();
        assert_eq!(
            dynamic.resolve(Appearance::Light),
            ("One Light", Appearance::Light)
        );
        assert_eq!(
            dynamic.resolve(Appearance::Dark),
            ("One Dark", Appearance::Dark)
        );
    }

    #[test]
    fn select_fills_the_visible_slot() {
        let mut selection = ThemeSelection::default();
        selection.select("Nord", Appearance::Dark);
        assert_eq!(selection.resolve(Appearance::Dark).0, "Nord");
        assert_eq!(selection.resolve(Appearance::Light).0, "Light");
    }

    #[test]
    fn set_mode_keeps_the_static_theme() {
        let mut registry = ThemeRegistry::default();
        registry.extend(loader::iced_entries());
        let mut selection = ThemeSelection::Static("Solarized Light".into());
        selection.set_mode(ThemeMode::Dark, &registry);
        assert_eq!(selection.resolve(Appearance::Light).0, "Dark");
        selection.set_mode(ThemeMode::Light, &registry);
        assert_eq!(selection.resolve(Appearance::Dark).0, "Solarized Light");
    }

    #[test]
    fn user_themes_shadow_built_ins() {
        let mut registry = ThemeRegistry::default();
        registry.extend(loader::iced_entries());
        let before = registry.all().len();
        let mut replacement = loader::iced_entry(iced::Theme::Dark);
        replacement.source = ThemeSource::User(PathBuf::from("dark.json"));
        registry.extend([replacement]);
        assert_eq!(registry.all().len(), before);
        assert!(matches!(
            registry.get("Dark").unwrap().source,
            ThemeSource::User(_)
        ));
    }

    #[test]
    fn overrides_patch_a_built_in() {
        let entry = loader::iced_entry(iced::Theme::Dark);
        let overrides: StyleMap =
            serde_json::from_str(r##"{ "editor.background": "#000000ff" }"##).unwrap();
        let theme = entry.build(Some(&overrides));
        assert_eq!(theme.iced.palette().background, iced::Color::BLACK);
        assert_eq!(theme.colors.editor_background, iced::Color::BLACK);
    }
}
