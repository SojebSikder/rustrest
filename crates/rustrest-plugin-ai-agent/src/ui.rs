//! The panel's screens (chat, settings, history) and the typed ids of every
//! control on them, so event handling matches on enums instead of strings.

use crate::chat::Role;
use crate::provider::Provider;
use crate::session::MAX_SAVED_SESSIONS;
use crate::usage::{format_cost, format_count, format_totals, format_usage};
use crate::AiAgentPlugin;
use rustrest_plugin_api::{
    ButtonVariant, DropdownItem, ProtocolRequestContext, RightPanelContext, TextTone, UiLength,
    UiNode, UiStyle,
};

const DOT: &str = " \u{00b7} ";

/// Which screen the panel shows.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    #[default]
    Chat,
    Settings,
    History,
}

/// Everything a button or menu item can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiAction {
    Send,
    NewSession,
    ShowHistory,
    CloseHistory,
    ToggleIncognito,
    ToggleMarkdown,
    ToggleSettings,
    SaveSettings,
    CancelSettings,
    SelectProvider(Provider),
    ResumeSession(u64),
    DeleteSession(u64),
    PickFiles,
    RemoveFile(usize),
}

impl UiAction {
    const SIMPLE: [(UiAction, &'static str); 10] = [
        (UiAction::Send, "send"),
        (UiAction::NewSession, "new-session"),
        (UiAction::ShowHistory, "show-sessions"),
        (UiAction::CloseHistory, "close-sessions"),
        (UiAction::ToggleIncognito, "toggle-incognito"),
        (UiAction::ToggleMarkdown, "toggle-markdown"),
        (UiAction::ToggleSettings, "toggle-settings"),
        (UiAction::SaveSettings, "save-settings"),
        (UiAction::CancelSettings, "cancel-settings"),
        (UiAction::PickFiles, "pick-files"),
    ];

    pub(crate) fn id(self) -> String {
        match self {
            UiAction::SelectProvider(p) => format!("provider-{}", p.id()),
            UiAction::ResumeSession(id) => format!("resume-session-{id}"),
            UiAction::DeleteSession(id) => format!("delete-session-{id}"),
            UiAction::RemoveFile(index) => format!("remove-file-{index}"),
            simple => Self::SIMPLE
                .iter()
                .find(|(action, _)| *action == simple)
                .map(|(_, id)| id.to_string())
                .expect("every argument-less action is listed in SIMPLE"),
        }
    }

    pub(crate) fn parse(id: &str) -> Option<Self> {
        if let Some((action, _)) = Self::SIMPLE.iter().find(|(_, s)| *s == id) {
            return Some(*action);
        }
        if let Some(provider) = id.strip_prefix("provider-") {
            return Provider::from_id(provider).map(UiAction::SelectProvider);
        }
        if let Some(n) = id.strip_prefix("resume-session-") {
            return n.parse().ok().map(UiAction::ResumeSession);
        }
        if let Some(n) = id.strip_prefix("delete-session-") {
            return n.parse().ok().map(UiAction::DeleteSession);
        }
        if let Some(n) = id.strip_prefix("remove-file-") {
            return n.parse().ok().map(UiAction::RemoveFile);
        }
        None
    }
}

/// Every text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    ApiKey,
    Model,
    BaseUrl,
    InputPrice,
    OutputPrice,
    ChatInput,
}

impl Field {
    const ALL: [(Field, &'static str); 6] = [
        (Field::ApiKey, "api-key"),
        (Field::Model, "model"),
        (Field::BaseUrl, "base-url"),
        (Field::InputPrice, "input-price"),
        (Field::OutputPrice, "output-price"),
        (Field::ChatInput, "chat-input"),
    ];

    pub(crate) fn id(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(field, _)| *field == self)
            .map(|(_, id)| *id)
            .expect("every field is listed in ALL")
    }

    pub(crate) fn parse(id: &str) -> Option<Self> {
        Self::ALL.iter().find(|(_, s)| *s == id).map(|(f, _)| *f)
    }
}

/// Every checkbox: the context picker's collections and environments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContextToggle {
    Collection(usize),
    Environment(String),
}

impl ContextToggle {
    pub(crate) fn id(&self) -> String {
        match self {
            ContextToggle::Collection(id) => format!("ctx-collection-{id}"),
            ContextToggle::Environment(name) => format!("ctx-env-{name}"),
        }
    }

    pub(crate) fn parse(id: &str) -> Option<Self> {
        if let Some(n) = id.strip_prefix("ctx-collection-") {
            return n.parse().ok().map(ContextToggle::Collection);
        }
        id.strip_prefix("ctx-env-")
            .map(|name| ContextToggle::Environment(name.to_string()))
    }
}

fn button(action: UiAction, label: &str, primary: bool) -> UiNode {
    UiNode::Button {
        id: action.id(),
        label: label.to_string(),
        primary,
    }
}

fn text_input(field: Field, value: &str, placeholder: &str, submit: UiAction) -> UiNode {
    UiNode::TextInput {
        id: field.id().to_string(),
        value: value.to_string(),
        placeholder: placeholder.to_string(),
        on_submit: Some(submit.id()),
    }
}

