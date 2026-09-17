use crate::error::PluginError;
use crate::files::FileEvent;
use crate::instance::{LoadedPlugin, compile, load_from_module, load_plugin};
use crate::manifest_toml::{self, MANIFEST_FILE_NAME, WASM_FILE_NAME};
use crate::network::{NetworkEvent, NetworkTable};
use crate::process::{ProcessEvent, ProcessTable};
use rustrest_plugin_api::{
    Capability, CommandDef, FormatDef, MenuItemDef, PanelDef, PluginManifest, RequestContext,
    ResponseContext, RightPanelAction, RightPanelContext, UiEvent, UiNode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use wasmtime::{Config, Engine, Module};

const APP_NAME: &str = "Rustrest";

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedState {
    #[serde(default)]
    disabled: HashSet<String>,
}

/// Discovers, loads, and drives every wasm plugin under the plugins
/// directory. Owns one `wasmtime::Engine` shared by all plugin instances.
pub struct PluginManager {
    engine: Option<Engine>,
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
        Ok(Self {
            engine: None,
            plugins_dir,
            state_path,
            plugins: Vec::new(),
        })
    }

    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    fn new_engine() -> Engine {
        let mut config = Config::new();
        config.consume_fuel(true);
        Engine::new(&config).expect("default wasmtime config is always valid")
    }

    /// returns a cloned handle to the shared wasmtime engine, creating it if
    /// this is the first plugin operation. `Engine` clones are cheap (an
    /// `Arc` handle) and `Send`, so this can be moved to a background thread
    /// to compile a plugin (the expensive part of an install) without
    /// blocking the UI.
    pub fn engine_handle(&mut self) -> Engine {
        self.engine.get_or_insert_with(Self::new_engine).clone()
    }

    /// discovers and (re)loads every plugin under the plugins directory.
    /// Safe to call again to pick up newly dropped-in plugin directories.
    /// A directory is only considered a plugin if it has a `plugin.toml` -
    /// that file alone is enough to list/validate it, no wasm is compiled
    /// or run just to discover what's installed.
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
                if !path.join(MANIFEST_FILE_NAME).is_file() {
                    continue;
                }
                let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                discovered.push((dir_name.to_string(), path));
            }
        }

        if discovered.is_empty() {
            self.plugins = Vec::new();
            return;
        }

        let engine = self.engine.get_or_insert_with(Self::new_engine);

        self.plugins = discovered
            .into_iter()
            .map(|(dir_name, plugin_dir)| {
                let enabled = !disabled.contains(&dir_name);
                load_plugin(engine, &dir_name, &plugin_dir, enabled)
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
            if !plugin.enabled
                && let Some(runtime) = &plugin.runtime
            {
                ProcessTable::kill_all(&runtime.processes);
            }
        }
        self.save_state();
    }

    /// validates the plugin folder at `source` (must contain `plugin.toml`
    /// and `plugin.wasm`), compiles its wasm, and copies both files into
    /// `plugins_dir` under a directory named after the manifest id.
    pub fn prepare_install(
        engine: &Engine,
        plugins_dir: &Path,
        source: &Path,
    ) -> Result<(String, PluginManifest, Module), PluginError> {
        let manifest = manifest_toml::read_from_dir(source)?;
        let wasm_path = source.join(WASM_FILE_NAME);
        if !wasm_path.is_file() {
            return Err(PluginError::Manifest(format!(
                "missing {WASM_FILE_NAME} in plugin folder"
            )));
        }
        let module = compile(engine, &wasm_path)?;

        fs::create_dir_all(plugins_dir)?;
        let dest_dir = plugins_dir.join(&manifest.id);
        if dest_dir.exists() {
            return Err(PluginError::Manifest(format!(
                "a plugin with id '{}' is already installed",
                manifest.id
            )));
        }
        fs::create_dir_all(&dest_dir)?;
        fs::copy(
            source.join(MANIFEST_FILE_NAME),
            dest_dir.join(MANIFEST_FILE_NAME),
        )?;
        fs::copy(&wasm_path, dest_dir.join(WASM_FILE_NAME))?;

        Ok((manifest.id.clone(), manifest, module))
    }

    /// finishes an install prepared by `prepare_install`: instantiates the
    /// already-compiled module and activates it.
    pub fn finish_install(
        &mut self,
        dir_name: String,
        manifest: PluginManifest,
        module: Module,
    ) -> Result<String, PluginError> {
        let engine = self.engine.get_or_insert_with(Self::new_engine);
        let plugin_dir = self.plugins_dir.join(&dir_name);
        let loaded = load_from_module(&dir_name, engine, &module, &manifest, &plugin_dir, true);
        let id = loaded.id().to_string();
        let error = loaded.load_error.clone();

        self.plugins.push(loaded);
        self.save_state();

        match error {
            Some(e) => Err(PluginError::Manifest(e)),
            None => Ok(id),
        }
    }

    pub fn install_from_dir(&mut self, source: &Path) -> Result<String, PluginError> {
        let engine = self.engine_handle();
        let (dir_name, manifest, module) =
            Self::prepare_install(&engine, &self.plugins_dir, source)?;
        self.finish_install(dir_name, manifest, module)
    }

    /// looks up the on-disk directory name for an installed plugin by its
    /// manifest id (or dir name, for plugins that failed to load).
    pub fn dir_name_for(&self, plugin_id: &str) -> Option<String> {
        self.plugins
            .iter()
            .find(|p| p.id() == plugin_id)
            .map(|p| p.dir_name.clone())
    }

    /// drops a plugin from the active list and persists state, without touching disk
    pub fn drop_plugin(&mut self, plugin_id: &str) {
        if let Some(plugin) = self.plugins.iter().find(|p| p.id() == plugin_id)
            && let Some(runtime) = &plugin.runtime
        {
            ProcessTable::kill_all(&runtime.processes);
        }
        self.plugins.retain(|p| p.id() != plugin_id);
        self.save_state();
    }

    /// synchronous convenience wrapper that removes an installed plugin's
    /// directory from disk and drops it from the in-memory list.
    pub fn uninstall(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        let dir_name = self
            .dir_name_for(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;
        fs::remove_dir_all(self.plugins_dir.join(&dir_name))?;
        self.drop_plugin(plugin_id);
        Ok(())
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

    /// drains buffered process output/exit events across every active
    /// plugin with a spawned process, delivering each into the owning
    /// plugin via the same synchronous call path `run_command`/
    /// `on_panel_event` already use, and returns which plugin ids had
    /// activity so the caller can re-render an open panel for one of them.
    pub fn pump_processes(&mut self) -> Vec<String> {
        let mut touched = Vec::new();
        for plugin in self.plugins.iter_mut().filter(|p| p.is_active()) {
            let plugin_id = plugin.id().to_string();
            let dir_name = plugin.dir_name.clone();
            let runtime = plugin.runtime.as_mut().expect("checked active");
            let events = ProcessTable::drain_events(&runtime.processes);
            if events.is_empty() {
                continue;
            }
            touched.push(plugin_id);
            for event in events {
                let result = match event {
                    ProcessEvent::Output(handle, stream, chunk) => {
                        runtime.handles.call_json::<_, ()>(
                            &mut runtime.store,
                            "on_process_output",
                            (handle, stream, chunk),
                        )
                    }
                    ProcessEvent::Exit(handle, code) => runtime.handles.call_json::<_, ()>(
                        &mut runtime.store,
                        "on_process_exit",
                        (handle, code),
                    ),
                };
                if let Err(e) = result {
                    log_hook_error(runtime, &dir_name, "process-event", &e);
                }
            }
        }
        touched
    }

    /// mirror of `pump_processes` for outbound HTTP requests started via
    /// `http_request`: drains buffered `NetworkEvent`s and delivers each into
    /// the owning plugin's `on_http_response`, returning which plugin ids had
    /// activity so the caller can re-render an open panel for one of them.
    pub fn pump_network(&mut self) -> Vec<String> {
        let mut touched = Vec::new();
        for plugin in self.plugins.iter_mut().filter(|p| p.is_active()) {
            let plugin_id = plugin.id().to_string();
            let dir_name = plugin.dir_name.clone();
            let runtime = plugin.runtime.as_mut().expect("checked active");
            let events = NetworkTable::drain_events(&runtime.network);
            if events.is_empty() {
                continue;
            }
            touched.push(plugin_id);
            for event in events {
                let call_result = match event {
                    NetworkEvent::Chunk(handle, chunk) => runtime.handles.call_json::<_, ()>(
                        &mut runtime.store,
                        "on_http_response_chunk",
                        (handle, chunk),
                    ),
                    NetworkEvent::Response(handle, result) => runtime.handles.call_json::<_, ()>(
                        &mut runtime.store,
                        "on_http_response",
                        (handle, result),
                    ),
                };
                if let Err(e) = call_result {
                    log_hook_error(runtime, &dir_name, "http-response", &e);
                }
            }
        }
        touched
    }

    /// mirror of `pump_network` for file-picker dialogs started via
    /// `pick_files`: drains buffered `FileEvent`s and delivers each into the
    /// owning plugin's `on_files_picked`, returning which plugin ids had
    /// activity so the caller can re-render an open panel for one of them.
    pub fn pump_files(&mut self) -> Vec<String> {
        let mut touched = Vec::new();
        for plugin in self.plugins.iter_mut().filter(|p| p.is_active()) {
            let plugin_id = plugin.id().to_string();
            let dir_name = plugin.dir_name.clone();
            let runtime = plugin.runtime.as_mut().expect("checked active");
            let events = crate::files::FileTable::drain_events(&runtime.files);
            if events.is_empty() {
                continue;
            }
            touched.push(plugin_id);
            for event in events {
                let FileEvent::Picked(handle, result) = event;
                let call_result = runtime.handles.call_json::<_, ()>(
                    &mut runtime.store,
                    "on_files_picked",
                    (handle, result),
                );
                if let Err(e) = call_result {
                    log_hook_error(runtime, &dir_name, "files-picked", &e);
                }
            }
        }
        touched
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

    pub fn right_panels(&self) -> Vec<(String, PanelDef)> {
        let mut out = Vec::new();
        for plugin in self.plugins.iter().filter(|p| p.is_active()) {
            let Some(manifest) = &plugin.manifest else {
                continue;
            };
            for cap in &manifest.capabilities {
                if let Capability::RightPanel(panel) = cap {
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

    pub fn render_right_panel(
        &mut self,
        plugin_id: &str,
        panel_id: &str,
        ctx: RightPanelContext,
    ) -> Result<RightPanelAction, PluginError> {
        let plugin = self.find_active(plugin_id)?;
        let runtime = plugin.runtime.as_mut().expect("checked active");
        runtime
            .handles
            .call_json(&mut runtime.store, "render_right_panel", (panel_id, ctx))
    }

    pub fn right_panel_event(
        &mut self,
        plugin_id: &str,
        panel_id: &str,
        ctx: RightPanelContext,
        event: UiEvent,
    ) -> Result<RightPanelAction, PluginError> {
        let plugin = self.find_active(plugin_id)?;
        let runtime = plugin.runtime.as_mut().expect("checked active");
        runtime.handles.call_json(
            &mut runtime.store,
            "on_right_panel_event",
            (panel_id, ctx, event),
        )
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
