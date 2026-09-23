//! Structured request authorization: the `RequestAuth` model, applying it to
//! an outgoing request (computing the headers/query params it contributes),
//! JWT signing, OAuth 1.0 request signing, and the OAuth 2.0 client-credentials
//! token exchange.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Sha256, Sha384, Sha512};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AuthType {
    #[default]
    NoAuth,
    /// a raw `Authorization` header value, sent verbatim - what every
    /// collection saved before structured auth existed used, so it's kept
    /// as a real auth type (not just a migration artifact).
    Custom,
    Bearer,
    ApiKey,
    Basic,
    JwtBearer,
    OAuth1,
    OAuth2,
}

impl AuthType {
    pub const ALL: [Self; 8] = [
        Self::NoAuth,
        Self::Bearer,
        Self::ApiKey,
        Self::Basic,
        Self::JwtBearer,
        Self::OAuth1,
        Self::OAuth2,
        Self::Custom,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::NoAuth => "No Auth",
            Self::Custom => "Custom Header",
            Self::Bearer => "Bearer Token",
            Self::ApiKey => "API Key",
            Self::Basic => "Basic Auth",
            Self::JwtBearer => "JWT Bearer",
            Self::OAuth1 => "OAuth 1.0",
            Self::OAuth2 => "OAuth 2.0",
        }
    }
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// where a computed auth value gets placed on the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AuthLocation {
    #[default]
    Header,
    Query,
}

impl AuthLocation {
    pub const ALL: [Self; 2] = [Self::Header, Self::Query];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Header => "Header",
            Self::Query => "Query Params",
        }
    }
}

impl std::fmt::Display for AuthLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum JwtAlgorithm {
    #[default]
    Hs256,
    Hs384,
    Hs512,
}

impl JwtAlgorithm {
    pub const ALL: [Self; 3] = [Self::Hs256, Self::Hs384, Self::Hs512];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Hs256 => "HS256",
            Self::Hs384 => "HS384",
            Self::Hs512 => "HS512",
        }
    }
}

impl std::fmt::Display for JwtAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// OAuth 1.0 signature methods; RSA-SHA1 (asymmetric, needs a private key)
/// isn't supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OAuth1SignatureMethod {
    #[default]
    HmacSha1,
    PlainText,
}

impl OAuth1SignatureMethod {
    pub const ALL: [Self; 2] = [Self::HmacSha1, Self::PlainText];

    /// the literal `oauth_signature_method` value, per RFC 5849.
    pub fn label(&self) -> &'static str {
        match self {
            Self::HmacSha1 => "HMAC-SHA1",
            Self::PlainText => "PLAINTEXT",
        }
    }
}

impl std::fmt::Display for OAuth1SignatureMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OAuth2GrantType {
    #[default]
    Manual,
    ClientCredentials,
}

impl OAuth2GrantType {
    pub const ALL: [Self; 2] = [Self::Manual, Self::ClientCredentials];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Manual => "Manual Token",
            Self::ClientCredentials => "Client Credentials",
        }
    }
}

impl std::fmt::Display for OAuth2GrantType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// how the OAuth 2.0 client credentials authenticate against the token
/// endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ClientAuthStyle {
    #[default]
    BasicAuthHeader,
    RequestBody,
}

impl ClientAuthStyle {
    pub const ALL: [Self; 2] = [Self::BasicAuthHeader, Self::RequestBody];

    pub fn label(&self) -> &'static str {
        match self {
            Self::BasicAuthHeader => "Send as Basic Auth header",
            Self::RequestBody => "Send in request body",
        }
    }
}

impl std::fmt::Display for ClientAuthStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RequestAuth {
    pub auth_type: AuthType,

    /// `AuthType::Custom`
    pub custom_raw: String,

    /// `AuthType::Bearer`
    pub bearer_token: String,

    /// `AuthType::ApiKey`
    pub api_key_key: String,
    pub api_key_value: String,
    pub api_key_add_to: AuthLocation,

    /// `AuthType::Basic`
    pub basic_username: String,
    pub basic_password: String,

    /// `AuthType::JwtBearer`
    pub jwt_algorithm: JwtAlgorithm,
    pub jwt_secret: String,
    /// JSON payload/claims, signed and embedded in the token.
    pub jwt_payload: String,
    pub jwt_header_prefix: String,
    pub jwt_add_to: AuthLocation,