fn muted(text: impl Into<String>) -> UiNode {
    UiNode::Muted(text.into())
}

/// A ✓ prefix for menu items that toggle something on.
fn check(on: bool) -> &'static str {
    if on {
        "\u{2713} "
    } else {
        ""
    }
}

impl AiAgentPlugin {
    pub(crate) fn render(&self, ctx: &RightPanelContext) -> UiNode {
        // nothing works without credentials, so ask for them first
        if self.config.is_missing_api_key() {
            return self.render_settings();
        }
        match self.view {
            View::Chat => self.render_chat(ctx),
            View::Settings => self.render_settings(),
            View::History => self.render_history(),
        }
    }

    fn render_settings(&self) -> UiNode {
        let form = &self.settings_form;
        let save = UiAction::SaveSettings;
        let provider_buttons = Provider::ALL
            .into_iter()
            .map(|p| button(UiAction::SelectProvider(p), p.label(), form.provider == p))
            .collect();

        UiNode::Column(vec![
            UiNode::Label("AI Agent settings".to_string()),
            UiNode::Row(provider_buttons),
            text_input(Field::ApiKey, &form.api_key, "API key", save)
                .styled(UiStyle::new().secure()),
            text_input(Field::Model, &form.model, "model name", save),
            text_input(Field::BaseUrl, &form.base_url, "https:// base URL", save),
            muted(form.pricing_summary()),
            UiNode::Row(vec![
                text_input(
                    Field::InputPrice,
                    &form.input_price,
                    "input $ / 1M tokens",
                    save,
                ),
                text_input(
                    Field::OutputPrice,
                    &form.output_price,
                    "output $ / 1M tokens",
                    save,
                ),
            ]),
            muted(
                "Requests must go to an https:// endpoint (a local Ollama server \
                 needs an https-terminating proxy/tunnel in front of it).",
            ),
            UiNode::Row(vec![
                button(UiAction::SaveSettings, "Save", true),
                button(UiAction::CancelSettings, "Cancel", false),
            ]),
        ])
    }

    fn render_history(&self) -> UiNode {
        let header = UiNode::Row(vec![
            UiNode::Label("Chat history".to_string()).styled(UiStyle::new().bold()),
            UiNode::HorizontalSpacer,
            button(UiAction::CloseHistory, "Back", false),
        ]);

        let entries = self.history.entries();
        if entries.is_empty() {
            return UiNode::Column(vec![
                header,
                muted("No saved sessions yet - chats are saved as you go."),
            ]);
        }

        let rows = entries
            .iter()
            .map(|session| {
                let is_current = self.session_id == Some(session.id);
                let plural = if session.message_count == 1 { "" } else { "s" };
                let mut details = vec![format!("{} message{plural}", session.message_count)];
                if session.total_tokens > 0 {
                    details.push(format!("{} tokens", format_count(session.total_tokens)));
                }
                if let Some(cost) = session.cost_usd {
                    details.push(format_cost(cost));
                }
                if is_current {
                    details.push("current".to_string());
                }

                UiNode::Row(vec![
                    UiNode::Column(vec![
                        UiNode::Label(session.title.clone()),
                        muted(details.join(DOT)),
                    ])
                    .styled(UiStyle::new().width(UiLength::Fill).spacing(2.0)),
                    UiNode::Row(vec![
                        button(UiAction::ResumeSession(session.id), "Resume", !is_current),
                        button(UiAction::DeleteSession(session.id), "Delete", false)
                            .styled(UiStyle::new().variant(ButtonVariant::Danger)),
                    ])
                    .styled(
                        UiStyle::new()
                            .text_size(12.0)
                            .spacing(4.0)
                            .width(UiLength::Shrink),
                    ),
                ])
            })
            .collect();

        UiNode::Column(vec![
            header,
            muted(format!(
                "Saved on this machine; the last {MAX_SAVED_SESSIONS} sessions are kept. \
                 Incognito chats are never saved."
            )),
            UiNode::Scrollable(Box::new(UiNode::Column(rows))),
        ])
    }

    fn render_chat(&self, ctx: &RightPanelContext) -> UiNode {
        let mut conversation = Vec::new();
        if self.incognito {
            conversation.push(muted("Incognito - this chat won't be saved to history."));
        }
        conversation.push(muted(active_tab_summary(ctx)));

        if self.chat.is_empty() {
            conversation.push(muted(
                "Ask a question, or tell me what to change - e.g. \"explain this response\" \
                 or \"add an Authorization header\".",
            ));
        } else {
            conversation.push(UiNode::Column(
                self.chat
                    .messages()
                    .iter()
                    .map(|m| self.render_message(m))
                    .collect(),
            ));
        }

        if self
            .in_flight
            .as_ref()
            .is_some_and(|reply| !reply.streaming)
        {
            conversation.push(muted("Thinking..."));
        }

        let input_row = UiNode::Row(vec![
            text_input(
                Field::ChatInput,
                &self.draft_input,
                "Message Rustrest Agent...",
                UiAction::Send,
            ),
            button(UiAction::Send, "Send", true),
        ]);

        UiNode::Column(vec![
            self.render_chat_header(),
            self.render_context_picker(ctx),
            UiNode::AutoScroll(Box::new(UiNode::Column(conversation))),
            input_row,
        ])
    }

