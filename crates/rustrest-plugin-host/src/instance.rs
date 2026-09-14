use crate::codec::CallHandles;
use crate::error::PluginError;
use crate::state::{PluginState, link_host_functions};
use rustrest_plugin_api::PluginManifest;
use std::path::Path;
use std::sync::{Arc, Mutex};
use wasmtime::{Engine, Linker, Module, Store};

pub struct PluginRuntime {
    pub store: Store<PluginState>,
    pub handles: CallHandles,
    pub logs: Arc<Mutex<Vec<String>>>,
}

/// A discovered plugin. Always present in `PluginManager::plugins`, even if
/// loading failed, so the plugin-manager UI can surface the failure instead
/// of the plugin silently vanishing.
pub struct LoadedPlugin {
    /// directory name under the plugins dir - the id used for lookups until
    /// (and unless) a manifest fails to load.
    pub dir_name: String,
    pub manifest: Option<PluginManifest>,
    pub enabled: bool,
    pub load_error: Option<String>,
    pub(crate) runtime: Option<PluginRuntime>,
}

impl LoadedPlugin {
    pub fn id(&self) -> &str {
        self.manifest
            .as_ref()
            .map(|m| m.id.as_str())
            .unwrap_or(&self.dir_name)
    }

    pub fn is_active(&self) -> bool {
        self.enabled && self.runtime.is_some()
    }
}

pub fn load_plugin(
    engine: &Engine,
    dir_name: &str,
    wasm_path: &Path,
    enabled: bool,
) -> LoadedPlugin {
    match try_load(engine, dir_name, wasm_path) {
        Ok((manifest, runtime)) => LoadedPlugin {
            dir_name: dir_name.to_string(),
            manifest: Some(manifest),
            enabled,
            load_error: None,
            runtime: Some(runtime),
        },
        Err(e) => LoadedPlugin {
            dir_name: dir_name.to_string(),
            manifest: None,
            enabled: false,
            load_error: Some(e.to_string()),
            runtime: None,
        },
    }
}

fn try_load(
    engine: &Engine,
    dir_name: &str,
    wasm_path: &Path,
) -> Result<(PluginManifest, PluginRuntime), PluginError> {
    let module = Module::from_file(engine, wasm_path)?;

    let mut linker: Linker<PluginState> = Linker::new(engine);
    link_host_functions(&mut linker)?;

    let logs = Arc::new(Mutex::new(Vec::new()));
    let state = PluginState {
        plugin_id: dir_name.to_string(),
        logs: logs.clone(),
    };
    let mut store = Store::new(engine, state);
    // generous one-off budget for instantiation/global-init; steady-state
    // calls each set their own smaller budget in `CallHandles::call_json`.
    store.set_fuel(u64::MAX / 2)?;

    let instance = linker.instantiate(&mut store, &module)?;
    let handles = CallHandles::resolve(&mut store, &instance, dir_name)?;

    let manifest: PluginManifest =
        handles.call_json(&mut store, "manifest", serde_json::Value::Null)?;
    if manifest.id != dir_name {
        return Err(PluginError::Manifest(format!(
            "manifest id '{}' does not match plugin directory name '{}'",
            manifest.id, dir_name
        )));
    }

    Ok((
        manifest,
        PluginRuntime {
            store,
            handles,
            logs,
        },
    ))
}
