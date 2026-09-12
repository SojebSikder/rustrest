pub use rustrest_core::workspace::{CollectionSource, SavedWorkspace, WorkspaceManifest};

pub fn save(manifest: &WorkspaceManifest) {
    rustrest_core::workspace::save(manifest, crate::APP_NAME);
}

pub fn load() -> Option<WorkspaceManifest> {
    rustrest_core::workspace::load(crate::APP_NAME)
}
