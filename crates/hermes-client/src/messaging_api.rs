//! Profile-scoped messaging and pairing calls on the loopback dashboard.
//!
//! Stored previews are discarded. Typed values stay in a zeroizing map until the
//! short-lived JSON request is constructed; neither form is logged or formatted.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

use crate::rest::{Method, RequestOpts, RestClient, RestError};

fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// The local environment path and unknown fields are discarded.
#[derive(Clone, Default, PartialEq, Deserialize)]
pub struct PlatformsReply {
    #[serde(default)]
    pub gateway_start_command: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub platforms: Vec<PlatformWire>,
}

/// One platform. A stored preview is never retained.
#[derive(Clone, Default, PartialEq, Deserialize)]
pub struct PlatformWire {
    #[serde(default, deserialize_with = "null_default")]
    pub id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub description: String,
    #[serde(default)]
    pub docs_url: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub configured: Option<bool>,
    #[serde(default)]
    pub gateway_running: Option<bool>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub home_channel: Option<HomeChannelWire>,
    #[serde(default)]
    pub ingress_url: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub env_vars: Vec<EnvWire>,
}

/// A home channel; identifiers are kept for the overlay only.
#[derive(Clone, Default, PartialEq, Deserialize)]
pub struct HomeChannelWire {
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub chat_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// One environment field. Only list fields may retain `value`.
#[derive(Clone, Default, PartialEq)]
pub struct EnvWire {
    pub key: String,
    pub required: bool,
    pub is_set: bool,
    pub value: Option<String>,
    pub description: Option<String>,
    pub prompt: String,
    pub help: Option<String>,
    pub url: Option<String>,
    pub is_password: bool,
    pub is_list: bool,
    pub advanced: bool,
}

impl<'de> Deserialize<'de> for EnvWire {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Field {
            #[serde(default, deserialize_with = "null_default")]
            key: String,
            #[serde(default, deserialize_with = "null_default")]
            required: bool,
            #[serde(default, deserialize_with = "null_default")]
            is_set: bool,
            #[serde(default)]
            value: Option<String>,
            #[serde(default)]
            description: Option<String>,
            #[serde(default, deserialize_with = "null_default")]
            prompt: String,
            #[serde(default)]
            help: Option<String>,
            #[serde(default)]
            url: Option<String>,
            #[serde(default, deserialize_with = "null_default")]
            is_password: bool,
            #[serde(default, deserialize_with = "null_default")]
            is_list: bool,
            #[serde(default, deserialize_with = "null_default")]
            advanced: bool,
        }
        let field = Field::deserialize(deserializer)?;
        Ok(Self {
            key: field.key,
            required: field.required,
            is_set: field.is_set,
            value: field.value.filter(|_| field.is_list),
            description: field.description,
            prompt: field.prompt,
            help: field.help,
            url: field.url,
            is_password: field.is_password,
            is_list: field.is_list,
            advanced: field.advanced,
        })
    }
}

/// A save or switch body. Dropping it wipes its value map.
pub struct PlatformUpdate {
    pub enabled: Option<bool>,
    pub env: BTreeMap<String, Zeroizing<String>>,
    pub clear_env: Vec<String>,
}

/// A save answer, with the same safe platform shape as a read.
#[derive(Clone, Default, PartialEq, Deserialize)]
pub struct UpdateReply {
    #[serde(default, deserialize_with = "null_default")]
    pub ok: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub hot_served: bool,
    #[serde(default)]
    pub platform: Option<PlatformWire>,
}

/// A platform test answer.
#[derive(Clone, Default, PartialEq, Deserialize)]
pub struct TestReply {
    #[serde(default, deserialize_with = "null_default")]
    pub ok: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub state: String,
    #[serde(default, deserialize_with = "null_default")]
    pub message: String,
    #[serde(default)]
    pub auth_posture: Option<String>,
}

/// Pairing rows. Names and identifiers belong only in the overlay.
#[derive(Clone, Default, PartialEq, Deserialize)]
pub struct PairingReply {
    #[serde(default, deserialize_with = "null_default")]
    pub pending: Vec<PendingWire>,
    #[serde(default, deserialize_with = "null_default")]
    pub approved: Vec<ApprovedWire>,
}

