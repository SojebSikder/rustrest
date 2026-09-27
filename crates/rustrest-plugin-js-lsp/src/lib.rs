//! JavaScript language server plugin. it only tells the host how to launch `rustrest-js-lsp`
//! (downloading it first if needed), the host runs the server and speaks LSP to it.

pub mod installer;
pub mod paths;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use crate::installer::{Host, Installer};
    use crate::paths::SERVER_NAME;
    use rustrest_plugin_api::process;
    use rustrest_plugin_api::{log, LanguageServerCommand, Plugin};

    /// [`Host`] backed by the real plugin API host calls.
    #[derive(Default)]
    pub struct WasmHost;

    impl Host for WasmHost {
        fn log(&mut self, message: &str) {
            log(message);
        }

        fn which(&mut self, name: &str) -> Option<String> {
            process::which(name)
        }

        fn host_target(&mut self) -> Result<String, String> {
            process::host_target()
        }

        fn storage_read(&mut self, filename: &str) -> Option<String> {
            process::storage_read(filename)
                .ok()
                .flatten()
                .and_then(|bytes| String::from_utf8(bytes).ok())
        }

        fn storage_write(&mut self, filename: &str, contents: &str) -> Result<(), String> {
            process::storage_write(filename, contents.as_bytes())
        }

        fn download_archive(
            &mut self,
            url: &str,
            checksum_url: &str,
            dest_dir: &str,
        ) -> Result<u32, String> {
            process::download_archive(url, Some(checksum_url), dest_dir)
        }

        fn make_executable(&mut self, path: &str) -> Result<(), String> {
            process::make_executable(path)
        }
    }

    #[derive(Default)]
    pub struct JsLspPlugin {
        installer: Installer<WasmHost>,
    }

    impl Plugin for JsLspPlugin {
        fn language_server_command(
            &mut self,
            server_id: &str,
        ) -> Result<Option<LanguageServerCommand>, String> {
            if server_id != SERVER_NAME {
                return Err(format!("unknown language server '{server_id}'"));
            }
            Ok(self
                .installer
                .server_path()?
                .map(|command| LanguageServerCommand {
                    command,
                    ..LanguageServerCommand::default()
                }))
        }

        fn on_download_finished(&mut self, handle: u32, result: Result<Vec<String>, String>) {
            self.installer.download_finished(handle, result);
        }

        fn on_command(&mut self, command_id: &str) -> Result<Option<String>, String> {
            match command_id {
                "reinstall-server" => self.installer.reinstall().map(Some),
                other => Err(format!("unknown command: {other}")),
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
rustrest_plugin_api::export_plugin!(wasm::JsLspPlugin);
