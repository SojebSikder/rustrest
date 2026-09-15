//! Template/example native Rustrest plugin. Demonstrates the three
//! capabilities most plugins will use: a request hook, a command, and a
//! sidebar panel. Build with:
//!
//! ```text
//! rustup target add wasm32-unknown-unknown
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! then copy `target/wasm32-unknown-unknown/release/rustrest_plugin_example.wasm`
//! to `<plugins-dir>/example/plugin.wasm` (the containing directory name
//! must match the plugin's `manifest().id`, here `"example"`).

use rustrest_plugin_api::{
    Capability, CommandDef, PanelDef, Plugin, PluginManifest, RequestContext, UiEvent, UiNode,
};

#[derive(Default)]
struct ExamplePlugin {
    clicks: u32,
}

impl Plugin for ExamplePlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            id: "example".to_string(),
            name: "Example Plugin".to_string(),
            version: "0.1.0".to_string(),
            author: "Rustrest".to_string(),
            description: "Demonstrates a request hook, a command, and a sidebar panel.".to_string(),
            capabilities: vec![
                Capability::RequestHooks,
                Capability::Commands(vec![CommandDef {
                    id: "say-hello".to_string(),
                    title: "Example: Say Hello".to_string(),
                    subtitle: Some("from the example plugin".to_string()),
                }]),
                Capability::SidebarPanel(PanelDef {
                    id: "main".to_string(),
                    title: "Example".to_string(),
                }),
            ],
        }
    }

    fn on_pre_request(&mut self, mut ctx: RequestContext) -> RequestContext {
        rustrest_plugin_api::log(&format!("injecting header into request to {}", ctx.url));
        ctx.headers
            .push(("X-Example-Plugin".to_string(), "1".to_string()));
        ctx
    }

    fn on_command(&mut self, command_id: &str) -> Result<Option<String>, String> {
        match command_id {
            "say-hello" => {
                rustrest_plugin_api::log("say-hello command invoked");
                Ok(Some("Hello from the example plugin!".to_string()))
            }
            other => Err(format!("unknown command: {other}")),
        }
    }

    fn render_panel(&mut self, _panel_id: &str) -> UiNode {
        UiNode::Column(vec![
            UiNode::Label("Example plugin panel".to_string()),
            UiNode::Label(format!("Button clicked {} time(s)", self.clicks)),
            UiNode::Button {
                id: "clicked".to_string(),
                label: "Click me".to_string(),
            },
        ])
    }

    fn on_panel_event(&mut self, panel_id: &str, event: UiEvent) -> Option<UiNode> {
        match event {
            UiEvent::Clicked(id) if id == "clicked" => {
                self.clicks += 1;
                Some(self.render_panel(panel_id))
            }
            _ => None,
        }
    }
}

rustrest_plugin_api::export_plugin!(ExamplePlugin);
