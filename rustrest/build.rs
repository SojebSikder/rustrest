fn main() {
    emit_commit_sha();

    #[cfg(windows)]
    embed_windows_resources();
}

/// exposes the git commit the binary was built from as `RUSTREST_COMMIT_SHA`
/// (shown in Help > About). CI's `GITHUB_SHA` wins; falls back to
/// `git rev-parse`, then "unknown" (e.g. building from a source tarball).
fn emit_commit_sha() {
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs/heads");

    let sha = std::env::var("GITHUB_SHA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| String::from_utf8(out.stdout).ok())
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=RUSTREST_COMMIT_SHA={sha}");
}

#[cfg(windows)]
fn embed_windows_resources() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("../assets/images/app-icon.ico");

    // executable file metadata details in Windows Properties
    res.set("ProductName", "Rustrest");
    res.set("FileDescription", "API Testing Platform");
    res.set("LegalCopyright", "Copyright © 2026");
    res.set("CompanyName", "sojebsikder");

    if let Err(e) = res.compile() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
