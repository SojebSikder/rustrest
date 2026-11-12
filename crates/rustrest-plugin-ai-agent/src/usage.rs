//! Token usage, the prices used to turn it into a cost estimate, and how both
//! are shown to the user. Reading usage out of provider payloads lives with
//! each provider's wire format, in [`crate::provider`].

use crate::provider::Provider;
use serde::{Deserialize, Serialize};

/// Token counts for one reply, or summed over a chat.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Usage {
    /// prompt tokens not served from the provider's prompt cache
    #[serde(default)]
    pub(crate) input_tokens: u64,
    #[serde(default)]
    pub(crate) output_tokens: u64,
    #[serde(default)]
    pub(crate) cache_read_tokens: u64,
    #[serde(default)]
    pub(crate) cache_write_tokens: u64,
}

impl Usage {
    pub(crate) fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_write_tokens
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.total() == 0
    }

    pub(crate) fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
    }
}

/// The usage fields found in one provider payload (a streamed event or a
/// whole reply). Providers report running totals, so applying an update keeps
/// the larger value per field - a later event never lowers a count.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct UsageUpdate {
    pub(crate) input: Option<u64>,
    pub(crate) output: Option<u64>,
    pub(crate) cache_read: Option<u64>,
    pub(crate) cache_write: Option<u64>,
}

impl UsageUpdate {
    pub(crate) fn apply(&self, usage: &mut Usage) {
        let merge = |field: &mut u64, value: Option<u64>| {
            if let Some(v) = value {
                *field = (*field).max(v);
            }
        };
        merge(&mut usage.input_tokens, self.input);
        merge(&mut usage.output_tokens, self.output);
        merge(&mut usage.cache_read_tokens, self.cache_read);
        merge(&mut usage.cache_write_tokens, self.cache_write);
    }
}

/// Rates in USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pricing {
    pub(crate) input: f64,
    pub(crate) output: f64,
    pub(crate) cache_read: f64,
    pub(crate) cache_write: f64,
}

impl Pricing {
    /// Cache writes are billed at 1.25x the input rate (5-minute TTL).
    pub(crate) const fn new(input: f64, output: f64, cache_read: f64) -> Self {
        Self {
            input,
            output,
            cache_read,
            cache_write: input * 1.25,
        }
    }

    /// User-supplied input/output rates. Cache reads are assumed to cost a
    /// tenth of the input rate, the common case across providers.
    pub(crate) const fn custom(input: f64, output: f64) -> Self {
        Self::new(input, output, input * 0.1)
    }

    pub(crate) fn cost(&self, usage: &Usage) -> f64 {
        (usage.input_tokens as f64 * self.input
            + usage.output_tokens as f64 * self.output
            + usage.cache_read_tokens as f64 * self.cache_read
            + usage.cache_write_tokens as f64 * self.cache_write)
            / 1_000_000.0
    }

    /// The built-in list price for a model, matched by model-id prefix.
    /// `None` for local (Ollama) models and models the table doesn't know.
    pub(crate) fn list_price(provider: Provider, model: &str) -> Option<Self> {
        let table = match provider {
            Provider::Anthropic => ANTHROPIC_PRICES,
            Provider::OpenAi => OPENAI_PRICES,
            Provider::Ollama => return None,
        };
        // tolerate gateway-style ids such as "anthropic/claude-..." or "openai/gpt-4o"
        let model = model.trim().to_ascii_lowercase();
        let model = model.rsplit('/').next().unwrap_or(&model);
        table
            .iter()
            .find(|(prefix, _)| model.starts_with(prefix))
            .map(|(_, pricing)| *pricing)
    }
}

