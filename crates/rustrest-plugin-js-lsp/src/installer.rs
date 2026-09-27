//! locates or installs the `rustrest-js-lsp` binary. Written against a
//! [`Host`] trait so it runs natively with a mock in tests.

use crate::paths::{binary_name, find_binary, release_url, SERVER_NAME, SERVER_VERSION};

const PATH_FILE: &str = "server-path.txt";
const VERSION_FILE: &str = "server-version.txt";

/// the host calls the installer needs.
pub trait Host {
    fn log(&mut self, message: &str);
    fn which(&mut self, name: &str) -> Option<String>;
    fn host_target(&mut self) -> Result<String, String>;
    fn storage_read(&mut self, filename: &str) -> Option<String>;
    fn storage_write(&mut self, filename: &str, contents: &str) -> Result<(), String>;
    fn download_archive(
        &mut self,
        url: &str,
        checksum_url: &str,
        dest_dir: &str,
    ) -> Result<u32, String>;
    fn make_executable(&mut self, path: &str) -> Result<(), String>;
}

struct Download {
    handle: u32,
    binary: String,
}

pub struct Installer<H: Host> {
    host: H,
    download: Option<Download>,
    /// last failure, reported until `reinstall` clears it
    error: Option<String>,
}

impl<H: Host + Default> Default for Installer<H> {
    fn default() -> Self {
        Self::new(H::default())
    }
}

impl<H: Host> Installer<H> {
    pub fn new(host: H) -> Self {
        Self {
            host,
            download: None,
            error: None,
        }
    }

    pub fn host(&self) -> &H {
        &self.host
    }

    /// path of the lsp server binary
    pub fn server_path(&mut self) -> Result<Option<String>, String> {
        if let Some(path) = self.host.which(SERVER_NAME) {
            return Ok(Some(path));
        }
        if let Some(path) = self.installed() {
            return Ok(Some(path));
        }
        if self.download.is_some() {
            return Ok(None);
        }
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        self.start_download().map(|()| None)
    }

    fn installed(&mut self) -> Option<String> {
        let version = self.host.storage_read(VERSION_FILE)?;
        if version.trim() != SERVER_VERSION {
            return None;
        }
        let path = self.host.storage_read(PATH_FILE)?;
        let path = path.trim();
        (!path.is_empty()).then(|| path.to_string())
    }

