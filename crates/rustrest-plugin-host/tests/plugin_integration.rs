//! End-to-end check of the guest/host ABI against the real example plugin
//! wasm build. Requires `rustrest-plugin-example` to have been built first:
//!
//! ```text
//! cd crates/rustrest-plugin-example
//! rustup target add wasm32-unknown-unknown
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! Skips itself (rather than failing) if that build output isn't present,
//! so a plain `cargo test --workspace` doesn't require the wasm32 target.

use rustrest_plugin_api::{RequestContext, UiEvent, UiNode};
use rustrest_plugin_host::PluginManager;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn example_crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("rustrest-plugin-example")
}

fn example_wasm_path() -> PathBuf {
    example_crate_dir()
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("rustrest_plugin_example.wasm")
}

fn example_manifest_path() -> PathBuf {
    example_crate_dir().join("plugin.toml")
}

/// stages `plugin.toml` (crate root) + the built `plugin.wasm` together into
/// one directory, matching the on-disk shape a real plugin install expects.
fn stage_plugin_dir(dest: &std::path::Path) {
    std::fs::create_dir_all(dest).unwrap();
    std::fs::copy(example_manifest_path(), dest.join("plugin.toml")).unwrap();
    std::fs::copy(example_wasm_path(), dest.join("plugin.wasm")).unwrap();
}

