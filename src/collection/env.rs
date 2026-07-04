use crate::tab::types::KeyValuePair;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    pub variables: Vec<KeyValuePair>,
}

impl Environment {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            variables: vec![KeyValuePair::new("", "")],
        }
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
