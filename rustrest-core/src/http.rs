use crate::common::{BodyType, FormDataRow, FormDataType};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
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
pub struct TestResult {
    pub name: String,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    pub headers: HashMap<String, String>,
    pub elapsed: Duration,
    pub test_results: Vec<TestResult>,
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::Custom(custom) => write!(f, "{}", custom.to_uppercase()),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Describes one HTTP request to send
pub struct RequestSpec {
    url: String,
    method: HttpMethod,
    body_type: BodyType,
    raw_body: String,
    form_data: Vec<FormDataRow>,
    binary_file_path: Option<String>,
    headers: Vec<(String, String)>,
    cookies: Vec<(String, String)>,
    auth_raw: String,
    timeout: Duration,
}

impl RequestSpec {
    pub fn new(url: impl Into<String>, method: HttpMethod) -> Self {
        Self {
            url: url.into(),
            method,
            body_type: BodyType::None,
            raw_body: String::new(),
            form_data: Vec::new(),
            binary_file_path: None,
            headers: Vec::new(),
            cookies: Vec::new(),
            auth_raw: String::new(),
            timeout: Duration::from_secs(30),
        }
    }

    pub fn body_type(mut self, body_type: BodyType) -> Self {
        self.body_type = body_type;
        self
    }

    pub fn raw_body(mut self, raw_body: impl Into<String>) -> Self {
        self.raw_body = raw_body.into();
        self
    }

    pub fn form_data(mut self, form_data: Vec<FormDataRow>) -> Self {
        self.form_data = form_data;
        self
    }

    pub fn binary_file_path(mut self, path: Option<String>) -> Self {
        self.binary_file_path = path;
        self
    }

    pub fn headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.headers = headers;
        self
    }

    pub fn cookies(mut self, cookies: Vec<(String, String)>) -> Self {
        self.cookies = cookies;
        self
    }

    pub fn auth_raw(mut self, auth_raw: impl Into<String>) -> Self {
        self.auth_raw = auth_raw.into();
        self
    }

    #[allow(dead_code)]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

fn map_method(method: &HttpMethod) -> Result<reqwest::Method, String> {
    Ok(match method {
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
    })
}

fn apply_headers(
    mut builder: reqwest::RequestBuilder,
    headers: Vec<(String, String)>,
) -> reqwest::RequestBuilder {
    // filter out completely blank header keys
    for (key, val) in headers {
        if !key.trim().is_empty() {
            builder = builder.header(key.trim(), val);
        }
    }
    builder
}

fn build_cookie_header(cookies: Vec<(String, String)>) -> Option<String> {
    let formatted: String = cookies
        .into_iter()
        .filter(|(key, _)| !key.trim().is_empty())
        .map(|(key, val)| format!("{}={}", key.trim(), val.trim()))
        .collect::<Vec<String>>()
        .join("; ");

    if formatted.is_empty() {
        None
    } else {
        Some(formatted)
    }
}

fn apply_auth(builder: reqwest::RequestBuilder, auth_raw: &str) -> reqwest::RequestBuilder {
    let trimmed = auth_raw.trim();
    if trimmed.is_empty() {
        builder
    } else {
        builder.header("Authorization", trimmed)
    }
}

/// Builds a multipart form from active rows, reading any file-typed rows from disk
async fn build_form_data(
    form_data_list: Vec<FormDataRow>,
) -> Result<Option<reqwest::multipart::Form>, String> {
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

                        let part = reqwest::multipart::Part::bytes(file_bytes).file_name(file_name);

                        form = form.part(row.key, part);
                    }
                }
            }
        }
    }

    Ok(if has_fields { Some(form) } else { None })
}

/// Reads the binary body file from disk, if a path was given and it exists.
async fn build_binary_body(binary_file_path: &Option<String>) -> Result<Option<Vec<u8>>, String> {
    if let Some(path_str) = binary_file_path {
        let path = Path::new(path_str);
        if path.exists() {
            let file_bytes = tokio::fs::read(path)
                .await
                .map_err(|e| format!("Binary File Read Failure: {}", e))?;
            return Ok(Some(file_bytes));
        }
    }
    Ok(None)
}

