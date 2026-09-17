//! SDK for outbound HTTP requests, gated by the `ExternalProcess` capability.

use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use crate::hostcall;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestSpec {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponseData {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// starts an outbound HTTPS request, returning a handle immediately. The
/// result is delivered later via `Plugin::on_http_response(handle, result)` -
/// this call never blocks waiting for the network. `https://` only,
/// size-capped and timed-out host-side.
#[cfg(target_arch = "wasm32")]
pub fn http_request(spec: HttpRequestSpec) -> Result<u32, String> {
    hostcall::call("http_request", spec)
}
