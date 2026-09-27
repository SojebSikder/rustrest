//! End-to-end check of the `LanguageServer` capability against the real
//! js-lsp plugin wasm build. Requires it to have been built first:
//!
//! ```text
//! cd crates/rustrest-plugin-js-lsp
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! Skips itself (rather than failing) if that build output isn't present.
//! Only exercises the "server found on PATH" path - no network.

use rustrest_plugin_host::{Capability, PluginManager};
use std::path::PathBuf;

fn plugin_crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("rustrest-plugin-js-lsp")
}

fn plugin_wasm_path() -> PathBuf {
    plugin_crate_dir()
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("rustrest_plugin_js_lsp.wasm")
}

#[test]
fn js_lsp_plugin_declares_and_locates_its_server() {
    if !plugin_wasm_path().is_file() {
        eprintln!("skipping: js-lsp plugin not built; see file header");
        return;
    }

    let tmp =
        std::env::temp_dir().join(format!("rustrest-js-lsp-host-test-{}", std::process::id()));
    let plugin_dir = tmp.join("plugins").join("js-lsp");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::copy(
        plugin_crate_dir().join("plugin.toml"),
        plugin_dir.join("plugin.toml"),
    )
    .unwrap();
    std::fs::copy(plugin_wasm_path(), plugin_dir.join("plugin.wasm")).unwrap();

    // a stand-in server binary on PATH; the plugin prefers it over downloading
    let bin_dir = tmp.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let fake_server = bin_dir.join(format!("rustrest-js-lsp{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&fake_server, b"").unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![bin_dir];
    paths.extend(std::env::split_paths(&path));
    // SAFETY: set before the plugin (the only other reader) is loaded; this
    // test binary runs no other threads that touch the environment
    unsafe { std::env::set_var("PATH", std::env::join_paths(paths).unwrap()) };

    let mut manager =
        PluginManager::with_dirs(tmp.join("plugins"), tmp.join("plugins.json")).unwrap();
    manager.load_all();

    let manifest = manager.installed()[0].manifest.clone().unwrap();
    assert!(
        manifest
            .capabilities
            .iter()
            .any(|c| matches!(c, Capability::LanguageServer(_)))
    );

    let servers = manager.language_servers();
    assert_eq!(servers.len(), 1);
    let (plugin_id, server) = &servers[0];
    assert_eq!(plugin_id, "js-lsp");
    assert_eq!(server.id, "rustrest-js-lsp");
    assert_eq!(server.languages, vec!["javascript".to_string()]);

    let command = manager
        .language_server_command(plugin_id, &server.id)
        .unwrap()
        .expect("found on PATH");
    // PATHEXT may upper-case the extension on Windows
    assert_eq!(
        command.command.to_lowercase(),
        fake_server.to_string_lossy().to_lowercase()
    );
    assert!(command.args.is_empty());
    assert_eq!(
        manager
            .language_server_initialization_options(plugin_id, &server.id)
            .unwrap(),
        None
    );
    assert!(manager.language_server_command(plugin_id, "nope").is_err());

    manager.set_enabled("js-lsp", false);
    assert!(manager.language_servers().is_empty());
    let _ = std::fs::remove_dir_all(&tmp);
}
