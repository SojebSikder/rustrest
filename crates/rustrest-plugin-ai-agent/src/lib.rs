//! AI agent panel plugin: a chat assistant docked in Rustrest's right panel
//! (`Capability::RightPanel`), with ambient access to the active request/response tab.
//! This file holds the plugin's state and turns host events into changes to it.

#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod actions;
mod chat;
mod config;
mod host;
mod prompt;
mod provider;
mod session;
mod ui;
mod usage;

use actions::ReplyAction;
use chat::{ChatMessage, Transcript};
use config::{AgentConfig, SettingsForm};
use prompt::ContextSelection;
use rustrest_plugin_api::{
    HttpResponseData, PickFilesResult, Plugin, RightPanelAction, RightPanelContext, UiEvent,
};
use session::SessionHistory;
use ui::{ContextToggle, Field, UiAction, View};
use usage::Usage;

/// The request for the reply currently being generated.
#[derive(Debug)]
struct InFlightReply {
    /// the host's handle for the outbound HTTP request
    handle: u32,
    /// true once the first text delta has arrived
    streaming: bool,
    /// usage reported so far, attached to the reply once it completes
    usage: Usage,
}

#[derive(Default)]
struct AiAgentPlugin {
    /// set once config and history have been read from storage
    loaded: bool,
    config: AgentConfig,
    view: View,
    settings_form: SettingsForm,

    chat: Transcript,
    /// what's typed in the message box
    draft_input: String,
    /// at most one at a time - there's a single Send action
    in_flight: Option<InFlightReply>,
    /// a request patch / collection change from the last reply, handed to
    /// the host on the next `render_right_panel` - `on_http_response` has no
    /// return value the host acts on.
    pending_action: Option<ReplyAction>,

    context: ContextSelection,
    /// handle of the open `pick_files` dialog, if any
    pending_file_pick: Option<u32>,

    history: SessionHistory,
    /// the saved session this chat is written to - assigned when the first
    /// message is sent, so empty chats never show up in history.
    session_id: Option<u64>,
    /// incognito chats are never written to storage or listed in history.
    incognito: bool,
}

impl AiAgentPlugin {
    fn ensure_loaded(&mut self) {
        if std::mem::replace(&mut self.loaded, true) {
            return;
        }
        self.config = AgentConfig::load();
        self.history = SessionHistory::load();
        self.settings_form = SettingsForm::from_config(&self.config);
    }

    fn handle_action(&mut self, action: UiAction, ctx: &RightPanelContext) {
        match action {
            UiAction::Send => self.send(ctx),
            UiAction::NewSession => self.reset_chat(),
            UiAction::ShowHistory => self.view = View::History,
            UiAction::CloseHistory => self.view = View::Chat,
            // switching either way starts a fresh chat: a normal one is
            // already saved, and an incognito one is discarded on purpose.
            UiAction::ToggleIncognito => {
                self.reset_chat();
                self.incognito = !self.incognito;
            }
            UiAction::ToggleMarkdown => {
                self.config.render_markdown = !self.config.render_markdown;
                self.config.save();
            }
            UiAction::ToggleSettings => {
                self.view = if self.view == View::Settings {
                    View::Chat
                } else {
                    View::Settings
                };
                self.settings_form = SettingsForm::from_config(&self.config);
            }
            UiAction::CancelSettings => {
                self.view = View::Chat;
                self.settings_form = SettingsForm::from_config(&self.config);
            }
            UiAction::SaveSettings => {
                self.settings_form.apply_to(&mut self.config);
                self.config.save();
                self.view = View::Chat;
            }
            UiAction::SelectProvider(provider) => self.settings_form.select_provider(provider),
            UiAction::ResumeSession(id) => self.resume_session(id),
            UiAction::DeleteSession(id) => {
                self.history.delete(id);
                // the chat stays on screen, but sending again starts a new entry
                if self.session_id == Some(id) {
                    self.session_id = None;
                }
            }
            UiAction::PickFiles => match host::pick_files() {
                Ok(handle) => self.pending_file_pick = Some(handle),
                Err(e) => self
                    .chat
                    .push_error(format!("couldn't open file picker: {e}")),
            },
            UiAction::RemoveFile(index) => self.context.remove_file(index),
        }
    }

    fn set_field(&mut self, field: Field, value: String) {
        let form = &mut self.settings_form;
        match field {
            Field::ApiKey => form.api_key = value,
            Field::Model => form.model = value,
            Field::BaseUrl => form.base_url = value,
            Field::InputPrice => form.input_price = value,
            Field::OutputPrice => form.output_price = value,
            Field::ChatInput => self.draft_input = value,
        }
    }

    fn set_context(&mut self, toggle: ContextToggle, selected: bool) {
        match toggle {
            ContextToggle::Collection(id) => self.context.set_collection(id, selected),
            ContextToggle::Environment(name) => self.context.set_environment(&name, selected),
        }
    }

    /// Sends the message box's text, with the chat so far, to the model.
    fn send(&mut self, ctx: &RightPanelContext) {
        let instruction = self.draft_input.clone();
        if self.in_flight.is_some() || instruction.trim().is_empty() {
            return;
        }
        if self.config.is_missing_api_key() {
            self.chat
                .push_error("No API key configured - open Settings first.");
            return;
        }

        let system_prompt = prompt::build_system_prompt(ctx, &self.context);
        let turns = self.chat.turns_with(&instruction);
        let request =
            provider::build_request(&self.config, &system_prompt, &turns).and_then(|spec| {
                host::http_request(spec).map_err(|e| format!("request failed to start: {e}"))
            });

        match request {
            Ok(handle) => {
                self.in_flight = Some(InFlightReply {
                    handle,
                    streaming: false,
                    usage: Usage::default(),
                });
                self.chat.push(ChatMessage::user(instruction));
                self.draft_input.clear();
                self.save_session();
            }
            Err(e) => self.chat.push_error(e),
        }
    }

