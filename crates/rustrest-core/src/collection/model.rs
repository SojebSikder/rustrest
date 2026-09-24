use crate::KeyValuePair;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanCollection {
    #[serde(skip)]
    pub id: usize, // track collection identity uniquely

    #[serde(skip)]
    pub file_path: Option<std::path::PathBuf>,

    #[serde(skip)]
    pub storage_dir: Option<std::path::PathBuf>,

    /// set when this collection is backed by a directory on a remote host
    /// (connected over SSH) instead of local disk.
    #[serde(skip)]
    pub remote_dir: Option<RemoteDirRef>,

    /// true when the collection tree/info has been mutated (folder/request
    /// added or renamed, collection renamed, variables edited, ...) since
    /// the last successful save to disk.
    #[serde(skip)]
    pub unsaved: bool,

    pub info: CollectionInfo,
    pub item: Vec<CollectionItem>,
    pub variable: Option<Vec<PostmanVariable>>,
}

/// identifies the remote host + directory a collection is synced against,
/// when it's backed by a directory on a remote SSH host rather than local
/// disk. `profile_id` refers to a `crate::remote::SshProfile::id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteDirRef {
    pub profile_id: usize,
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanVariable {
    pub key: String,
    pub value: Option<serde_json::Value>,
    pub r#type: Option<String>,
}

impl PostmanCollection {
    // rename collection root
    pub fn rename(&mut self, new_name: &str) {
        self.info.name = new_name.to_string();
    }

    // recursively find a folder by its current path and rename it
    pub fn rename_folder_by_path(&mut self, path: &[String], new_name: &str) -> bool {
        crate::collection::tree_ops::rename_nested_folder(&mut self.item, path, new_name)
    }

    /// clears the unsaved flag on the collection and every item in its tree;
    /// called once the collection has actually been persisted to disk.
    pub fn clear_unsaved(&mut self) {
        self.unsaved = false;
        for item in &mut self.item {
            item.clear_unsaved();
        }
    }

    // extracts raw postman variables into native application KeyValuePairs
    pub fn get_native_variables(&self) -> Vec<KeyValuePair> {
        let mut native_vars = Vec::new();
        if let Some(ref variables) = self.variable {
            for var in variables {
                let val_str = match &var.value {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(other) => other.to_string().trim_matches('"').to_string(),
                    None => String::new(),
                };
                let mut kv = KeyValuePair::new(&var.key, &val_str);
                kv.is_active = true;
                native_vars.push(kv);
            }
        }
        native_vars
    }

    pub fn assign_request_ids(&mut self, start_id: &mut usize) {
        fn assign_item_ids(items: &mut [CollectionItem], start_id: &mut usize) {
            for item in items {
                match item {
                    CollectionItem::Request(node) => {
                        node.id = *start_id;
                        *start_id += 1;
                    }
                    CollectionItem::Folder(folder) => {
                        assign_item_ids(&mut folder.item, start_id);
                    }
                }
            }
        }
        assign_item_ids(&mut self.item, start_id);
    }

    // set default headers
    pub fn set_headers(&mut self, headers: Vec<KeyValuePair>) {
        let postman_headers: Vec<PostmanHeader> = headers
            .iter()
            .map(|kv| PostmanHeader {
                key: kv.key.clone(),
                value: kv.value.clone(),
                disabled: None,
            })
            .collect();

        for item in &mut self.item {
            apply_headers_to_item(item, &postman_headers);
        }
    }

    pub fn to_postman_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize collection schema: {}", e))
    }
}