    /// One line: provider, running usage/cost, and a menu holding every
    /// action, so it fits a narrow panel.
    fn render_chat_header(&self) -> UiNode {
        let mut status = vec![format!("Provider: {}", self.config.provider.id())];
        let (usage, cost) = self.chat.totals();
        status.extend(format_totals(&usage, cost));
        if self.incognito {
            status.push("Incognito".to_string());
        }

        let item = |action: UiAction, label: String| DropdownItem::new(action.id(), label);
        let menu = UiNode::Dropdown {
            label: "Menu".to_string(),
            items: vec![
                item(UiAction::NewSession, "New Session".to_string()),
                item(UiAction::ShowHistory, "History".to_string()),
                item(
                    UiAction::ToggleIncognito,
                    format!("{}Incognito", check(self.incognito)),
                ),
                item(
                    UiAction::ToggleMarkdown,
                    format!("{}Render Markdown", check(self.config.render_markdown)),
                ),
                item(UiAction::ToggleSettings, "Settings".to_string()),
            ],
        }
        .styled(UiStyle::new().text_size(12.0));

        UiNode::Row(vec![
            muted(status.join(DOT)),
            UiNode::HorizontalSpacer,
            menu,
        ])
    }

    fn render_message(&self, message: &crate::chat::ChatMessage) -> UiNode {
        let body = match message.role {
            Role::Assistant if self.config.render_markdown => {
                UiNode::Markdown(message.text.clone())
            }
            Role::Error => {
                UiNode::Label(message.text.clone()).styled(UiStyle::new().tone(TextTone::Danger))
            }
            _ => UiNode::Label(message.text.clone()),
        };
        let mut parts = vec![
            muted(message.role.label()).styled(UiStyle::new().bold()),
            body,
        ];
        if let Some(usage) = &message.usage {
            parts.push(
                muted(format_usage(usage, message.cost_usd)).styled(UiStyle::new().text_size(11.0)),
            );
        }
        UiNode::Column(parts)
    }

    /// Checkboxes opting collections/environments into the prompt, plus the
    /// file-attachment controls - shown above the conversation.
    fn render_context_picker(&self, ctx: &RightPanelContext) -> UiNode {
        let selection = &self.context;
        let mut rows = vec![muted(if ctx.collections.is_empty() {
            "Context"
        } else {
            "Context - the agent sees every collection; tick one to focus on it"
        })];

        let checkbox = |toggle: ContextToggle, label: &str, checked: bool| UiNode::Checkbox {
            id: toggle.id(),
            label: label.to_string(),
            checked,
        };
        for col in &ctx.collections {
            rows.push(checkbox(
                ContextToggle::Collection(col.id),
                &col.name,
                selection.collections.contains(&col.id),
            ));
        }
        for env in &ctx.environments {
            rows.push(checkbox(
                ContextToggle::Environment(env.name.clone()),
                &env.name,
                selection.environments.contains(&env.name),
            ));
        }
        if !ctx.environments.is_empty() {
            rows.push(muted(
                "Selected environments' variable values are sent to the configured LLM provider.",
            ));
        }

        rows.push(
            button(UiAction::PickFiles, "+ Attach Files", false)
                .styled(UiStyle::new().text_size(11.0).padding(2.0, 8.0)),
        );
        for (index, file) in selection.files.iter().enumerate() {
            rows.push(UiNode::Row(vec![
                muted(file.name.clone()).styled(UiStyle::new().width(UiLength::Fill)),
                button(UiAction::RemoveFile(index), "Remove", false),
            ]));
        }

        UiNode::Column(rows)
    }
}

/// What the active tab holds, shown above the chat.
fn active_tab_summary(ctx: &RightPanelContext) -> String {
    match (&ctx.active_request, &ctx.active_response) {
        (Some(req), Some(resp)) => format!("{} {} \u{2192} {}", req.method, req.url, resp.status),
        (Some(req), None) => format!("{} {}", req.method, req.url),
        (None, _) => ctx
            .active_protocol_request
            .as_ref()
            .map(protocol_summary)
            .unwrap_or_else(|| "No active request - open one for context.".to_string()),
    }
}

/// one-line description of a WebSocket/GraphQL/gRPC tab shown above the chat.
fn protocol_summary(protocol: &ProtocolRequestContext) -> String {
    match protocol {
        ProtocolRequestContext::WebSocket { url, connected, .. } => format!(
            "WebSocket {url} \u{00b7} {}",
            if *connected {
                "connected"
            } else {
                "disconnected"
            }
        ),
        ProtocolRequestContext::GraphQl { url, .. } => format!("GraphQL {url}"),
        ProtocolRequestContext::Grpc {
            endpoint,
            service,
            method,
            ..
        } => match (service, method) {
            (Some(service), Some(method)) => format!("gRPC {endpoint} \u{2192} {service}/{method}"),
            _ => format!("gRPC {endpoint}"),
        },
    }
}
