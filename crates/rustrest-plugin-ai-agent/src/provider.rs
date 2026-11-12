//! The LLM providers the agent can talk to, and each one's wire format:
//! how to build a chat request, and how to read text and token usage back out
//! of a reply (whole or streamed).

use crate::chat::Turn;
use crate::config::AgentConfig;
use crate::usage::UsageUpdate;
use rustrest_plugin_api::HttpRequestSpec;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Reply token budget. Large enough for a full test script plus its
/// explanation, so the trailing action block isn't cut off mid-JSON.
const MAX_TOKENS: u32 = 8192;

/// Anthropic Messages API version sent with every request.
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", from = "String")]
pub(crate) enum Provider {
    #[default]
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    Ollama,
}

/// Unknown ids (e.g. from a hand-edited config) fall back to Anthropic, so a
/// bad value never makes the whole config unreadable.
impl From<String> for Provider {
    fn from(id: String) -> Self {
        Provider::from_id(&id).unwrap_or_default()
    }
}

/// A complete (non-streamed) reply.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Reply {
    pub(crate) text: String,
    pub(crate) usage: UsageUpdate,
}

impl Provider {
    pub(crate) const ALL: [Provider; 3] = [Provider::Anthropic, Provider::OpenAi, Provider::Ollama];

    /// Stable id, as stored in the config file.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
            Provider::Ollama => "ollama",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::OpenAi => "OpenAI",
            Provider::Ollama => "Ollama",
        }
    }

    pub(crate) fn default_base_url(self) -> &'static str {
        match self {
            Provider::Anthropic => "https://api.anthropic.com",
            Provider::OpenAi => "https://api.openai.com",
            // a local server - there's no one URL that fits everyone
            Provider::Ollama => "",
        }
    }

    pub(crate) fn default_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-sonnet-5-5",
            Provider::OpenAi => "gpt-4o-mini",
            Provider::Ollama => "llama3",
        }
    }

    /// Ollama servers usually run without auth; the hosted APIs never do.
    pub(crate) fn requires_api_key(self) -> bool {
        self != Provider::Ollama
    }

    fn endpoint_path(self) -> &'static str {
        match self {
            Provider::Anthropic => "/v1/messages",
            Provider::OpenAi => "/v1/chat/completions",
            Provider::Ollama => "/api/chat",
        }
    }

    /// The JSON body of a streamed chat request.
    fn request_body(self, model: &str, system_prompt: &str, history: &[Turn]) -> Value {
        let turns = history
            .iter()
            .map(|t| json!({"role": t.role, "content": t.text}));

        match self {
            // Anthropic takes the system prompt as its own field
            Provider::Anthropic => json!({
                "model": model,
                "max_tokens": MAX_TOKENS,
                "stream": true,
                "system": system_prompt,
                "messages": turns.collect::<Vec<_>>(),
            }),
            // the others take it as the first message
            Provider::OpenAi | Provider::Ollama => {
                let messages: Vec<Value> =
                    std::iter::once(json!({"role": "system", "content": system_prompt}))
                        .chain(turns)
                        .collect();
                let mut body = json!({
                    "model": model,
                    "stream": true,
                    "messages": messages,
                });
                // OpenAI only reports token usage on a stream when asked to (in
                // a final chunk with empty `choices`); Ollama always does.
                if self == Provider::OpenAi {
                    body["stream_options"] = json!({"include_usage": true});
                }
                body
            }
        }
    }

    fn headers(self, api_key: &str) -> Vec<(String, String)> {
        let header = |name: &str, value: String| (name.to_string(), value);
        let mut headers = vec![header("content-type", "application/json".to_string())];
        match self {
            Provider::Anthropic => {
                headers.push(header("x-api-key", api_key.to_string()));
                headers.push(header("anthropic-version", ANTHROPIC_VERSION.to_string()));
            }
            Provider::OpenAi => headers.push(header("Authorization", format!("Bearer {api_key}"))),
            Provider::Ollama if !api_key.is_empty() => {
                headers.push(header("Authorization", format!("Bearer {api_key}")));
            }
            Provider::Ollama => {}
        }
        headers
    }

    /// Decodes a whole (non-streamed) reply body - also how a failed request's
    /// JSON error object surfaces, since nothing streams in that case.
    pub(crate) fn parse_reply(self, body: &[u8]) -> Result<Reply, String> {
        let value: Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        if let Some(err) = value.get("error") {
            return Err(err.to_string());
        }

        let pointer = match self {
            Provider::Anthropic => "/content/0/text",
            Provider::OpenAi => "/choices/0/message/content",
            Provider::Ollama => "/message/content",
        };
        let text = value
            .pointer(pointer)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("unrecognized response shape: {value}"))?;

        Ok(Reply {
            text: text.to_string(),
            usage: self.usage(&value),
        })
    }

    /// Decodes one line of a streamed reply - SSE `data:` lines for
    /// Anthropic/OpenAI, bare NDJSON lines for Ollama. `None` for blank
    /// lines, SSE `event:` lines, the `[DONE]` marker and anything unparsable.
    pub(crate) fn stream_event(self, line: &str) -> Option<Value> {
        let line = line.trim();
        let data = match self {
            Provider::Ollama => line,
            Provider::Anthropic | Provider::OpenAi => line.strip_prefix("data:")?.trim(),
        };
        if data.is_empty() || data == "[DONE]" {
            return None;
        }
        serde_json::from_str(data).ok()
    }

    /// The incremental text in a streamed event, if it carries any.
    pub(crate) fn text_delta(self, event: &Value) -> Option<String> {
        let text = match self {
            Provider::Anthropic => {
                if event.get("type").and_then(Value::as_str) != Some("content_block_delta") {
                    return None;
                }
                event.pointer("/delta/text")
            }
            Provider::OpenAi => event.pointer("/choices/0/delta/content"),
            Provider::Ollama => event.pointer("/message/content"),
        };
        text.and_then(Value::as_str).map(str::to_string)
    }

    /// Token usage in a provider payload - a whole reply or one streamed
    /// event. Prompt tokens are split into uncached input and cache
    /// reads/writes, so each can be priced at its own rate.
    pub(crate) fn usage(self, payload: &Value) -> UsageUpdate {
        let count = |v: &Value, ptr: &str| v.pointer(ptr).and_then(Value::as_u64);
        match self {
            Provider::Anthropic => {
                // a whole reply / message_delta has a top-level `usage`;
                // message_start nests it under `message`
                let Some(u) = payload
                    .get("usage")
                    .or_else(|| payload.pointer("/message/usage"))
                    .filter(|u| u.is_object())
                else {
                    return UsageUpdate::default();
                };
                UsageUpdate {
                    input: count(u, "/input_tokens"),
                    output: count(u, "/output_tokens"),
                    cache_read: count(u, "/cache_read_input_tokens"),
                    cache_write: count(u, "/cache_creation_input_tokens"),
                }
            }
            Provider::OpenAi => {
                let Some(u) = payload.get("usage").filter(|u| u.is_object()) else {
                    return UsageUpdate::default();
                };
                // OpenAI's prompt_tokens includes the cached ones
                let prompt = count(u, "/prompt_tokens");
                let cached = count(u, "/prompt_tokens_details/cached_tokens");
                UsageUpdate {
                    input: prompt.map(|p| p.saturating_sub(cached.unwrap_or(0))),
                    output: count(u, "/completion_tokens"),
                    cache_read: cached,
                    cache_write: None,
                }
            }
            Provider::Ollama => UsageUpdate {
                input: count(payload, "/prompt_eval_count"),
                output: count(payload, "/eval_count"),
                ..UsageUpdate::default()
            },
        }
    }
}

/// The HTTP request for the next reply in a conversation.
pub(crate) fn build_request(
    config: &AgentConfig,
    system_prompt: &str,
    history: &[Turn],
) -> Result<HttpRequestSpec, String> {
    let base_url = config.base_url.trim_end_matches('/');
    if base_url.is_empty() {
        return Err("no base URL configured - open Settings first.".to_string());
    }
    if !base_url.starts_with("https://") {
        return Err("base URL must start with https://".to_string());
    }

    let provider = config.provider;
    let body = provider.request_body(&config.model, system_prompt, history);
    Ok(HttpRequestSpec {
        method: "POST".to_string(),
        url: format!("{base_url}{}", provider.endpoint_path()),
        headers: provider.headers(&config.api_key),
        body: Some(
            serde_json::to_vec(&body).map_err(|e| format!("failed to encode request: {e}"))?,
        ),
    })
}
