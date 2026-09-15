use crate::ui::settings::AppTheme;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSettings {
    pub theme: AppTheme,
    #[serde(default = "default_true")]
    pub close_on_outside_click: bool,
}

fn default_true() -> bool {
    true
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            theme: AppTheme::default(),
            close_on_outside_click: true,
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join(crate::APP_NAME);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("settings.json"))
}

pub fn save(settings: &PersistedSettings) {
    if let Some(path) = settings_path() {
        if let Ok(json) = serde_json::to_string_pretty(settings) {
            let _ = std::fs::write(path, json);
        }
    }
}

pub fn load() -> PersistedSettings {
    settings_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}
