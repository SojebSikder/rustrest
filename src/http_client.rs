use crate::tab::types::{FormDataRow, FormDataType};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    pub headers: HashMap<String, String>,
    pub elapsed: Duration,
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::Custom(custom) => write!(f, "{}", custom.to_uppercase()),
            _ => write!(f, "{:?}", self),
        }
    }
}

pub async fn send_request(
    url: String,
    method: HttpMethod,
    body_type: crate::tab::types::BodyType,
    raw_body: String,
    form_data_list: Vec<FormDataRow>,
    binary_file_path: Option<String>,
    headers_list: Vec<(String, String)>,
    cookies_list: Vec<(String, String)>,
    auth_raw: String,
    cancel_token: CancellationToken,
) -> Result<HttpResponse, String> {
    let reqwest_url =
        reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL pattern: {}", e))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to initialize client: {}", e))?;

    let req_method = match &method {
        HttpMethod::GET => reqwest::Method::GET,
        HttpMethod::POST => reqwest::Method::POST,
        HttpMethod::PUT => reqwest::Method::PUT,
        HttpMethod::DELETE => reqwest::Method::DELETE,
        HttpMethod::PATCH => reqwest::Method::PATCH,
        HttpMethod::HEAD => reqwest::Method::HEAD,
        HttpMethod::OPTIONS => reqwest::Method::OPTIONS,
        HttpMethod::Custom(custom_str) => {
            let upper = custom_str.trim().to_uppercase();
            reqwest::Method::from_bytes(upper.as_bytes())
                .map_err(|_| format!("Invalid custom HTTP method: '{}'", custom_str))?
        }
    };

    let mut req_builder = client.request(req_method, reqwest_url);

    // filter out completely blank header keys
    for (key, val) in headers_list {
        if !key.trim().is_empty() {
            req_builder = req_builder.header(key.trim(), val);
        }
    }

    let formatted_cookies: String = cookies_list
        .into_iter()
        .filter(|(key, _)| !key.trim().is_empty())
        .map(|(key, val)| format!("{}={}", key.trim(), val.trim()))
        .collect::<Vec<String>>()
        .join("; ");

    if !formatted_cookies.is_empty() {
        req_builder = req_builder.header("Cookie", formatted_cookies);
    }

    let auth_trimmed = auth_raw.trim();
    if !auth_trimmed.is_empty() {
        req_builder = req_builder.header("Authorization", auth_trimmed);
    }

    if method != HttpMethod::GET && method != HttpMethod::HEAD {
        match body_type {
            // handling files/fields matching multipart/form-data rules
            crate::tab::types::BodyType::FormData => {
                let mut form = reqwest::multipart::Form::new();
                let mut has_fields = false;

                for row in form_data_list {
                    if !row.is_active || row.key.trim().is_empty() {
                        continue;
                    }
                    has_fields = true;

                    match row.field_type {
                        FormDataType::Text => {
                            form = form.text(row.key, row.value);
                        }
                        FormDataType::File => {
                            if !row.value.trim().is_empty() {
                                let path = Path::new(&row.value);
                                if path.exists() {
                                    let file_bytes = tokio::fs::read(path)
                                        .await
                                        .map_err(|e| format!("Form File Read Failure: {}", e))?;

                                    let file_name = path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("file")
                                        .to_string();

                                    let part = reqwest::multipart::Part::bytes(file_bytes)
                                        .file_name(file_name);

                                    form = form.part(row.key, part);
                                }
                            }
                        }
                    }
                }
                if has_fields {
                    req_builder = req_builder.multipart(form);
                }
            }

            // handling raw stream binary uploads
            crate::tab::types::BodyType::Binary => {
                if let Some(ref path_str) = binary_file_path {
                    let path = Path::new(path_str);
                    if path.exists() {
                        let file_bytes = tokio::fs::read(path)
                            .await
                            .map_err(|e| format!("Binary File Read Failure: {}", e))?;
                        req_builder = req_builder.body(file_bytes);
                    }
                }
            }

            // fallback text states (raw JSON, URLencoded forms, etc.)
            _ => {
                if !raw_body.trim().is_empty() {
                    req_builder = req_builder.body(raw_body);
                }
            }
        }
    }

    let start_time = Instant::now();

    let response = tokio::select! {
        res = req_builder.send() => res.map_err(|e| format!("Network Dispatch Error: {}", e))?,
        _ = cancel_token.cancelled() => return Err(String::from("Request cancelled by user.")),
    };

    let elapsed = start_time.elapsed();
    let status = response.status().as_u16();
    let mut headers = HashMap::new();
    for (key, value) in response.headers().iter() {
        if let Ok(val_str) = value.to_str() {
            headers.insert(key.to_string(), val_str.to_string());
        }
    }

    let body_text = tokio::select! {
        body_res = response.text() => body_res.map_err(|e| format!("Payload Parsing Error: {}", e))?,
        _ = cancel_token.cancelled() => return Err(String::from("Request cancelled by user.")),
    };

    let finalized_body = if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body_text)
    {
        serde_json::to_string_pretty(&json_val).unwrap_or(body_text)
    } else {
        body_text
    };

    Ok(HttpResponse {
        status,
        body: finalized_body,
        headers,
        elapsed,
    })
}
