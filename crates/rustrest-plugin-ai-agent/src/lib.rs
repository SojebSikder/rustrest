//! AI agent panel plugin: a chat assistant docked in Rustrest's
//! right panel (`Capability::RightPanel`), with ambient access to the active
//! request/response tab.

use rustrest_plugin_api::{
    CollectionOperation, HttpRequestSpec, HttpResponseData, Plugin, RequestPatch,
    RightPanelAction, RightPanelContext, UiEvent, UiNode,
};
use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentConfig {
    provider: String,
    api_key: String,
    model: String,
    base_url: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        let (base_url, model) = provider_defaults("anthropic");
        Self {
            provider: "anthropic".to_string(),
            api_key: String::new(),
            model,
            base_url,
        }
    }
}

/// sensible (base_url, model) defaults for a provider id.
fn provider_defaults(provider: &str) -> (String, String) {
    match provider {
        "openai" => (
            "https://api.openai.com".to_string(),
            "gpt-4o-mini".to_string(),
        ),
        "ollama" => (String::new(), "llama3".to_string()),
        _ => (
            "https://api.anthropic.com".to_string(),
            "claude-sonnet-5".to_string(),
        ),
    }
}

#[derive(Debug, Clone)]
struct ChatMessage {
    role: String,
    text: String,
}

#[derive(Default)]
struct AiAgentPlugin {
    config: AgentConfig,
    config_loaded: bool,
    show_settings: bool,
    draft_provider: String,
    draft_api_key: String,
    draft_model: String,
    draft_base_url: String,
    messages: Vec<ChatMessage>,
    draft_input: String,
    /// the outbound request's handle, while one is in flight - at most one
    /// at a time, since there's a single Send action to start one.
    pending: Option<u32>,
    busy: bool,
    /// a patch discovered while handling an `on_http_response` (which has no
    /// return value the host acts on) - flushed on the next
    /// `render_right_panel` call.
    pending_patch: Option<RequestPatch>,
    /// mirror of `pending_patch` for a collection-tree operation (create/
    /// rename/delete/duplicate/move) the model proposed instead of a request
    /// patch - at most one of the two is ever set for a given reply.
    pending_collection_op: Option<CollectionOperation>,
}

impl AiAgentPlugin {
    fn load_config_once(&mut self) {
        if self.config_loaded {
            return;
        }
        self.config_loaded = true;
        if let Ok(Some(bytes)) = rustrest_plugin_api::storage_read(CONFIG_FILE)
            && let Ok(cfg) = serde_json::from_slice::<AgentConfig>(&bytes)
        {
            self.config = cfg;
        }
        self.seed_drafts_from_config();
    }

    fn seed_drafts_from_config(&mut self) {
        self.draft_provider = self.config.provider.clone();
        self.draft_api_key = self.config.api_key.clone();
        self.draft_model = self.config.model.clone();
        self.draft_base_url = self.config.base_url.clone();
    }

    fn save_config(&self) {
        if let Ok(bytes) = serde_json::to_vec(&self.config) {
            let _ = rustrest_plugin_api::storage_write(CONFIG_FILE, &bytes);
        }
    }

    fn render_tree(&self, ctx: &RightPanelContext) -> UiNode {
        if self.show_settings || self.config.api_key.is_empty() {
            self.render_settings()
        } else {
            self.render_chat(ctx)
        }
    }

    fn render_settings(&self) -> UiNode {
        let provider_button = |id: &str, label: &str| UiNode::Button {
            id: format!("provider-{id}"),
            label: label.to_string(),
            primary: self.draft_provider == id,
        };

        UiNode::Column(vec![
            UiNode::Label("AI Agent settings".to_string()),
            UiNode::Row(vec![
                provider_button("anthropic", "Anthropic"),
                provider_button("openai", "OpenAI"),
                provider_button("ollama", "Ollama"),
            ]),
            UiNode::TextInput {
                id: "api-key".to_string(),
                value: self.draft_api_key.clone(),
                placeholder: "API key".to_string(),
                on_submit: Some("save-settings".to_string()),
            },
            UiNode::TextInput {
                id: "model".to_string(),
                value: self.draft_model.clone(),
                placeholder: "model name".to_string(),
                on_submit: Some("save-settings".to_string()),
            },
            UiNode::TextInput {
                id: "base-url".to_string(),
                value: self.draft_base_url.clone(),
                placeholder: "https:// base URL".to_string(),
                on_submit: Some("save-settings".to_string()),
            },
            UiNode::Muted(
                "Requests must go to an https:// endpoint (a local Ollama server \
                 needs an https-terminating proxy/tunnel in front of it)."
                    .to_string(),
            ),
            UiNode::Row(vec![
                UiNode::Button {
                    id: "save-settings".to_string(),
                    label: "Save".to_string(),
                    primary: true,
                },
                UiNode::Button {
                    id: "cancel-settings".to_string(),
                    label: "Cancel".to_string(),
                    primary: false,
                },
            ]),
        ])
    }

