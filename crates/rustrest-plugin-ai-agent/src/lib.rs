//! AI agent panel plugin: a chat assistant docked in Rustrest's
//! right panel (`Capability::RightPanel`), with ambient access to the active
//! request/response tab.

use rustrest_plugin_api::{
    HttpRequestSpec, HttpResponseData, Plugin, RequestPatch, RightPanelAction, RightPanelContext,
    UiEvent, UiNode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Intent {
    Chat,
    ExplainResponse,
    EditRequest,
    GenerateTests,
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
    pending: HashMap<u32, Intent>,
    busy: bool,
    /// a patch discovered while handling an `on_http_response` (which has no
    /// return value the host acts on) - flushed on the next
    /// `render_right_panel` call.
    pending_patch: Option<RequestPatch>,
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
            label: if self.draft_provider == id {
                format!("> {label}")
            } else {
                label.to_string()
            },
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
            },
            UiNode::TextInput {
                id: "model".to_string(),
                value: self.draft_model.clone(),
                placeholder: "model name".to_string(),
            },
            UiNode::TextInput {
                id: "base-url".to_string(),
                value: self.draft_base_url.clone(),
                placeholder: "https:// base URL".to_string(),
            },
            UiNode::Label(
                "Requests must go to an https:// endpoint (a local Ollama server \
                 needs an https-terminating proxy/tunnel in front of it)."
                    .to_string(),
            ),
            UiNode::Row(vec![
                UiNode::Button {
                    id: "save-settings".to_string(),
                    label: "Save".to_string(),
                },
                UiNode::Button {
                    id: "cancel-settings".to_string(),
                    label: "Cancel".to_string(),
                },
            ]),
        ])
    }

    fn render_chat(&self, ctx: &RightPanelContext) -> UiNode {
        let mut children = vec![UiNode::Row(vec![
            UiNode::Label(format!("Provider: {}", self.config.provider)),
            UiNode::Button {
                id: "toggle-settings".to_string(),
                label: "Settings".to_string(),
            },
        ])];

        if let Some(req) = &ctx.active_request {
            children.push(UiNode::Label(format!("Active: {} {}", req.method, req.url)));
        }
        if let Some(resp) = &ctx.active_response {
            children.push(UiNode::Label(format!("Last response: {}", resp.status)));
        }

        children.push(UiNode::Column(
            self.messages
                .iter()
                .map(|m| UiNode::Label(format!("{}: {}", m.role, m.text)))
                .collect(),
        ));

        if self.busy {
            children.push(UiNode::Label("Thinking...".to_string()));
        }

        children.push(UiNode::Row(vec![
            UiNode::TextInput {
                id: "chat-input".to_string(),
                value: self.draft_input.clone(),
                placeholder: "Ask about this request...".to_string(),
            },
            UiNode::Button {
                id: "send".to_string(),
                label: "Send".to_string(),
            },
        ]));
        children.push(UiNode::Row(vec![
            UiNode::Button {
                id: "explain-response".to_string(),
                label: "Explain response".to_string(),
            },
            UiNode::Button {
                id: "generate-tests".to_string(),
                label: "Generate tests".to_string(),
            },
            UiNode::Button {
                id: "edit-request".to_string(),
                label: "Edit request".to_string(),
            },
        ]));

        UiNode::Column(children)
    }

    fn start_request(&mut self, intent: Intent, instruction: String, ctx: &RightPanelContext) {
        if self.config.api_key.is_empty() && self.config.provider != "ollama" {
            self.messages.push(ChatMessage {
                role: "Error".to_string(),
                text: "No API key configured - open Settings first.".to_string(),
            });
            return;
        }
        if instruction.trim().is_empty() {
            return;
        }

        let system_prompt = build_system_prompt(intent, ctx);
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
                self.pending.insert(handle, intent);
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
                            self.start_request(Intent::Chat, instruction, &ctx);
                        }
                        "edit-request" => {
                            let instruction = self.draft_input.clone();
                            self.start_request(Intent::EditRequest, instruction, &ctx);
                        }
                        "generate-tests" => {
                            self.start_request(
                                Intent::GenerateTests,
                                "Write a post-response test script for this request/response."
                                    .to_string(),
                                &ctx,
                            );
                        }
                        "explain-response" => {
                            self.start_request(
                                Intent::ExplainResponse,
                                "Explain this response, including any errors, in plain language."
                                    .to_string(),
                                &ctx,
                            );
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
        let Some(intent) = self.pending.remove(&handle) else {
            return;
        };
        self.busy = false;

        match result {
            Err(e) => self.messages.push(ChatMessage {
                role: "Error".to_string(),
                text: e,
            }),
            Ok(data) => match parse_provider_reply(&self.config.provider, &data.body) {
                Ok(text) => {
                    if matches!(intent, Intent::EditRequest | Intent::GenerateTests)
                        && let Some(patch) = extract_patch(&text)
                    {
                        self.pending_patch = Some(patch);
                    }
                    self.messages.push(ChatMessage {
                        role: "Assistant".to_string(),
                        text,
                    });
                }
                Err(e) => self.messages.push(ChatMessage {
                    role: "Error".to_string(),
                    text: format!("couldn't parse provider response (HTTP {}): {e}", data.status),
                }),
            },
        }
    }
}

fn build_system_prompt(intent: Intent, ctx: &RightPanelContext) -> String {
    let mut prompt = String::from(
        "You are an AI assistant embedded in Rustrest, a REST API client. \
         You help the user with the request/response currently open in their active tab.",
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

    match intent {
        Intent::Chat | Intent::ExplainResponse => {
            prompt.push_str("\n\nAnswer in plain, concise language.");
        }
        Intent::EditRequest => {
            prompt.push_str(
                "\n\nThe user wants you to edit the active request. After a short explanation, \
                 append a fenced ```json code block containing ONLY the fields that should \
                 change, matching this shape: \
                 {\"method\":string|null,\"url\":string|null,\"headers\":[[string,string]]|null,\"body\":string|null}. \
                 Omit fields you don't want to change (or set them to null).",
            );
        }
        Intent::GenerateTests => {
            prompt.push_str(
                "\n\nThe user wants a post-response test script (JavaScript, using the pm.* \
                 scripting API this app exposes, e.g. pm.test(name, fn), pm.expect(...), \
                 pm.response.status/json()). After a short explanation, append a fenced ```json \
                 code block of the shape {\"post_response_script\": string} containing the \
                 script as a single string.",
            );
        }
    }

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
        _ => value
            .pointer("/content/0/text")
            .and_then(|v| v.as_str()),
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

rustrest_plugin_api::export_plugin!(AiAgentPlugin);
