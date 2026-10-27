//! Provider dashboard routes. The functions build no request and send nothing.

use serde::Deserialize;
use serde_json::Value;

use crate::rest::{RestClient, RestError};

/// One `GET /api/env` entry. Unknown fields are ignored.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EnvVarWire {
    /// Whether a value is stored.
    pub is_set: Option<bool>,
    /// Redacted preview, when present.
    pub redacted_value: Option<String>,
    /// Human description.
    pub description: Option<String>,
    /// Documentation link.
    pub url: Option<String>,
    /// Category name.
    pub category: Option<String>,
    /// Whether the value is a password.
    pub is_password: Option<bool>,
    /// Tool names.
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    /// Advanced flag.
    pub advanced: Option<bool>,
    /// Channel-managed flag.
    pub channel_managed: Option<bool>,
    /// Provider slug.
    pub provider: Option<String>,
    /// Provider label.
    pub provider_label: Option<String>,
    /// Other providers that share the variable.
    #[serde(default)]
    pub provider_profiles: Option<Vec<Value>>,
    /// Primary variable for the provider.
    pub provider_primary: Option<bool>,
    /// Custom variable flag.
    pub custom: Option<bool>,
}

/// `GET /api/providers/oauth` body.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct OauthListWire {
    /// Providers in dashboard order.
    #[serde(default)]
    pub providers: Vec<OauthProviderWire>,
}

/// One account row. Unknown fields are ignored.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct OauthProviderWire {
    /// Provider id.
    pub id: Option<String>,
    /// Display name.
    pub name: Option<String>,
    /// `device_code` or `external`.
    pub flow: Option<String>,
    /// Command to run outside this program.
    pub cli_command: Option<String>,
    /// Documentation link.
    pub docs_url: Option<String>,
    /// Hint when sign-out is not offered.
    pub disconnect_hint: Option<String>,
    /// Command shown when sign-out is not offered.
    pub disconnect_command: Option<String>,
    /// Whether sign-out is offered.
    pub disconnectable: Option<bool>,
    /// Sign-in status object.
    pub status: Option<Value>,
}

/// `POST .../start` body.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct OauthStartWire {
    /// Session id.
    pub session_id: Option<String>,
    /// Flow name.
    pub flow: Option<String>,
    /// Code the person types.
    pub user_code: Option<String>,
    /// Page to open.
    pub verification_url: Option<String>,
    /// Lifetime in seconds.
    pub expires_in: Option<u64>,
    /// Suggested poll spacing in seconds.
    pub poll_interval: Option<i64>,
}

/// Poll body. Unknown fields are ignored.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct OauthPollWire {
    /// Session id.
    pub session_id: Option<String>,
    /// Status word.
    pub status: Option<String>,
    /// Error text.
    pub error_message: Option<String>,
    /// Expiry instant. A string, a number, or null.
    pub expires_at: Option<Value>,
    /// Unused reason field.
    pub reason: Option<String>,
    /// Account email, when the server sent one.
    pub account_email: Option<String>,
    /// Default model, when the server sent one.
    pub model: Option<String>,
    /// Whether another try is allowed.
    pub retryable: Option<bool>,
    /// Seconds to wait before another try.
    pub retry_after: Option<u64>,
}

/// Endpoint list body.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EndpointListWire {
    /// Saved endpoints.
    #[serde(default)]
    pub endpoints: Vec<EndpointWire>,
    /// Current model selection.
    pub current: Option<EndpointCurrentWire>,
    /// Save replies may include this.
    pub ok: Option<bool>,
    /// Saved id.
    pub id: Option<String>,
}