    fn render_chat(&self, ctx: &RightPanelContext) -> UiNode {
        let header = UiNode::Row(vec![
            UiNode::Muted(format!("Provider: {}", self.config.provider)),
            UiNode::HorizontalSpacer,
            UiNode::Button {
                id: "toggle-settings".to_string(),
                label: "\u{2699}".to_string(), // gear icon
                primary: false,
            },
        ]);

        let mut conversation = Vec::new();

        match (&ctx.active_request, &ctx.active_response) {
            (None, _) => conversation.push(UiNode::Muted(
                "No active request - open one for context.".to_string(),
            )),
            (Some(req), resp) => {
                let mut summary = format!("{} {}", req.method, req.url);
                if let Some(resp) = resp {
                    summary.push_str(&format!(" \u{2192} {}", resp.status));
                }
                conversation.push(UiNode::Muted(summary));
            }
        }

        if self.messages.is_empty() {
            conversation.push(UiNode::Muted(
                "Ask a question, or tell me what to change - e.g. \"explain this response\" \
                 or \"add an Authorization header\"."
                    .to_string(),
            ));
        } else {
            conversation.push(UiNode::Column(
                self.messages
                    .iter()
                    .map(|m| {
                        UiNode::Column(vec![
                            UiNode::Muted(m.role.clone()),
                            UiNode::Label(m.text.clone()),
                        ])
                    })
                    .collect(),
            ));
        }

        if self.busy {
            conversation.push(UiNode::Muted("Thinking...".to_string()));
        }

        let input_row = UiNode::Row(vec![
            UiNode::TextInput {
                id: "chat-input".to_string(),
                value: self.draft_input.clone(),
                placeholder: "Message Rustrest Agent...".to_string(),
                on_submit: Some("send".to_string()),
            },
            UiNode::Button {
                id: "send".to_string(),
                label: "Send".to_string(),
                primary: true,
            },
        ]);

        UiNode::Column(vec![
            header,
            UiNode::Scrollable(Box::new(UiNode::Column(conversation))),
            input_row,
        ])
    }

    fn start_request(&mut self, instruction: String, ctx: &RightPanelContext) {
        if self.busy || instruction.trim().is_empty() {
            return;
        }
        if self.config.api_key.is_empty() && self.config.provider != "ollama" {
            self.messages.push(ChatMessage {
                role: "Error".to_string(),
                text: "No API key configured - open Settings first.".to_string(),
            });
            return;
        }

        let system_prompt = build_system_prompt(ctx);
        let spec = match build_request_spec(&self.config, &system_prompt, &instruction) {
            Ok(spec) => spec,
            Err(e) => {
                self.messages.push(ChatMessage {
                    role: "Error".to_string(),
                    text: e,
                });
                return;
            }
        };

        match rustrest_plugin_api::http_request(spec) {
            Ok(handle) => {
                self.pending = Some(handle);
                self.busy = true;
                self.messages.push(ChatMessage {
                    role: "You".to_string(),
                    text: instruction,
                });
                self.draft_input.clear();
            }
            Err(e) => {
                self.messages.push(ChatMessage {
                    role: "Error".to_string(),
                    text: format!("request failed to start: {e}"),
                });
            }
        }
    }
}

impl Plugin for AiAgentPlugin {
    fn render_right_panel(&mut self, _panel_id: &str, ctx: RightPanelContext) -> RightPanelAction {
        self.load_config_once();
        let tree = self.render_tree(&ctx);
        if let Some(op) = self.pending_collection_op.take() {
            return RightPanelAction::UpdateUiAndProposeCollectionOp(tree, op);
        }
        match self.pending_patch.take() {
            Some(patch) => RightPanelAction::UpdateUiAndApplyPatch(tree, patch),
            None => RightPanelAction::UpdateUi(tree),
        }
    }

