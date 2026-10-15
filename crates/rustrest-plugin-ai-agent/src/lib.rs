//! AI agent panel plugin: a chat assistant docked in Rustrest's
//! right panel (`Capability::RightPanel`), with ambient access to the active
//! request/response tab.

use rustrest_plugin_api::{
    ButtonVariant, CollectionOperation, HttpRequestSpec, HttpResponseData, PickFilesResult, PickedFile, Plugin,
    RequestPatch, RightPanelAction, RightPanelContext, TextTone, UiEvent, UiNode, UiStyle,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const CONFIG_FILE: &str = "config.json";
/// list of saved chat sessions, most recently updated first. Each session's
/// transcript lives in its own `session-<id>.json` (see [`session_file`]).
const SESSIONS_INDEX_FILE: &str = "sessions.json";
/// saved sessions kept in history - the oldest are deleted beyond this.
const MAX_SAVED_SESSIONS: usize = 50;
/// longest session title shown in history, in chars.
const MAX_TITLE_CHARS: usize = 60;
/// most recent chat messages (user + assistant) sent back to the model as
/// conversation history. bounds the prompt size on long sessions.
const MAX_HISTORY_MESSAGES: usize = 20;
/// reply token budget. large enough for a full test script plus its
/// explanation, so the trailing action block isn't cut off mid-JSON.
const MAX_TOKENS: u32 = 8192;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentConfig {
    provider: String,
    api_key: String,
    model: String,
    base_url: String,
    /// render assistant replies as markdown (true) or plain text (false).
    #[serde(default = "default_true")]
    render_markdown: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AgentConfig {
    fn default() -> Self {
        let (base_url, model) = provider_defaults("anthropic");
        Self {
            provider: "anthropic".to_string(),
            api_key: String::new(),
            model,
            base_url,
            render_markdown: true,
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
            "claude-sonnet-5-5".to_string(),
        ),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    text: String,
}

/// one entry in the session history list.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionMeta {
    id: u64,
    title: String,
    message_count: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SessionIndex {
    next_id: u64,
    /// most recently updated first
    sessions: Vec<SessionMeta>,
}

/// a saved chat transcript, resumable from the history list.
#[derive(Debug, Serialize, Deserialize)]
struct SavedSession {
    id: u64,
    title: String,
    messages: Vec<ChatMessage>,
}

fn session_file(id: u64) -> String {
    format!("session-{id}.json")
}

/// the first line of the session's first user message, shortened.
fn session_title(messages: &[ChatMessage]) -> String {
    let first = messages
        .iter()
        .find(|m| m.role == "You")
        .and_then(|m| m.text.lines().find(|line| !line.trim().is_empty()))
        .unwrap_or("Untitled session")
        .trim();
    if first.chars().count() > MAX_TITLE_CHARS {
        let short: String = first.chars().take(MAX_TITLE_CHARS - 1).collect();
        format!("{}\u{2026}", short.trim_end())
    } else {
        first.to_string()
    }
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
    /// mirror of `pending_patch` for collection-tree operations (create/
    /// rename/update/delete/duplicate/move) the model proposed instead of a
    /// request patch - at most one of the two is ever set for a given reply.
    pending_collection_ops: Vec<CollectionOperation>,
    /// true once the in-flight reply's first text delta has arrived via
    /// `on_http_response_chunk` - i.e. an "Assistant" message has already
    /// been pushed and is being appended to live, so `on_http_response`
    /// knows to finalize it rather than push a second one from the whole body.
    streaming: bool,
    /// ids of the collections the user has opted into sending as extra
    /// context (beyond the always-included active request/response).
    selected_collections: HashSet<usize>,
    /// names of the environments the user has opted into sending as extra
    /// context - variable *values* included, so this is opt-in per name
    /// rather than on by default.
    selected_envs: HashSet<String>,
    /// files the user attached via the "Attach Files..." button, sent as
    /// extra context on every request until removed or a new session starts.
    attached_files: Vec<PickedFile>,
    /// handle of an in-flight `pick_files` dialog, if one is open.
    pending_file_pick: Option<u32>,
    /// the saved session the current chat is written to - assigned when the
    /// first message is sent, so empty chats never show up in history.
    session_id: Option<u64>,
    sessions: SessionIndex,
    show_sessions: bool,
    /// incognito chats are never written to storage or listed in history.
    incognito: bool,
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
        if let Ok(Some(bytes)) = rustrest_plugin_api::storage_read(SESSIONS_INDEX_FILE)
            && let Ok(index) = serde_json::from_slice::<SessionIndex>(&bytes)
        {
            self.sessions = index;
        }
        self.seed_drafts_from_config();
    }

    fn save_session_index(&self) {
        if let Ok(bytes) = serde_json::to_vec(&self.sessions) {
            let _ = rustrest_plugin_api::storage_write(SESSIONS_INDEX_FILE, &bytes);
        }
    }

    /// writes the current chat to its session file and moves it to the top
    /// of history. A no-op in incognito mode and until something's been sent.
    fn save_session(&mut self) {
        if self.incognito || !self.messages.iter().any(|m| m.role == "You") {
            return;
        }
        let id = *self.session_id.get_or_insert_with(|| {
            let id = self.sessions.next_id;
            self.sessions.next_id += 1;
            id
        });
        let title = session_title(&self.messages);

        let session = SavedSession {
            id,
            title: title.clone(),
            messages: self.messages.clone(),
        };
        let Ok(bytes) = serde_json::to_vec(&session) else {
            return;
        };

        if rustrest_plugin_api::storage_write(&session_file(id), &bytes).is_err() {
            return;
        }

        self.sessions.sessions.retain(|s| s.id != id);
        self.sessions.sessions.insert(
            0,
            SessionMeta {
                id,
                title,
                message_count: self.messages.len(),
            },
        );
        let keep = self.sessions.sessions.len().min(MAX_SAVED_SESSIONS);
        for dropped in self.sessions.sessions.split_off(keep) {
            let _ = rustrest_plugin_api::storage_delete(&session_file(dropped.id));
        }
        self.save_session_index();
    }

    /// clears the chat and its context for a fresh session.
    fn reset_session(&mut self) {
        self.messages.clear();
        self.draft_input.clear();
        self.pending = None;
        self.busy = false;
        self.streaming = false;
        self.selected_collections.clear();
        self.selected_envs.clear();
        self.attached_files.clear();
        self.pending_file_pick = None;
        self.session_id = None;
        self.show_sessions = false;
    }

    fn resume_session(&mut self, id: u64) {
        let session = rustrest_plugin_api::storage_read(&session_file(id))
            .ok()
            .flatten()
            .and_then(|bytes| serde_json::from_slice::<SavedSession>(&bytes).ok());
        let Some(session) = session else {
            // the file is gone or unreadable - drop the dangling entry
            self.sessions.sessions.retain(|s| s.id != id);
            self.save_session_index();
            return;
        };

        self.reset_session();
        self.incognito = false;
        self.messages = session.messages;
        self.session_id = Some(session.id);
    }

    fn delete_session(&mut self, id: u64) {
        let _ = rustrest_plugin_api::storage_delete(&session_file(id));
        self.sessions.sessions.retain(|s| s.id != id);
        self.save_session_index();
        // the chat stays on screen, but sending again starts a new entry
        if self.session_id == Some(id) {
            self.session_id = None;
        }
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
        } else if self.show_sessions {
            self.render_sessions()
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
            }
            .styled(UiStyle::new().secure()),
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

    fn render_sessions(&self) -> UiNode {
        let header = UiNode::Row(vec![
            UiNode::Label("Chat history".to_string()).styled(UiStyle::new().bold()),
            UiNode::HorizontalSpacer,
            UiNode::Button {
                id: "close-sessions".to_string(),
                label: "Back".to_string(),
                primary: false,
            },
        ]);

        if self.sessions.sessions.is_empty() {
            return UiNode::Column(vec![
                header,
                UiNode::Muted("No saved sessions yet - chats are saved as you go.".to_string()),
            ]);
        }

        let rows = self
            .sessions
            .sessions
            .iter()
            .map(|session| {
                let is_current = self.session_id == Some(session.id);
                let plural = if session.message_count == 1 { "" } else { "s" };
                let mut details = format!("{} message{plural}", session.message_count);
                if is_current {
                    details.push_str(" \u{00b7} current");
                }

                UiNode::Row(vec![
                    UiNode::Column(vec![
                        UiNode::Label(session.title.clone()),
                        UiNode::Muted(details),
                    ]),
                    UiNode::HorizontalSpacer,
                    UiNode::Row(vec![
                        UiNode::Button {
                            id: format!("resume-session-{}", session.id),
                            label: "Resume".to_string(),
                            primary: !is_current,
                        },
                        UiNode::Button {
                            id: format!("delete-session-{}", session.id),
                            label: "Delete".to_string(),
                            primary: false,
                        }
                        .styled(UiStyle::new().variant(ButtonVariant::Danger)),
                    ])
                    .styled(UiStyle::new().text_size(12.0).spacing(4.0)),
                ])
            })
            .collect();

        UiNode::Column(vec![
            header,
            UiNode::Muted(format!(
                "Saved on this machine; the last {MAX_SAVED_SESSIONS} sessions are kept. \
                 Incognito chats are never saved."
            )),
            UiNode::Scrollable(Box::new(UiNode::Column(rows))),
        ])
    }

    /// inline checkbox list letting the user opt collections/environments
    /// into the system prompt as extra context, plus the file-attachment
    /// controls - rendered above the conversation.
    fn render_context_picker(&self, ctx: &RightPanelContext) -> UiNode {
        let mut rows = vec![UiNode::Muted("Context".to_string())];

        for col in &ctx.collections {
            rows.push(UiNode::Checkbox {
                id: format!("ctx-collection-{}", col.id),
                label: col.name.clone(),
                checked: self.selected_collections.contains(&col.id),
            });
        }

        if !ctx.environments.is_empty() {
            for env in &ctx.environments {
                rows.push(UiNode::Checkbox {
                    id: format!("ctx-env-{}", env.name),
                    label: env.name.clone(),
                    checked: self.selected_envs.contains(&env.name),
                });
            }
            rows.push(UiNode::Muted(
                "Selected environments' variable values are sent to the configured LLM \
                 provider."
                    .to_string(),
            ));
        }

        rows.push(UiNode::Button {
            id: "pick-files".to_string(),
            label: "Attach Files...".to_string(),
            primary: false,
        });
        for (idx, file) in self.attached_files.iter().enumerate() {
            rows.push(UiNode::Row(vec![
                UiNode::Muted(file.name.clone()),
                UiNode::HorizontalSpacer,
                UiNode::Button {
                    id: format!("remove-file-{idx}"),
                    label: "Remove".to_string(),
                    primary: false,
                },
            ]));
        }

        UiNode::Column(rows)
    }

    fn render_chat(&self, ctx: &RightPanelContext) -> UiNode {
        // compact toolbar buttons, so the header stays one line in a narrow panel
        let header = UiNode::Row(vec![
            UiNode::Muted(format!("Provider: {}", self.config.provider)),
            UiNode::HorizontalSpacer,
            UiNode::Row(vec![
                UiNode::Button {
                    id: "toggle-markdown".to_string(),
                    label: "Markdown".to_string(),
                    primary: self.config.render_markdown,
                },
                UiNode::Button {
                    id: "show-sessions".to_string(),
                    label: "History".to_string(),
                    primary: false,
                },
                UiNode::Button {
                    id: "toggle-incognito".to_string(),
                    label: "Incognito".to_string(),
                    primary: self.incognito,
                },
                UiNode::Button {
                    id: "new-session".to_string(),
                    label: "New Session".to_string(),
                    primary: false,
                },
                UiNode::Button {
                    id: "toggle-settings".to_string(),
                    label: "\u{2699}".to_string(), // gear icon
                    primary: false,
                },
            ])
            .styled(UiStyle::new().text_size(12.0).spacing(4.0)),
        ]);

        let mut conversation = Vec::new();

        if self.incognito {
            conversation.push(UiNode::Muted(
                "Incognito - this chat won't be saved to history.".to_string(),
            ));
        }

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
                        let body = if m.role == "Assistant" && self.config.render_markdown {
                            UiNode::Markdown(m.text.clone())
                        } else if m.role == "Error" {
                            UiNode::Label(m.text.clone())
                                .styled(UiStyle::new().tone(TextTone::Danger))
                        } else {
                            UiNode::Label(m.text.clone())
                        };
                        UiNode::Column(vec![
                            UiNode::Muted(m.role.clone()).styled(UiStyle::new().bold()),
                            body,
                        ])
                    })
                    .collect(),
            ));
        }

        if self.busy && !self.streaming {
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
            self.render_context_picker(ctx),
            UiNode::AutoScroll(Box::new(UiNode::Column(conversation))),
            input_row,
        ])
    }

    /// queues whatever the model's finished reply asks for - collection
    /// operations take precedence over a request patch, since the prompt
    /// tells it never to send both.
    fn take_actions_from_reply(&mut self, text: &str) {
        let ops = extract_collection_ops(text);
        if !ops.is_empty() {
            self.pending_collection_ops = ops;
        } else if let Some(patch) = extract_patch(text) {
            self.pending_patch = Some(patch);
        }
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

        let system_prompt = build_system_prompt(
            ctx,
            &self.selected_collections,
            &self.selected_envs,
            &self.attached_files,
        );
        let history = conversation_history(&self.messages, &instruction);
        let spec = match build_request_spec(&self.config, &system_prompt, &history) {
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
                self.streaming = false;
                self.messages.push(ChatMessage {
                    role: "You".to_string(),
                    text: instruction,
                });
                self.draft_input.clear();
                self.save_session();
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
        let mut ops = std::mem::take(&mut self.pending_collection_ops);
        match ops.len() {
            0 => {}
            // the single-op variant keeps working against older hosts
            1 => return RightPanelAction::UpdateUiAndProposeCollectionOp(tree, ops.remove(0)),
            _ => return RightPanelAction::UpdateUiAndProposeCollectionOps(tree, ops),
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
                } else if let Some(session_id) = id
                    .strip_prefix("resume-session-")
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    self.resume_session(session_id);
                } else if let Some(session_id) = id
                    .strip_prefix("delete-session-")
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    self.delete_session(session_id);
                } else if let Some(idx) = id
                    .strip_prefix("remove-file-")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    if idx < self.attached_files.len() {
                        self.attached_files.remove(idx);
                    }
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
                                render_markdown: self.config.render_markdown,
                            };
                            self.save_config();
                            self.show_settings = false;
                        }
                        "toggle-markdown" => {
                            self.config.render_markdown = !self.config.render_markdown;
                            self.save_config();
                        }
                        "send" => {
                            let instruction = self.draft_input.clone();
                            self.start_request(instruction, &ctx);
                        }
                        "new-session" => self.reset_session(),
                        "show-sessions" => self.show_sessions = true,
                        "close-sessions" => self.show_sessions = false,
                        // switching either way starts a fresh chat: a normal
                        // one is already saved, and an incognito one is discarded on purpose.
                        "toggle-incognito" => {
                            self.reset_session();
                            self.incognito = !self.incognito;
                        }
                        "pick-files" => match rustrest_plugin_api::pick_files() {
                            Ok(handle) => self.pending_file_pick = Some(handle),
                            Err(e) => self.messages.push(ChatMessage {
                                role: "Error".to_string(),
                                text: format!("couldn't open file picker: {e}"),
                            }),
                        },
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
            UiEvent::Toggled(id, checked) => {
                if let Some(col_id) = id
                    .strip_prefix("ctx-collection-")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    if checked {
                        self.selected_collections.insert(col_id);
                    } else {
                        self.selected_collections.remove(&col_id);
                    }
                } else if let Some(env_name) = id.strip_prefix("ctx-env-") {
                    if checked {
                        self.selected_envs.insert(env_name.to_string());
                    } else {
                        self.selected_envs.remove(env_name);
                    }
                } else {
                    return RightPanelAction::None;
                }
            }
        }

        RightPanelAction::UpdateUi(self.render_tree(&ctx))
    }

    fn on_http_response_chunk(&mut self, handle: u32, chunk: Vec<u8>) {
        if self.pending != Some(handle) {
            return;
        }
        let Ok(line) = String::from_utf8(chunk) else {
            return;
        };
        let Some(delta) = extract_stream_delta(&self.config.provider, &line) else {
            return;
        };
        if !self.streaming {
            self.streaming = true;
            self.messages.push(ChatMessage {
                role: "Assistant".to_string(),
                text: String::new(),
            });
        }
        if let Some(last) = self.messages.last_mut() {
            last.text.push_str(&delta);
        }
    }

    fn on_files_picked(&mut self, handle: u32, result: PickFilesResult) {
        if self.pending_file_pick != Some(handle) {
            return;
        }
        self.pending_file_pick = None;
        for skipped in result.skipped {
            self.messages.push(ChatMessage {
                role: "Error".to_string(),
                text: format!("couldn't attach {skipped}"),
            });
        }
        self.attached_files.extend(result.files);
    }

    fn on_http_response(&mut self, handle: u32, result: Result<HttpResponseData, String>) {
        if self.pending != Some(handle) {
            return;
        }
        self.pending = None;
        self.busy = false;
        let was_streaming = std::mem::take(&mut self.streaming);

        match result {
            Err(e) => self.messages.push(ChatMessage {
                role: "Error".to_string(),
                text: e,
            }),
            // the streamed reply has already been appended to live via
            // `on_http_response_chunk`, just extract any trailing patch/
            // collection-action block from the now-complete text.
            Ok(_) if was_streaming => {
                let text = self
                    .messages
                    .last()
                    .map(|m| m.text.clone())
                    .unwrap_or_default();
                // the model decides for itself, per the system prompt,
                // whether a reply warrants a patch or a collection-tree
                // operation - so every reply is checked, regardless of
                // what was asked. At most one of the two ever applies.
                self.take_actions_from_reply(&text);
            }
            // nothing streamed - either the provider ignored `stream: true`
            // and replied in one shot, or (more likely) the request failed
            // outright and the body is a plain JSON error object.
            Ok(data) => match parse_provider_reply(&self.config.provider, &data.body) {
                Ok(text) => {
                    self.take_actions_from_reply(&text);
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
        self.save_session();
    }
}

fn build_system_prompt(
    ctx: &RightPanelContext,
    selected_collections: &HashSet<usize>,
    selected_envs: &HashSet<String>,
    attached_files: &[PickedFile],
) -> String {
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

    let selected: Vec<_> = ctx
        .collections
        .iter()
        .filter(|c| selected_collections.contains(&c.id))
        .collect();
    if !selected.is_empty() {
        prompt.push_str(
            "\n\nSelected collections (reference these by id, not by guessing; anything not \
             listed here wasn't selected as context and its ids/paths must not be guessed at):",
        );

        for col in selected {
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
                    "\n    request #{} \"{}\" ({} {}) in {}",
                    req.id, req.name, req.method, req.url, location
                ));
            }
        }
    }

    let selected_env_list: Vec<_> = ctx
        .environments
        .iter()
        .filter(|e| selected_envs.contains(&e.name))
        .collect();
    if !selected_env_list.is_empty() {
        prompt.push_str("\n\nSelected environments (variable values included as-is):");

        for env in selected_env_list {
            prompt.push_str(&format!("\n- environment \"{}\"", env.name));

            for (key, value) in &env.variables {
                prompt.push_str(&format!("\n    {key} = {value}"));
            }
        }
    }

    if !attached_files.is_empty() {
        prompt.push_str("\n\nAttached files:");
        for file in attached_files {
            prompt.push_str(&format!("\n\n--- {} ---\n{}", file.name, file.text));
        }
    }

    prompt.push_str(
        "\n\nAnswer concisely; markdown formatting is fine.\n\n\
         EDITING THE ACTIVE REQUEST: if, and only if, the user's message asks you to change the \
         active request or add/replace its pre-request or post-response script, follow your \
         explanation with exactly one fenced ```request-patch code block containing ONLY the \
         fields that should change, matching this shape: {\"method\":string|null,\
         \"url\":string|null,\"headers\":[[string,string]]|null,\"body\":string|null,\
         \"pre_request_script\":string|null,\"post_response_script\":string|null}. Omit fields \
         you don't want to change (or set them to null); `headers` replaces the whole header \
         list. Scripts are JavaScript using the pm.* scripting API this app exposes \
         (pm.test(name, fn), pm.expect(...), pm.response.status/json(), \
         pm.variables.get/set(...)). Use a ```request-patch fence only for this - format any \
         other example JSON with a plain ```json fence.\n\n\
         CHANGING COLLECTIONS: if, and only if, the user's message asks you to create, rename, \
         update, delete, duplicate, or move a collection, folder, or request, follow your \
         explanation with exactly one fenced ```collection-action code block (instead of the \
         ```request-patch one - never both) containing either one JSON object or a JSON array of \
         them, applied in order. Each object is shaped as one of:\n\
         {\"op\":\"create_collection\",\"name\":string}\n\
         {\"op\":\"rename_collection\",\"collection_id\":number,\"new_name\":string}\n\
         {\"op\":\"delete_collection\",\"collection_id\":number}\n\
         {\"op\":\"create_folder\",\"collection_id\":number,\"parent_path\":[string],\"name\":string}\n\
         {\"op\":\"rename_folder\",\"collection_id\":number,\"path\":[string],\"new_name\":string}\n\
         {\"op\":\"delete_folder\",\"collection_id\":number,\"path\":[string]}\n\
         {\"op\":\"create_request\",\"collection_id\":number,\"parent_path\":[string],\"name\":string,\"method\":string,\"url\":string}\n\
         {\"op\":\"update_request\",\"collection_id\":number,\"request_id\":number,\"method\":string|null,\"url\":string|null,\"headers\":[[string,string]]|null,\"body\":string|null,\"pre_request_script\":string|null,\"post_response_script\":string|null}\n\
         {\"op\":\"rename_request\",\"collection_id\":number,\"request_id\":number,\"new_name\":string}\n\
         {\"op\":\"delete_request\",\"collection_id\":number,\"parent_path\":[string],\"request_id\":number}\n\
         {\"op\":\"duplicate_request\",\"collection_id\":number,\"parent_path\":[string],\"request_id\":number}\n\
         {\"op\":\"move_request\",\"collection_id\":number,\"from_path\":[string],\"request_id\":number,\"to_path\":[string]}\n\
         `parent_path`/`path`/`from_path`/`to_path` are folder-name arrays (e.g. [] for the \
         collection root, [\"Auth\"] for a top-level folder named Auth). Within one batch, a \
         folder created earlier can be used as a later parent_path, but a request's id is only \
         known once it exists - so update_request can only target requests listed below/above, \
         not ones created in the same batch. Use only the collection and request ids/paths \
         listed above under \"Selected collections\" - never invent one; if the collection the \
         user means isn't listed, ask them to tick it under Context. Deleting, moving, or \
         updating something will ask the user to confirm before it's applied; say so in your \
         explanation. Otherwise, don't include this block at all.",
    );

    prompt
}

