//! Sanitized HTTP evidence for the explicit probe; normal requests are never captured.
use serde_json::{Value, json};
const SENSITIVE: &[&str] = &[
    "access_token",
    "refresh_token",
    "code",
    "code_verifier",
    "state",
    "ticket",
    "code_challenge",
    "user_id",
    "email",
    "display_name",
];
fn clean(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        if SENSITIVE.contains(&key.to_ascii_lowercase().as_str()) {
                            Value::String("[redacted]".into())
                        } else {
                            clean(value)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(array) => Value::Array(array.iter().map(clean).collect()),
        _ => value.clone(),
    }
}
pub fn sanitize(method: &str, url: &str, headers: &[(String, String)], body: &Value) -> Value {
    let url = url::Url::parse(url)
        .ok()
        .and_then(|mut u| {
            let pairs: Vec<_> = u
                .query_pairs()
                .map(|(key, value)| {
                    let sensitive = SENSITIVE.contains(&key.to_ascii_lowercase().as_str());
                    (
                        key.into_owned(),
                        if sensitive {
                            "[redacted]".to_owned()
                        } else {
                            value.into_owned()
                        },
                    )
                })
                .collect();
            u.set_host(Some("gateway.example.com")).ok()?;
            let _ = u.set_username("");
            let _ = u.set_password(None);
            u.set_fragment(None);
            if !pairs.is_empty() {
                u.query_pairs_mut().clear().extend_pairs(pairs);
            }
            Some(u.to_string())
        })
        .unwrap_or_else(|| "[redacted]".into());
    let headers: Vec<_> = headers
        .iter()
        .filter(|(key, _)| {
            !matches!(
                key.to_ascii_lowercase().as_str(),
                "authorization" | "cookie" | "set-cookie"
            )
        })
        .map(|(key, value)| {
            (
                key.clone(),
                if SENSITIVE.contains(&key.to_ascii_lowercase().as_str()) {
                    "[redacted]".into()
                } else {
                    value.clone()
                },
            )
        })
        .collect();
    json!({"method":method,"url":url,"headers":headers,"body":clean(body)})
}

/// Explicit operator probe. It signs in and creates one empty fixture session.
pub async fn probe(
    cfg: &super::RemoteConfig,
    store: &super::TokenStore,
    no_browser: bool,
) -> Result<Value, super::RemoteError> {
    use super::{RemoteError, Secret, WsTicket};
    let gate = super::probe_status(&cfg.base_url).await?;
    let mut http = vec![sanitize(
        "GET",
        super::endpoint(&cfg.base_url, "/api/status").as_str(),
        &[],
        &json!({"auth_required":gate.auth_required,"auth_providers":gate.auth_providers,"auth_flows":gate.auth_flows}),
    )];
    let tokens = super::sign_in(
        cfg,
        super::SignInOptions {
            no_browser,
            paste: false,
            cancel: tokio_util::sync::CancellationToken::new(),
        },
    )
    .await
    .map_err(|_| RemoteError::Other(String::new()))?;
    http.push(sanitize("POST", super::endpoint(&cfg.base_url, "/auth/native/token").as_str(), &[], &json!({"access_token":tokens.access.0,"refresh_token":tokens.refresh.0,"token_type":"Bearer","expires_at":tokens.expires_at,"provider":tokens.provider,"user_id":tokens.user_id})));
    store.save(&tokens)?;
    let refused =
        match super::connect::connect_remote(cfg, WsTicket(Secret("invalid-probe-ticket".into())))
            .await
        {
            Err(crate::ConnectError::UpgradeRejected { status }) => status,
            _ => None,
        };
    let ticket = super::mint_ws_ticket(cfg, store).await?;
    http.push(sanitize(
        "POST",
        super::endpoint(&cfg.base_url, "/api/auth/ws-ticket").as_str(),
        &[],
        &json!({"ticket":ticket.0.0,"ttl_seconds":30}),
    ));
    let dial_url = super::connect::dial_url(cfg, &ticket);
    http.push(sanitize("GET", dial_url.as_str(), &[], &Value::Null));
    let (mut connection, protocol_echo) = super::connect::connect_observed(cfg, ticket, None)
        .await
        .map_err(|_| RemoteError::Other(String::new()))?;
    let handshake = crate::handshake(&mut connection)
        .await
        .map_err(|_| RemoteError::Other(String::new()))?;
    let (client, _inbound) = crate::Client::start(connection.into_parts());
    client.set_param_injector(Some(super::profile_injector("fixture".into())));
    let route = async {
        client.call_raw("session.list", json!({"limit":1}), crate::rpc::CallOptions::default()).await?;
        let session = client.call_raw("session.create", json!({"request_id":format!("talaria-probe-{:016x}", fastrand::u64(..)),"source":"oneshot"}), crate::rpc::CallOptions::default()).await?;
        Ok::<_, crate::CallError>(session["session_id"].as_str().is_some_and(|id| !id.is_empty()) && session["info"]["profile_name"] == "fixture")
    }.await;
    client.close().await;
    let created = route.map_err(|_| RemoteError::Other(String::new()))?;
    Ok(
        json!({"version":1,"hermes_commit":include_str!("../../../hermes-contract/PINNED").lines().find_map(|line| line.strip_prefix("commit=")).unwrap_or("unknown"),"http":http,"refusal_status":refused,"session_authority":handshake.ready.session_authority,"subprotocol_echo":protocol_echo,"fixture_list":true,"fixture_session_created":created,"verified":refused == Some(403) && protocol_echo.is_none() && handshake.ready.session_authority == Some(true) && created}),
    )
}