// helper function to apply headers to a collection item
fn apply_headers_to_item(item: &mut CollectionItem, headers: &[PostmanHeader]) {
    match item {
        // if it's a request node, merge the new headers with the existin ones
        CollectionItem::Request(node) => {
            let mut merged_headers = headers.to_vec();

            if let Some(existing_headers) = node.request.header.take() {
                merged_headers.extend(existing_headers);
            }

            node.request.header = Some(merged_headers)
        }
        // if it's a folder, iterate through its sub-items and apply headers recursively
        CollectionItem::Folder(folder) => {
            for sub_item in &mut folder.item {
                apply_headers_to_item(sub_item, headers);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub name: String,
    #[serde(rename = "_postman_id")]
    pub postman_id: Option<String>,
    pub schema: String,
    /// markdown documentation for the collection as a whole.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_description"
    )]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanFolder {
    pub name: String,
    #[serde(rename = "protocolProfileBehavior")]
    pub protocol_profile_behavior: Option<PostmanProtocolProfileBehavior>,
    pub item: Vec<CollectionItem>,
    pub event: Option<Vec<PostmanEvent>>,
    /// markdown documentation for this folder.
    #[serde(default, deserialize_with = "deserialize_description")]
    pub description: Option<String>,

    /// true when this folder was added/renamed since the last save.
    #[serde(skip)]
    pub unsaved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanProtocolProfileBehavior {
    #[serde(rename = "disableBodyPruning")]
    pub disable_body_pruning: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CollectionItem {
    Folder(PostmanFolder),
    Request(PostmanRequestNode),
}

impl CollectionItem {
    /// recursively clears the unsaved flag on this item and its descendants.
    pub fn clear_unsaved(&mut self) {
        match self {
            CollectionItem::Folder(folder) => {
                folder.unsaved = false;
                for item in &mut folder.item {
                    item.clear_unsaved();
                }
            }
            CollectionItem::Request(req) => {
                req.unsaved = false;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanEvent {
    pub listen: String, // "prerequest" or "test"
    pub script: Option<PostmanScript>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanScript {
    pub r#type: Option<String>, // e.g. "text/javascript"
    pub exec: Option<PostmanScriptExec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PostmanScriptExec {
    List(Vec<String>),
    Single(String),
}

impl PostmanScriptExec {
    pub fn to_string_contents(&self) -> String {
        match self {
            Self::List(lines) => lines.join("\n"),
            Self::Single(s) => s.clone(),
        }
    }

    pub fn from_string(s: &str) -> Self {
        Self::List(s.lines().map(|l| l.to_string()).collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanRequestNode {
    #[serde(skip)]
    pub id: usize,
    pub name: String,
    pub event: Option<Vec<PostmanEvent>>,
    pub request: PostmanRequestDetails,

    /// true when this request was added since the last save.
    #[serde(skip)]
    pub unsaved: bool,

    /// saved response snapshots for this request
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Vec<PostmanResponseExample>>,

    /// used when this request is a WebSocket/SSE/GraphQL/gRPC request
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_request: Option<ProtocolRequestDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProtocolRequestDetails {
    WebSocket(WebSocketRequestDetails),
    GraphQl(GraphQlRequestDetails),
    Grpc(GrpcRequestDetails),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebSocketRequestDetails {
    pub url: String,
    pub headers: Vec<PostmanHeader>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphQlRequestDetails {
    pub url: String,
    pub headers: Vec<PostmanHeader>,
    pub query: String,
    pub variables: String,
    pub operation_name: Option<String>,
    /// endpoint used for `graphql-transport-ws` subscriptions, if different
    /// from `url` (e.g. `url` is `https://...` and this is `wss://...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GrpcRequestDetails {
    pub endpoint: String,
    pub use_tls: bool,
    pub service: String,
    pub method: String,
    pub request_json: String,
    pub metadata: Vec<PostmanHeader>,
    /// empty means "discover via server reflection"
    pub proto_files: Vec<String>,
}

/// a saved snapshot of a response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanResponseExample {
    pub name: String,
    pub code: u16,
    pub header: Option<Vec<PostmanHeader>>,
    pub body: Option<String>,

    #[serde(rename = "responseTime")]
    pub response_time: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanRequestDetails {
    pub method: String,
    pub url: Option<PostmanUrl>,
    pub header: Option<Vec<PostmanHeader>>,
    pub body: Option<PostmanBody>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_auth"
    )]
    pub auth: Option<crate::auth::RequestAuth>,
    /// markdown documentation for this request.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_description"
    )]
    pub description: Option<String>,
}

/// accepts either a plain markdown string or Postman's `{ "content": ...,
/// "type": "text/markdown" }` object form, so imported Postman collections
/// keep their docs.
pub(crate) fn deserialize_description<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Object(obj)) => obj
            .get("content")
            .and_then(|c| c.as_str())
            .map(str::to_string),
        _ => None,
    })
}

/// accepts either the structured `RequestAuth` object, or a plain string -
/// collections saved before structured auth existed have `"auth"` as a raw
/// `Authorization` header value, which loads as `AuthType::Custom` so
/// nothing is lost.
fn deserialize_auth<'de, D>(deserializer: D) -> Result<Option<crate::auth::RequestAuth>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    Ok(match value {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => Some(crate::auth::RequestAuth::from_legacy_string(s)),
        Some(v) => serde_json::from_value(v).ok(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PostmanUrl {
    String(String),
    Object { raw: String },
}

impl PostmanUrl {
    pub fn to_string(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Object { raw } => raw.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanHeader {
    pub key: String,
    pub value: String,
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanBody {
    pub mode: Option<String>,
    pub raw: Option<String>,
    pub formdata: Option<Vec<PostmanBodyRow>>,
    pub urlencoded: Option<Vec<PostmanBodyRow>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphql: Option<PostmanGraphQlBody>,
    /// a binary body's file (Postman's `mode: "file"`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PostmanBodyFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanBodyFile {
    pub src: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanGraphQlBody {
    pub query: String,
    pub variables: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanBodyRow {
    pub key: String,
    pub value: Option<String>,
    pub disabled: Option<bool>,
    pub r#type: Option<String>,
    /// a form-data part's own `Content-Type` (Postman's `contentType`)
    #[serde(
        rename = "contentType",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub content_type: Option<String>,
    /// a form-data file row's path(s) (Postman's `src`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<PostmanFileSrc>,
}

/// Postman writes a single-file `src` as a string and a multi-file one as an array
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PostmanFileSrc {
    One(String),
    Many(Vec<String>),
}

impl PostmanFileSrc {
    pub fn from_paths(paths: &[String]) -> Option<Self> {
        match paths {
            [] => None,
            [one] => Some(Self::One(one.clone())),
            many => Some(Self::Many(many.to_vec())),
        }
    }

    pub fn paths(&self) -> Vec<String> {
        match self {
            Self::One(path) if path.is_empty() => Vec::new(),
            Self::One(path) => vec![path.clone()],
            Self::Many(paths) => paths.clone(),
        }
    }
}

#[cfg(test)]
mod auth_backcompat_tests {
    use super::*;
    use crate::auth::AuthType;

    fn minimal_details_json(auth: &str) -> String {
        format!(r#"{{"method":"GET","url":null,"header":null,"body":null,"auth":{auth}}}"#)
    }

    #[test]
    fn legacy_string_auth_loads_as_custom() {
        let json = minimal_details_json(r#""Bearer some-old-token""#);
        let details: PostmanRequestDetails = serde_json::from_str(&json).unwrap();
        let auth = details.auth.expect("auth present");
        assert_eq!(auth.auth_type, AuthType::Custom);
        assert_eq!(auth.custom_raw, "Bearer some-old-token");
    }

    #[test]
    fn missing_auth_field_loads_as_none() {
        let json = r#"{"method":"GET","url":null,"header":null,"body":null}"#;
        let details: PostmanRequestDetails = serde_json::from_str(json).unwrap();
        assert!(details.auth.is_none());
    }

    #[test]
    fn structured_auth_round_trips_through_json() {
        let mut auth = crate::auth::RequestAuth::default();
        auth.auth_type = AuthType::Bearer;
        auth.bearer_token = "abc".to_string();
        let details = PostmanRequestDetails {
            method: "GET".to_string(),
            url: None,
            header: None,
            body: None,
            auth: Some(auth),
            description: None,
        };
        let json = serde_json::to_string(&details).unwrap();
        let round_tripped: PostmanRequestDetails = serde_json::from_str(&json).unwrap();
        let auth = round_tripped.auth.expect("auth present");
        assert_eq!(auth.auth_type, AuthType::Bearer);
        assert_eq!(auth.bearer_token, "abc");
    }

    #[test]
    fn description_accepts_string_and_postman_object() {
        let json =
            r##"{"method":"GET","url":null,"header":null,"body":null,"description":"# Hi"}"##;
        let details: PostmanRequestDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.description.as_deref(), Some("# Hi"));

        let json = r#"{"method":"GET","url":null,"header":null,"body":null,
            "description":{"content":"**bold**","type":"text/markdown"}}"#;
        let details: PostmanRequestDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.description.as_deref(), Some("**bold**"));
    }

    #[test]
    fn form_data_file_src_accepts_a_string_or_an_array_and_round_trips() {
        let rows: Vec<PostmanBodyRow> = serde_json::from_str(
            r#"[{"key":"a","type":"file","src":"/tmp/one.png"},
                {"key":"b","type":"file","src":["/tmp/x.txt","/tmp/y.txt"]},
                {"key":"c","type":"file","src":""},
                {"key":"d","type":"text","value":"hi"}]"#,
        )
        .unwrap();
        assert_eq!(rows[0].src.as_ref().unwrap().paths(), vec!["/tmp/one.png"]);
        assert_eq!(
            rows[1].src.as_ref().unwrap().paths(),
            vec!["/tmp/x.txt", "/tmp/y.txt"]
        );
        assert!(rows[2].src.as_ref().unwrap().paths().is_empty());
        assert!(rows[3].src.is_none());

        let paths = vec!["/tmp/x.txt".to_string(), "/tmp/y.txt".to_string()];
        let json = serde_json::to_value(PostmanFileSrc::from_paths(&paths)).unwrap();
        assert_eq!(json, serde_json::json!(["/tmp/x.txt", "/tmp/y.txt"]));
        let json = serde_json::to_value(PostmanFileSrc::from_paths(&paths[..1])).unwrap();
        assert_eq!(json, serde_json::json!("/tmp/x.txt"));
    }
}