/// one turn of the conversation as sent to the provider.
#[derive(Debug, Clone, PartialEq)]
struct Turn {
    /// "user" or "assistant"
    role: &'static str,
    text: String,
}

/// the chat so far
fn conversation_history(messages: &[ChatMessage], instruction: &str) -> Vec<Turn> {
    let recent: Vec<(&'static str, &str)> = messages
        .iter()
        .filter_map(|m| match m.role.as_str() {
            "You" => Some(("user", m.text.as_str())),
            "Assistant" => Some(("assistant", m.text.as_str())),
            _ => None,
        })
        .collect();
    let skip = recent.len().saturating_sub(MAX_HISTORY_MESSAGES);

    let mut turns: Vec<Turn> = Vec::new();
    for (role, text) in recent
        .into_iter()
        .skip(skip)
        .chain(std::iter::once(("user", instruction)))
    {
        if text.trim().is_empty() {
            continue;
        }
        match turns.last_mut() {
            Some(last) if last.role == role => {
                last.text.push_str("\n\n");
                last.text.push_str(text);
            }
            None if role != "user" => {}
            _ => turns.push(Turn {
                role,
                text: text.to_string(),
            }),
        }
    }
    turns
}

fn build_request_spec(
    config: &AgentConfig,
    system_prompt: &str,
    history: &[Turn],
) -> Result<HttpRequestSpec, String> {
    let history_json: Vec<serde_json::Value> = history
        .iter()
        .map(|t| serde_json::json!({"role": t.role, "content": t.text}))
        .collect();

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
            let mut messages =
                vec![serde_json::json!({"role": "system", "content": system_prompt})];
            messages.extend(history_json);
            let body = serde_json::json!({
                "model": config.model,
                "stream": true,
                "messages": messages,
            });
            (format!("{base_url}{endpoint}"), body)
        }
        _ => {
            let body = serde_json::json!({
                "model": config.model,
                "max_tokens": MAX_TOKENS,
                "stream": true,
                "system": system_prompt,
                "messages": history_json,
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

/// pulls the incremental text delta (if any) out of one line of a streamed reply.
fn extract_stream_delta(provider: &str, line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    match provider {
        "openai" => {
            let data = line.strip_prefix("data:")?.trim();
            if data == "[DONE]" {
                return None;
            }
            let value: serde_json::Value = serde_json::from_str(data).ok()?;
            value
                .pointer("/choices/0/delta/content")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        }
        "ollama" => {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            value
                .pointer("/message/content")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        }
        _ => {
            let data = line.strip_prefix("data:")?.trim();
            let value: serde_json::Value = serde_json::from_str(data).ok()?;
            if value.get("type").and_then(|t| t.as_str()) != Some("content_block_delta") {
                return None;
            }
            value
                .pointer("/delta/text")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        }
    }
}

/// contents of every fenced code block in `text` whose info string is
/// exactly `lang` (so e.g. ```json doesn't also match ```jsonc).
fn fenced_blocks<'a>(text: &'a str, lang: &str) -> Vec<&'a str> {
    let fence = format!("```{lang}");
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&fence) {
        let after = &rest[start + fence.len()..];
        let Some(line_end) = after.find('\n') else {
            break;
        };
        // the info string must end right after `lang`
        if !after[..line_end].trim().is_empty() {
            rest = after;
            continue;
        }
        let body = &after[line_end + 1..];
        let Some(end) = body.find("```") else {
            break;
        };
        blocks.push(body[..end].trim());
        rest = &body[end + 3..];
    }
    blocks
}

/// the last ```request-patch block in `text` that decodes to a patch
/// actually changing something (every field is optional, so any JSON object
/// would otherwise "decode").
fn extract_patch(text: &str) -> Option<RequestPatch> {
    fenced_blocks(text, "request-patch")
        .into_iter()
        .rev()
        .filter_map(|block| serde_json::from_str::<RequestPatch>(block).ok())
        .find(|patch| !patch.is_empty())
}

/// the operations in the last ```collection-action block in `text` that
/// decodes, either as a single operation object or an array of them.
fn extract_collection_ops(text: &str) -> Vec<CollectionOperation> {
    fenced_blocks(text, "collection-action")
        .into_iter()
        .rev()
        .find_map(|block| {
            serde_json::from_str::<Vec<CollectionOperation>>(block)
                .ok()
                .or_else(|| {
                    serde_json::from_str::<CollectionOperation>(block)
                        .ok()
                        .map(|op| vec![op])
                })
        })
        .unwrap_or_default()
}

rustrest_plugin_api::export_plugin!(AiAgentPlugin);
