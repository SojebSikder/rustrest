use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
}

impl HttpMethod {
    /// Helper to provide an iterable list of variants for UI PickLists
    pub const ALL: [Self; 4] = [Self::GET, Self::POST, Self::PUT, Self::DELETE];
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            HttpMethod::GET => "GET",
            HttpMethod::POST => "POST",
            HttpMethod::PUT => "PUT",
            HttpMethod::DELETE => "DELETE",
        };
        write!(f, "{}", s)
    }
}

/// Asynchronous, reusable network request function returning an Iced Command
pub async fn send_request(url: String, method: HttpMethod) -> Result<String, String> {
    let client = reqwest::Client::new();

    let req = match method {
        HttpMethod::GET => client.get(&url),
        HttpMethod::POST => client.post(&url).body("{}"),
        HttpMethod::PUT => client.put(&url).body("{}"),
        HttpMethod::DELETE => client.delete(&url),
    };

    let response = req
        .send()
        .await
        .map_err(|e| format!("Network Error: {}", e))?;
    response
        .text()
        .await
        .map_err(|e| format!("Reading Body Error: {}", e))
}
