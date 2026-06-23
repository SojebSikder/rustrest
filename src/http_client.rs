use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
}

impl HttpMethod {
    pub const ALL: [Self; 5] = [Self::GET, Self::POST, Self::PUT, Self::DELETE, Self::PATCH];
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    pub headers: HashMap<String, String>,
    pub elapsed: Duration,
}

pub async fn send_request(
    url: String,
    method: HttpMethod,
    body: String,
    headers_list: Vec<(String, String)>,
    auth_raw: String,
) -> Result<HttpResponse, String> {
    let reqwest_url =
        reqwest::Url::parse(&url).map_err(|e| format!("Invalid URL pattern: {}", e))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to initialize client: {}", e))?;

    let req_method = match method {
        HttpMethod::GET => reqwest::Method::GET,
        HttpMethod::POST => reqwest::Method::POST,
        HttpMethod::PUT => reqwest::Method::PUT,
        HttpMethod::DELETE => reqwest::Method::DELETE,
        HttpMethod::PATCH => reqwest::Method::PATCH,
    };

    let mut req_builder = client.request(req_method, reqwest_url);

    // append filtered clean header key-value items
    for (key, val) in headers_list {
        req_builder = req_builder.header(key, val);
    }

    // append authorization header if specified
    let auth_trimmed = auth_raw.trim();
    if !auth_trimmed.is_empty() {
        req_builder = req_builder.header("Authorization", auth_trimmed);
    }

    if method != HttpMethod::GET && method != HttpMethod::DELETE && !body.trim().is_empty() {
        req_builder = req_builder.body(body);
    }

    let start_time = Instant::now();
    let response = req_builder
        .send()
        .await
        .map_err(|e| format!("Network Dispatch Error: {}", e))?;
    let elapsed = start_time.elapsed();

    let status = response.status().as_u16();
    let mut headers = HashMap::new();
    for (key, value) in response.headers().iter() {
        if let Ok(val_str) = value.to_str() {
            headers.insert(key.to_string(), val_str.to_string());
        }
    }

    let body_text = response
        .text()
        .await
        .map_err(|e| format!("Payload Parsing Error: {}", e))?;

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
