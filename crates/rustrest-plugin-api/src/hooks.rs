use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An outgoing request, threaded through every enabled plugin with the
/// `RequestHooks` capability before it is sent. Mirrors the shape of the
/// existing `pm.*` JS pre-request scripting context so behavior stays
/// consistent between the two mechanisms.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestContext {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub variables: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
}

/// A received response, threaded through every enabled plugin with the
/// `RequestHooks` capability right after it arrives, mirroring the existing
/// `pm.response`/post-response test-script context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResponseContext {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub variables: HashMap<String, String>,
    #[serde(default)]
    pub test_results: Vec<TestResult>,
}
