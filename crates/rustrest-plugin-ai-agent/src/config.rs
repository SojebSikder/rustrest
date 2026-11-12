//! The agent's saved settings, and the settings form that edits them.

use crate::host;
use crate::provider::Provider;
use crate::usage::{format_price, parse_price, Pricing};
use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentConfig {
    pub(crate) provider: Provider,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) base_url: String,
    /// render assistant replies as markdown (true) or plain text (false).
    #[serde(default = "default_true")]
    pub(crate) render_markdown: bool,
    /// optional USD-per-1M-token rates overriding the built-in price table.
    /// used only when both are set.
    #[serde(default)]
    pub(crate) input_price_per_mtok: Option<f64>,
    #[serde(default)]
    pub(crate) output_price_per_mtok: Option<f64>,
}

fn default_true() -> bool {
    true
}

impl Default for AgentConfig {
    fn default() -> Self {
        let provider = Provider::default();
        Self {
            provider,
            api_key: String::new(),
            model: provider.default_model().to_string(),
            base_url: provider.default_base_url().to_string(),
            render_markdown: true,
            input_price_per_mtok: None,
            output_price_per_mtok: None,
        }
    }
}

impl AgentConfig {
    /// The saved config, or the defaults if there isn't a readable one.
    pub(crate) fn load() -> Self {
        host::read_json(CONFIG_FILE).unwrap_or_default()
    }

    pub(crate) fn save(&self) {
        let _ = host::write_json(CONFIG_FILE, self);
    }

    /// True until the user has entered the credentials the provider needs.
    pub(crate) fn is_missing_api_key(&self) -> bool {
        self.provider.requires_api_key() && self.api_key.is_empty()
    }

    /// The rates replies are priced at; `None` if unknown.
    pub(crate) fn pricing(&self) -> Option<Pricing> {
        resolve_pricing(
            self.provider,
            &self.model,
            self.input_price_per_mtok,
            self.output_price_per_mtok,
        )
    }
}

/// The user's override when both rates are set, else the built-in list price.
fn resolve_pricing(
    provider: Provider,
    model: &str,
    input: Option<f64>,
    output: Option<f64>,
) -> Option<Pricing> {
    match (input, output) {
        (Some(input), Some(output)) => Some(Pricing::custom(input, output)),
        _ => Pricing::list_price(provider, model),
    }
}

/// The settings form's fields. Kept as text so half-typed values (like a price of "0.")
/// survive until Save. Only then are they parsed into an [`AgentConfig`].
#[derive(Debug, Default, Clone)]
pub(crate) struct SettingsForm {
    pub(crate) provider: Provider,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) base_url: String,
    pub(crate) input_price: String,
    pub(crate) output_price: String,
}

impl SettingsForm {
    pub(crate) fn from_config(config: &AgentConfig) -> Self {
        Self {
            provider: config.provider,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            base_url: config.base_url.clone(),
            input_price: format_price(config.input_price_per_mtok),
            output_price: format_price(config.output_price_per_mtok),
        }
    }

    /// Switches provider, resetting the URL and model to its defaults.
    pub(crate) fn select_provider(&mut self, provider: Provider) {
        self.provider = provider;
        self.base_url = provider.default_base_url().to_string();
        self.model = provider.default_model().to_string();
    }

    /// Writes the form's values into `config`, leaving settings the form
    /// doesn't show (like markdown rendering) as they were.
    pub(crate) fn apply_to(&self, config: &mut AgentConfig) {
        config.provider = self.provider;
        config.api_key = self.api_key.clone();
        config.model = self.model.clone();
        config.base_url = self.base_url.clone();
        config.input_price_per_mtok = parse_price(&self.input_price);
        config.output_price_per_mtok = parse_price(&self.output_price);
    }

    /// What replies would be priced at if the form were saved now - shown in
    /// the form, so the user can tell whether the built-in table knows their model.
    pub(crate) fn pricing_summary(&self) -> String {
        let input = parse_price(&self.input_price);
        let output = parse_price(&self.output_price);
        let source = if input.is_some() && output.is_some() {
            "custom"
        } else {
            "built-in estimate"
        };
        match resolve_pricing(self.provider, &self.model, input, output) {
            Some(p) => format!(
                "Pricing: ${} in / ${} out per 1M tokens ({source}). Set both rates below to override.",
                p.input, p.output
            ),
            None if self.provider == Provider::Ollama => {
                "Pricing: none (local model) - set both rates below to estimate cost.".to_string()
            }
            None => "Pricing: unknown for this model - set both rates below to estimate cost."
                .to_string(),
        }
    }
}