/// Built-in list prices, matched by model-id prefix - so a more specific
/// prefix must come before any shorter one that would shadow it (e.g.
/// `claude-opus-5-5` before `claude-opus-5`, `gpt-4o-mini` before `gpt-4o`).
/// These are estimates; Settings can override them.
const ANTHROPIC_PRICES: &[(&str, Pricing)] = &[
    ("claude-fable-5-2", Pricing::new(8.0, 40.0, 0.20)),
    ("claude-mythos-5-2", Pricing::new(8.0, 40.0, 0.20)),
    ("claude-fable-5", Pricing::new(10.0, 50.0, 1.0)),
    ("claude-mythos-5", Pricing::new(10.0, 50.0, 1.0)),
    ("claude-opus-5-5", Pricing::new(4.0, 20.0, 0.20)),
    ("claude-opus-5", Pricing::new(5.0, 25.0, 0.50)),
    ("claude-opus-4-8", Pricing::new(5.0, 25.0, 0.50)),
    ("claude-opus-4-7", Pricing::new(5.0, 25.0, 0.50)),
    ("claude-opus-4-6", Pricing::new(5.0, 25.0, 0.50)),
    ("claude-opus-4-5", Pricing::new(5.0, 25.0, 0.50)),
    ("claude-opus-4", Pricing::new(15.0, 75.0, 1.50)),
    ("claude-sonnet-5-5", Pricing::new(2.0, 10.0, 0.10)),
    ("claude-sonnet-5", Pricing::new(2.0, 10.0, 0.20)),
    ("claude-sonnet-4", Pricing::new(3.0, 15.0, 0.30)),
    ("claude-3-7-sonnet", Pricing::new(3.0, 15.0, 0.30)),
    ("claude-3-5-sonnet", Pricing::new(3.0, 15.0, 0.30)),
    ("claude-haiku-5-5", Pricing::new(0.10, 0.50, 0.01)),
    ("claude-haiku-4-5", Pricing::new(1.0, 5.0, 0.10)),
    ("claude-3-5-haiku", Pricing::new(0.80, 4.0, 0.08)),
];

const OPENAI_PRICES: &[(&str, Pricing)] = &[
    ("gpt-5-nano", Pricing::new(0.05, 0.40, 0.005)),
    ("gpt-5-mini", Pricing::new(0.25, 2.0, 0.025)),
    ("gpt-5", Pricing::new(1.25, 10.0, 0.125)),
    ("gpt-4.1-nano", Pricing::new(0.10, 0.40, 0.025)),
    ("gpt-4.1-mini", Pricing::new(0.40, 1.60, 0.10)),
    ("gpt-4.1", Pricing::new(2.0, 8.0, 0.50)),
    ("gpt-4o-mini", Pricing::new(0.15, 0.60, 0.075)),
    ("gpt-4o", Pricing::new(2.50, 10.0, 1.25)),
    ("o4-mini", Pricing::new(1.10, 4.40, 0.275)),
    ("o3-mini", Pricing::new(1.10, 4.40, 0.55)),
    ("o3", Pricing::new(2.0, 8.0, 0.50)),
];

/// Parses an optional "$ per 1M tokens" settings field; blank or invalid = unset.
pub(crate) fn parse_price(text: &str) -> Option<f64> {
    text.trim()
        .trim_start_matches('$')
        .parse::<f64>()
        .ok()
        .filter(|p| p.is_finite() && *p >= 0.0)
}

/// The settings-field text for an optional price (blank when unset).
pub(crate) fn format_price(price: Option<f64>) -> String {
    price.map(|p| p.to_string()).unwrap_or_default()
}

/// 1234567 -> "1,234,567"
pub(crate) fn format_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Two decimals, or four for sub-cent amounts so they don't show as $0.00.
pub(crate) fn format_cost(cost: f64) -> String {
    if cost > 0.0 && cost < 0.01 {
        format!("${cost:.4}")
    } else {
        format!("${cost:.2}")
    }
}

/// "1,234 in · 567 out · 89 cached · $0.0123" for one reply.
pub(crate) fn format_usage(usage: &Usage, cost: Option<f64>) -> String {
    let mut parts = vec![
        format!("{} in", format_count(usage.input_tokens)),
        format!("{} out", format_count(usage.output_tokens)),
    ];
    if usage.cache_read_tokens > 0 {
        parts.push(format!("{} cached", format_count(usage.cache_read_tokens)));
    }
    if usage.cache_write_tokens > 0 {
        parts.push(format!(
            "{} cache write",
            format_count(usage.cache_write_tokens)
        ));
    }
    if let Some(cost) = cost {
        parts.push(format_cost(cost));
    }
    parts.join(" \u{00b7} ")
}

/// "12,345 tokens · $0.0456" for a whole chat; `None` if nothing was used yet.
pub(crate) fn format_totals(usage: &Usage, cost: Option<f64>) -> Option<String> {
    if usage.is_empty() {
        return None;
    }
    let mut text = format!("{} tokens", format_count(usage.total()));
    if let Some(cost) = cost {
        text.push_str(&format!(" \u{00b7} {}", format_cost(cost)));
    }
    Some(text)
}
