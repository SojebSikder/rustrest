use self_update::backends::github::Update;
use self_update::cargo_crate_version;

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

/// downloads the matching release asset for the current platform and replaces
/// the running executable in place. Blocking, call via spawn_blocking.
pub fn perform_update() -> Result<String, String> {
    let status = Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .show_download_progress(false)
        .no_confirm(true)
        .current_version(cargo_crate_version!())
        .build()
        .map_err(|e| e.to_string())?
        .update()
        .map_err(|e| e.to_string())?;

    Ok(status.version().to_string())
}