/// Attaches the request body appropriate to `body_type`
fn apply_body(
    builder: reqwest::RequestBuilder,
    method: &HttpMethod,
    body_type: BodyType,
    raw_body: String,
    form: Option<reqwest::multipart::Form>,
    binary: Option<Vec<u8>>,
) -> reqwest::RequestBuilder {
    // no-op for GET/HEAD requests
    if *method == HttpMethod::GET || *method == HttpMethod::HEAD {
        return builder;
    }
    match body_type {
        BodyType::FormData => match form {
            Some(form) => builder.multipart(form),
            None => builder,
        },
        BodyType::Binary => match binary {
            Some(bytes) => builder.body(bytes),
            None => builder,
        },
        // fallback text states (raw JSON, URLencoded forms, etc.)
        _ => {
            if raw_body.trim().is_empty() {
                builder
            } else {
                builder.body(raw_body)
            }
        }
    }
}

async fn cancellable<T>(
    fut: impl Future<Output = Result<T, reqwest::Error>>,
    cancel_token: &CancellationToken,
    err_prefix: &str,
) -> Result<T, String> {
    tokio::select! {
        res = fut => res.map_err(|e| format!("{}: {}", err_prefix, e)),
        _ = cancel_token.cancelled() => Err(String::from("Request cancelled by user.")),
    }
}

fn shape_response(
    status: u16,
    header_map: reqwest::header::HeaderMap,
    body_text: String,
    elapsed: Duration,
) -> HttpResponse {
    let mut headers = HashMap::new();
    for (key, value) in header_map.iter() {
        if let Ok(val_str) = value.to_str() {
            headers.insert(key.to_string(), val_str.to_string());
        }
    }

    let finalized_body = if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&body_text)
    {
        serde_json::to_string_pretty(&json_val).unwrap_or(body_text)
    } else {
        body_text
    };

    HttpResponse {
        status,
        body: finalized_body,
        headers,
        elapsed,
        test_results: Vec::new(),
    }
}

pub async fn send_request(
    spec: RequestSpec,
    cancel_token: CancellationToken,
) -> Result<HttpResponse, String> {
    let reqwest_url =
        reqwest::Url::parse(&spec.url).map_err(|e| format!("Invalid URL pattern: {}", e))?;

    let client = reqwest::Client::builder()
        .timeout(spec.timeout)
        .build()
        .map_err(|e| format!("Failed to initialize client: {}", e))?;

    let req_method = map_method(&spec.method)?;
    let mut req_builder = client.request(req_method, reqwest_url);

    req_builder = apply_headers(req_builder, spec.headers);
    if let Some(cookie_header) = build_cookie_header(spec.cookies) {
        req_builder = req_builder.header("Cookie", cookie_header);
    }
    req_builder = apply_auth(req_builder, &spec.auth_raw);

    if spec.method != HttpMethod::GET && spec.method != HttpMethod::HEAD {
        let form = if spec.body_type == BodyType::FormData {
            build_form_data(spec.form_data).await?
        } else {
            None
        };
        let binary = if spec.body_type == BodyType::Binary {
            build_binary_body(&spec.binary_file_path).await?
        } else {
            None
        };
        req_builder = apply_body(
            req_builder,
            &spec.method,
            spec.body_type,
            spec.raw_body,
            form,
            binary,
        );
    }

    let start_time = Instant::now();

    let response = cancellable(req_builder.send(), &cancel_token, "Network Dispatch Error").await?;

    let elapsed = start_time.elapsed();
    let status = response.status().as_u16();
    let headers = response.headers().clone();

    let body_text = cancellable(response.text(), &cancel_token, "Payload Parsing Error").await?;

    Ok(shape_response(status, headers, body_text, elapsed))
}
