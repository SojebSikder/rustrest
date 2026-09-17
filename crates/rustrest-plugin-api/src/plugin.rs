use crate::hooks::{RequestContext, ResponseContext, RightPanelAction, RightPanelContext};
use crate::network::HttpResponseData;
use crate::process::ProcessStream;
use crate::ui::{UiEvent, UiNode};

/// Implemented by a plugin's single entry-point type. Every method has a
/// no-op default so a plugin only needs to override the capabilities it
/// declared in its `plugin.toml` manifest; the host only ever calls the
/// methods matching a declared `Capability`.
pub trait Plugin: Default + Send + 'static {
    fn on_pre_request(&mut self, ctx: RequestContext) -> RequestContext {
        ctx
    }

    fn on_post_response(&mut self, ctx: ResponseContext) -> ResponseContext {
        ctx
    }

    /// `command_id` is one of the ids this plugin declared via
    /// `Capability::Commands`/`Capability::MenuItems`. The returned string, if
    /// any, is shown to the user as a toast.
    fn on_command(&mut self, _command_id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    /// `panel_id` is one this plugin declared via `Capability::SidebarPanel`.
    fn render_panel(&mut self, _panel_id: &str) -> UiNode {
        UiNode::Column(Vec::new())
    }

    /// Returns the updated panel tree if the interaction changed anything,
    /// or `None` to leave the currently rendered tree as-is.
    fn on_panel_event(&mut self, _panel_id: &str, _event: UiEvent) -> Option<UiNode> {
        None
    }

    /// `format_id` is one this plugin declared via `Capability::ImportFormat`.
    /// Returns Rustrest's own collection JSON model on success.
    fn import(&mut self, _format_id: &str, _bytes: Vec<u8>) -> Result<serde_json::Value, String> {
        Err("import not supported by this plugin".to_string())
    }

    /// `format_id` is one this plugin declared via `Capability::ExportFormat`.
    /// `collection` is Rustrest's own collection JSON model.
    fn export(
        &mut self,
        _format_id: &str,
        _collection: serde_json::Value,
    ) -> Result<Vec<u8>, String> {
        Err("export not supported by this plugin".to_string())
    }

    /// delivered whenever a process spawned via `process::Process::spawn`
    /// produces new output. Requires `Capability::ExternalProcess`.
    fn on_process_output(&mut self, _handle: u32, _stream: ProcessStream, _chunk: Vec<u8>) {}

    /// delivered once a process spawned via `process::Process::spawn` exits.
    /// `code` is `None` if it was killed by a signal. Requires
    /// `Capability::ExternalProcess`.
    fn on_process_exit(&mut self, _handle: u32, _code: Option<i32>) {}

    /// `panel_id` is one this plugin declared via `Capability::RightPanel`.
    /// `ctx` is a snapshot of the active request tab, if any. Returns a
    /// `RightPanelAction` rather than a plain `UiNode` (unlike
    /// `render_panel`) so a plugin can flush a patch discovered
    /// asynchronously - e.g. in `on_http_response`, after an outbound
    /// request the user kicked off has completed - on its next render,
    /// since `on_http_response` itself has no return value the host acts on.
    fn render_right_panel(&mut self, _panel_id: &str, _ctx: RightPanelContext) -> RightPanelAction {
        RightPanelAction::UpdateUi(UiNode::Column(Vec::new()))
    }

    /// handles a widget interaction inside the right panel. Return
    /// `RightPanelAction::None` to leave both the panel and the active tab
    /// as-is.
    fn on_right_panel_event(
        &mut self,
        _panel_id: &str,
        _ctx: RightPanelContext,
        _event: UiEvent,
    ) -> RightPanelAction {
        RightPanelAction::None
    }

    /// delivered when a request started via `network::http_request` completes
    /// (or fails). Requires `Capability::ExternalProcess`.
    fn on_http_response(&mut self, _handle: u32, _result: Result<HttpResponseData, String>) {}

    /// delivered for each line of a response body as it's read off the
    /// socket, before the terminal `on_http_response` call with the
    /// complete body - lets a plugin render a streamed reply (SSE/NDJSON)
    /// incrementally. Default no-op, for plugins that only need the final
    /// result. Requires `Capability::ExternalProcess`.
    fn on_http_response_chunk(&mut self, _handle: u32, _chunk: Vec<u8>) {}
}