    fn on_right_panel_event(
        &mut self,
        _panel_id: &str,
        ctx: RightPanelContext,
        event: UiEvent,
    ) -> RightPanelAction {
        self.load_config_once();

        match event {
            UiEvent::Clicked(id) => {
                if let Some(provider) = id.strip_prefix("provider-") {
                    self.draft_provider = provider.to_string();
                    let (base_url, model) = provider_defaults(provider);
                    self.draft_base_url = base_url;
                    self.draft_model = model;
                } else {
                    match id.as_str() {
                        "toggle-settings" => {
                            self.show_settings = !self.show_settings;
                            self.seed_drafts_from_config();
                        }
                        "cancel-settings" => {
                            self.show_settings = false;
                            self.seed_drafts_from_config();
                        }
                        "save-settings" => {
                            self.config = AgentConfig {
                                provider: self.draft_provider.clone(),
                                api_key: self.draft_api_key.clone(),
                                model: self.draft_model.clone(),
                                base_url: self.draft_base_url.clone(),
                            };
                            self.save_config();
                            self.show_settings = false;
                        }
                        "send" => {
                            let instruction = self.draft_input.clone();
                            self.start_request(instruction, &ctx);
                        }
                        _ => return RightPanelAction::None,
                    }
                }
            }
            UiEvent::Changed(id, value) => match id.as_str() {
                "api-key" => self.draft_api_key = value,
                "model" => self.draft_model = value,
                "base-url" => self.draft_base_url = value,
                "chat-input" => self.draft_input = value,
                _ => return RightPanelAction::None,
            },
            UiEvent::Toggled(..) => return RightPanelAction::None,
        }

        RightPanelAction::UpdateUi(self.render_tree(&ctx))
    }

    fn on_http_response(&mut self, handle: u32, result: Result<HttpResponseData, String>) {
        if self.pending != Some(handle) {
            return;
        }
        self.pending = None;
        self.busy = false;

        match result {
            Err(e) => self.messages.push(ChatMessage {
                role: "Error".to_string(),
                text: e,
            }),
            Ok(data) => match parse_provider_reply(&self.config.provider, &data.body) {
                Ok(text) => {
                    // the model decides for itself, per the system prompt,
                    // whether a reply warrants a patch or a collection-tree
                    // operation - so every reply is checked, regardless of
                    // what was asked. At most one of the two ever applies.
                    if let Some(op) = extract_collection_op(&text) {
                        self.pending_collection_op = Some(op);
                    } else if let Some(patch) = extract_patch(&text) {
                        self.pending_patch = Some(patch);
                    }
                    self.messages.push(ChatMessage {
                        role: "Assistant".to_string(),
                        text,
                    });
                }
                Err(e) => self.messages.push(ChatMessage {
                    role: "Error".to_string(),
                    text: format!(
                        "couldn't parse provider response (HTTP {}): {e}",
                        data.status
                    ),
                }),
            },
        }
    }
}

fn build_system_prompt(ctx: &RightPanelContext) -> String {
    let mut prompt = String::from(
        "You are an AI assistant embedded in Rustrest, a REST API client, docked next to \
         whatever request the user has open. You can: answer questions, explain responses \
         and errors in plain language, edit the active request, and write post-response test \
         scripts. Decide what's needed from the user's message - don't ask them to pick a mode.",
    );

    if let Some(req) = &ctx.active_request {
        prompt.push_str(&format!(
            "\n\nActive request:\nmethod: {}\nurl: {}\nheaders: {:?}\nbody: {}",
            req.method, req.url, req.headers, req.body
        ));
    }
    if let Some(resp) = &ctx.active_response {
        prompt.push_str(&format!(
            "\n\nLast response:\nstatus: {}\nheaders: {:?}\nbody: {}",
            resp.status, resp.headers, resp.body
        ));
    }

    if !ctx.collections.is_empty() {
        prompt.push_str("\n\nExisting collections (reference these by id, not by guessing):");
        for col in &ctx.collections {
            prompt.push_str(&format!("\n- collection #{} \"{}\"", col.id, col.name));
            for folder in &col.folders {
                prompt.push_str(&format!("\n    folder: {}", folder.join("/")));
            }
            for req in &col.requests {
                let location = if req.folder_path.is_empty() {
                    "collection root".to_string()
                } else {
                    req.folder_path.join("/")
                };
                prompt.push_str(&format!(
                    "\n    request #{} \"{}\" ({}) in {}",
                    req.id, req.name, req.method, location
                ));
            }
        }
    }

    prompt.push_str(
        "\n\nAnswer in plain, concise language. If, and only if, the user's message asks you to \
         change the active request or add/replace its post-response test script, follow your \
         explanation with exactly one fenced ```json code block containing ONLY the fields that \
         should change, matching this shape: {\"method\":string|null,\"url\":string|null,\
         \"headers\":[[string,string]]|null,\"body\":string|null,\"post_response_script\":string|null}. \
         Omit fields you don't want to change (or set them to null). The test script, if any, is \
         JavaScript using the pm.* scripting API this app exposes (pm.test(name, fn), \
         pm.expect(...), pm.response.status/json()).\n\n\
         If, and only if, the user's message asks you to create, rename, delete, duplicate, or \
         move a collection, folder, or request, follow your explanation with exactly one fenced \
         ```collection-action code block (instead of the ```json one above - never both) \
         containing ONLY one JSON object shaped as one of:\n\
         {\"op\":\"create_collection\",\"name\":string}\n\
         {\"op\":\"rename_collection\",\"collection_id\":number,\"new_name\":string}\n\
         {\"op\":\"delete_collection\",\"collection_id\":number}\n\
         {\"op\":\"create_folder\",\"collection_id\":number,\"parent_path\":[string],\"name\":string}\n\
         {\"op\":\"rename_folder\",\"collection_id\":number,\"path\":[string],\"new_name\":string}\n\
         {\"op\":\"delete_folder\",\"collection_id\":number,\"path\":[string]}\n\
         {\"op\":\"create_request\",\"collection_id\":number,\"parent_path\":[string],\"name\":string,\"method\":string,\"url\":string}\n\
         {\"op\":\"rename_request\",\"collection_id\":number,\"request_id\":number,\"new_name\":string}\n\
         {\"op\":\"delete_request\",\"collection_id\":number,\"parent_path\":[string],\"request_id\":number}\n\
         {\"op\":\"duplicate_request\",\"collection_id\":number,\"parent_path\":[string],\"request_id\":number}\n\
         {\"op\":\"move_request\",\"collection_id\":number,\"from_path\":[string],\"request_id\":number,\"to_path\":[string]}\n\
         `parent_path`/`path`/`from_path`/`to_path` are folder-name arrays (e.g. [] for the \
         collection root, [\"Auth\"] for a top-level folder named Auth). Use the ids/paths listed \
         above under \"Existing collections\" - never invent one. Deleting or moving something \
         will ask the user to confirm before it's applied; say so in your explanation. Otherwise, \
         don't include this block at all.",
    );

    prompt
}

