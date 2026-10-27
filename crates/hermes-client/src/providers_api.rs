//! Profile-bound provider routes. Request bodies are never recorded here.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroize;

use crate::rest::{RestClient, RestError};

#[derive(Clone, Default, Deserialize, PartialEq)]
pub struct EnvVarWire {
    pub is_set: Option<bool>,
    pub redacted_value: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub category: Option<String>,
    pub is_password: Option<bool>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    pub advanced: Option<bool>,
    pub channel_managed: Option<bool>,
    pub provider: Option<String>,
    pub provider_label: Option<String>,
    #[serde(default)]
    pub provider_profiles: Option<Vec<Value>>,
    pub provider_primary: Option<bool>,
    pub custom: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ProviderProfileWire {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub provider_label: String,
    pub description: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct OauthListWire {
    #[serde(default)]
    pub providers: Vec<OauthProviderWire>,
}

#[derive(Clone, Default, Deserialize, PartialEq)]
pub struct OauthProviderWire {
    pub id: Option<String>,
    pub name: Option<String>,
    pub flow: Option<String>,
    pub cli_command: Option<String>,
    pub docs_url: Option<String>,
    pub disconnect_hint: Option<String>,
    pub disconnect_command: Option<String>,
    pub disconnectable: Option<bool>,
    pub status: Option<Value>,
}

#[derive(Clone, Default, Deserialize, PartialEq)]
pub struct OauthStatusWire {
    #[serde(default)]
    pub logged_in: bool,
    pub source: Option<String>,
    pub source_label: Option<String>,
    pub expires_at: Option<Value>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct OauthStartWire {
    pub session_id: Option<String>,
    pub flow: Option<String>,
    pub user_code: Option<String>,
    pub verification_url: Option<String>,
    pub expires_in: Option<u64>,
    pub poll_interval: Option<i64>,
}

#[derive(Clone, Default, Deserialize, PartialEq)]
pub struct OauthPollWire {
    pub session_id: Option<String>,
    pub status: Option<String>,
    pub error_message: Option<String>,
    pub expires_at: Option<Value>,
    pub reason: Option<String>,
    pub account_email: Option<String>,
    pub model: Option<String>,
    pub retryable: Option<bool>,
    pub retry_after: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EndpointListWire {
    #[serde(default)]
    pub endpoints: Vec<EndpointWire>,
    pub current: Option<EndpointCurrentWire>,
    pub ok: Option<bool>,
    pub id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EndpointCurrentWire {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Clone, Default, Deserialize, PartialEq)]
pub struct EndpointWire {
    pub id: Option<String>,
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub models: Option<Vec<String>>,
    pub api_mode: Option<String>,
    pub context_length: Option<u64>,
    pub discover_models: Option<bool>,
    pub has_api_key: Option<bool>,
    pub api_key_preview: Option<String>,
    pub is_current: Option<bool>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ProviderValidationWire {
    pub ok: Option<bool>,
    pub reachable: Option<bool>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct EndpointValidationWire {
    pub ok: Option<bool>,
    pub reachable: Option<bool>,
    pub message: Option<String>,
    #[serde(default)]
    pub models: Option<Vec<String>>,
    #[serde(default)]
    pub model_details: Option<Vec<Value>>,
    pub transport_checked: Option<String>,
    pub resolved_base_url: Option<String>,
}

#[derive(Serialize)]
pub struct KeyBodyWire<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

#[derive(Default, Serialize)]
pub struct EndpointBodyWire<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discover_models: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub make_default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_details: Option<Vec<Value>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DetailWire {
    Text(String),
    List(Vec<Value>),
}

macro_rules! redacted_debug {
    ($($ty:ty),+ $(,)?) => {$(impl fmt::Debug for $ty {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(concat!(stringify!($ty), "([redacted])"))
        }
    })+};
}
redacted_debug!(
    EnvVarWire,
    EndpointWire,
    OauthProviderWire,
    OauthStatusWire,
    OauthPollWire,
    KeyBodyWire<'_>,
    EndpointBodyWire<'_>
);

struct SecretBody(Value);
impl Drop for SecretBody {
    fn drop(&mut self) {
        for field in ["value", "api_key"] {
            if let Some(Value::String(value)) = self.0.get_mut(field) {
                value.zeroize();
            }
        }
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, RestError> {
    serde_json::from_value(value)
        .map_err(|_| RestError::BadBody("provider reply did not decode".into()))
}

async fn post<T: serde::de::DeserializeOwned>(
    client: &RestClient,
    path: &str,
    body: &impl Serialize,
) -> Result<T, RestError> {
    let body = SecretBody(
        serde_json::to_value(body)
            .map_err(|_| RestError::BadBody("provider request did not encode".into()))?,
    );
    decode(client.post_json(path, &body.0).await?)
}

pub async fn load_env(client: &RestClient) -> Result<BTreeMap<String, EnvVarWire>, RestError> {
    decode(client.get_json("/api/env").await?)
}
pub async fn check_key(
    client: &RestClient,
    key: &str,
    value: &str,
) -> Result<ProviderValidationWire, RestError> {
    post(
        client,
        "/api/providers/validate",
        &KeyBodyWire { key, value },
    )
    .await
}
pub async fn load_oauth(client: &RestClient) -> Result<OauthListWire, RestError> {
    decode(client.get_json("/api/providers/oauth").await?)
}
pub async fn start_sign_in(client: &RestClient, id: &str) -> Result<OauthStartWire, RestError> {
    post(
        client,
        &format!("/api/providers/oauth/{}/start", percent_encode(id)),
        &serde_json::json!({}),
    )
    .await
}
pub async fn poll_sign_in(
    client: &RestClient,
    id: &str,
    session_id: &str,
) -> Result<OauthPollWire, RestError> {
    decode(
        client
            .get_json(&format!(
                "/api/providers/oauth/{}/poll/{}",
                percent_encode(id),
                percent_encode(session_id)
            ))
            .await?,
    )
}
pub async fn cancel_sign_in(client: &RestClient, session_id: &str) -> Result<Value, RestError> {
    client
        .delete_json(&format!(
            "/api/providers/oauth/sessions/{}",
            percent_encode(session_id)
        ))
        .await
}
pub async fn sign_out(client: &RestClient, id: &str) -> Result<Value, RestError> {
    client
        .delete_json(&format!("/api/providers/oauth/{}", percent_encode(id)))
        .await
}
pub async fn load_endpoints(client: &RestClient) -> Result<EndpointListWire, RestError> {
    decode(client.get_json("/api/providers/custom-endpoints").await?)
}
pub async fn check_endpoint(
    client: &RestClient,
    body: &EndpointBodyWire<'_>,
) -> Result<EndpointValidationWire, RestError> {
    post(client, "/api/providers/custom-endpoints/validate", body).await
}
pub async fn save_endpoint(
    client: &RestClient,
    body: &EndpointBodyWire<'_>,
) -> Result<EndpointListWire, RestError> {
    post(client, "/api/providers/custom-endpoints", body).await
}
pub async fn delete_endpoint(client: &RestClient, id: &str) -> Result<EndpointListWire, RestError> {
    decode(
        client
            .delete_json(&format!(
                "/api/providers/custom-endpoints/{}",
                percent_encode(id)
            ))
            .await?,
    )
}

pub async fn get_env(client: &RestClient) -> Result<serde_json::Map<String, Value>, RestError> {
    match client.get_json("/api/env").await? {
        Value::Object(rows) => Ok(rows),
        _ => Err(RestError::BadBody("env list was not an object".into())),
    }
}
pub use cancel_sign_in as cancel_oauth;
pub use check_endpoint as validate_endpoint;
pub use check_key as validate_provider;
pub use load_endpoints as list_endpoints;
pub use load_oauth as list_oauth;
pub use poll_sign_in as poll_oauth;
pub use sign_out as sign_out_oauth;
pub use start_sign_in as start_oauth;

/// Encode one path segment, including slashes and non-ASCII bytes.
pub fn percent_encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_fields_and_new_fields_are_lenient() {
        let row: EnvVarWire =
            serde_json::from_value(serde_json::json!({"future": true, "provider_profiles": null}))
                .unwrap();
        assert_eq!(row.provider, None);
        let list: OauthListWire =
            serde_json::from_value(serde_json::json!({"future": []})).unwrap();
        assert!(list.providers.is_empty());
        let reply: EndpointValidationWire = serde_json::from_value(
            serde_json::json!({"models": null, "transport_checked": "chat_completions"}),
        )
        .unwrap();
        assert_eq!(reply.models, None);
        assert_eq!(reply.transport_checked.as_deref(), Some("chat_completions"));
    }

    #[test]
    fn keys_are_borrowed_and_never_debugged() {
        let secret = zeroize::Zeroizing::new("test-only-credential".to_string());
        let body = KeyBodyWire {
            key: "EXAMPLE_KEY",
            value: &secret,
        };
        assert!(!format!("{body:?}").contains(secret.as_str()));
        assert_eq!(
            serde_json::to_value(&body).unwrap()["value"],
            secret.as_str()
        );
        let poll: OauthPollWire = serde_json::from_value(
            serde_json::json!({"status": "approved", "account_email": "person@example.invalid"}),
        )
        .unwrap();
        assert!(!format!("{poll:?}").contains("person@"));
    }

    #[test]
    fn path_segments_encode_reserved_and_utf8_bytes() {
        assert_eq!(percent_encode("a/b ?#%é"), "a%2Fb%20%3F%23%25%C3%A9");
        assert_eq!(percent_encode("abc-._~09"), "abc-._~09");
    }
}
