use crate::process::ProcessTable;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct PluginState {
    pub plugin_id: String,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub external_process_allowed: bool,
    pub storage_dir: PathBuf,
    pub processes: Arc<Mutex<ProcessTable>>,
}
