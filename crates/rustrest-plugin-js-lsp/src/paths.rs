//! naming conventions: the server binary and its release archive URL.

/// version of `rustrest-js-lsp` this plugin downloads. The server is built and
/// published with the app (`v<version>` releases), so this is a Rustrest
/// release version that includes it.
pub const SERVER_VERSION: &str = "0.1.18";
pub const SERVER_NAME: &str = "rustrest-js-lsp";

const RELEASES: &str = "https://github.com/SojebSikder/rustrest/releases/download";

fn is_windows(target: &str) -> bool {
    target.contains("windows")
}

/// executable file name for a host target triple.
pub fn binary_name(target: &str) -> String {
    if is_windows(target) {
        format!("{SERVER_NAME}.exe")
    } else {
        SERVER_NAME.to_string()
    }
}

/// release archive URL for `version` built for `target`.
pub fn release_url(version: &str, target: &str) -> String {
    let ext = if is_windows(target) { "zip" } else { "tar.xz" };
    format!("{RELEASES}/v{version}/{SERVER_NAME}-{target}.{ext}")
}

/// picks the server binary out of an archive's extracted file list.
pub fn find_binary<'a>(files: &'a [String], binary: &str) -> Option<&'a String> {
    files
        .iter()
        .find(|f| f.rsplit(['/', '\\']).next() == Some(binary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_urls() {
        assert_eq!(
            release_url("0.1.18", "x86_64-pc-windows-msvc"),
            "https://github.com/SojebSikder/rustrest/releases/download/v0.1.18/rustrest-js-lsp-x86_64-pc-windows-msvc.zip"
        );
        assert_eq!(
            release_url("0.1.18", "aarch64-apple-darwin"),
            "https://github.com/SojebSikder/rustrest/releases/download/v0.1.18/rustrest-js-lsp-aarch64-apple-darwin.tar.xz"
        );
        assert_eq!(binary_name("x86_64-pc-windows-msvc"), "rustrest-js-lsp.exe");
        assert_eq!(binary_name("x86_64-unknown-linux-gnu"), "rustrest-js-lsp");
    }

    #[test]
    fn binary_lookup() {
        let files = vec![
            "/s/bin/rustrest-js-lsp-x86_64-unknown-linux-gnu/README.md".to_string(),
            "/s/bin/rustrest-js-lsp-x86_64-unknown-linux-gnu/rustrest-js-lsp".to_string(),
        ];
        assert_eq!(find_binary(&files, "rustrest-js-lsp"), Some(&files[1]));
        let files = vec![r"C:\s\bin\rustrest-js-lsp.exe".to_string()];
        assert_eq!(find_binary(&files, "rustrest-js-lsp.exe"), Some(&files[0]));
        assert_eq!(find_binary(&files, "rustrest-js-lsp"), None);
    }
}
