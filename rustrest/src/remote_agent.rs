use std::fs;
use std::path::{Path, PathBuf};

use self_update::Extract;

use crate::updater::{download_to_file, find_file, verify_sha256};

const REPO_OWNER: &str = "sojebsikder";
const REPO_NAME: &str = "rustrest";
const BIN_NAME: &str = "rustrest-remote-agent";

fn bin_file_name(target: &str) -> String {
    if target.contains("windows") {
        format!("{BIN_NAME}.exe")
    } else {
        BIN_NAME.to_string()
    }
}

fn archive_name(target: &str) -> String {
    if target.contains("windows") {
        format!("{BIN_NAME}-{target}.zip")
    } else {
        format!("{BIN_NAME}-{target}.tar.xz")
    }
}

fn cache_path(target: &str, app_version: &str) -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(crate::APP_NAME)
        .join("remote_agents")
        .join(app_version)
        .join(target)
        .join(bin_file_name(target))
}

/// returns the bytes of a `rustrest-remote-agent` binary built for `target`
/// and matching `app_version` - from the local cache if a prior connect
/// already fetched this exact combination, otherwise downloaded fresh.
pub fn provision(target: &str, app_version: &str) -> Result<Vec<u8>, String> {
    let cached = cache_path(target, app_version);
    if let Ok(bytes) = fs::read(&cached) {
        return Ok(bytes);
    }

    let tmp_dir = std::env::temp_dir().join(format!(
        ".rustrest-remote-agent-fetch-{}",
        std::process::id()
    ));
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let result = provision_into(&tmp_dir, target, app_version, &cached);
    let _ = fs::remove_dir_all(&tmp_dir);
    result
}

fn provision_into(
    tmp_dir: &Path,
    target: &str,
    app_version: &str,
    cache_dest: &Path,
) -> Result<Vec<u8>, String> {
    let archive = archive_name(target);
    let base_url =
        format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/download/v{app_version}");

    let archive_path = tmp_dir.join(&archive);
    download_to_file(&format!("{base_url}/{archive}"), &archive_path)?;

    let checksum_path = tmp_dir.join(format!("{archive}.sha256"));
    download_to_file(&format!("{base_url}/{archive}.sha256"), &checksum_path)?;

    verify_sha256(&archive_path, &checksum_path)?;

    let extract_dir = tmp_dir.join("extracted");
    Extract::from_source(&archive_path)
        .extract_into(&extract_dir)
        .map_err(|e| e.to_string())?;

    let extracted_bin = find_file(&extract_dir, &bin_file_name(target))?;

    if let Some(parent) = cache_dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&extracted_bin, cache_dest).map_err(|e| e.to_string())?;

    fs::read(cache_dest).map_err(|e| e.to_string())
}
