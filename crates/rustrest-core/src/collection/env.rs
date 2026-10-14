use crate::KeyValuePair;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    pub variables: Vec<KeyValuePair>,
    /// keys whose values never leave this machine. a cloud environment
    /// shares only their names, so each teammate keeps their own value.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub local_keys: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<EnvCloudLink>,
}

/// ties an environment to its copy in Rustrest Cloud
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EnvCloudLink {
    pub id: String,
    pub team_id: String,
    /// server rev the base is from; 0 until the first upload lands
    pub rev: i64,
    /// the server's name and shared data as of `rev`, the common ancestor
    /// for merging changes made here and in the cloud
    #[serde(default)]
    pub base_name: String,
    #[serde(default)]
    pub base: serde_json::Value,
}

impl Environment {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            variables: vec![KeyValuePair::new("", "")],
            ..Self::default()
        }
    }

    pub fn is_local(&self, key: &str) -> bool {
        self.local_keys.contains(key.trim())
    }

    /// sets a variable from a script or plugin. in a cloud environment a
    /// variable created this way starts out local, so a token stored by a
    /// login script isn't shared with the team unless someone syncs it
    pub fn set_var(&mut self, key: &str, value: String) {
        if let Some(existing) = self.variables.iter_mut().find(|kv| kv.key == key) {
            existing.value = value;
            existing.is_active = true;
            return;
        }
        if self.cloud.is_some() {
            self.local_keys.insert(key.trim().to_string());
        }
        self.variables.push(KeyValuePair::new(key, &value));
    }

    // replaces occurrences of {{key}} with values, checking active environment
    pub fn replace_vars(&self, input: &str, collection_vars: Option<&[KeyValuePair]>) -> String {
        // prefer environment variables over collection variables
        let layers: [&[KeyValuePair]; 2] = [collection_vars.unwrap_or(&[]), &self.variables];
        replace_vars_in(input, &layers)
    }
}

/// how deep a variable whose value references other variables is expanded
const MAX_VAR_DEPTH: usize = 10;

/// replaces `{{key}}` (whitespace inside the braces allowed) using `layers`,
/// later layers taking precedence. unknown variables are left as is, and
/// values that reference other variables are expanded too.
pub fn replace_vars_in(input: &str, layers: &[&[KeyValuePair]]) -> String {
    let mut merged: HashMap<&str, &str> = HashMap::new();
    for layer in layers {
        for var in layer.iter() {
            let key = var.key.trim();
            if var.is_active && !key.is_empty() {
                merged.insert(key, &var.value);
            }
        }
    }

    let mut output = input.to_string();
    for _ in 0..MAX_VAR_DEPTH {
        let next = replace_once(&output, &merged);
        if next == output {
            break;
        }
        output = next;
    }
    output
}

fn replace_once(input: &str, vars: &HashMap<&str, &str>) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };

        out.push_str(&rest[..start]);
        match vars.get(after[..end].trim()) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}
