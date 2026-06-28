use crate::http_client::HttpMethod;
use crate::tab::Tab;
use crate::tab::types::{KeyValuePair, RequestSubTab};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanCollection {
    #[serde(skip)]
    pub id: usize, // track collection identity uniquely
    pub info: CollectionInfo,
    pub item: Vec<CollectionItem>,
    pub variable: Option<Vec<PostmanVariable>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanVariable {
    pub key: String,
    pub value: Option<serde_json::Value>,
    pub r#type: Option<String>,
}

impl PostmanCollection {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub name: String,
    #[serde(rename = "_postman_id")]
    pub postman_id: Option<String>,
    pub schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CollectionItem {
    Folder {
        name: String,
        item: Vec<CollectionItem>,
    },
    Request(PostmanRequestNode),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanRequestNode {
    pub name: String,
    pub request: PostmanRequestDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanRequestDetails {
    pub method: String,
    pub url: PostmanUrl,
    pub header: Option<Vec<PostmanHeader>>,
    pub body: Option<PostmanBody>,
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
}

// helper to recursively transform a Postman Request Node into our app's live tab state
pub fn create_tab_from_request(
    id: usize,
    node: &PostmanRequestNode,
    collection_id: Option<usize>,
) -> Tab {
    let mut tab = Tab::new(id);
    tab.name = node.name.clone();
    tab.url = node.request.url.to_string();
    tab.collection_id = collection_id;

    tab.method = match node.request.method.to_uppercase().as_str() {
        "GET" => HttpMethod::GET,
        "POST" => HttpMethod::POST,
        "PUT" => HttpMethod::PUT,
        "DELETE" => HttpMethod::DELETE,
        "PATCH" => HttpMethod::PATCH,
        "HEAD" => HttpMethod::HEAD,
        "OPTIONS" => HttpMethod::OPTIONS,
        custom => HttpMethod::Custom(custom.to_string()),
    };

    if let Some(headers) = &node.request.header {
        tab.request_headers = headers
            .iter()
            .map(|h| {
                let mut kv = KeyValuePair::new(&h.key, &h.value);
                kv.is_active = !h.disabled.unwrap_or(false);
                kv
            })
            .collect();
    }

    if let Some(body) = &node.request.body {
        if let Some(raw_text) = &body.raw {
            tab.request_body = iced::widget::text_editor::Content::with_text(raw_text);
            tab.active_sub_tab = RequestSubTab::Body;
        }
    }

    tab
}
