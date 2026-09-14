//! persists small app-level preferences (currently just the theme) to a
//! `settings.json` next to the session/workspace files. add fields to
//! `PersistedSettings` as new settings are introduced.

use crate::ui::settings::AppTheme;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedSettings {
    theme: AppTheme,
}

fn settings_path() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join(crate::APP_NAME);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("settings.json"))
}

pub fn save(theme: AppTheme) {
    if let Some(path) = settings_path() {
        if let Ok(json) = serde_json::to_string_pretty(&PersistedSettings { theme }) {
            let _ = std::fs::write(path, json);
        }
    }
}

pub fn load() -> Option<AppTheme> {
    let path = settings_path()?;
    let content = std::fs::read_to_string(path).ok()?;
    let parsed: PersistedSettings = serde_json::from_str(&content).ok()?;
    Some(parsed.theme)
}