fn build_request_spec(
    config: &AgentConfig,
    system_prompt: &str,
    instruction: &str,
) -> Result<HttpRequestSpec, String> {
    let base_url = if config.base_url.is_empty() {
        return Err("no base URL configured - open Settings first.".to_string());
    } else {
        config.base_url.trim_end_matches('/')
    };
    if !base_url.starts_with("https://") {
        return Err("base URL must start with https://".to_string());
    }

    let (url, body): (String, serde_json::Value) = match config.provider.as_str() {
        "openai" | "ollama" => {
            let endpoint = if config.provider == "openai" {
                "/v1/chat/completions"
            } else {
                "/api/chat"
            };
            let mut body = serde_json::json!({
                "model": config.model,
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": instruction},
                ],
            });
            if config.provider == "ollama" {
                body["stream"] = serde_json::Value::Bool(false);
            }
            (format!("{base_url}{endpoint}"), body)
        }
        _ => {
            let body = serde_json::json!({
                "model": config.model,
                "max_tokens": 1024,
                "system": system_prompt,
                "messages": [{"role": "user", "content": instruction}],
            });
            (format!("{base_url}/v1/messages"), body)
        }
    };

    let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
    match config.provider.as_str() {
        "openai" => headers.push((
            "Authorization".to_string(),
            format!("Bearer {}", config.api_key),
        )),
        "ollama" => {
            if !config.api_key.is_empty() {
                headers.push((
                    "Authorization".to_string(),
                    format!("Bearer {}", config.api_key),
                ));
            }
        }
        _ => {
            headers.push(("x-api-key".to_string(), config.api_key.clone()));
            headers.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
        }
    }

    Ok(HttpRequestSpec {
        method: "POST".to_string(),
        url,
        headers,
        body: Some(
            serde_json::to_vec(&body).map_err(|e| format!("failed to encode request: {e}"))?,
        ),
    })
}

fn parse_provider_reply(provider: &str, body: &[u8]) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;

    if let Some(err) = value.get("error") {
        return Err(err.to_string());
    }

    let text = match provider {
        "openai" => value
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str()),
        "ollama" => value.pointer("/message/content").and_then(|v| v.as_str()),
        _ => value.pointer("/content/0/text").and_then(|v| v.as_str()),
    };

    text.map(str::to_string)
        .ok_or_else(|| format!("unrecognized response shape: {value}"))
}

/// pulls a fenced ```json ... ``` block out of `text` and decodes it as a
/// `RequestPatch`.
fn extract_patch(text: &str) -> Option<RequestPatch> {
    let start = text.find("```json")? + "```json".len();
    let rest = &text[start..];
    let end = rest.find("```")?;
    serde_json::from_str(rest[..end].trim()).ok()
}

/// pulls a fenced ```collection-action ... ``` block out of `text` and
/// decodes it as a `CollectionOperation`.
fn extract_collection_op(text: &str) -> Option<CollectionOperation> {
    let start = text.find("```collection-action")? + "```collection-action".len();
    let rest = &text[start..];
    let end = rest.find("```")?;
    serde_json::from_str(rest[..end].trim()).ok()
}

rustrest_plugin_api::export_plugin!(AiAgentPlugin);
