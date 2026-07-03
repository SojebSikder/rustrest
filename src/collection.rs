use crate::http_client::HttpMethod;
use crate::tab::Tab;
use crate::tab::types::{BodyType, FormDataRow, FormDataType, KeyValuePair, RequestSubTab};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanCollection {
    #[serde(skip)]
    pub id: usize, // track collection identity uniquely

    #[serde(skip)]
    pub file_path: Option<std::path::PathBuf>,

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
    // rename collection root
    pub fn rename(&mut self, new_name: &str) {
        self.info.name = new_name.to_string();
    }

    // recursively find a folder by its current path and rename it
    pub fn rename_folder_by_path(&mut self, path: &[String], new_name: &str) -> bool {
        fn rename_recursive(items: &mut [CollectionItem], path: &[String], new_name: &str) -> bool {
            if path.is_empty() {
                return false;
            }
            let target = &path[0];
            let is_last = path.len() == 1;

            for item in items {
                if let CollectionItem::Folder {
                    name,
                    item: sub_items,
                } = item
                {
                    if name == target {
                        if is_last {
                            *name = new_name.to_string();
                            return true;
                        } else {
                            return rename_recursive(sub_items, &path[1..], new_name);
                        }
                    }
                }
            }
            false
        }
        rename_recursive(&mut self.item, path, new_name)
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
                    CollectionItem::Folder {
                        item: sub_items, ..
                    } => {
                        assign_item_ids(sub_items, start_id);
                    }
                }
            }
        }
        assign_item_ids(&mut self.item, start_id);
    }

    pub fn to_postman_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize collection schema: {}", e))
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
    #[serde(skip)]
    pub id: usize,
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
    pub formdata: Option<Vec<PostmanBodyRow>>,
    pub urlencoded: Option<Vec<PostmanBodyRow>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostmanBodyRow {
    pub key: String,
    pub value: Option<String>,
    pub disabled: Option<bool>,
    pub r#type: Option<String>,
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
    tab.request_id = Some(node.id);

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
        if let Some(mode) = &body.mode {
            match mode.as_str() {
                "raw" => {
                    if let Some(raw_text) = &body.raw {
                        tab.request_body = iced::widget::text_editor::Content::with_text(raw_text);
                        tab.body_type = BodyType::Raw;
                        tab.active_sub_tab = RequestSubTab::Body;
                    }
                }
                "formdata" => {
                    tab.body_type = BodyType::FormData;
                    tab.active_sub_tab = RequestSubTab::Body;
                    if let Some(rows) = &body.formdata {
                        tab.body_form_data = rows
                            .iter()
                            .map(|r| {
                                let f_type = match r.r#type.as_deref() {
                                    Some("file") => FormDataType::File,
                                    _ => FormDataType::Text,
                                };
                                let mut row = FormDataRow::new(
                                    &r.key,
                                    &r.value.clone().unwrap_or_default(),
                                    f_type,
                                );
                                row.is_active = !r.disabled.unwrap_or(false);
                                row
                            })
                            .collect();
                    }
                }
                "urlencoded" => {
                    tab.body_type = BodyType::Raw; // default to raw fallback safely
                    tab.active_sub_tab = RequestSubTab::Body;

                    if let Some(rows) = &body.urlencoded {
                        tab.body_urlencoded = rows
                            .iter()
                            .map(|r| {
                                let mut kv =
                                    KeyValuePair::new(&r.key, &r.value.clone().unwrap_or_default());
                                kv.is_active = !r.disabled.unwrap_or(false);
                                kv
                            })
                            .collect();

                        let encoded_string = rows
                            .iter()
                            .filter(|r| !r.disabled.unwrap_or(false))
                            .map(|r| {
                                format!(
                                    "{}={}",
                                    urlencoding::encode(&r.key),
                                    urlencoding::encode(&r.value.as_deref().unwrap_or(""))
                                )
                            })
                            .collect::<Vec<String>>()
                            .join("&");

                        tab.request_body =
                            iced::widget::text_editor::Content::with_text(&encoded_string);
                    }
                }
                _ => {}
            }
        }
    }

    tab
}
