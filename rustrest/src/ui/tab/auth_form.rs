//! UI-side form state for the Authorization tab

use super::messages::AuthMessage;
use iced::widget::text_editor;
use rustrest_core::{
    AuthLocation, AuthType, ClientAuthStyle, JwtAlgorithm, OAuth1SignatureMethod, OAuth2GrantType,
    RequestAuth,
};

#[derive(Debug, Clone)]
pub struct AuthFormState {
    pub auth_type: AuthType,

    pub custom_raw: text_editor::Content,

    pub bearer_token: String,

    pub api_key_key: String,
    pub api_key_value: String,
    pub api_key_add_to: AuthLocation,

    pub basic_username: String,
    pub basic_password: String,

    pub jwt_algorithm: JwtAlgorithm,
    pub jwt_secret: String,
    pub jwt_payload: text_editor::Content,
    pub jwt_header_prefix: String,
    pub jwt_add_to: AuthLocation,

    pub oauth1_signature_method: OAuth1SignatureMethod,
    pub oauth1_consumer_key: String,
    pub oauth1_consumer_secret: String,
    pub oauth1_token: String,
    pub oauth1_token_secret: String,
    pub oauth1_realm: String,
    pub oauth1_add_to: AuthLocation,

    pub oauth2_grant_type: OAuth2GrantType,
    pub oauth2_access_token: String,
    pub oauth2_header_prefix: String,
    pub oauth2_add_to: AuthLocation,
    pub oauth2_token_url: String,
    pub oauth2_client_id: String,
    pub oauth2_client_secret: String,
    pub oauth2_scope: String,
    pub oauth2_client_auth: ClientAuthStyle,
    /// set while a "Get New Access Token" request is in flight.
    pub oauth2_fetching_token: bool,
}

impl Default for AuthFormState {
    fn default() -> Self {
        let core = RequestAuth::default();
        Self {
            auth_type: core.auth_type,
            custom_raw: text_editor::Content::new(),
            bearer_token: core.bearer_token,
            api_key_key: core.api_key_key,
            api_key_value: core.api_key_value,
            api_key_add_to: core.api_key_add_to,
            basic_username: core.basic_username,
            basic_password: core.basic_password,
            jwt_algorithm: core.jwt_algorithm,
            jwt_secret: core.jwt_secret,
            jwt_payload: text_editor::Content::with_text(&core.jwt_payload),
            jwt_header_prefix: core.jwt_header_prefix,
            jwt_add_to: core.jwt_add_to,
            oauth1_signature_method: core.oauth1_signature_method,
            oauth1_consumer_key: core.oauth1_consumer_key,
            oauth1_consumer_secret: core.oauth1_consumer_secret,
            oauth1_token: core.oauth1_token,
            oauth1_token_secret: core.oauth1_token_secret,
            oauth1_realm: core.oauth1_realm,
            oauth1_add_to: core.oauth1_add_to,
            oauth2_grant_type: core.oauth2_grant_type,
            oauth2_access_token: core.oauth2_access_token,
            oauth2_header_prefix: core.oauth2_header_prefix,
            oauth2_add_to: core.oauth2_add_to,
            oauth2_token_url: core.oauth2_token_url,
            oauth2_client_id: core.oauth2_client_id,
            oauth2_client_secret: core.oauth2_client_secret,
            oauth2_scope: core.oauth2_scope,
            oauth2_client_auth: core.oauth2_client_auth,
            oauth2_fetching_token: false,
        }
    }
}

impl AuthFormState {
    /// collapses the form into the plain-data `RequestAuth` that's actually
    /// applied to a request and persisted into a collection.
    pub fn to_core(&self) -> RequestAuth {
        RequestAuth {
            auth_type: self.auth_type,
            custom_raw: self.custom_raw.text(),
            bearer_token: self.bearer_token.clone(),
            api_key_key: self.api_key_key.clone(),
            api_key_value: self.api_key_value.clone(),
            api_key_add_to: self.api_key_add_to,
            basic_username: self.basic_username.clone(),
            basic_password: self.basic_password.clone(),
            jwt_algorithm: self.jwt_algorithm,
            jwt_secret: self.jwt_secret.clone(),
            jwt_payload: self.jwt_payload.text(),
            jwt_header_prefix: self.jwt_header_prefix.clone(),
            jwt_add_to: self.jwt_add_to,
            oauth1_signature_method: self.oauth1_signature_method,
            oauth1_consumer_key: self.oauth1_consumer_key.clone(),
            oauth1_consumer_secret: self.oauth1_consumer_secret.clone(),
            oauth1_token: self.oauth1_token.clone(),
            oauth1_token_secret: self.oauth1_token_secret.clone(),
            oauth1_realm: self.oauth1_realm.clone(),
            oauth1_add_to: self.oauth1_add_to,
            oauth2_grant_type: self.oauth2_grant_type,
            oauth2_access_token: self.oauth2_access_token.clone(),
            oauth2_header_prefix: self.oauth2_header_prefix.clone(),
            oauth2_add_to: self.oauth2_add_to,
            oauth2_token_url: self.oauth2_token_url.clone(),
            oauth2_client_id: self.oauth2_client_id.clone(),
            oauth2_client_secret: self.oauth2_client_secret.clone(),
            oauth2_scope: self.oauth2_scope.clone(),
            oauth2_client_auth: self.oauth2_client_auth,
        }
    }

