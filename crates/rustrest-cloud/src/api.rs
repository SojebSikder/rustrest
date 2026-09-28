//! typed client for the cloud REST API. access tokens are refreshed transparently on a 401,
//! read `session()` afterwards to persist the rotated tokens.

use std::sync::{Arc, Mutex};

use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::wire::*;

#[derive(Debug, Clone)]
pub enum CloudError {
    /// not signed in, or the session expired and couldn't be refreshed
    Unauthorized,
    /// a batch was rejected; nothing in it was applied
    Conflicts(Vec<WireConflict>),
    /// the collection meta changed under us; carries the server's copy
    MetaConflict(CloudCollection),
    /// any other non-2xx answer
    Api {
        status: u16,
        message: String,
    },
    Network(String),
    Decode(String),
}

impl std::fmt::Display for CloudError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloudError::Unauthorized => write!(f, "Not signed in to Rustrest Cloud"),
            CloudError::Conflicts(c) => {
                write!(f, "{} change(s) conflicted with the server", c.len())
            }
            CloudError::MetaConflict(_) => write!(f, "Collection settings changed on the server"),
            CloudError::Api { status, message } => write!(f, "Cloud error {status}: {message}"),
            CloudError::Network(e) => write!(f, "Couldn't reach Rustrest Cloud: {e}"),
            CloudError::Decode(e) => write!(f, "Unexpected response from Rustrest Cloud: {e}"),
        }
    }
}

impl std::error::Error for CloudError {}

impl CloudError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, CloudError::Api { status: 404, .. })
    }
}

#[derive(Clone)]
pub struct CloudClient {
    base_url: String,
    http: reqwest::Client,
    session: Arc<Mutex<Option<Session>>>,
}

// never prints the tokens
impl std::fmt::Debug for CloudClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudClient")
            .field("base_url", &self.base_url)
            .field("signed_in", &self.is_signed_in())
            .finish()
    }
}

impl CloudClient {
    pub fn new(base_url: &str, session: Option<Session>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            session: Arc::new(Mutex::new(session)),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// the current tokens (they rotate on every refresh)
    pub fn session(&self) -> Option<Session> {
        self.session.lock().unwrap().clone()
    }

    pub fn is_signed_in(&self) -> bool {
        self.session.lock().unwrap().is_some()
    }

    fn access_token(&self) -> Option<String> {
        self.session
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.access_token.clone())
    }