/// A request waiting for approval.
#[derive(Clone, Default, PartialEq, Deserialize)]
pub struct PendingWire {
    #[serde(default, deserialize_with = "null_default")]
    pub platform: String,
    #[serde(default, deserialize_with = "null_default")]
    pub request_id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub user_id: String,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub age_minutes: Option<i64>,
}

/// An approved person.
#[derive(Clone, Default, PartialEq, Deserialize)]
pub struct ApprovedWire {
    #[serde(default, deserialize_with = "null_default")]
    pub platform: String,
    #[serde(default, deserialize_with = "null_default")]
    pub user_id: String,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub approved_at: Option<String>,
}

/// The person returned by an approval is deliberately ignored.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ApproveReply {
    #[serde(default, deserialize_with = "null_default")]
    pub ok: bool,
}

macro_rules! count_debug {
    ($kind:ty, $name:literal, $value:ident, $($key:literal => $field:expr),+ $(,)?) => {
        impl std::fmt::Debug for $kind {
            fn fmt(&$value, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.debug_struct($name)$(.field($key, &$field))+.finish()
            }
        }
    };
}

count_debug!(PlatformsReply, "PlatformsReply", self, "platform_count" => self.platforms.len());
count_debug!(PlatformWire, "PlatformWire", self, "field_count" => self.env_vars.len());
count_debug!(HomeChannelWire, "HomeChannelWire", self, "named" => self.name.is_some());
count_debug!(EnvWire, "EnvWire", self, "key" => self.key, "is_set" => self.is_set, "is_list" => self.is_list);
count_debug!(PlatformUpdate, "PlatformUpdate", self, "enabled" => self.enabled, "env_keys" => self.env.keys().collect::<Vec<_>>(), "clear_count" => self.clear_env.len());
count_debug!(UpdateReply, "UpdateReply", self, "ok" => self.ok, "hot_served" => self.hot_served);
count_debug!(TestReply, "TestReply", self, "ok" => self.ok, "message_len" => self.message.len());
count_debug!(PairingReply, "PairingReply", self, "pending_count" => self.pending.len(), "approved_count" => self.approved.len());
count_debug!(PendingWire, "PendingWire", self, "request_len" => self.request_id.len(), "user_len" => self.user_id.len());
count_debug!(ApprovedWire, "ApprovedWire", self, "user_len" => self.user_id.len());