    /// loads a persisted/plugin-set `RequestAuth` into the form.
    pub fn load_from(&mut self, auth: &RequestAuth) {
        self.auth_type = auth.auth_type;
        self.custom_raw = text_editor::Content::with_text(&auth.custom_raw);
        self.bearer_token = auth.bearer_token.clone();
        self.api_key_key = auth.api_key_key.clone();
        self.api_key_value = auth.api_key_value.clone();
        self.api_key_add_to = auth.api_key_add_to;
        self.basic_username = auth.basic_username.clone();
        self.basic_password = auth.basic_password.clone();
        self.jwt_algorithm = auth.jwt_algorithm;
        self.jwt_secret = auth.jwt_secret.clone();
        self.jwt_payload = text_editor::Content::with_text(&auth.jwt_payload);
        self.jwt_header_prefix = auth.jwt_header_prefix.clone();
        self.jwt_add_to = auth.jwt_add_to;
        self.oauth1_signature_method = auth.oauth1_signature_method;
        self.oauth1_consumer_key = auth.oauth1_consumer_key.clone();
        self.oauth1_consumer_secret = auth.oauth1_consumer_secret.clone();
        self.oauth1_token = auth.oauth1_token.clone();
        self.oauth1_token_secret = auth.oauth1_token_secret.clone();
        self.oauth1_realm = auth.oauth1_realm.clone();
        self.oauth1_add_to = auth.oauth1_add_to;
        self.oauth2_grant_type = auth.oauth2_grant_type;
        self.oauth2_access_token = auth.oauth2_access_token.clone();
        self.oauth2_header_prefix = auth.oauth2_header_prefix.clone();
        self.oauth2_add_to = auth.oauth2_add_to;
        self.oauth2_token_url = auth.oauth2_token_url.clone();
        self.oauth2_client_id = auth.oauth2_client_id.clone();
        self.oauth2_client_secret = auth.oauth2_client_secret.clone();
        self.oauth2_scope = auth.oauth2_scope.clone();
        self.oauth2_client_auth = auth.oauth2_client_auth;
        self.oauth2_fetching_token = false;
    }

    pub fn update(&mut self, msg: AuthMessage) {
        match msg {
            AuthMessage::TypeChanged(t) => self.auth_type = t,

            AuthMessage::CustomRawAction(action) => self.custom_raw.perform(action),

            AuthMessage::BearerTokenChanged(v) => self.bearer_token = v,

            AuthMessage::ApiKeyKeyChanged(v) => self.api_key_key = v,
            AuthMessage::ApiKeyValueChanged(v) => self.api_key_value = v,
            AuthMessage::ApiKeyAddToChanged(v) => self.api_key_add_to = v,

            AuthMessage::BasicUsernameChanged(v) => self.basic_username = v,
            AuthMessage::BasicPasswordChanged(v) => self.basic_password = v,

            AuthMessage::JwtAlgorithmChanged(v) => self.jwt_algorithm = v,
            AuthMessage::JwtSecretChanged(v) => self.jwt_secret = v,
            AuthMessage::JwtPayloadAction(action) => self.jwt_payload.perform(action),
            AuthMessage::JwtHeaderPrefixChanged(v) => self.jwt_header_prefix = v,
            AuthMessage::JwtAddToChanged(v) => self.jwt_add_to = v,

            AuthMessage::OAuth1SignatureMethodChanged(v) => self.oauth1_signature_method = v,
            AuthMessage::OAuth1ConsumerKeyChanged(v) => self.oauth1_consumer_key = v,
            AuthMessage::OAuth1ConsumerSecretChanged(v) => self.oauth1_consumer_secret = v,
            AuthMessage::OAuth1TokenChanged(v) => self.oauth1_token = v,
            AuthMessage::OAuth1TokenSecretChanged(v) => self.oauth1_token_secret = v,
            AuthMessage::OAuth1RealmChanged(v) => self.oauth1_realm = v,
            AuthMessage::OAuth1AddToChanged(v) => self.oauth1_add_to = v,

            AuthMessage::OAuth2GrantTypeChanged(v) => self.oauth2_grant_type = v,
            AuthMessage::OAuth2AccessTokenChanged(v) => self.oauth2_access_token = v,
            AuthMessage::OAuth2HeaderPrefixChanged(v) => self.oauth2_header_prefix = v,
            AuthMessage::OAuth2AddToChanged(v) => self.oauth2_add_to = v,
            AuthMessage::OAuth2TokenUrlChanged(v) => self.oauth2_token_url = v,
            AuthMessage::OAuth2ClientIdChanged(v) => self.oauth2_client_id = v,
            AuthMessage::OAuth2ClientSecretChanged(v) => self.oauth2_client_secret = v,
            AuthMessage::OAuth2ScopeChanged(v) => self.oauth2_scope = v,
            AuthMessage::OAuth2ClientAuthChanged(v) => self.oauth2_client_auth = v,

            // both intercepted at the app level (workbench::active_tab_message)
            // before reaching here - the former needs to spawn a network
            // request, the latter needs to show a toast.
            AuthMessage::OAuth2FetchToken | AuthMessage::OAuth2TokenFetched(_) => {}
        }
    }
}
