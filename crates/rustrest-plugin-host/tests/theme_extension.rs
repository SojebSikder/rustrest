//! Theme-only extensions: no wasm, just a manifest (Rustrest's `plugin.toml` or a Zed `extension.toml`) and a `themes/` folder.

use rustrest_plugin_host::PluginManager;
use std::fs;
use std::path::Path;

const THEME_JSON: &str = r##"{
  "$schema": "https://zed.dev/schema/themes/v0.2.0.json",
  "name": "Test",
  "author": "tests",
  "themes": [{ "name": "Test Dark", "appearance": "dark", "style": { "editor.background": "#101010ff" } }]
}"##;

fn stage(dir: &Path, manifest_name: &str, manifest: &str) {
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir.join("themes")).unwrap();
    fs::write(dir.join(manifest_name), manifest).unwrap();
    fs::write(dir.join("themes").join("test.json"), THEME_JSON).unwrap();
}

fn temp(name: &str) -> std::path::PathBuf {
    let tmp =
        std::env::temp_dir().join(format!("rustrest-theme-ext-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    tmp
}

#[test]
fn installs_a_zed_theme_extension_without_wasm() {
    let tmp = temp("zed");
    let source = tmp.join("source");
    stage(
        &source,
        "extension.toml",
        r#"
id = "test-theme"
name = "Test Theme"
version = "0.1.0"
schema_version = 1
authors = ["tests"]
description = "a theme"
"#,
    );

    let mut manager =
        PluginManager::with_dirs(tmp.join("plugins"), tmp.join("plugins.json")).unwrap();
    manager.load_all();

    let id = manager.install_from_dir(&source).unwrap();
    assert_eq!(id, "test-theme");
    let installed = &manager.installed()[0];
    assert!(installed.is_theme_only());
    assert!(installed.enabled);
    assert!(!installed.is_active(), "nothing to run");
    assert!(
        manager
            .plugins_dir()
            .join("test-theme/themes/test.json")
            .is_file()
    );
    assert_eq!(manager.theme_files().len(), 1);

    // disabling hides its themes, and survives a reload
    manager.set_enabled("test-theme", false);
    assert!(manager.theme_files().is_empty());
    manager.load_all();
    assert!(!manager.installed()[0].enabled);
    assert!(manager.theme_files().is_empty());
    manager.set_enabled("test-theme", true);
    manager.load_all();
    assert_eq!(manager.theme_files().len(), 1);

    manager.uninstall("test-theme").unwrap();
    assert!(manager.installed().is_empty());
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn rejects_a_folder_with_neither_wasm_nor_themes() {
    let tmp = temp("empty");
    let source = tmp.join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("plugin.toml"),
        r#"
id = "nothing"
name = "Nothing"
version = "0.1.0"
author = "tests"
description = "empty"
"#,
    )
    .unwrap();

    let mut manager =
        PluginManager::with_dirs(tmp.join("plugins"), tmp.join("plugins.json")).unwrap();
    let err = manager.install_from_dir(&source).unwrap_err().to_string();
    assert!(err.contains("plugin.wasm"), "{err}");
    let _ = fs::remove_dir_all(&tmp);
}
