use crate::error::PluginError;
use crate::files::FileTable;
use crate::hostcall::link_host_functions;
use crate::manifest_toml::{self, WASM_FILE_NAME};
use crate::network::NetworkTable;
use crate::process::ProcessTable;
use crate::state::PluginState;
use rustrest_plugin_api::{Capability, PluginManifest};
use std::path::Path;
use std::sync::{Arc, Mutex};
use wasmtime::{Engine, Linker, Module, Store};

use crate::codec::CallHandles;

pub struct PluginRuntime {
    pub store: Store<PluginState>,
    pub handles: CallHandles,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub processes: Arc<Mutex<ProcessTable>>,
    pub network: Arc<Mutex<NetworkTable>>,
    pub files: Arc<Mutex<FileTable>>,
}

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

/// discovers, parses `plugin.toml`, and (if that succeeds) compiles +
/// instantiates `plugin.wasm` for the plugin directory `plugin_dir`.
pub fn load_plugin(
    engine: &Engine,
    dir_name: &str,
    plugin_dir: &Path,
    enabled: bool,
) -> LoadedPlugin {
    let manifest = match manifest_toml::read_from_dir(plugin_dir) {
        Ok(m) => m,
        Err(e) => {
            return LoadedPlugin {
                dir_name: dir_name.to_string(),
                manifest: None,
                enabled: false,
                load_error: Some(e.to_string()),
                runtime: None,
            };
        }
    };

    if manifest.id != dir_name {
        return LoadedPlugin {
            dir_name: dir_name.to_string(),
            load_error: Some(format!(
                "manifest id '{}' does not match plugin directory name '{}'",
                manifest.id, dir_name
            )),
            manifest: Some(manifest),
            enabled: false,
            runtime: None,
        };
    }

    let wasm_path = plugin_dir.join(WASM_FILE_NAME);
    match instantiate(engine, dir_name, &wasm_path, &manifest, plugin_dir) {
        Ok(runtime) => LoadedPlugin {
            dir_name: dir_name.to_string(),
            manifest: Some(manifest),
            enabled,
            load_error: None,
            runtime: Some(runtime),
        },
        Err(e) => LoadedPlugin {
            dir_name: dir_name.to_string(),
            manifest: Some(manifest),
            enabled: false,
            load_error: Some(e.to_string()),
            runtime: None,
        },
    }
}

fn instantiate(
    engine: &Engine,
    label: &str,
    wasm_path: &Path,
    manifest: &PluginManifest,
    plugin_dir: &Path,
) -> Result<PluginRuntime, PluginError> {
    let module = Module::from_file(engine, wasm_path)?;
    instantiate_module(engine, label, &module, manifest, plugin_dir)
}

fn instantiate_module(
    engine: &Engine,
    label: &str,
    module: &Module,
    manifest: &PluginManifest,
    plugin_dir: &Path,
) -> Result<PluginRuntime, PluginError> {
    let mut linker: Linker<PluginState> = Linker::new(engine);
    link_host_functions(&mut linker)?;

    let logs = Arc::new(Mutex::new(Vec::new()));
    let processes = Arc::new(Mutex::new(ProcessTable::default()));
    let network = Arc::new(Mutex::new(NetworkTable::default()));
    let files = Arc::new(Mutex::new(FileTable::default()));
    let external_process_allowed = manifest
        .capabilities
        .iter()
        .any(|c| matches!(c, Capability::ExternalProcess));

    let state = PluginState {
        plugin_id: label.to_string(),
        logs: logs.clone(),
        external_process_allowed,
        storage_dir: plugin_dir.join("storage"),
        processes: processes.clone(),
        network: network.clone(),
        files: files.clone(),
    };
    let mut store = Store::new(engine, state);
    // generous one-off budget for instantiation/global-init; steady-state
    // calls each set their own smaller budget in `CallHandles::call_json`.
    store.set_fuel(u64::MAX / 2)?;

    let instance = linker.instantiate(&mut store, module)?;
    let handles = CallHandles::resolve(&mut store, &instance, label)?;

    Ok(PluginRuntime {
        store,
        handles,
        logs,
        processes,
        network,
        files,
    })
}

/// Compiles a plugin's wasm module without activating it. Used while
/// preparing an install so the (potentially slow) compile step can run on a
/// background thread ahead of committing anything to disk.
pub fn compile(engine: &Engine, wasm_path: &Path) -> Result<Module, PluginError> {
    Ok(Module::from_file(engine, wasm_path)?)
}

/// Instantiates an already-compiled module as an active plugin.
pub fn load_from_module(
    dir_name: &str,
    engine: &Engine,
    module: &Module,
    manifest: &PluginManifest,
    plugin_dir: &Path,
    enabled: bool,
) -> LoadedPlugin {
    match instantiate_module(engine, dir_name, module, manifest, plugin_dir) {
        Ok(runtime) => LoadedPlugin {
            dir_name: dir_name.to_string(),
            manifest: Some(manifest.clone()),
            enabled,
            load_error: None,
            runtime: Some(runtime),
        },
        Err(e) => LoadedPlugin {
            dir_name: dir_name.to_string(),
            manifest: Some(manifest.clone()),
            enabled: false,
            load_error: Some(e.to_string()),
            runtime: None,
        },
    }
}