/// Percent-encode one path segment or query value, including spaces as `%20`.
fn component(value: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn route(path: &str, profile: &str) -> String {
    format!("{path}?profile={}", component(profile))
}

async fn request<T: DeserializeOwned>(
    rest: &RestClient,
    method: Method,
    path: &str,
    body: Option<&Value>,
    deadline: Option<Duration>,
) -> Result<T, RestError> {
    let value = rest
        .request(
            method,
            path,
            body,
            RequestOpts {
                deadline,
                ..RequestOpts::default()
            },
        )
        .await?;
    serde_json::from_value(value).map_err(|_| RestError::BadBody("invalid messaging reply".into()))
}

/// Read platforms using the connected ticket's profile.
pub async fn list_platforms(rest: &RestClient, profile: &str) -> Result<PlatformsReply, RestError> {
    let value: Value = request(
        rest,
        Method::Get,
        &route("/api/messaging/platforms", profile),
        None,
        None,
    )
    .await?;
    if value.get("platforms").is_none() {
        return Err(RestError::BadBody("the reply had no platforms".into()));
    }
    serde_json::from_value(value).map_err(|_| RestError::BadBody("invalid messaging reply".into()))
}

/// Save edited fields or switch a platform. Untouched fields are omitted.
pub async fn update_platform(
    rest: &RestClient,
    profile: &str,
    id: &str,
    body: &PlatformUpdate,
) -> Result<UpdateReply, RestError> {
    let mut fields = Map::new();
    if let Some(enabled) = body.enabled {
        fields.insert("enabled".into(), Value::Bool(enabled));
    }
    if !body.env.is_empty() || !body.clear_env.is_empty() {
        fields.insert(
            "env".into(),
            Value::Object(
                body.env
                    .iter()
                    .map(|(key, value)| (key.clone(), Value::String(value.as_str().to_owned())))
                    .collect(),
            ),
        );
        fields.insert("clear_env".into(), json!(body.clear_env));
    }
    let value = Value::Object(fields);
    let path = route(
        &format!("/api/messaging/platforms/{}", component(id)),
        profile,
    );
    request(
        rest,
        Method::Put,
        &path,
        Some(&value),
        Some(Duration::from_secs(30)),
    )
    .await
}

/// Ask Hermes to test one platform.
pub async fn test_platform(
    rest: &RestClient,
    profile: &str,
    id: &str,
) -> Result<TestReply, RestError> {
    let path = route(
        &format!("/api/messaging/platforms/{}/test", component(id)),
        profile,
    );
    request(
        rest,
        Method::Post,
        &path,
        None,
        Some(Duration::from_secs(15)),
    )
    .await
}

/// Read pairing rows using the connected ticket's profile.
pub async fn list_pairing(rest: &RestClient, profile: &str) -> Result<PairingReply, RestError> {
    request(
        rest,
        Method::Get,
        &route("/api/pairing", profile),
        None,
        None,
    )
    .await
}

/// Approve a pending request by its request identifier.
pub async fn approve_pairing(
    rest: &RestClient,
    profile: &str,
    platform: &str,
    request_id: &str,
) -> Result<ApproveReply, RestError> {
    let body = json!({"platform": platform, "request_id": request_id, "profile": profile});
    request(
        rest,
        Method::Post,
        "/api/pairing/approve",
        Some(&body),
        Some(Duration::from_secs(15)),
    )
    .await
}

/// Revoke a pairing grant and its allowlist mirror.
pub async fn revoke_pairing(
    rest: &RestClient,
    profile: &str,
    platform: &str,
    user_id: &str,
) -> Result<(), RestError> {
    let body = json!({"platform": platform, "user_id": user_id, "profile": profile});
    let reply: ApproveReply = request(
        rest,
        Method::Post,
        "/api/pairing/revoke",
        Some(&body),
        Some(Duration::from_secs(15)),
    )
    .await?;
    if reply.ok {
        Ok(())
    } else {
        Err(RestError::BadBody("invalid revoke reply".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_and_non_list_value_are_discarded_at_decode() {
        let reply: PlatformsReply = serde_json::from_value(json!({
            "env_path": "/synthetic-path-not-retained",
            "platforms": [{"env_vars": [
                {"key": "CREDENTIAL", "is_set": true, "value": "stored-value", "redacted_value": "stored-preview"},
                {"key": "ALLOWLIST", "is_list": true, "value": "person-one,person-two"}
            ]}]
        })).expect("platform reply");
        let fields = &reply.platforms[0].env_vars;
        assert!(fields[0].value.is_none());
        assert_eq!(fields[1].value.as_deref(), Some("person-one,person-two"));
        let debug = format!("{reply:?} {fields:?}");
        for private in [
            "stored-value",
            "stored-preview",
            "person-one",
            "person-two",
            "synthetic-path",
        ] {
            assert!(!debug.contains(private));
        }
    }

    #[test]
    fn wire_nulls_are_defaults_and_debug_withholds_people_and_links() {
        let reply: PlatformsReply =
            serde_json::from_value(json!({"platforms": null})).expect("null list");
        assert!(reply.platforms.is_empty());
        let test: TestReply =
            serde_json::from_value(json!({"ok": null, "message": null, "state": null}))
                .expect("null test fields");
        assert_eq!(test.message, "");
        let pairing: PairingReply = serde_json::from_value(
            json!({"pending": [{"user_id": "personal-id", "user_name": "personal-name"}]}),
        )
        .expect("pairing");
        assert!(!format!("{pairing:?} {:?}", pairing.pending).contains("personal-"));
        let update = PlatformUpdate {
            enabled: None,
            env: BTreeMap::from([("CREDENTIAL".into(), Zeroizing::new("typed-value".into()))]),
            clear_env: vec![],
        };
        let debug = format!("{update:?}");
        assert!(debug.contains("CREDENTIAL"));
        assert!(!debug.contains("typed-value"));
    }

    #[test]
    fn components_cannot_change_the_route_or_query() {
        assert_eq!(component("fixture "), "fixture%20");
        assert_eq!(component("a/b?x=#%é"), "a%2Fb%3Fx%3D%23%25%C3%A9");
    }
}