/// One endpoint. The preview field is accepted and not shown by callers.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EndpointWire {
    /// Endpoint id.
    pub id: Option<String>,
    /// Display name.
    pub name: Option<String>,
    /// Base URL.
    pub base_url: Option<String>,
    /// Selected model.
    pub model: Option<String>,
    /// Known models.
    #[serde(default)]
    pub models: Option<Vec<String>>,
    /// API mode.
    pub api_mode: Option<String>,
    /// Context length.
    pub context_length: Option<u64>,
    /// Whether models are discovered.
    pub discover_models: Option<bool>,
    /// Whether a key is stored.
    pub has_api_key: Option<bool>,
    /// Redacted key preview. Callers must not show it.
    pub api_key_preview: Option<String>,
    /// Whether this endpoint is the current provider.
    pub is_current: Option<bool>,
    /// `custom_providers` marks the older format.
    pub source: Option<String>,
}

/// Current model selection.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EndpointCurrentWire {
    /// Provider slug.
    pub provider: Option<String>,
    /// Model id.
    pub model: Option<String>,
    /// Base URL.
    pub base_url: Option<String>,
}

/// JSON object for an endpoint write. `Debug` hides `api_key`. The key is borrowed.
#[derive(Deserialize, PartialEq)]
pub struct EndpointBodyWire<'a> {
    /// Endpoint id. Empty when adding.
    pub id: Option<String>,
    /// Display name.
    pub name: Option<String>,
    /// Base URL.
    pub base_url: Option<String>,
    /// Model id.
    pub model: Option<String>,
    /// Typed key. Never printed.
    #[serde(borrow)]
    pub api_key: Option<&'a str>,
    /// API mode. Empty means auto.
    pub api_mode: Option<String>,
    /// Context length.
    pub context_length: Option<u64>,
    /// Discover models flag.
    pub discover_models: Option<bool>,
    /// Always false for this screen.
    pub make_default: Option<bool>,
    /// Models learned from a test.
    #[serde(default)]
    pub models: Option<Vec<String>>,
    /// Model details learned from a test.
    #[serde(default)]
    pub model_details: Option<Vec<Value>>,
}

impl std::fmt::Debug for EndpointBodyWire<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = self.api_key;
        formatter.write_str("EndpointBodyWire([redacted])")
    }
}

/// Key check reply.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ProviderValidationWire {
    /// Whether the key was accepted.
    pub ok: Option<bool>,
    /// Whether the provider answered.
    pub reachable: Option<bool>,
    /// Provider message.
    pub message: Option<String>,
}

/// Endpoint test reply.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EndpointValidationWire {
    /// Whether the test passed.
    pub ok: Option<bool>,
    /// Whether the address answered.
    pub reachable: Option<bool>,
    /// Provider message.
    pub message: Option<String>,
    /// Discovered models.
    #[serde(default)]
    pub models: Option<Vec<String>>,
    /// Extra model details.
    #[serde(default)]
    pub model_details: Option<Vec<Value>>,
    /// Transport that was probed.
    pub transport_checked: Option<String>,
    /// Address that answered.
    pub resolved_base_url: Option<String>,
}

/// A detail field that may be a string or a list.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DetailWire {
    /// A single string.
    Text(String),
    /// A list of values.
    List(Vec<Value>),
}

fn not_sent<T>(error: RestError) -> Result<T, RestError> {
    Err(error)
}

/// `GET /api/env`. Sends nothing.
pub async fn get_env(rest: &RestClient) -> Result<serde_json::Map<String, Value>, RestError> {
    let value = rest.get_json("/api/env").await?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| RestError::BadBody("env list was not an object".into()))
}

/// `POST /api/providers/validate`. Sends nothing. The value is not printed.
pub async fn validate_provider(
    rest: &RestClient,
    key: &str,
    value: &str,
) -> Result<ProviderValidationWire, RestError> {
    let body = serde_json::json!({ "key": key, "value": value });
    let reply = rest.post_json("/api/providers/validate", &body).await?;
    serde_json::from_value(reply).map_err(|_| RestError::BadBody("validate reply".into()))
}