    /// `AuthType::OAuth1`
    pub oauth1_signature_method: OAuth1SignatureMethod,
    pub oauth1_consumer_key: String,
    pub oauth1_consumer_secret: String,
    pub oauth1_token: String,
    pub oauth1_token_secret: String,
    pub oauth1_realm: String,
    pub oauth1_add_to: AuthLocation,

    /// `AuthType::OAuth2`
    pub oauth2_grant_type: OAuth2GrantType,
    /// the token actually sent - either pasted manually, or the result of
    /// the last successful client-credentials token exchange.
    pub oauth2_access_token: String,
    pub oauth2_header_prefix: String,
    pub oauth2_add_to: AuthLocation,
    pub oauth2_token_url: String,
    pub oauth2_client_id: String,
    pub oauth2_client_secret: String,
    pub oauth2_scope: String,
    pub oauth2_client_auth: ClientAuthStyle,
}

impl Default for RequestAuth {
    fn default() -> Self {
        Self {
            auth_type: AuthType::NoAuth,
            custom_raw: String::new(),
            bearer_token: String::new(),
            api_key_key: String::new(),
            api_key_value: String::new(),
            api_key_add_to: AuthLocation::Header,
            basic_username: String::new(),
            basic_password: String::new(),
            jwt_algorithm: JwtAlgorithm::Hs256,
            jwt_secret: String::new(),
            jwt_payload: "{}".to_string(),
            jwt_header_prefix: "Bearer".to_string(),
            jwt_add_to: AuthLocation::Header,
            oauth1_signature_method: OAuth1SignatureMethod::HmacSha1,
            oauth1_consumer_key: String::new(),
            oauth1_consumer_secret: String::new(),
            oauth1_token: String::new(),
            oauth1_token_secret: String::new(),
            oauth1_realm: String::new(),
            oauth1_add_to: AuthLocation::Header,
            oauth2_grant_type: OAuth2GrantType::Manual,
            oauth2_access_token: String::new(),
            oauth2_header_prefix: "Bearer".to_string(),
            oauth2_add_to: AuthLocation::Header,
            oauth2_token_url: String::new(),
            oauth2_client_id: String::new(),
            oauth2_client_secret: String::new(),
            oauth2_scope: String::new(),
            oauth2_client_auth: ClientAuthStyle::BasicAuthHeader,
        }
    }
}

impl RequestAuth {
    /// wraps a pre-existing raw `Authorization` header value (from a
    /// collection saved before structured auth existed) as `Custom`.
    pub fn from_legacy_string(raw: String) -> Self {
        Self {
            auth_type: AuthType::Custom,
            custom_raw: raw,
            ..Default::default()
        }
    }
}

/// the extra headers/query params a `RequestAuth` contributes to a request.
#[derive(Debug, Clone, Default)]
pub struct AppliedAuth {
    pub headers: Vec<(String, String)>,
    pub query_params: Vec<(String, String)>,
}

impl RequestAuth {
    /// computes the header(s)/query param(s) this auth config contributes to
    /// a request. `method` is the HTTP verb (e.g. "GET"); `url` is the
    /// fully-resolved request URL (env vars already substituted, existing
    /// query params already present) - needed for OAuth 1.0 signing, which
    /// signs over the method, URL, and params. The only failure case is a
    /// malformed OAuth 1.0 URL or an invalid JWT payload.
    pub fn apply(&self, method: &str, url: &str) -> Result<AppliedAuth, String> {
        let mut applied = AppliedAuth::default();
        match self.auth_type {
            AuthType::NoAuth => {}
            AuthType::Custom => {
                let trimmed = self.custom_raw.trim();
                if !trimmed.is_empty() {
                    applied
                        .headers
                        .push(("Authorization".to_string(), trimmed.to_string()));
                }
            }
            AuthType::Bearer => {
                let token = self.bearer_token.trim();
                if !token.is_empty() {
                    applied
                        .headers
                        .push(("Authorization".to_string(), format!("Bearer {token}")));
                }
            }
            AuthType::ApiKey => {
                if !self.api_key_key.trim().is_empty() {
                    let pair = (self.api_key_key.clone(), self.api_key_value.clone());
                    match self.api_key_add_to {
                        AuthLocation::Header => applied.headers.push(pair),
                        AuthLocation::Query => applied.query_params.push(pair),
                    }
                }
            }
            AuthType::Basic => {
                let token = BASE64_STANDARD
                    .encode(format!("{}:{}", self.basic_username, self.basic_password));
                applied
                    .headers
                    .push(("Authorization".to_string(), format!("Basic {token}")));
            }
            AuthType::JwtBearer => {
                let token = sign_jwt(self.jwt_algorithm, &self.jwt_secret, &self.jwt_payload)?;
                let prefix = self.jwt_header_prefix.trim();
                let value = if prefix.is_empty() {
                    token.clone()
                } else {
                    format!("{prefix} {token}")
                };
                match self.jwt_add_to {
                    AuthLocation::Header => {
                        applied.headers.push(("Authorization".to_string(), value))
                    }
                    AuthLocation::Query => applied.query_params.push(("token".to_string(), token)),
                }
            }
            AuthType::OAuth1 => {
                let signed = sign_oauth1(self, method, url)?;
                match self.oauth1_add_to {
                    AuthLocation::Header => applied
                        .headers
                        .push(("Authorization".to_string(), signed.authorization_header)),
                    AuthLocation::Query => applied.query_params.extend(signed.params),
                }
            }
            AuthType::OAuth2 => {
                let token = self.oauth2_access_token.trim();
                if !token.is_empty() {
                    let prefix = self.oauth2_header_prefix.trim();
                    let value = if prefix.is_empty() {
                        token.to_string()
                    } else {
                        format!("{prefix} {token}")
                    };
                    match self.oauth2_add_to {
                        AuthLocation::Header => {
                            applied.headers.push(("Authorization".to_string(), value))
                        }
                        AuthLocation::Query => applied
                            .query_params
                            .push(("access_token".to_string(), token.to_string())),
                    }
                }
            }
        }
        Ok(applied)
    }
}

