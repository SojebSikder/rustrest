use crate::KeyValuePair;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
        let mut output = input.to_string();

        // collect all valid variables, prioritising Environment over Collection
        let mut merged_vars: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        // first add collection variables as the fallback layer
        if let Some(col_vars) = collection_vars {
            for var in col_vars {
                if var.is_active && !var.key.trim().is_empty() {
                    merged_vars.insert(var.key.trim().to_string(), var.value.clone());
                }
            }
        }

        // overwrite or append with Environment variables (higher precedence)
        for var in &self.variables {
            if var.is_active && !var.key.trim().is_empty() {
                merged_vars.insert(var.key.trim().to_string(), var.value.clone());
            }
        }

        // perform string replacements
        for (key, value) in merged_vars {
            let placeholder = format!("{{{{{}}}}}", key);
            output = output.replace(&placeholder, &value);
        }

        output
    }
}