    fn start_download(&mut self) -> Result<(), String> {
        let result = self.host.host_target().and_then(|target| {
            let url = release_url(SERVER_VERSION, &target);
            let handle = self
                .host
                .download_archive(&url, &format!("{url}.sha256"), "bin")?;
            self.host.log(&format!(
                "downloading {SERVER_NAME} {SERVER_VERSION} from {url}"
            ));
            Ok(Download {
                handle,
                binary: binary_name(&target),
            })
        });

        match result {
            Ok(download) => {
                self.download = Some(download);
                Ok(())
            }
            Err(e) => {
                let error = format!(
                    "can't install {SERVER_NAME}: {e}. Install it manually \
                     (`cargo install --path crates/rustrest-js-lsp`) so it's on PATH."
                );
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub fn download_finished(&mut self, handle: u32, result: Result<Vec<String>, String>) {
        if self.download.as_ref().is_none_or(|d| d.handle != handle) {
            return;
        }
        let Download { binary, .. } = self.download.take().expect("checked above");
        let installed = result.and_then(|files| {
            let path = find_binary(&files, &binary)
                .cloned()
                .ok_or_else(|| format!("the archive has no {binary}"))?;
            if let Err(e) = self.host.make_executable(&path) {
                self.host.log(&format!("make_executable failed: {e}"));
            }
            self.host.storage_write(PATH_FILE, &path)?;
            self.host.storage_write(VERSION_FILE, SERVER_VERSION)?;
            Ok(path)
        });
        match installed {
            Ok(path) => self.host.log(&format!("installed {SERVER_NAME} at {path}")),
            Err(e) => {
                let error = format!("failed to install {SERVER_NAME}: {e}");
                self.host.log(&error);
                self.error = Some(error);
            }
        }
    }

    /// forgets any previous download (and failure) and fetches it again.
    pub fn reinstall(&mut self) -> Result<String, String> {
        if self.download.is_some() {
            return Ok(format!("{SERVER_NAME} is already downloading"));
        }
        self.error = None;
        self.host.storage_write(VERSION_FILE, "")?;
        self.start_download()?;
        Ok(match self.host.which(SERVER_NAME) {
            Some(path) => format!("Downloading {SERVER_NAME}; {path} on PATH is used meanwhile"),
            None => format!("Downloading {SERVER_NAME}..."),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Mock {
        on_path: Option<String>,
        target: Option<String>,
        storage: HashMap<String, String>,
        downloads: Vec<(String, String, String)>,
        executable: Vec<String>,
        logs: Vec<String>,
        fail_download: bool,
    }

    impl Host for Mock {
        fn log(&mut self, message: &str) {
            self.logs.push(message.to_string());
        }
        fn which(&mut self, _name: &str) -> Option<String> {
            self.on_path.clone()
        }
        fn host_target(&mut self) -> Result<String, String> {
            self.target.clone().ok_or_else(|| "no target".to_string())
        }
        fn storage_read(&mut self, filename: &str) -> Option<String> {
            self.storage.get(filename).cloned()
        }
        fn storage_write(&mut self, filename: &str, contents: &str) -> Result<(), String> {
            self.storage
                .insert(filename.to_string(), contents.to_string());
            Ok(())
        }
        fn download_archive(&mut self, url: &str, sum: &str, dest: &str) -> Result<u32, String> {
            if self.fail_download {
                return Err("offline".to_string());
            }
            self.downloads
                .push((url.to_string(), sum.to_string(), dest.to_string()));
            Ok(self.downloads.len() as u32)
        }
        fn make_executable(&mut self, path: &str) -> Result<(), String> {
            self.executable.push(path.to_string());
            Ok(())
        }
    }

    fn linux() -> Mock {
        Mock {
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            ..Mock::default()
        }
    }

    #[test]
    fn prefers_path() {
        let mut installer = Installer::new(Mock {
            on_path: Some("/usr/bin/rustrest-js-lsp".to_string()),
            ..linux()
        });
        assert_eq!(
            installer.server_path(),
            Ok(Some("/usr/bin/rustrest-js-lsp".to_string()))
        );
        assert!(installer.host().downloads.is_empty());
    }

    #[test]
    fn downloads_once_then_uses_the_installed_copy() {
        let mut installer = Installer::new(linux());
        assert_eq!(installer.server_path(), Ok(None));
        assert_eq!(installer.server_path(), Ok(None)); // no second download
        let (url, sum, dest) = installer.host().downloads[0].clone();
        assert!(url.ends_with("rustrest-js-lsp-x86_64-unknown-linux-gnu.tar.xz"));
        assert_eq!(sum, format!("{url}.sha256"));
        assert_eq!(dest, "bin");
        assert_eq!(installer.host().downloads.len(), 1);

        installer.download_finished(99, Err("stale handle".to_string()));
        installer.download_finished(
            1,
            Ok(vec![
                "/s/bin/x/README.md".to_string(),
                "/s/bin/x/rustrest-js-lsp".to_string(),
            ]),
        );
        assert_eq!(
            installer.host().executable,
            vec!["/s/bin/x/rustrest-js-lsp"]
        );
        assert_eq!(
            installer.server_path(),
            Ok(Some("/s/bin/x/rustrest-js-lsp".to_string()))
        );
    }

    #[test]
    fn outdated_install_is_replaced() {
        let mut mock = linux();
        mock.storage
            .insert(PATH_FILE.to_string(), "/old/rustrest-js-lsp".to_string());
        mock.storage
            .insert(VERSION_FILE.to_string(), "0.0.1".to_string());
        let mut installer = Installer::new(mock);
        assert_eq!(installer.server_path(), Ok(None));
        assert_eq!(installer.host().downloads.len(), 1);
    }

    #[test]
    fn failures_are_reported_until_reinstall() {
        let mut installer = Installer::new(linux());
        assert_eq!(installer.server_path(), Ok(None));
        installer.download_finished(1, Err("HTTP 404".to_string()));
        let error = installer.server_path().unwrap_err();
        assert!(error.contains("HTTP 404"), "{error}");
        assert_eq!(installer.host().downloads.len(), 1);

        assert_eq!(
            installer.reinstall(),
            Ok("Downloading rustrest-js-lsp...".to_string())
        );
        assert_eq!(installer.host().downloads.len(), 2);
        assert_eq!(installer.server_path(), Ok(None));
    }

    #[test]
    fn missing_archive_binary_or_target_is_an_error() {
        let mut installer = Installer::new(linux());
        installer.server_path().unwrap();
        installer.download_finished(1, Ok(vec!["/s/bin/README.md".to_string()]));
        assert!(installer
            .server_path()
            .unwrap_err()
            .contains("no rustrest-js-lsp"));

        let mut installer = Installer::new(Mock::default());
        let error = installer.server_path().unwrap_err();
        assert!(error.contains("cargo install"), "{error}");

        let mut installer = Installer::new(Mock {
            fail_download: true,
            ..linux()
        });
        assert!(installer.server_path().unwrap_err().contains("offline"));
    }
}
