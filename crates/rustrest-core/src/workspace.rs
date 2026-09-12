use crate::KeyValuePair;
use crate::collection::env::Environment;
use crate::session::SavedSession;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollectionSource {
    File(PathBuf),
    Dir(PathBuf),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedWorkspace {
    pub id: usize,
    pub name: String,
    pub collection_sources: Vec<CollectionSource>,
    pub environments: Vec<Environment>,
    pub active_env_index: Option<usize>,
    #[serde(default)]
    pub globals: Vec<KeyValuePair>,

    // sidebar collapse/expand state, persisted per workspace
    #[serde(default)]
    pub collapsed_collections: HashSet<usize>,
    #[serde(default)]
    pub collapsed_folders: HashSet<(usize, Vec<String>)>,
    /// request ids whose saved-responses list is collapsed in the sidebar.
    #[serde(default)]
    pub collapsed_saved_responses: HashSet<usize>,

    pub session: SavedSession,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub workspaces: Vec<SavedWorkspace>,
    pub active_workspace_id: usize,
    pub next_workspace_id: usize,
}

fn manifest_path(app_name: &str) -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join(app_name);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("workspaces.json"))
}

pub fn save(manifest: &WorkspaceManifest, app_name: &str) {
    if let Some(path) = manifest_path(app_name) {
        if let Ok(json) = serde_json::to_string_pretty(manifest) {
            let _ = std::fs::write(path, json);
        }
    }
}

pub fn load(app_name: &str) -> Option<WorkspaceManifest> {
    let path = manifest_path(app_name)?;
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}