    /// Attaches the finished reply's usage (priced at the current model's
    /// rates) and queues whatever change it asks for.
    fn finish_reply(&mut self, usage: Usage) {
        let pricing = self.config.pricing();
        let Some(reply) = self.chat.last_reply_mut() else {
            return;
        };
        if !usage.is_empty() {
            reply.cost_usd = pricing.map(|p| p.cost(&usage));
            reply.usage = Some(usage);
        }
        // the model decides for itself, per the system prompt, whether a
        // reply warrants a change - so every reply is checked.
        self.pending_action = ReplyAction::from_reply(&reply.text);
    }

    /// Clears the chat and its context for a fresh session.
    fn reset_chat(&mut self) {
        self.chat.clear();
        self.draft_input.clear();
        self.in_flight = None;
        self.context = ContextSelection::default();
        self.pending_file_pick = None;
        self.session_id = None;
        self.view = View::Chat;
    }

    fn resume_session(&mut self, id: u64) {
        let Some(transcript) = self.history.open(id) else {
            return;
        };
        self.reset_chat();
        self.incognito = false;
        self.chat = transcript;
        self.session_id = Some(id);
    }

    /// Writes the chat to its session and moves it to the top of history.
    /// A no-op in incognito mode and until something's been sent.
    fn save_session(&mut self) {
        if self.incognito || !self.chat.has_user_message() {
            return;
        }
        let id = *self
            .session_id
            .get_or_insert_with(|| self.history.allocate_id());
        self.history.save(id, &self.chat);
    }
}

impl Plugin for AiAgentPlugin {
    fn render_right_panel(&mut self, _panel_id: &str, ctx: RightPanelContext) -> RightPanelAction {
        self.ensure_loaded();
        let tree = self.render(&ctx);
        match self.pending_action.take() {
            Some(action) => action.into_panel_action(tree),
            None => RightPanelAction::UpdateUi(tree),
        }
    }

    fn on_right_panel_event(
        &mut self,
        _panel_id: &str,
        ctx: RightPanelContext,
        event: UiEvent,
    ) -> RightPanelAction {
        self.ensure_loaded();

        let handled = match event {
            UiEvent::Clicked(id) => UiAction::parse(&id)
                .map(|action| self.handle_action(action, &ctx))
                .is_some(),
            UiEvent::Changed(id, value) => Field::parse(&id)
                .map(|field| self.set_field(field, value))
                .is_some(),
            UiEvent::Toggled(id, checked) => ContextToggle::parse(&id)
                .map(|toggle| self.set_context(toggle, checked))
                .is_some(),
        };
        if !handled {
            return RightPanelAction::None;
        }
        RightPanelAction::UpdateUi(self.render(&ctx))
    }

    fn on_http_response_chunk(&mut self, handle: u32, chunk: Vec<u8>) {
        let provider = self.config.provider;
        let Some(reply) = self.in_flight.as_mut().filter(|r| r.handle == handle) else {
            return;
        };
        let Some(event) = String::from_utf8(chunk)
            .ok()
            .and_then(|line| provider.stream_event(&line))
        else {
            return;
        };

        // usage arrives in its own events (Anthropic's message_start/message_delta,
        // OpenAI's final chunk, Ollama's done line), so it's collected independently of any text.
        provider.usage(&event).apply(&mut reply.usage);
        let Some(delta) = provider.text_delta(&event) else {
            return;
        };
        if !reply.streaming {
            reply.streaming = true;
            self.chat.push(ChatMessage::assistant(String::new()));
        }
        if let Some(message) = self.chat.last_reply_mut() {
            message.text.push_str(&delta);
        }
    }

    fn on_http_response(&mut self, handle: u32, result: Result<HttpResponseData, String>) {
        let Some(mut reply) = self.in_flight.take_if(|r| r.handle == handle) else {
            return;
        };

        let completed = match result {
            Err(e) => {
                self.chat.push_error(e);
                false
            }
            // the text was already appended live, chunk by chunk
            Ok(_) if reply.streaming => true,
            // nothing streamed - either the provider ignored `stream: true`
            // and replied in one shot, or (more likely) the request failed
            // and the body is a JSON error object.
            Ok(data) => match self.config.provider.parse_reply(&data.body) {
                Ok(whole) => {
                    whole.usage.apply(&mut reply.usage);
                    self.chat.push(ChatMessage::assistant(whole.text));
                    true
                }
                Err(e) => {
                    self.chat.push_error(format!(
                        "couldn't parse provider response (HTTP {}): {e}",
                        data.status
                    ));
                    false
                }
            },
        };
        if completed {
            self.finish_reply(reply.usage);
        }
        self.save_session();
    }

    fn on_files_picked(&mut self, handle: u32, result: PickFilesResult) {
        if self.pending_file_pick.take_if(|h| *h == handle).is_none() {
            return;
        }
        for skipped in result.skipped {
            self.chat.push_error(format!("couldn't attach {skipped}"));
        }
        self.context.files.extend(result.files);
    }
}

#[cfg(target_arch = "wasm32")]
rustrest_plugin_api::export_plugin!(AiAgentPlugin);