// ---- JWT (HS256/HS384/HS512) ----

fn base64url(bytes: &[u8]) -> String {
    BASE64_URL_SAFE_NO_PAD.encode(bytes)
}

fn sign_jwt(algorithm: JwtAlgorithm, secret: &str, payload_json: &str) -> Result<String, String> {
    // validate the payload is actually JSON before embedding it, so a typo
    // fails fast with a clear message instead of silently producing a token
    // no server will accept.
    let payload_value: serde_json::Value = serde_json::from_str(payload_json.trim())
        .map_err(|e| format!("JWT payload isn't valid JSON: {e}"))?;
    let payload_compact = serde_json::to_string(&payload_value).map_err(|e| e.to_string())?;

    let header_json = format!(r#"{{"alg":"{}","typ":"JWT"}}"#, algorithm.label());
    let signing_input = format!(
        "{}.{}",
        base64url(header_json.as_bytes()),
        base64url(payload_compact.as_bytes())
    );

    let signature = match algorithm {
        JwtAlgorithm::Hs256 => hmac_sha256(secret.as_bytes(), signing_input.as_bytes()),
        JwtAlgorithm::Hs384 => hmac_sha384(secret.as_bytes(), signing_input.as_bytes()),
        JwtAlgorithm::Hs512 => hmac_sha512(secret.as_bytes(), signing_input.as_bytes()),
    };

    Ok(format!("{signing_input}.{}", base64url(&signature)))
}

// HMAC has no key-length restriction (RFC 2104: too-long keys are hashed
// down, too-short ones are zero-padded), so `new_from_slice` never fails
// here regardless of what the user typed as a secret.

fn hmac_sha1(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("HMAC accepts a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha384(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha384>::new_from_slice(key).expect("HMAC accepts a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha512(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha512>::new_from_slice(key).expect("HMAC accepts a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

// ---- OAuth 1.0 request signing (RFC 5849) ----

const OAUTH1_ENCODE_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

fn oauth1_percent_encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, OAUTH1_ENCODE_SET).to_string()
}

struct SignedOAuth1 {
    /// the full `Authorization: OAuth ...` header value.
    authorization_header: String,
    /// the same oauth_* params, for when they're added as query params
    /// instead of a header.
    params: Vec<(String, String)>,
}

fn oauth1_nonce() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
        .collect()
}

fn oauth1_base_string(method: &str, base_url: &str, params: &[(String, String)]) -> String {
    let mut encoded: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| (oauth1_percent_encode(k), oauth1_percent_encode(v)))
        .collect();
    encoded.sort();
    let param_string = encoded
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    format!(
        "{}&{}&{}",
        method.to_uppercase(),
        oauth1_percent_encode(base_url),
        oauth1_percent_encode(&param_string)
    )
}

fn sign_oauth1(auth: &RequestAuth, method: &str, url: &str) -> Result<SignedOAuth1, String> {
    let parsed =
        url::Url::parse(url).map_err(|e| format!("Invalid URL for OAuth 1.0 signing: {e}"))?;

    // the base URL for signing purposes: scheme://host[:non-default-port]/path,
    // no query string or fragment (RFC 5849 3.4.1.2).
    let mut base_url = format!(
        "{}://{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or_default()
    );
    if let Some(port) = parsed.port() {
        let is_default_port = matches!((parsed.scheme(), port), ("http", 80) | ("https", 443));
        if !is_default_port {
            base_url.push_str(&format!(":{port}"));
        }
    }
    base_url.push_str(parsed.path());

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string();

    let mut oauth_params: Vec<(String, String)> = vec![
        (
            "oauth_consumer_key".to_string(),
            auth.oauth1_consumer_key.clone(),
        ),
        ("oauth_nonce".to_string(), oauth1_nonce()),
        (
            "oauth_signature_method".to_string(),
            auth.oauth1_signature_method.label().to_string(),
        ),
        ("oauth_timestamp".to_string(), timestamp),
        ("oauth_version".to_string(), "1.0".to_string()),
    ];
    if !auth.oauth1_token.trim().is_empty() {
        oauth_params.push(("oauth_token".to_string(), auth.oauth1_token.clone()));
    }

    let signing_key = format!(
        "{}&{}",
        oauth1_percent_encode(&auth.oauth1_consumer_secret),
        oauth1_percent_encode(&auth.oauth1_token_secret)
    );

    let signature = match auth.oauth1_signature_method {
        OAuth1SignatureMethod::PlainText => signing_key.clone(),
        OAuth1SignatureMethod::HmacSha1 => {
            // signs over the existing query params (from the URL) plus the
            // oauth_* params above
            let mut all_params: Vec<(String, String)> = parsed
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            all_params.extend(oauth_params.iter().cloned());
            let base_string = oauth1_base_string(method, &base_url, &all_params);
            BASE64_STANDARD.encode(hmac_sha1(signing_key.as_bytes(), base_string.as_bytes()))
        }
    };
    oauth_params.push(("oauth_signature".to_string(), signature));

    let mut header_parts: Vec<String> = Vec::new();
    let realm = auth.oauth1_realm.trim();
    if !realm.is_empty() {
        header_parts.push(format!(r#"realm="{}""#, oauth1_percent_encode(realm)));
    }
    for (k, v) in &oauth_params {
        header_parts.push(format!(
            r#"{}="{}""#,
            oauth1_percent_encode(k),
            oauth1_percent_encode(v)
        ));
    }

    Ok(SignedOAuth1 {
        authorization_header: format!("OAuth {}", header_parts.join(", ")),
        params: oauth_params,
    })
}

// ---- OAuth 2.0 client-credentials token exchange ----

#[derive(Debug, Clone, Deserialize)]
pub struct OAuth2TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
}

/// runs the OAuth 2.0 "Client Credentials" grant (RFC 6749) against
/// `token_url`, a single `POST` with `grant_type=client_credentials`, authenticated
/// either via HTTP Basic or by putting the client id/secret in the body.
pub async fn fetch_oauth2_client_credentials_token(
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    scope: &str,
    client_auth: ClientAuthStyle,
) -> Result<OAuth2TokenResponse, String> {
    let mut form: Vec<(&str, &str)> = vec![("grant_type", "client_credentials")];
    let trimmed_scope = scope.trim();
    if !trimmed_scope.is_empty() {
        form.push(("scope", trimmed_scope));
    }
    if matches!(client_auth, ClientAuthStyle::RequestBody) {
        form.push(("client_id", client_id));
        form.push(("client_secret", client_secret));
    }

    let client = reqwest::Client::new();
    let mut request = client.post(token_url).form(&form);
    if matches!(client_auth, ClientAuthStyle::BasicAuthHeader) {
        request = request.basic_auth(client_id, Some(client_secret));
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Token request failed: {e}"))?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read token response: {e}"))?;

    if !status.is_success() {
        return Err(format!("Token endpoint returned {status}: {body_text}"));
    }

    serde_json::from_str::<OAuth2TokenResponse>(&body_text)
        .map_err(|e| format!("Couldn't parse token response as JSON: {e} (body: {body_text})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_auth_produces_nothing() {
        let auth = RequestAuth::default();
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert!(applied.headers.is_empty());
        assert!(applied.query_params.is_empty());
    }

    #[test]
    fn bearer_sends_authorization_header() {
        let auth = RequestAuth {
            auth_type: AuthType::Bearer,
            bearer_token: "abc123".to_string(),
            ..Default::default()
        };
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert_eq!(
            applied.headers,
            vec![("Authorization".to_string(), "Bearer abc123".to_string())]
        );
    }

    #[test]
    fn custom_sends_the_raw_value_verbatim() {
        let auth = RequestAuth::from_legacy_string("Bearer legacy-token".to_string());
        assert_eq!(auth.auth_type, AuthType::Custom);
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert_eq!(
            applied.headers,
            vec![(
                "Authorization".to_string(),
                "Bearer legacy-token".to_string()
            )]
        );
    }

    #[test]
    fn custom_with_blank_text_sends_no_header() {
        let auth = RequestAuth::from_legacy_string("   ".to_string());
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert!(applied.headers.is_empty());
    }

    #[test]
    fn basic_auth_matches_rfc_7617_example() {
        // the canonical "Aladdin:open sesame" example from RFC 7617 / the
        // original HTTP Basic Auth spec.
        let auth = RequestAuth {
            auth_type: AuthType::Basic,
            basic_username: "Aladdin".to_string(),
            basic_password: "open sesame".to_string(),
            ..Default::default()
        };
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert_eq!(
            applied.headers,
            vec![(
                "Authorization".to_string(),
                "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==".to_string()
            )]
        );
    }

    #[test]
    fn api_key_goes_to_header_or_query_depending_on_add_to() {
        let mut auth = RequestAuth {
            auth_type: AuthType::ApiKey,
            api_key_key: "X-API-Key".to_string(),
            api_key_value: "secret-key".to_string(),
            api_key_add_to: AuthLocation::Header,
            ..Default::default()
        };
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert_eq!(
            applied.headers,
            vec![("X-API-Key".to_string(), "secret-key".to_string())]
        );
        assert!(applied.query_params.is_empty());

        auth.api_key_add_to = AuthLocation::Query;
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert!(applied.headers.is_empty());
        assert_eq!(
            applied.query_params,
            vec![("X-API-Key".to_string(), "secret-key".to_string())]
        );
    }

    #[test]
    fn oauth2_manual_token_matches_bearer_shape() {
        let auth = RequestAuth {
            auth_type: AuthType::OAuth2,
            oauth2_access_token: "the-token".to_string(),
            oauth2_header_prefix: "Bearer".to_string(),
            ..Default::default()
        };
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert_eq!(
            applied.headers,
            vec![("Authorization".to_string(), "Bearer the-token".to_string())]
        );
    }

    #[test]
    fn oauth2_with_no_token_yet_sends_nothing() {
        let auth = RequestAuth {
            auth_type: AuthType::OAuth2,
            ..Default::default()
        };
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert!(applied.headers.is_empty());
    }

    /// decodes a JWT's header/payload segments (base64url, no signature
    /// verification) for asserting on their contents in tests.
    fn decode_jwt_parts(token: &str) -> (serde_json::Value, serde_json::Value, String) {
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "a JWT must have 3 dot-separated parts");
        let header: serde_json::Value =
            serde_json::from_slice(&BASE64_URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        let payload: serde_json::Value =
            serde_json::from_slice(&BASE64_URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        (header, payload, parts[2].to_string())
    }

    #[test]
    fn jwt_bearer_produces_a_well_formed_signed_token() {
        let auth = RequestAuth {
            auth_type: AuthType::JwtBearer,
            jwt_algorithm: JwtAlgorithm::Hs256,
            jwt_secret: "my-secret".to_string(),
            jwt_payload: r#"{"sub":"1234567890","name":"Ada"}"#.to_string(),
            jwt_header_prefix: "Bearer".to_string(),
            ..Default::default()
        };
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert_eq!(applied.headers.len(), 1);
        let (name, value) = &applied.headers[0];
        assert_eq!(name, "Authorization");
        let token = value.strip_prefix("Bearer ").expect("has Bearer prefix");

        let (header, payload, _sig) = decode_jwt_parts(token);
        assert_eq!(header["alg"], "HS256");
        assert_eq!(header["typ"], "JWT");
        assert_eq!(payload["sub"], "1234567890");
        assert_eq!(payload["name"], "Ada");

        // same input -> same token (deterministic), different secret -> different signature
        let token_again = sign_jwt(JwtAlgorithm::Hs256, "my-secret", &auth.jwt_payload).unwrap();
        assert_eq!(token, token_again);
        let token_other_secret =
            sign_jwt(JwtAlgorithm::Hs256, "a-different-secret", &auth.jwt_payload).unwrap();
        assert_ne!(token, token_other_secret);
    }

    #[test]
    fn jwt_bearer_rejects_invalid_json_payload() {
        let auth = RequestAuth {
            auth_type: AuthType::JwtBearer,
            jwt_secret: "s".to_string(),
            jwt_payload: "not json".to_string(),
            ..Default::default()
        };
        let result = auth.apply("GET", "https://example.com");
        assert!(result.is_err());
    }

    #[test]
    fn oauth1_plaintext_signature_is_the_encoded_secrets() {
        let auth = RequestAuth {
            auth_type: AuthType::OAuth1,
            oauth1_signature_method: OAuth1SignatureMethod::PlainText,
            oauth1_consumer_key: "ck".to_string(),
            oauth1_consumer_secret: "cs".to_string(),
            oauth1_token: "tok".to_string(),
            oauth1_token_secret: "ts".to_string(),
            ..Default::default()
        };
        let applied = auth.apply("GET", "https://example.com/resource").unwrap();
        let (_, header) = &applied.headers[0];
        assert!(header.starts_with("OAuth "));
        assert!(header.contains(r#"oauth_signature="cs%26ts""#));
        assert!(header.contains(r#"oauth_consumer_key="ck""#));
        assert!(header.contains(r#"oauth_token="tok""#));
        assert!(header.contains(r#"oauth_signature_method="PLAINTEXT""#));
    }

    #[test]
    fn oauth1_hmac_sha1_signs_deterministically_for_the_same_nonce_and_timestamp() {
        // `sign_oauth1` generates its own nonce/timestamp, so two calls
        // won't match - but the lower-level base-string + HMAC pipeline
        // should be deterministic given the same inputs.
        let base_string = oauth1_base_string(
            "GET",
            "https://example.com/resource",
            &[
                ("oauth_consumer_key".to_string(), "ck".to_string()),
                ("oauth_nonce".to_string(), "fixed-nonce".to_string()),
                (
                    "oauth_signature_method".to_string(),
                    "HMAC-SHA1".to_string(),
                ),
                ("oauth_timestamp".to_string(), "1000000000".to_string()),
                ("oauth_version".to_string(), "1.0".to_string()),
                ("q".to_string(), "1".to_string()),
            ],
        );
        let sig1 = BASE64_STANDARD.encode(hmac_sha1(b"cs&ts", base_string.as_bytes()));
        let sig2 = BASE64_STANDARD.encode(hmac_sha1(b"cs&ts", base_string.as_bytes()));
        assert_eq!(sig1, sig2, "same inputs must sign to the same signature");
        assert!(!sig1.is_empty());

        // params are sorted by (percent-encoded) key before signing, so
        // base string construction shouldn't depend on input order.
        let base_string_reordered = oauth1_base_string(
            "GET",
            "https://example.com/resource",
            &[
                ("q".to_string(), "1".to_string()),
                ("oauth_version".to_string(), "1.0".to_string()),
                ("oauth_timestamp".to_string(), "1000000000".to_string()),
                (
                    "oauth_signature_method".to_string(),
                    "HMAC-SHA1".to_string(),
                ),
                ("oauth_nonce".to_string(), "fixed-nonce".to_string()),
                ("oauth_consumer_key".to_string(), "ck".to_string()),
            ],
        );
        assert_eq!(base_string, base_string_reordered);
    }

    #[test]
    fn oauth1_can_be_placed_in_query_instead_of_header() {
        let auth = RequestAuth {
            auth_type: AuthType::OAuth1,
            oauth1_add_to: AuthLocation::Query,
            oauth1_consumer_key: "ck".to_string(),
            oauth1_consumer_secret: "cs".to_string(),
            ..Default::default()
        };
        let applied = auth.apply("GET", "https://example.com").unwrap();
        assert!(applied.headers.is_empty());
        assert!(
            applied
                .query_params
                .iter()
                .any(|(k, v)| k == "oauth_consumer_key" && v == "ck")
        );
        assert!(
            applied
                .query_params
                .iter()
                .any(|(k, _)| k == "oauth_signature")
        );
    }

    #[test]
    fn percent_encoding_matches_rfc5849_unreserved_set() {
        // letters/digits/-._~ pass through unencoded; everything else
        // (including space and '&') gets percent-encoded.
        assert_eq!(oauth1_percent_encode("abc123-._~"), "abc123-._~");
        assert_eq!(oauth1_percent_encode("a b&c"), "a%20b%26c");
    }
}
