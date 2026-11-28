//! The dashboard's stored child session contains its resolved reasoning setting.

use std::{path::PathBuf, sync::Arc, time::Duration};

use hermes_client::{
    DiscoverOptions, EnsurePolicy,
    cron_api::encode_component,
    remote::{self, RemoteConfig, RemoteError, TokenStore},
    rest::{Method, NativeHttpMinter, RequestOpts, RestClient, RestError, RestOptions},
};
use serde_json::Value;

pub(crate) enum EffortSource {
    Local(PathBuf),
    Remote {
        config: RemoteConfig,
        store: TokenStore,
    },
}

pub(super) async fn read_effort(
    source: &EffortSource,
    profile: &str,
    child_session_id: &str,
    subagent_id: &str,
) -> Option<(String, String)> {
    if child_session_id.is_empty()
        || !child_session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return None;
    }
    let local = match source {
        EffortSource::Local(home) => {
            let options = DiscoverOptions {
                ensure: EnsurePolicy::Never,
                ..Default::default()
            };
            let endpoint = Arc::new(hermes_client::discover_with(home, &options).await.ok()?);
            let profile_id = endpoint.resolve_profile(profile).ok()?;
            Some(
                RestClient::new(
                    endpoint.api_origin(),
                    Arc::new(NativeHttpMinter::new(
                        home.clone(),
                        endpoint.clone(),
                        profile_id,
                    )),
                    RestOptions::default(),
                )
                .ok()?,
            )
        }
        EffortSource::Remote { .. } => None,
    };
    let path = format!(
        "/api/sessions/{child_session_id}?profile={}",
        encode_component(profile)
    );
    for delay in [0, 200, 550, 1250] {
        if delay != 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        let deadline = Duration::from_secs(2);
        let session = match source {
            EffortSource::Local(_) => match local
                .as_ref()?
                .request(
                    Method::Get,
                    &path,
                    None,
                    RequestOpts {
                        deadline: Some(deadline),
                        ..Default::default()
                    },
                )
                .await
            {
                Ok(session) => session,
                Err(RestError::NotFound { .. }) => continue,
                Err(_) => return None,
            },
            EffortSource::Remote { config, store } => match remote::stored_child_session(
                config,
                store,
                profile,
                child_session_id,
                deadline,
            )
            .await
            {
                Ok(session) => session,
                Err(RemoteError::Http(404)) => continue,
                _ => return None,
            },
        };
        if let Some(effort) = stored_effort(&session, profile, child_session_id, subagent_id) {
            return Some((session["model"].as_str()?.to_owned(), effort));
        }
    }
    None
}

fn stored_effort(
    session: &Value,
    profile: &str,
    child_session_id: &str,
    subagent_id: &str,
) -> Option<String> {
    if session["id"].as_str()? != child_session_id
        || session["profile"].as_str()? != profile
        || session["source"].as_str()? != "subagent"
        || session["model"].as_str()?.is_empty()
    {
        return None;
    }
    let config_json = &session["model_config"];
    let decoded;
    let config = if let Some(text) = config_json.as_str() {
        decoded = serde_json::from_str::<Value>(text).ok()?;
        decoded.as_object()?
    } else {
        config_json.as_object()?
    };
    if config.get("_subagent_id").and_then(Value::as_str) != Some(subagent_id) {
        return None;
    }
    match config.get("reasoning_config") {
        Some(Value::Null) => Some("default".into()),
        Some(Value::Object(reasoning)) => {
            if reasoning.get("enabled").and_then(Value::as_bool) == Some(false) {
                return Some("none".into());
            }
            let effort = reasoning.get("effort")?.as_str()?;
            // The setting is rendered as a short label, including levels newer Hermes adds.
            (!effort.is_empty()
                && effort.len() <= 32
                && effort
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')))
            .then(|| effort.to_owned())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::stored_effort;

    fn row(config: Value) -> Value {
        json!({
            "id": "child", "profile": "default", "source": "subagent", "model": "muse",
            "model_config": config,
        })
    }

    #[test]
    fn reads_newer_effort_from_serialized_child_config() {
        let config = json!({"_subagent_id":"sa-1","reasoning_config":{"enabled":true,"effort":"deep-think"}});
        let session = row(json!(config.to_string()));
        assert_eq!(
            stored_effort(&session, "default", "child", "sa-1").as_deref(),
            Some("deep-think")
        );
        assert_eq!(stored_effort(&session, "other", "child", "sa-1"), None);
        assert_eq!(stored_effort(&session, "default", "other", "sa-1"), None);
        assert_eq!(stored_effort(&session, "default", "child", "other"), None);
    }

    #[test]
    fn distinguishes_default_disabled_and_not_initialized() {
        let reserved = row(json!({"_delegate_from":"parent"}));
        let omitted = row(json!({"_subagent_id":"sa-1"}));
        let default = row(json!({"_subagent_id":"sa-1","reasoning_config":null}));
        let disabled = row(json!({"_subagent_id":"sa-1","reasoning_config":{"enabled":false}}));
        assert_eq!(stored_effort(&reserved, "default", "child", "sa-1"), None);
        assert_eq!(stored_effort(&omitted, "default", "child", "sa-1"), None);
        assert_eq!(
            stored_effort(&default, "default", "child", "sa-1").as_deref(),
            Some("default")
        );
        assert_eq!(
            stored_effort(&disabled, "default", "child", "sa-1").as_deref(),
            Some("none")
        );
    }
}
