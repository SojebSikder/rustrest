//! Turns a request into a ready-to-run code snippet.

mod curl;

use crate::common::{BodyType, FormDataRow, FormDataType};
use std::collections::HashMap;

pub use curl::Curl;

/// a request, already resolved, in the shape every generator consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenRequest {
    /// upper-cased, e.g. `GET` or a custom `PURGE`.
    pub method: String,
    pub url: String,
    /// non-blank header keys, trimmed, in the user's order.
    pub headers: Vec<(String, String)>,
    /// non-blank cookie names, trimmed; generators may emit these as a
    /// `Cookie` header or through a dedicated API.
    pub cookies: Vec<(String, String)>,
    pub body: CodegenBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenBody {
    None,
    /// raw text, GraphQL payloads included.
    Raw(String),
    /// decoded `x-www-form-urlencoded` pairs, so generators can encode them
    /// in whatever way is idiomatic for the target.
    UrlEncoded(Vec<(String, String)>),
    FormData(Vec<FormPart>),
    /// path to the file sent as the body.
    Binary(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormPart {
    pub name: String,
    pub value: FormPartValue,
    /// explicit part `Content-Type`, if the user set one.
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormPartValue {
    Text(String),
    File(String),
}

/// the compiled pieces of a request, as `Tab::compile_request_fields` and
/// `http::RequestSpec` already use them.
pub struct RequestParts {
    pub method: String,
    pub url: String,
    pub body_type: BodyType,
    pub raw_body: String,
    pub form_data: Vec<FormDataRow>,
    pub binary_file_path: Option<String>,
    pub headers: Vec<(String, String)>,
    pub cookies: Vec<(String, String)>,
}

impl CodegenRequest {
    pub fn from_parts(parts: RequestParts) -> Self {
        let method = parts.method.trim().to_uppercase();
        let non_blank = |pairs: Vec<(String, String)>| -> Vec<(String, String)> {
            pairs
                .into_iter()
                .filter(|(k, _)| !k.trim().is_empty())
                .map(|(k, v)| (k.trim().to_string(), v))
                .collect()
        };
        let mut headers = non_blank(parts.headers);
        let cookies = non_blank(parts.cookies);

        // same rule as `http::build_body`: GET/HEAD never carry a body
        let body = if method == "GET" || method == "HEAD" {
            CodegenBody::None
        } else {
            match parts.body_type {
                BodyType::None => CodegenBody::None,
                BodyType::Raw | BodyType::GraphQl => {
                    if parts.raw_body.trim().is_empty() {
                        CodegenBody::None
                    } else {
                        CodegenBody::Raw(parts.raw_body)
                    }
                }
                BodyType::XWwwFormUrlencoded => {
                    let pairs: Vec<(String, String)> =
                        url::form_urlencoded::parse(parts.raw_body.as_bytes())
                            .into_owned()
                            .collect();
                    if pairs.is_empty() {
                        CodegenBody::None
                    } else {
                        CodegenBody::UrlEncoded(pairs)
                    }
                }
                BodyType::FormData => {
                    let form = form_parts(parts.form_data);
                    if form.is_empty() {
                        CodegenBody::None
                    } else {
                        // the encoder sets its own boundary header, so a
                        // user-set Content-Type would only break it
                        headers.retain(|(k, _)| !k.eq_ignore_ascii_case("content-type"));
                        CodegenBody::FormData(form)
                    }
                }
                BodyType::Binary => match parts.binary_file_path {
                    Some(path) if !path.trim().is_empty() => CodegenBody::Binary(path),
                    _ => CodegenBody::None,
                },
            }
        };

        Self {
            method,
            url: parts.url,
            headers,
            cookies,
            body,
        }
    }
}

/// active, named rows, with one part per file (as `http::multipart` sends them).
fn form_parts(rows: Vec<FormDataRow>) -> Vec<FormPart> {
    let mut parts = Vec::new();
    for row in rows
        .into_iter()
        .filter(|r| r.is_active && !r.key.trim().is_empty())
    {
        let content_type = Some(row.content_type.trim().to_string()).filter(|c| !c.is_empty());
        match row.field_type {
            FormDataType::Text => parts.push(FormPart {
                name: row.key,
                value: FormPartValue::Text(row.value),
                content_type,
            }),
            FormDataType::File => {
                for file in row.files.into_iter().filter(|f| !f.trim().is_empty()) {
                    parts.push(FormPart {
                        name: row.key.clone(),
                        value: FormPartValue::File(file),
                        content_type: content_type.clone(),
                    });
                }
            }
        }
    }
    parts
}

/// a boolean setting a generator exposes, e.g. cURL's "Use long-form flags".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorOption {
    /// stable id, unique within its generator.
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub default: bool,
}

/// the user's choices for a generator's options; anything unset falls back
/// to the option's default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OptionValues(HashMap<&'static str, bool>);

impl OptionValues {
    pub fn get(&self, option: &GeneratorOption) -> bool {
        self.0.get(option.key).copied().unwrap_or(option.default)
    }

    pub fn set(&mut self, key: &'static str, value: bool) {
        self.0.insert(key, value);
    }
}

pub trait CodeGenerator: Send + Sync {
    /// stable id, e.g. `curl`.
    fn id(&self) -> &'static str;
    /// shown in the language picker, e.g. `cURL`.
    fn label(&self) -> &'static str;
    /// a syntect token (extension or language name) for highlighting the
    /// output, e.g. `sh`.
    fn syntax(&self) -> &'static str;
    fn options(&self) -> &'static [GeneratorOption] {
        &[]
    }
    fn generate(&self, request: &CodegenRequest, options: &OptionValues) -> String;
}

/// every available target, in picker order.
pub fn generators() -> &'static [&'static dyn CodeGenerator] {
    &[&Curl]
}

pub fn find(id: &str) -> Option<&'static dyn CodeGenerator> {
    generators().iter().copied().find(|g| g.id() == id)
}
