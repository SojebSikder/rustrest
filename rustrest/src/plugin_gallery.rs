//! "Browse" tab of the Manage Plugins UI: fetches a remote index of
//! installable plugins and installs one by downloading its zip, extracting it,
//! and feeding the result through the same `PluginManager::prepare_install` path
//! a local folder install uses.

use std::fs;
use std::io;
use std::path::Path;

use rustrest_plugin_host::{Engine, Module, PluginManager, PluginManifest};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Raw GitHub content URL for the curated plugin index. This points at a
/// separate repo (like `zed-industries/extensions`) so publishing a plugin
/// is a PR there rather than a change to Rustrest itself.
const DEFAULT_INDEX_URL: &str =
    "https://raw.githubusercontent.com/Rustrest/plugins/main/index.json";

#[derive(Debug, Clone, Deserialize)]
pub struct GalleryEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    /// direct link to a .zip containing `plugin.toml` + `plugin.wasm` at its root
    pub download_url: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GalleryIndex {
    plugins: Vec<GalleryEntry>,
}

pub fn fetch_index() -> Result<Vec<GalleryEntry>, String> {
    let response = reqwest::blocking::get(DEFAULT_INDEX_URL).map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "failed to fetch plugin index: HTTP {}",
            response.status()
        ));
    }
    let index: GalleryIndex = response.json().map_err(|e| e.to_string())?;
    Ok(index.plugins)
}

fn verify_sha256(path: &Path, expected_hex: &str) -> Result<(), String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    let actual = format!("{:x}", hasher.finalize());
    if actual.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(format!(
            "checksum mismatch: expected {expected_hex}, got {actual}"
        ))
    }
}

/// downloads and extracts `entry`'s zip, then runs it through
/// `PluginManager::prepare_install` (compile and copy into `plugins_dir`) -
/// same return shape as the local folder install flow so both can finish
/// through the same `Message::PluginInstallPrepared` handler.
pub fn download_and_prepare(
    engine: &Engine,
    plugins_dir: &Path,
    entry: &GalleryEntry,
) -> Result<(String, PluginManifest, Module), String> {
    let tmp_dir = std::env::temp_dir().join(format!(
        "rustrest-plugin-gallery-{}-{}",
        entry.id,
        std::process::id()
    ));
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let result = download_and_prepare_into(engine, plugins_dir, entry, &tmp_dir);
    let _ = fs::remove_dir_all(&tmp_dir);
    result
}

fn download_and_prepare_into(
    engine: &Engine,
    plugins_dir: &Path,
    entry: &GalleryEntry,
    tmp_dir: &Path,
) -> Result<(String, PluginManifest, Module), String> {
    let archive_path = tmp_dir.join(format!("{}.zip", entry.id));
    crate::updater::download_to_file(&entry.download_url, &archive_path)?;

    if let Some(expected) = &entry.sha256 {
        verify_sha256(&archive_path, expected)?;
    }

    let extract_dir = tmp_dir.join("extracted");
    self_update::Extract::from_source(&archive_path)
        .extract_into(&extract_dir)
        .map_err(|e| e.to_string())?;

    let manifest_path = crate::updater::find_file(&extract_dir, "plugin.toml")?;
    let source_dir = manifest_path
        .parent()
        .ok_or_else(|| "invalid archive layout".to_string())?;

    PluginManager::prepare_install(engine, plugins_dir, source_dir).map_err(|e| e.to_string())
}
