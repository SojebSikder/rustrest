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

/// ambient context handed to a `RightPanel` plugin on every render/event -
/// a snapshot of whatever the active tab currently holds, so a docked panel
/// (e.g. an AI assistant) can act on "the current request" without needing
/// its own request-tracking plumbing. Both fields are `None` when the active
/// tab isn't an HTTP request tab (or there is no active tab).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RightPanelContext {
    pub active_request: Option<RequestContext>,
    pub active_response: Option<ResponseContext>,
}

/// a set of edits a `RightPanel` plugin wants applied to the active request
/// tab. Every field is optional - only the fields a plugin actually wants to
/// change need be `Some`; everything else is left as-is.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestPatch {
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers: Option<Vec<(String, String)>>,
    pub body: Option<String>,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
}

/// returned by `Plugin::on_right_panel_event` to tell the host what changed:
/// the panel's own UI, the active request tab, or both.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RightPanelAction {
    /// nothing changed; leave the currently rendered tree and tab as-is.
    None,
    /// re-render the panel with this tree; don't touch the active tab.
    UpdateUi(crate::ui::UiNode),
    /// apply this patch to the active request tab; don't re-render the panel.
    ApplyPatch(RequestPatch),
    /// re-render the panel with this tree AND apply this patch to the active
    /// request tab.
    UpdateUiAndApplyPatch(crate::ui::UiNode, RequestPatch),
}
