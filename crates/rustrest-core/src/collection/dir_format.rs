//! storage-agnostic pieces of the directory-backed collection format shared
//! between the local (`std::fs`-backed) and remote (SSH RPC-backed)
//! implementations, so both write/read byte-identical trees.

use crate::collection::model::{
    CollectionInfo, PostmanEvent, PostmanProtocolProfileBehavior, PostmanVariable,
};

pub const COLLECTION_META_FILE: &str = "_collection.json";
pub const FOLDER_META_FILE: &str = "_folder.json";

/// metadata persisted at the root of a directory-backed collection.
/// mirrors `CollectionInfo` + variables + explicit child ordering.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionMeta {
    pub info: CollectionInfo,
    pub variable: Option<Vec<PostmanVariable>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<crate::auth::RequestAuth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<Vec<PostmanEvent>>,
    /// ordered list of child entry names (file or directory names,
    /// relative to this directory).
    pub order: Vec<String>,
}

/// metadata persisted inside every folder directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FolderMeta {
    pub description: Option<String>,
    #[serde(rename = "protocolProfileBehavior")]
    pub protocol_profile_behavior: Option<PostmanProtocolProfileBehavior>,
    pub event: Option<Vec<PostmanEvent>>,
    pub order: Vec<String>,
}

/// turns an arbitrary item name into a filesystem-safe slug. Keeps things
/// human-readable (good for diffs) while avoiding characters that are
/// illegal or awkward on common filesystems.
pub fn sanitize_name(name: &str) -> String {
    let mut out: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();

    if out.is_empty() {
        out = "untitled".to_string();
    }
    out
}

/// ensures a filename is unique within `used`, appending `-2`, `-3`, ... on
/// collision (e.g. two requests both named "Get User").
pub fn dedupe_name(base: &str, used: &mut std::collections::HashSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}
