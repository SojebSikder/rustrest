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

fn example_wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("rustrest-plugin-example")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("rustrest_plugin_example.wasm")
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
    let plugin_dir = plugins_dir.join("example");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::copy(&wasm_path, plugin_dir.join("plugin.wasm")).unwrap();

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
