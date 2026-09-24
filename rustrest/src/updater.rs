use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

use self_update::cargo_crate_version;
use self_update::{Extract, Move};
use sha2::{Digest, Sha256};

/// download progress notification for the self-update flow, reported as
/// bytes accumulate so the UI can render a fill percentage. `total` is 0
/// when the server didn't report a `Content-Length`.
#[derive(Debug, Clone, Copy)]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: u64,
}

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

pub(crate) fn download_to_file(url: &str, dest: &Path) -> Result<(), String> {
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

async fn download_to_file_async(url: &str, dest: &Path) -> Result<(), String> {
    let response = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "failed to download {url}: HTTP {}",
            response.status()
        ));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    fs::write(dest, &bytes).map_err(|e| e.to_string())?;
    Ok(())
}

/// downloads `url` into `dest`, reporting a running `UpdateProgress` after
/// every chunk so the UI can drive a fill percentage.
async fn download_to_file_with_progress(
    url: &str,
    dest: &Path,
    mut on_progress: impl FnMut(UpdateProgress),
) -> Result<(), String> {
    let mut response = reqwest::get(url).await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "failed to download {url}: HTTP {}",
            response.status()
        ));
    }
    let total = response.content_length().unwrap_or(0);
    let mut downloaded = 0u64;
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;

    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        on_progress(UpdateProgress { downloaded, total });
    }

    Ok(())
}

pub(crate) fn verify_sha256(path: &Path, sha256_path: &Path) -> Result<(), String> {
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
pub(crate) fn find_file(dir: &Path, name: &str) -> Result<PathBuf, String> {
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

pub async fn perform_update_with_progress(
    mut on_progress: impl FnMut(UpdateProgress) + Send + 'static,
) -> Result<String, String> {
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;

    let install_dir = current_exe
        .parent()
        .ok_or_else(|| "could not determine the install directory".to_string())?
        .to_path_buf();
    let tmp_dir = install_dir.join(format!(".rustrest-update-{}", std::process::id()));
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;

    let result = perform_update_into_with_progress(&tmp_dir, &current_exe, &mut on_progress).await;
    let _ = fs::remove_dir_all(&tmp_dir);
    result
}

async fn perform_update_into_with_progress(
    tmp_dir: &Path,
    current_exe: &Path,
    on_progress: &mut impl FnMut(UpdateProgress),
) -> Result<String, String> {
    let version = tokio::task::spawn_blocking(latest_release_tag)
        .await
        .map_err(|e| e.to_string())??;
    let target = detect_target()?;
    let archive = archive_name(target);
    let base_url =
        format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/download/v{version}");

    let archive_path = tmp_dir.join(&archive);
    download_to_file_with_progress(&format!("{base_url}/{archive}"), &archive_path, on_progress)
        .await?;

    let checksum_path = tmp_dir.join(format!("{archive}.sha256"));
    download_to_file_async(&format!("{base_url}/{archive}.sha256"), &checksum_path).await?;

    let target = target.to_string();
    let tmp_dir = tmp_dir.to_path_buf();
    let current_exe = current_exe.to_path_buf();
    tokio::task::spawn_blocking(move || {
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
            .to_dest(&current_exe)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(version)
}

/// markdown notes of the GitHub release for `version`
pub async fn fetch_release_notes(version: &str) -> Result<String, String> {
    let url = format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/tag/v{version}");
    let response = reqwest::Client::new()
        .get(&url)
        .header(reqwest::header::USER_AGENT, BIN_NAME)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("no GitHub release found for v{version}"));
    }
    if !response.status().is_success() {
        return Err(format!("GitHub returned HTTP {}", response.status()));
    }

    let html = response.text().await.map_err(|e| e.to_string())?;
    // a release published without notes has no body element at all
    let Some(body) = extract_release_body(&html) else {
        return Ok(String::new());
    };
    htmd::convert(body).map_err(|e| e.to_string())
}

/// inner HTML of the release page's rendered-notes `<div>`, matched up to
/// its own closing tag by counting nested `<div>`s.
fn extract_release_body(html: &str) -> Option<&str> {
    const MARKER: &str = r#"data-test-selector="body-content""#;
    let marker = html.find(MARKER)?;
    let start = marker + html[marker..].find('>')? + 1;

    let mut depth = 1;
    let mut pos = start;
    loop {
        let rest = &html[pos..];
        let close = rest.find("</div")?;
        match rest.find("<div") {
            Some(open) if open < close => {
                depth += 1;
                pos += open + "<div".len();
            }
            _ => {
                depth -= 1;
                if depth == 0 {
                    return Some(&html[start..pos + close]);
                }
                pos += close + "</div".len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::extract_release_body;

    #[test]
    fn extracts_nested_release_body() {
        let html = r#"<div class="box"><div data-pjax="true" data-test-selector="body-content" class="markdown-body"><h2>Changelog</h2><div class="snippet"><pre>x</pre></div><p>end</p></div><div>footer</div></div>"#;
        assert_eq!(
            extract_release_body(html),
            Some(r#"<h2>Changelog</h2><div class="snippet"><pre>x</pre></div><p>end</p>"#)
        );
    }

    #[test]
    fn missing_body_is_none() {
        assert_eq!(extract_release_body("<div>no notes</div>"), None);
    }
}
