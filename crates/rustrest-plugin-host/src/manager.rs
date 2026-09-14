use crate::error::PluginError;
use crate::instance::{LoadedPlugin, load_plugin};
use rustrest_plugin_api::{
    Capability, CommandDef, FormatDef, MenuItemDef, PanelDef, RequestContext, ResponseContext,
    UiEvent, UiNode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use wasmtime::{Config, Engine};

const APP_NAME: &str = "Rustrest";

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedState {
    #[serde(default)]
    disabled: HashSet<String>,
}

/// Discovers, loads, and drives every wasm plugin under the plugins
/// directory. Owns one `wasmtime::Engine` shared by all plugin instances.
pub struct PluginManager {
    engine: Engine,
    plugins_dir: PathBuf,
    state_path: PathBuf,
    plugins: Vec<LoadedPlugin>,
}

impl PluginManager {
    pub fn new() -> Result<Self, PluginError> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| {
                PluginError::Manifest("no data directory available on this platform".to_string())
            })?
            .join(APP_NAME);
        Self::with_dirs(data_dir.join("plugins"), data_dir.join("plugins.json"))
    }

    pub fn with_dirs(plugins_dir: PathBuf, state_path: PathBuf) -> Result<Self, PluginError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)?;
        Ok(Self {
            engine,
            plugins_dir,
            state_path,
            plugins: Vec::new(),
        })
    }

    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// discovers and (re)loads every plugin under the plugins directory.
    /// Safe to call again to pick up newly dropped-in plugins.
    pub fn load_all(&mut self) {
        let disabled = self.read_disabled();
        fs::create_dir_all(&self.plugins_dir).ok();

        let mut discovered = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.plugins_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let wasm_path = path.join("plugin.wasm");
                if !wasm_path.is_file() {
                    continue;
                }
                let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                discovered.push((dir_name.to_string(), wasm_path));
            }
        }

        self.plugins = discovered
            .into_iter()
            .map(|(dir_name, wasm_path)| {
                let enabled = !disabled.contains(&dir_name);
                load_plugin(&self.engine, &dir_name, &wasm_path, enabled)
            })
            .collect();
    }

    fn read_disabled(&self) -> HashSet<String> {
        fs::read_to_string(&self.state_path)
            .ok()
            .and_then(|s| serde_json::from_str::<PersistedState>(&s).ok())
            .map(|s| s.disabled)
            .unwrap_or_default()
    }

    fn save_state(&self) {
        let disabled: HashSet<String> = self
            .plugins
            .iter()
            .filter(|p| !p.enabled)
            .map(|p| p.id().to_string())
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&PersistedState { disabled }) {
            if let Some(parent) = self.state_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(&self.state_path, json).ok();
        }
    }

    pub fn installed(&self) -> &[LoadedPlugin] {
        &self.plugins
    }

    pub fn set_enabled(&mut self, plugin_id: &str, enabled: bool) {
        if let Some(plugin) = self.plugins.iter_mut().find(|p| p.id() == plugin_id) {
            plugin.enabled = enabled && plugin.runtime.is_some();
        }
        self.save_state();
    }

    fn find_active(&mut self, plugin_id: &str) -> Result<&mut LoadedPlugin, PluginError> {
        self.plugins
            .iter_mut()
            .find(|p| p.is_active() && p.id() == plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))
    }

    /// drains every plugin's pending `host_log()` lines (already prefixed
    /// with the emitting plugin's id) for the caller to forward into its own
    /// log/console UI.
    pub fn drain_logs(&mut self) -> Vec<String> {
        let mut all = Vec::new();
        for plugin in &self.plugins {
            if let Some(runtime) = &plugin.runtime
                && let Ok(mut logs) = runtime.logs.lock()
            {
                all.append(&mut logs);
            }
        }
        all
    }

    pub fn commands(&self) -> Vec<(String, CommandDef)> {
        let mut out = Vec::new();
        for plugin in self.plugins.iter().filter(|p| p.is_active()) {
            let Some(manifest) = &plugin.manifest else {
                continue;
            };
            for cap in &manifest.capabilities {
                if let Capability::Commands(cmds) = cap {
                    out.extend(cmds.iter().cloned().map(|c| (plugin.id().to_string(), c)));
                }
            }
        }
        out
    }

    pub fn menu_items(&self) -> Vec<(String, MenuItemDef)> {
        let mut out = Vec::new();
        for plugin in self.plugins.iter().filter(|p| p.is_active()) {
            let Some(manifest) = &plugin.manifest else {
                continue;
            };
            for cap in &manifest.capabilities {
                if let Capability::MenuItems(items) = cap {
                    out.extend(items.iter().cloned().map(|i| (plugin.id().to_string(), i)));
                }
            }
        }
        out
    }

    pub fn sidebar_panels(&self) -> Vec<(String, PanelDef)> {
        let mut out = Vec::new();
        for plugin in self.plugins.iter().filter(|p| p.is_active()) {
            let Some(manifest) = &plugin.manifest else {
                continue;
            };
            for cap in &manifest.capabilities {
                if let Capability::SidebarPanel(panel) = cap {
                    out.push((plugin.id().to_string(), panel.clone()));
                }
            }
        }
        out
    }

    pub fn import_formats(&self) -> Vec<(String, FormatDef)> {
        let mut out = Vec::new();
        for plugin in self.plugins.iter().filter(|p| p.is_active()) {
            let Some(manifest) = &plugin.manifest else {
                continue;
            };
            for cap in &manifest.capabilities {
                if let Capability::ImportFormat(format) = cap {
                    out.push((plugin.id().to_string(), format.clone()));
                }
            }
        }
        out
    }

    pub fn export_formats(&self) -> Vec<(String, FormatDef)> {
        let mut out = Vec::new();
        for plugin in self.plugins.iter().filter(|p| p.is_active()) {
            let Some(manifest) = &plugin.manifest else {
                continue;
            };
            for cap in &manifest.capabilities {
                if let Capability::ExportFormat(format) = cap {
                    out.push((plugin.id().to_string(), format.clone()));
                }
            }
        }
        out
    }

    pub fn run_command(
        &mut self,
        plugin_id: &str,
        command_id: &str,
    ) -> Result<Option<String>, PluginError> {
        let plugin = self.find_active(plugin_id)?;
        let runtime = plugin.runtime.as_mut().expect("checked active");
        runtime
            .handles
            .call_json(&mut runtime.store, "on_command", command_id)
    }

    pub fn render_panel(&mut self, plugin_id: &str, panel_id: &str) -> Result<UiNode, PluginError> {
        let plugin = self.find_active(plugin_id)?;
        let runtime = plugin.runtime.as_mut().expect("checked active");
        runtime
            .handles
            .call_json(&mut runtime.store, "render_panel", panel_id)
    }

    pub fn panel_event(
        &mut self,
        plugin_id: &str,
        panel_id: &str,
        event: UiEvent,
    ) -> Result<Option<UiNode>, PluginError> {
        let plugin = self.find_active(plugin_id)?;
        let runtime = plugin.runtime.as_mut().expect("checked active");
        runtime
            .handles
            .call_json(&mut runtime.store, "on_panel_event", (panel_id, event))
    }

    pub fn import(
        &mut self,
        plugin_id: &str,
        format_id: &str,
        bytes: Vec<u8>,
    ) -> Result<serde_json::Value, PluginError> {
        let plugin = self.find_active(plugin_id)?;
        let runtime = plugin.runtime.as_mut().expect("checked active");
        runtime
            .handles
            .call_json(&mut runtime.store, "import", (format_id, bytes))
    }

    pub fn export(
        &mut self,
        plugin_id: &str,
        format_id: &str,
        collection: serde_json::Value,
    ) -> Result<Vec<u8>, PluginError> {
        let plugin = self.find_active(plugin_id)?;
        let runtime = plugin.runtime.as_mut().expect("checked active");
        runtime
            .handles
            .call_json(&mut runtime.store, "export", (format_id, collection))
    }

    /// runs every enabled `RequestHooks` plugin's pre-request hook in order,
    /// threading the request context through each. A plugin error is logged
    /// (visible via `drain_logs`) and that plugin's step is skipped rather
    /// than aborting the request or the rest of the chain.
    pub fn run_pre_request_hooks(&mut self, mut ctx: RequestContext) -> RequestContext {
        for plugin in self.plugins.iter_mut().filter(|p| p.is_active()) {
            let has_hook = plugin.manifest.as_ref().is_some_and(|m| {
                m.capabilities
                    .iter()
                    .any(|c| matches!(c, Capability::RequestHooks))
            });
            if !has_hook {
                continue;
            }
            let runtime = plugin.runtime.as_mut().expect("checked active");
            match runtime.handles.call_json::<_, RequestContext>(
                &mut runtime.store,
                "on_pre_request",
                ctx.clone(),
            ) {
                Ok(updated) => ctx = updated,
                Err(e) => log_hook_error(runtime, plugin.dir_name.as_str(), "pre-request", &e),
            }
        }
        ctx
    }

    /// mirror of `run_pre_request_hooks` for the response side.
    pub fn run_post_response_hooks(&mut self, mut ctx: ResponseContext) -> ResponseContext {
        for plugin in self.plugins.iter_mut().filter(|p| p.is_active()) {
            let has_hook = plugin.manifest.as_ref().is_some_and(|m| {
                m.capabilities
                    .iter()
                    .any(|c| matches!(c, Capability::RequestHooks))
            });
            if !has_hook {
                continue;
            }
            let runtime = plugin.runtime.as_mut().expect("checked active");
            match runtime.handles.call_json::<_, ResponseContext>(
                &mut runtime.store,
                "on_post_response",
                ctx.clone(),
            ) {
                Ok(updated) => ctx = updated,
                Err(e) => log_hook_error(runtime, plugin.dir_name.as_str(), "post-response", &e),
            }
        }
        ctx
    }
}

fn log_hook_error(
    runtime: &crate::instance::PluginRuntime,
    plugin_id: &str,
    hook: &str,
    error: &PluginError,
) {
    if let Ok(mut logs) = runtime.logs.lock() {
        logs.push(format!("[plugin:{plugin_id}] {hook} hook failed: {error}"));
    }
}
