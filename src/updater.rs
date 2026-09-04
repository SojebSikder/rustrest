use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use self_update::cargo_crate_version;
use self_update::{Extract, Move};
use sha2::{Digest, Sha256};

const REPO_OWNER: &str = "sojebsikder"; // github username
const REPO_NAME: &str = "rustrest";
const BIN_NAME: &str = "rustrest"; // name of the release asset binary

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

fn latest_release_tag() -> Result<String, String> {
    let url = format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/latest");
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(&url).send().map_err(|e| e.to_string())?;
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "could not determine the latest release version".to_string())?;

    let tag = location
        .rfind("/tag/")
        .map(|idx| &location[idx + "/tag/".len()..])
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| "could not parse the latest release version".to_string())?;

    Ok(tag.strip_prefix('v').unwrap_or(tag).to_string())
}

/// check for updates on github
pub fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    let current_version = cargo_crate_version!();
    let latest_version = latest_release_tag()?;

    if self_update::version::bump_is_greater(current_version, &latest_version)
        .map_err(|e| e.to_string())?
    {
        Ok(Some(UpdateInfo {
            version: latest_version,
            notes: None,
        }))
    } else {
        Ok(None)
    }
}

fn detect_target() -> Result<&'static str, String> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        (os, arch) => Err(format!("unsupported platform: {os}-{arch}")),
    }
}

fn archive_name(target: &str) -> String {
    if target.contains("windows") {
        format!("{BIN_NAME}-{target}.zip")
    } else {
        format!("{BIN_NAME}-{target}.tar.xz")
    }
}

fn download_to_file(url: &str, dest: &Path) -> Result<(), String> {
    let mut response = reqwest::blocking::get(url).map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "failed to download {url}: HTTP {}",
            response.status()
        ));
    }
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    response.copy_to(&mut file).map_err(|e| e.to_string())?;
    Ok(())
}

fn verify_sha256(path: &Path, sha256_path: &Path) -> Result<(), String> {
    let sums = fs::read_to_string(sha256_path).map_err(|e| e.to_string())?;
    let expected = sums
        .split_whitespace()
        .next()
        .ok_or_else(|| "empty checksum file".to_string())?
        .to_lowercase();

    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    let actual = format!("{:x}", hasher.finalize());

    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "checksum mismatch for {}: expected {expected}, got {actual}",
            path.display()
        ))
    }
}

/// recursively searches `dir` for a file named `name`, depth-first.
fn find_file(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut subdirs = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if path.file_name().is_some_and(|f| f == name) {
            return Ok(path);
        }
    }

    for subdir in subdirs {
        if let Ok(found) = find_file(&subdir, name) {
            return Ok(found);
        }
    }

    Err(format!(
        "could not find '{name}' inside the downloaded archive"
    ))
}

pub fn perform_update() -> Result<String, String> {
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;

    let install_dir = current_exe
        .parent()
        .ok_or_else(|| "could not determine the install directory".to_string())?;
    let tmp_dir = install_dir.join(format!(".rustrest-update-{}", std::process::id()));
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;

    let result = perform_update_into(&tmp_dir, &current_exe);
    let _ = fs::remove_dir_all(&tmp_dir);
    result
}

fn perform_update_into(tmp_dir: &Path, current_exe: &Path) -> Result<String, String> {
    let version = latest_release_tag()?;
    let target = detect_target()?;
    let archive = archive_name(target);
    let base_url =
        format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/download/v{version}");

    let archive_path = tmp_dir.join(&archive);
    download_to_file(&format!("{base_url}/{archive}"), &archive_path)?;

    let checksum_path = tmp_dir.join(format!("{archive}.sha256"));
    download_to_file(&format!("{base_url}/{archive}.sha256"), &checksum_path)?;

    verify_sha256(&archive_path, &checksum_path)?;

    let extract_dir = tmp_dir.join("extracted");
    Extract::from_source(&archive_path)
        .extract_into(&extract_dir)
        .map_err(|e| e.to_string())?;

    let bin_file_name = if target.contains("windows") {
        format!("{BIN_NAME}.exe")
    } else {
        BIN_NAME.to_string()
    };
    let extracted_bin = find_file(&extract_dir, &bin_file_name)?;

    Move::from_source(&extracted_bin)
        .replace_using_temp(tmp_dir.join("old_bin"))
        .to_dest(current_exe)
        .map_err(|e| e.to_string())?;

    Ok(version)
}
