use self_update::backends::github::Update;
use self_update::cargo_crate_version;
use self_update::update::Release;

const REPO_OWNER: &str = "sojebsikder"; // github username
const REPO_NAME: &str = "rustrest";
const BIN_NAME: &str = "rustrest"; // name of the release asset binary

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

/// Check for updates on github
/// note: runs blocking network I/O, call this via spawn_blocking / Task::perform.
pub fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    let updater = Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .current_version(cargo_crate_version!())
        .build()
        .map_err(|e| e.to_string())?;

    let maybe_release: Option<Release> =
        updater.is_update_available().map_err(|e| e.to_string())?;

    Ok(maybe_release.map(|latest| UpdateInfo {
        version: latest.version().to_string(),
        notes: latest.body().map(|s| s.to_string()),
    }))
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
