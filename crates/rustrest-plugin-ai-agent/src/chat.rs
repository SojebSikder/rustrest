//! The conversation shown in the panel, and what gets sent back to the model.

use crate::usage::Usage;
use serde::{Deserialize, Serialize};

/// Most recent chat messages (user + assistant) sent back to the model as
/// conversation history. Bounds the prompt size on long sessions.
const MAX_HISTORY_MESSAGES: usize = 20;

/// Longest session title, in chars.
const MAX_TITLE_CHARS: usize = 60;

/// Who a message is from. Serialized under the label shown in the panel, which
/// is also what older saved sessions used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Role {
    #[serde(rename = "You")]
    User,
    Assistant,
    /// a failure shown in the chat - never sent to the model
    Error,
}

impl Role {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Role::User => "You",
            Role::Assistant => "Assistant",
            Role::Error => "Error",
        }
    }

    /// The role name LLM APIs use, or `None` for messages the model never sees.
    fn api_role(self) -> Option<&'static str> {
        match self {
            Role::User => Some("user"),
            Role::Assistant => Some("assistant"),
            Role::Error => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatMessage {
    pub(crate) role: Role,
    pub(crate) text: String,
    /// tokens the provider reported for this reply (assistant messages only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) usage: Option<Usage>,
    /// estimated cost of this reply in USD, priced when it arrived - `None`
    /// when the model's pricing is unknown (e.g. a local Ollama model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cost_usd: Option<f64>,
}

impl ChatMessage {
    fn new(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
            usage: None,
            cost_usd: None,
        }
    }

    pub(crate) fn user(text: impl Into<String>) -> Self {
        Self::new(Role::User, text)
    }

    pub(crate) fn assistant(text: impl Into<String>) -> Self {
        Self::new(Role::Assistant, text)
    }

    pub(crate) fn error(text: impl Into<String>) -> Self {
        Self::new(Role::Error, text)
    }
}

/// One turn of the conversation as sent to the provider.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Turn {
    /// "user" or "assistant"
    pub(crate) role: &'static str,
    pub(crate) text: String,
}

/// Every message in the current chat, oldest first.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Transcript {
    messages: Vec<ChatMessage>,
}

impl Transcript {
    pub(crate) fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub(crate) fn len(&self) -> usize {
        self.messages.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub(crate) fn push(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    pub(crate) fn push_error(&mut self, text: impl Into<String>) {
        self.push(ChatMessage::error(text));
    }

    pub(crate) fn clear(&mut self) {
        self.messages.clear();
    }

    /// The last message, if it's an assistant reply.
    pub(crate) fn last_reply_mut(&mut self) -> Option<&mut ChatMessage> {
        self.messages
            .last_mut()
            .filter(|m| m.role == Role::Assistant)
    }

    /// True once the user has sent something - empty chats aren't saved.
    pub(crate) fn has_user_message(&self) -> bool {
        self.messages.iter().any(|m| m.role == Role::User)
    }

    /// The first line of the first user message, shortened - used as the
    /// chat's title in history.
    pub(crate) fn title(&self) -> String {
        let first = self
            .messages
            .iter()
            .find(|m| m.role == Role::User)
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

    /// Summed usage and cost over the chat. Cost is `None` if no reply was priced.
    pub(crate) fn totals(&self) -> (Usage, Option<f64>) {
        let mut usage = Usage::default();
        let mut cost = None;
        for m in &self.messages {
            if let Some(u) = &m.usage {
                usage.add(u);
            }
            if let Some(c) = m.cost_usd {
                *cost.get_or_insert(0.0) += c;
            }
        }
        (usage, cost)
    }

    /// The conversation to send for a new `instruction`: the most recent
    /// messages (errors dropped) plus the instruction, merged so roles
    /// alternate and the first turn is the user's - as the APIs require.
    pub(crate) fn turns_with(&self, instruction: &str) -> Vec<Turn> {
        let recent: Vec<(&'static str, &str)> = self
            .messages
            .iter()
            .filter_map(|m| Some((m.role.api_role()?, m.text.as_str())))
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
}
