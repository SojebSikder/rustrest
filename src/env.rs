use crate::tab::types::KeyValuePair;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    pub variables: Vec<KeyValuePair>,
}

impl Environment {
    // pub fn new(name: &str) -> Self {
    //     Self {
    //         name: name.to_string(),
    //         variables: vec![KeyValuePair::new("", "")],
    //     }
    // }

    // replace all occurrences of {{key}} with its corresponding active value
    pub fn replace_vars(&self, input: &str) -> String {
        let mut output = input.to_string();
        for var in &self.variables {
            if var.is_active && !var.key.trim().is_empty() {
                let placeholder = format!("{{{{{}}}}}", var.key.trim());
                output = output.replace(&placeholder, &var.value);
            }
        }
        output
    }
}
