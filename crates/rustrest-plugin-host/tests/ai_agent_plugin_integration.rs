//! End-to-end check of the `RightPanel` capability against the real AI
//! agent plugin wasm build. Requires `rustrest-plugin-ai-agent` to have been
//! built first:
//!
//! ```text
//! cd crates/rustrest-plugin-ai-agent
//! rustup target add wasm32-unknown-unknown
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! Skips itself (rather than failing) if that build output isn't present,
//! so a plain `cargo test --workspace` doesn't require the wasm32 target.
//! Doesn't exercise an actual LLM round trip (no real API key/network in
//! CI) - just the settings/config/right-panel-dispatch plumbing this
//! feature added.

use rustrest_plugin_host::{
    Capability, PluginManager, RightPanelAction, RightPanelContext, UiEvent, UiNode,
};
use std::path::PathBuf;

fn agent_crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("rustrest-plugin-ai-agent")
}

fn agent_wasm_path() -> PathBuf {
    agent_crate_dir()
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("rustrest_plugin_ai_agent.wasm")
}

fn stage_plugin_dir(dest: &std::path::Path) {
    std::fs::create_dir_all(dest).unwrap();
    std::fs::copy(
        agent_crate_dir().join("plugin.toml"),
        dest.join("plugin.toml"),
    )
    .unwrap();
    std::fs::copy(agent_wasm_path(), dest.join("plugin.wasm")).unwrap();
}

fn tree_contains(node: &UiNode, needle: &str) -> bool {
    match node {
        UiNode::Label(s) => s.contains(needle),
        UiNode::Button { label, .. } => label.contains(needle),
        UiNode::List(items) => items.iter().any(|s| s.contains(needle)),
        UiNode::Row(children) | UiNode::Column(children) => {
            children.iter().any(|c| tree_contains(c, needle))
        }
        _ => false,
    }
}

#[test]
fn loads_and_drives_the_ai_agent_right_panel() {
    let wasm_path = agent_wasm_path();
    if !wasm_path.is_file() {
        eprintln!(
            "skipping: ai-agent plugin not built at {}; see file header for build instructions",
            wasm_path.display()
        );
        return;
    }

    let tmp = std::env::temp_dir().join(format!(
        "rustrest-ai-agent-host-test-{}",
        std::process::id()
    ));
    let plugins_dir = tmp.join("plugins");
    stage_plugin_dir(&plugins_dir.join("ai-agent"));

    let mut manager = PluginManager::with_dirs(plugins_dir, tmp.join("plugins.json")).unwrap();
    manager.load_all();

    let installed = manager.installed();
    assert_eq!(installed.len(), 1);
    let plugin = &installed[0];
    assert!(
        plugin.load_error.is_none(),
        "load error: {:?}",
        plugin.load_error
    );
    assert!(plugin.is_active());
    assert_eq!(plugin.id(), "ai-agent");
    assert!(
        plugin
            .manifest
            .as_ref()
            .unwrap()
            .capabilities
            .iter()
            .any(|c| matches!(c, Capability::RightPanel(_))),
        "expected the manifest.toml-declared RightPanel capability to survive parsing"
    );

    let panels = manager.right_panels();
    assert_eq!(panels.len(), 1);
    assert_eq!(panels[0].0, "ai-agent");
    assert_eq!(panels[0].1.id, "chat");

    // no api key saved yet -> the settings view, not the chat view.
    let action = manager
        .render_right_panel("ai-agent", "chat", RightPanelContext::default())
        .unwrap();
    let RightPanelAction::UpdateUi(tree) = action else {
        panic!("expected UpdateUi on first render, got an action carrying a patch");
    };
    assert!(
        tree_contains(&tree, "AI Agent settings"),
        "expected the settings view before any key is configured, got {tree:?}"
    );

    // fill in and save a (fake) provider/key/model - exercises the
    // right_panel_event dispatch path and the new storage_read/storage_write
    // host calls (gated by ExternalProcess, which this plugin declares).
    manager
        .right_panel_event(
            "ai-agent",
            "chat",
            RightPanelContext::default(),
            UiEvent::Clicked("provider-openai".to_string()),
        )
        .unwrap();
    manager
        .right_panel_event(
            "ai-agent",
            "chat",
            RightPanelContext::default(),
            UiEvent::Changed("api-key".to_string(), "test-key".to_string()),
        )
        .unwrap();
    manager
        .right_panel_event(
            "ai-agent",
            "chat",
            RightPanelContext::default(),
            UiEvent::Clicked("save-settings".to_string()),
        )
        .unwrap();

    // re-render (a fresh plugin instance would reload from storage) and
    // confirm it now shows the chat view with the saved provider.
    let action = manager
        .render_right_panel("ai-agent", "chat", RightPanelContext::default())
        .unwrap();
    let RightPanelAction::UpdateUi(tree) = action else {
        panic!("expected UpdateUi, got an action carrying a patch");
    };
    assert!(
        tree_contains(&tree, "openai"),
        "expected the chat view to show the saved provider, got {tree:?}"
    );
    assert!(!tree_contains(&tree, "AI Agent settings"));

    std::fs::remove_dir_all(&tmp).ok();
}
