//! SDK for the `LanguageServer` capability. The host runs the server and
//! its LSP client drives the script editors (completions, hover, diagnostics).

use serde::{Deserialize, Serialize};

/// how the host should launch a declared language server.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LanguageServerCommand {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
}
