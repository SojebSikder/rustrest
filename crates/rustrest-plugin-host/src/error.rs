use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid plugin manifest: {0}")]
    Manifest(String),
    #[error("wasm error: {0}")]
    Wasm(#[from] wasmtime::Error),
    #[error("plugin json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("plugin memory access error: {0}")]
    Memory(String),
    #[error("plugin '{0}' has no exported linear memory")]
    NoMemory(String),
    #[error("plugin returned an error: {0}")]
    Plugin(String),
    #[error("plugin '{0}' not found or not enabled")]
    NotFound(String),
}