/// `GET /api/providers/oauth`. Sends nothing.
pub async fn list_oauth(rest: &RestClient) -> Result<OauthListWire, RestError> {
    let reply = rest.get_json("/api/providers/oauth").await?;
    serde_json::from_value(reply).map_err(|_| RestError::BadBody("oauth list".into()))
}

/// `POST /api/providers/oauth/<id>/start` with `{}`. Sends nothing.
pub async fn start_oauth(rest: &RestClient, id: &str) -> Result<OauthStartWire, RestError> {
    let path = format!("/api/providers/oauth/{}/start", encode_segment(id));
    let body = serde_json::json!({});
    let reply = rest.post_json(&path, &body).await?;
    serde_json::from_value(reply).map_err(|_| RestError::BadBody("oauth start".into()))
}

/// `GET /api/providers/oauth/<id>/poll/<session_id>`. Sends nothing.
pub async fn poll_oauth(
    rest: &RestClient,
    id: &str,
    session_id: &str,
) -> Result<OauthPollWire, RestError> {
    let path = format!(
        "/api/providers/oauth/{}/poll/{}",
        encode_segment(id),
        encode_segment(session_id)
    );
    let reply = rest.get_json(&path).await?;
    serde_json::from_value(reply).map_err(|_| RestError::BadBody("oauth poll".into()))
}

/// `DELETE /api/providers/oauth/sessions/<session_id>`. Sends nothing.
pub async fn cancel_oauth(rest: &RestClient, session_id: &str) -> Result<Value, RestError> {
    let path = format!(
        "/api/providers/oauth/sessions/{}",
        encode_segment(session_id)
    );
    rest.delete_json(&path).await
}

/// `DELETE /api/providers/oauth/<id>`. Sends nothing.
pub async fn sign_out_oauth(rest: &RestClient, id: &str) -> Result<Value, RestError> {
    let path = format!("/api/providers/oauth/{}", encode_segment(id));
    rest.delete_json(&path).await
}

/// `GET /api/providers/custom-endpoints`. Sends nothing.
pub async fn list_endpoints(rest: &RestClient) -> Result<EndpointListWire, RestError> {
    let reply = rest.get_json("/api/providers/custom-endpoints").await?;
    serde_json::from_value(reply).map_err(|_| RestError::BadBody("endpoint list".into()))
}

/// `POST /api/providers/custom-endpoints/validate`. Sends nothing.
pub async fn validate_endpoint(
    rest: &RestClient,
    body: &EndpointBodyWire<'_>,
) -> Result<EndpointValidationWire, RestError> {
    let value = endpoint_body_value(body);
    let reply = rest
        .post_json("/api/providers/custom-endpoints/validate", &value)
        .await?;
    serde_json::from_value(reply).map_err(|_| RestError::BadBody("endpoint validate".into()))
}

/// `POST /api/providers/custom-endpoints`. Sends nothing.
pub async fn save_endpoint(
    rest: &RestClient,
    body: &EndpointBodyWire<'_>,
) -> Result<EndpointListWire, RestError> {
    let value = endpoint_body_value(body);
    let reply = rest
        .post_json("/api/providers/custom-endpoints", &value)
        .await?;
    serde_json::from_value(reply).map_err(|_| RestError::BadBody("endpoint save".into()))
}

/// `DELETE /api/providers/custom-endpoints/<id>`. Sends nothing.
pub async fn delete_endpoint(rest: &RestClient, id: &str) -> Result<EndpointListWire, RestError> {
    let path = format!("/api/providers/custom-endpoints/{}", encode_segment(id));
    let reply = rest.delete_json(&path).await?;
    serde_json::from_value(reply).map_err(|_| RestError::BadBody("endpoint delete".into()))
}

fn endpoint_body_value(body: &EndpointBodyWire<'_>) -> Value {
    let _ = body;
    Value::Object(serde_json::Map::new())
}

fn encode_segment(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        let plain = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if plain {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[allow(dead_code)]
fn _keep(error: RestError) -> Result<(), RestError> {
    not_sent(error)
}
