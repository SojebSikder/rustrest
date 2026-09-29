//! Finds theme files and turns them into `ThemeEntry`s, and watches the
//! places they live so edits show up live.

use super::schema::{self, Appearance};
use super::{ThemeEntry, ThemeRegistry, ThemeSource};
use crate::message::Message;
use iced::futures::{SinkExt, StreamExt, stream::BoxStream};
use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, new_debouncer};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// theme families compiled into the binary.
const BUNDLED: &[&str] = &[include_str!("../../../assets/themes/one.json")];

/// `<data dir>/Rustrest/themes` - the user's own theme files.
pub fn user_themes_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(crate::APP_NAME).join("themes"))
}

pub fn iced_entry(theme: iced::Theme) -> ThemeEntry {
    ThemeEntry {
        name: theme.to_string(),
        appearance: if theme.extended_palette().is_dark {
            Appearance::Dark
        } else {
            Appearance::Light
        },
        author: None,
        source: ThemeSource::Iced,
        style: Default::default(),
        iced: Some(theme),
    }
}

pub fn iced_entries() -> Vec<ThemeEntry> {
    iced::Theme::ALL.iter().cloned().map(iced_entry).collect()
}

/// every theme in one Zed theme family file.
pub fn entries_from_json(json: &str, source: ThemeSource) -> Result<Vec<ThemeEntry>, String> {
    let family = schema::parse_family(json)?;
    Ok(family
        .themes
        .into_iter()
        .map(|theme| ThemeEntry {
            name: theme.name,
            appearance: theme.appearance,
            author: family.author.clone(),
            source: source.clone(),
            style: theme.style,
            iced: None,
        })
        .collect())
}

pub fn entries_from_file(path: &Path, source: ThemeSource) -> Result<Vec<ThemeEntry>, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    entries_from_json(&json, source).map_err(|e| format!("{}: {e}", path.display()))
}

/// the `*.json` files directly inside `dir`, sorted for a stable load order.
pub fn json_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    files
}

/// builds the full registry. `extension_themes` is `(extension id, theme file)`
/// for every enabled extension, from `PluginManager::theme_files`.
pub fn load_registry(extension_themes: &[(String, PathBuf)]) -> ThemeRegistry {
    let mut registry = ThemeRegistry::default();
    registry.extend(iced_entries());

    for json in BUNDLED {
        match entries_from_json(json, ThemeSource::Bundled) {
            Ok(entries) => registry.extend(entries),
            Err(e) => registry.errors.push(format!("bundled theme: {e}")),
        }
    }

    for (id, path) in extension_themes {
        match entries_from_file(path, ThemeSource::Extension(id.clone())) {
            Ok(entries) => registry.extend(entries),
            Err(e) => registry.errors.push(e),
        }
    }

    if let Some(dir) = user_themes_dir() {
        for path in json_files_in(&dir) {
            match entries_from_file(&path, ThemeSource::User(path.clone())) {
                Ok(entries) => registry.extend(entries),
                Err(e) => registry.errors.push(e),
            }
        }
    }

    registry
}

/// copies a theme file into the user themes folder after checking it parses,
/// returning the names of the themes it adds.
pub fn import_file(source: &Path) -> Result<Vec<String>, String> {
    let entries = entries_from_file(source, ThemeSource::User(source.to_path_buf()))?;
    if entries.is_empty() {
        return Err("the file doesn't define any themes".to_string());
    }

    let dir = user_themes_dir().ok_or("no data directory on this platform")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file_name = source.file_name().ok_or("invalid file name")?;

    std::fs::copy(source, dir.join(file_name)).map_err(|e| e.to_string())?;
    Ok(entries.into_iter().map(|e| e.name).collect())
}

/// what to watch for theme changes: `(path, recursive)`.
pub fn watch_targets(plugins_dir: &Path) -> Vec<(PathBuf, bool)> {
    let mut targets = Vec::new();
    if let Some(dir) = user_themes_dir() {
        let _ = std::fs::create_dir_all(&dir);
        targets.push((dir, false));
    }
    if plugins_dir.is_dir() {
        targets.push((plugins_dir.to_path_buf(), true));
    }
    // settings.json lives here, `is_relevant` filters out the app's other files
    // (the session autosave writes every few seconds).
    if let Some(dir) = dirs::data_dir().map(|d| d.join(crate::APP_NAME)) {
        targets.push((dir, false));
    }
    targets
}

/// whether a changed path could affect themes: a theme json, or settings.json.
pub fn is_relevant(path: &Path) -> bool {
    let is_json = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    let in_themes_dir = path
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "themes");
    let is_settings = path
        .file_name()
        .is_some_and(|n| n == "settings.json" || n == "plugins.json");
    // an extension directory appearing/disappearing (install, uninstall)
    let is_extension_dir = path
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "plugins");
    (is_json && in_themes_dir) || is_settings || is_extension_dir
}

pub fn watch_stream(targets: &Vec<(PathBuf, bool)>) -> BoxStream<'static, Message> {
    let targets = targets.clone();
    iced::stream::channel(16, async move |mut output| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<PathBuf>>();
        let debouncer = new_debouncer(
            Duration::from_millis(200),
            move |result: DebounceEventResult| {
                if let Ok(events) = result {
                    let paths: Vec<PathBuf> = events
                        .into_iter()
                        .map(|e| e.path)
                        .filter(|p| is_relevant(p))
                        .collect();
                    if !paths.is_empty() {
                        let _ = tx.send(paths);
                    }
                }
            },
        );
        let mut debouncer = match debouncer {
            Ok(debouncer) => debouncer,
            Err(err) => {
                eprintln!("Failed to start theme watcher: {err}");
                return;
            }
        };
        for (path, recursive) in &targets {
            let mode = if *recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if let Err(err) = debouncer.watcher().watch(path, mode) {
                eprintln!("Failed to watch {path:?}: {err}");
            }
        }
        while let Some(paths) = rx.recv().await {
            if output
                .send(Message::ThemeFilesChanged(paths))
                .await
                .is_err()
            {
                break;
            }
        }
    })
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_themes_parse() {
        for json in BUNDLED {
            let entries = entries_from_json(json, ThemeSource::Bundled).unwrap();
            assert!(!entries.is_empty());
            for entry in entries {
                // building exercises palette, colors and the syntax table
                let theme = entry.build(None);
                assert_eq!(theme.name, entry.name);
            }
        }
    }

    #[test]
    fn example_theme_extension_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/theme-extension/themes/midnight.json");
        let entries = entries_from_file(&path, ThemeSource::Extension("midnight".into())).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Midnight", "Midnight Light"]);
        for entry in &entries {
            entry.build(None);
        }
    }

    #[test]
    fn relevance_filter() {
        assert!(is_relevant(Path::new("/d/Rustrest/themes/x.json")));
        assert!(is_relevant(Path::new(
            "/d/Rustrest/plugins/ext/themes/x.json"
        )));
        assert!(is_relevant(Path::new("/d/Rustrest/settings.json")));
        assert!(!is_relevant(Path::new("/d/Rustrest/session.json")));
        assert!(!is_relevant(Path::new(
            "/d/Rustrest/plugins/ext/storage/x.json"
        )));
    }
}