#[test]
fn loads_and_drives_the_example_plugin() {
    let wasm_path = example_wasm_path();
    if !wasm_path.is_file() {
        eprintln!(
            "skipping: example plugin not built at {}; see file header for build instructions",
            wasm_path.display()
        );
        return;
    }

    let tmp =
        std::env::temp_dir().join(format!("rustrest-plugin-host-test-{}", std::process::id()));
    let plugins_dir = tmp.join("plugins");
    stage_plugin_dir(&plugins_dir.join("example"));

    let mut manager = PluginManager::with_dirs(plugins_dir, tmp.join("plugins.json")).unwrap();
    manager.load_all();

    let installed = manager.installed();
    assert_eq!(installed.len(), 1, "expected exactly one discovered plugin");
    let plugin = &installed[0];
    assert!(
        plugin.load_error.is_none(),
        "plugin failed to load: {:?}",
        plugin.load_error
    );
    assert!(plugin.is_active());
    assert_eq!(plugin.id(), "example");
    assert!(
        plugin
            .manifest
            .as_ref()
            .unwrap()
            .capabilities
            .iter()
            .any(|c| matches!(c, rustrest_plugin_host::Capability::ExternalProcess)),
        "expected the manifest.toml-declared ExternalProcess capability to survive parsing"
    );

    let commands = manager.commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].1.id, "say-hello");

    let panels = manager.sidebar_panels();
    assert_eq!(panels.len(), 1);
    assert_eq!(panels[0].1.id, "main");

    // command dispatch
    let result = manager.run_command("example", "say-hello").unwrap();
    assert_eq!(result.as_deref(), Some("Hello from the example plugin!"));

    // panel render + event round-trip
    let tree = manager.render_panel("example", "main").unwrap();
    assert!(matches!(tree, UiNode::Column(_)));

    let updated = manager
        .panel_event("example", "main", UiEvent::Clicked("clicked".to_string()))
        .unwrap();
    let Some(UiNode::Column(nodes)) = updated else {
        panic!("expected an updated panel tree after the click event");
    };
    let has_one_click_label = nodes
        .iter()
        .any(|n| matches!(n, UiNode::Label(l) if l.contains("1 time")));
    assert!(
        has_one_click_label,
        "expected the click counter to have incremented, got {nodes:?}"
    );

    // pre-request hook mutates the outgoing request
    let ctx = RequestContext {
        method: "GET".to_string(),
        url: "https://example.com".to_string(),
        ..Default::default()
    };
    let ctx = manager.run_pre_request_hooks(ctx);
    assert!(
        ctx.headers
            .iter()
            .any(|(k, v)| k == "X-Example-Plugin" && v == "1"),
        "expected the pre-request hook to inject a header, got {:?}",
        ctx.headers
    );

    let logs = manager.drain_logs();
    assert!(
        logs.iter().any(|l| l.contains("say-hello command invoked")),
        "expected a log line from the command, got {logs:?}"
    );

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn installs_and_uninstalls_a_plugin_from_a_local_folder() {
    let wasm_path = example_wasm_path();
    if !wasm_path.is_file() {
        eprintln!(
            "skipping: example plugin not built at {}; see file header for build instructions",
            wasm_path.display()
        );
        return;
    }

    let tmp = std::env::temp_dir().join(format!(
        "rustrest-plugin-host-install-test-{}",
        std::process::id()
    ));
    let plugins_dir = tmp.join("plugins");
    let source_dir = tmp.join("source");
    stage_plugin_dir(&source_dir);

    let mut manager = PluginManager::with_dirs(plugins_dir, tmp.join("plugins.json")).unwrap();
    manager.load_all();
    assert!(manager.installed().is_empty());

    let id = manager.install_from_dir(&source_dir).unwrap();
    assert_eq!(id, "example");
    assert!(
        manager
            .plugins_dir()
            .join("example")
            .join("plugin.wasm")
            .is_file()
    );
    assert!(
        manager
            .plugins_dir()
            .join("example")
            .join("plugin.toml")
            .is_file()
    );

    let installed = manager.installed();
    assert_eq!(installed.len(), 1);
    assert!(installed[0].is_active());

    // installing the same plugin again is rejected rather than overwritten.
    assert!(manager.install_from_dir(&source_dir).is_err());

    manager.uninstall("example").unwrap();
    assert!(manager.installed().is_empty());
    assert!(!manager.plugins_dir().join("example").exists());

    // uninstalling a plugin that isn't installed reports an error.
    assert!(manager.uninstall("example").is_err());

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn spawns_and_streams_output_from_an_external_process() {
    let wasm_path = example_wasm_path();
    if !wasm_path.is_file() {
        eprintln!(
            "skipping: example plugin not built at {}; see file header for build instructions",
            wasm_path.display()
        );
        return;
    }

    let tmp = std::env::temp_dir().join(format!(
        "rustrest-plugin-host-process-test-{}",
        std::process::id()
    ));
    let plugins_dir = tmp.join("plugins");
    stage_plugin_dir(&plugins_dir.join("example"));

    let mut manager = PluginManager::with_dirs(plugins_dir, tmp.join("plugins.json")).unwrap();
    manager.load_all();
    assert!(manager.installed()[0].load_error.is_none());

    // clicking "spawn" in the panel spawns a persistent echo-style process
    // (`cat` on unix, `findstr /R "^"` on Windows) via the ExternalProcess
    // capability - the same shape a real plugin would use to drive a
    // downloaded binary like `rust-analyzer`.
    manager
        .panel_event("example", "main", UiEvent::Clicked("spawn".to_string()))
        .expect("spawning the echo process should succeed");

    manager
        .panel_event(
            "example",
            "main",
            UiEvent::Changed("stdin-input".to_string(), "hello from the host".to_string()),
        )
        .unwrap();
    manager
        .panel_event("example", "main", UiEvent::Clicked("send".to_string()))
        .expect("writing to the process's stdin should succeed");

    // background OS threads feed the echoed line back asynchronously; poll
    // `pump_processes` (what the app's timer subscription drives) until it
    // shows up or we give up.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saw_echo = false;
    while Instant::now() < deadline && !saw_echo {
        manager.pump_processes();
        let tree = manager.render_panel("example", "main").unwrap();
        saw_echo = panel_contains(&tree, "hello from the host");
        if !saw_echo {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    assert!(
        saw_echo,
        "expected the spawned process's echoed output to reach the plugin panel"
    );

    std::fs::remove_dir_all(&tmp).ok();
}

fn panel_contains(node: &UiNode, needle: &str) -> bool {
    match node {
        UiNode::Label(s) => s.contains(needle),
        UiNode::List(items) => items.iter().any(|s| s.contains(needle)),
        UiNode::Row(children) | UiNode::Column(children) => {
            children.iter().any(|c| panel_contains(c, needle))
        }
        _ => false,
    }
}