    /// websocket endpoint for realtime events
    pub fn ws_url(&self) -> String {
        let url = if let Some(rest) = self.base_url.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = self.base_url.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            self.base_url.clone()
        };
        format!("{url}/api/ws")
    }

    // ---- auth ----

    pub async fn register(
        &self,
        name: &str,
        email: &str,
        password: &str,
    ) -> Result<(), CloudError> {
        let body = serde_json::json!({ "name": name, "email": email, "password": password });
        let (status, text) = self
            .send(Method::POST, "/api/auth/register", Some(&body), None)
            .await?;
        check::<Value>(status, &text).map(|_| ())
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<Session, CloudError> {
        let body = serde_json::json!({ "email": email, "password": password });
        let (status, text) = self
            .send(Method::POST, "/api/auth/login", Some(&body), None)
            .await?;
        if status == StatusCode::UNAUTHORIZED {
            return Err(CloudError::Api {
                status: 401,
                message: "Invalid email or password".to_string(),
            });
        }

        let tokens: TokenPair = required(check(status, &text)?)?;
        let session = Session {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
        };
        *self.session.lock().unwrap() = Some(session.clone());
        Ok(session)
    }

    /// revokes the refresh token and forgets the session
    pub async fn logout(&self) -> Result<(), CloudError> {
        let Some(session) = self.session.lock().unwrap().take() else {
            return Ok(());
        };
        let body = serde_json::json!({ "refresh_token": session.refresh_token });
        let (status, text) = self
            .send(Method::POST, "/api/auth/logout", Some(&body), None)
            .await?;
        check::<Value>(status, &text).map(|_| ())
    }

    async fn refresh(&self) -> Result<(), CloudError> {
        let Some(refresh_token) = self
            .session
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.refresh_token.clone())
        else {
            return Err(CloudError::Unauthorized);
        };

        let body = serde_json::json!({ "refresh_token": refresh_token });
        let (status, text) = self
            .send(Method::POST, "/api/auth/refresh", Some(&body), None)
            .await?;
        if !status.is_success() {
            *self.session.lock().unwrap() = None;
            return Err(CloudError::Unauthorized);
        }

        let tokens: TokenPair = required(check(status, &text)?)?;
        *self.session.lock().unwrap() = Some(Session {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
        });
        Ok(())
    }

    /// makes sure the access token is fresh and returns it
    pub async fn fresh_access_token(&self) -> Result<String, CloudError> {
        match self.me().await {
            Ok(_) => self.access_token().ok_or(CloudError::Unauthorized),
            Err(err) => Err(err),
        }
    }

    pub async fn me(&self) -> Result<User, CloudError> {
        self.get("/api/auth/me").await
    }

    // ---- teams ----

    pub async fn teams(&self) -> Result<Vec<Team>, CloudError> {
        self.get("/api/teams").await
    }

    pub async fn create_team(&self, name: &str) -> Result<Team, CloudError> {
        self.call(
            Method::POST,
            "/api/teams",
            Some(&serde_json::json!({ "name": name })),
        )
        .await
    }

    pub async fn members(&self, team_id: &str) -> Result<Vec<TeamMember>, CloudError> {
        self.get(&format!("/api/teams/{team_id}/members")).await
    }

    pub async fn add_member(
        &self,
        team_id: &str,
        email: &str,
        role: Role,
    ) -> Result<(), CloudError> {
        let body = serde_json::json!({ "email": email, "role": role });
        self.call::<Value, _>(
            Method::POST,
            &format!("/api/teams/{team_id}/members"),
            Some(&body),
        )
        .await
        .map(|_| ())
    }

    pub async fn remove_member(&self, team_id: &str, user_id: &str) -> Result<(), CloudError> {
        self.call::<Value, ()>(
            Method::DELETE,
            &format!("/api/teams/{team_id}/members/{user_id}"),
            None,
        )
        .await
        .map(|_| ())
    }

    // ---- collections ----

    pub async fn collections(&self, team_id: &str) -> Result<Vec<CloudCollection>, CloudError> {
        self.get(&format!("/api/teams/{team_id}/collections")).await
    }

    pub async fn create_collection(
        &self,
        team_id: &str,
        name: &str,
        meta: &Value,
        items: &[NewItem],
    ) -> Result<Snapshot, CloudError> {
        let body = CreateCollection { name, meta, items };
        self.call(
            Method::POST,
            &format!("/api/teams/{team_id}/collections"),
            Some(&body),
        )
        .await
    }

    pub async fn snapshot(&self, collection_id: &str) -> Result<Snapshot, CloudError> {
        self.get(&format!("/api/collections/{collection_id}")).await
    }

    pub async fn changes(&self, collection_id: &str, since: i64) -> Result<ChangeSet, CloudError> {
        self.get(&format!(
            "/api/collections/{collection_id}/changes?since={since}"
        ))
        .await
    }

    pub async fn history(
        &self,
        collection_id: &str,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>, CloudError> {
        self.get(&format!(
            "/api/collections/{collection_id}/history?limit={limit}"
        ))
        .await
    }

    pub async fn batch(
        &self,
        collection_id: &str,
        ops: &[ItemOp],
    ) -> Result<BatchResult, CloudError> {
        let path = format!("/api/collections/{collection_id}/batch");
        let (status, text) = self
            .authed(Method::POST, &path, Some(&Batch { ops }))
            .await?;
        if status == StatusCode::CONFLICT {
            let body: ConflictBody = required(envelope(&text)?.data)?;
            return Err(CloudError::Conflicts(body.conflicts));
        }
        required(check(status, &text)?)
    }

    pub async fn update_collection(
        &self,
        collection_id: &str,
        base_rev: i64,
        name: &str,
        meta: &Value,
    ) -> Result<CloudCollection, CloudError> {
        let path = format!("/api/collections/{collection_id}");
        let body = UpdateCollection {
            base_rev,
            name,
            meta,
        };
        let (status, text) = self.authed(Method::PATCH, &path, Some(&body)).await?;
        if status == StatusCode::CONFLICT {
            return Err(CloudError::MetaConflict(required(envelope(&text)?.data)?));
        }
        required(check(status, &text)?)
    }

    pub async fn delete_collection(&self, collection_id: &str) -> Result<(), CloudError> {
        self.call::<Value, ()>(
            Method::DELETE,
            &format!("/api/collections/{collection_id}"),
            None,
        )
        .await
        .map(|_| ())
    }

    // ---- environments ----

    pub async fn environments(&self, team_id: &str) -> Result<Vec<CloudEnvironment>, CloudError> {
        self.get(&format!("/api/teams/{team_id}/environments"))
            .await
    }

    pub async fn create_environment(
        &self,
        team_id: &str,
        name: &str,
        data: &Value,
    ) -> Result<CloudEnvironment, CloudError> {
        let body = EnvironmentBody {
            id: None,
            name,
            data,
            base_rev: 0,
        };
        self.call(
            Method::POST,
            &format!("/api/teams/{team_id}/environments"),
            Some(&body),
        )
        .await
    }

    pub async fn update_environment(
        &self,
        id: &str,
        base_rev: i64,
        name: &str,
        data: &Value,
    ) -> Result<CloudEnvironment, CloudError> {
        let body = EnvironmentBody {
            id: None,
            name,
            data,
            base_rev,
        };
        self.call(Method::PUT, &format!("/api/environments/{id}"), Some(&body))
            .await
    }

    pub async fn delete_environment(&self, id: &str) -> Result<(), CloudError> {
        self.call::<Value, ()>(Method::DELETE, &format!("/api/environments/{id}"), None)
            .await
            .map(|_| ())
    }

    // ---- plumbing ----

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, CloudError> {
        self.call::<T, ()>(Method::GET, path, None).await
    }

    async fn call<T: DeserializeOwned, B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, CloudError> {
        let (status, text) = self.authed(method, path, body).await?;
        let data = check::<T>(status, &text)?;
        // DELETEs and some POSTs answer without data
        match data {
            Some(data) => Ok(data),
            None => {
                serde_json::from_value(Value::Null).map_err(|e| CloudError::Decode(e.to_string()))
            }
        }
    }

    /// sends with the access token, refreshing once on a 401
    async fn authed<B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<(StatusCode, String), CloudError> {
        let token = self.access_token().ok_or(CloudError::Unauthorized)?;
        let (status, text) = self.send(method.clone(), path, body, Some(&token)).await?;
        if status != StatusCode::UNAUTHORIZED {
            return Ok((status, text));
        }
        self.refresh().await?;
        let token = self.access_token().ok_or(CloudError::Unauthorized)?;
        let (status, text) = self.send(method, path, body, Some(&token)).await?;
        if status == StatusCode::UNAUTHORIZED {
            return Err(CloudError::Unauthorized);
        }
        Ok((status, text))
    }

    async fn send<B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        token: Option<&str>,
    ) -> Result<(StatusCode, String), CloudError> {
        let mut req = self
            .http
            .request(method, format!("{}{}", self.base_url, path));
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        let res = req
            .send()
            .await
            .map_err(|e| CloudError::Network(e.to_string()))?;
        let status = res.status();
        let text = res
            .text()
            .await
            .map_err(|e| CloudError::Network(e.to_string()))?;
        Ok((status, text))
    }
}

fn envelope<T: DeserializeOwned>(text: &str) -> Result<Envelope<T>, CloudError> {
    serde_json::from_str(text).map_err(|e| CloudError::Decode(format!("{e}: {text:.200}")))
}

/// unwraps a 2xx envelope's data, or turns the error envelope into a CloudError
fn check<T: DeserializeOwned>(status: StatusCode, text: &str) -> Result<Option<T>, CloudError> {
    if status.is_success() {
        return Ok(envelope::<T>(text)?.data);
    }
    let message = serde_json::from_str::<Envelope<Value>>(text)
        .map(|e| e.message)
        .unwrap_or_else(|_| text.chars().take(200).collect());
    if status == StatusCode::UNAUTHORIZED {
        return Err(CloudError::Unauthorized);
    }
    Err(CloudError::Api {
        status: status.as_u16(),
        message,
    })
}

fn required<T>(data: Option<T>) -> Result<T, CloudError> {
    data.ok_or_else(|| CloudError::Decode("response had no data".to_string()))
}
