use crate::collection::collection::PostmanRequestNode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub enum SavedTabEntry {
    HttpRequest {
        request_id: Option<usize>,
        collection_id: Option<usize>,
        node: PostmanRequestNode,
    },
    CollectionRoot {
        collection_id: usize,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedSession {
    pub tabs: Vec<SavedTabEntry>,
    pub active_tab_index: usize,
    pub next_tab_id: usize,
    pub next_request_id: usize,
}

fn session_path() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join(crate::APP_NAME);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("session.json"))
}

pub fn save(session: &SavedSession) {
    if let Some(path) = session_path() {
        if let Ok(json) = serde_json::to_string_pretty(session) {
            let _ = std::fs::write(path, json);
        }
    }
}

pub fn load() -> Option<SavedSession> {
    let path = session_path()?;
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}
