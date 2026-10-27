//! Execute provider effects without letting credentials enter domain state.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hermes_client::providers_api::{self as api, EndpointBodyWire};
use hermes_client::rest::{RestClient, RestError};
use hermes_client::{CallError, CallOptions, Client};
use hermes_contract::methods::legacy::{model_disconnect, model_options, model_save_key};
use hermes_contract::schemas::{ModelDisconnectParams, ModelOptionsParams, ModelSaveKeyParams};
use serde_json::Value;
use talaria_core::providers::*;
use zeroize::{Zeroize, Zeroizing};

#[derive(Default)]
struct SignIns {
    sessions: HashSet<String>,
    closed: bool,
    generation: u64,
    newest_attempt: u64,
}

impl SignIns {
    fn open(&mut self) {
        self.closed = false;
        self.generation = self.generation.wrapping_add(1);
        self.newest_attempt = 0;
    }

    fn begin_close(&mut self) -> Vec<String> {
        self.closed = true;
        self.sessions.drain().collect()
    }
}

/// One service belongs to one connected profile and outlives its overlay.
pub struct ProviderService {
    client: Client,
    rest: Arc<RestClient>,
    profile_name: String,
    sign_ins: Mutex<SignIns>,
}

impl ProviderService {
    pub fn new(client: Client, rest: impl Into<Arc<RestClient>>, profile_name: String) -> Self {
        Self {
            client,
            rest: rest.into(),
            profile_name,
            sign_ins: Mutex::default(),
        }
    }

    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    /// Called synchronously when opening an overlay, before dispatching its effects.
    pub fn open(&self) {
        self.sign_ins().open();
    }

    fn sign_ins(&self) -> MutexGuard<'_, SignIns> {
        self.sign_ins
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub async fn execute(
        &self,
        effect: ProvidersEffect,
        secret: Option<Zeroizing<String>>,
    ) -> Vec<ProvidersInput> {
        let options = CallOptions {
            timeout: Some(Duration::from_secs(30)),
        };
        let mut input = match effect {
            ProvidersEffect::LoadEnv => ProvidersInput::EnvLoaded(
                api::load_env(&self.rest)
                    .await
                    .map(env_rows)
                    .map_err(rest_failure),
            ),
            ProvidersEffect::LoadOptions => ProvidersInput::OptionsLoaded(
                self.client
                    .call_with::<model_options::Marker>(
                        ModelOptionsParams {
                            include_unconfigured: true,
                            explicit_only: false,
                            refresh: false,
                            profile: None,
                            session_id: None,
                        },
                        options,
                    )
                    .await
                    .map_err(call_failure),
            ),
            ProvidersEffect::LoadOauth => ProvidersInput::OauthLoaded(
                api::load_oauth(&self.rest)
                    .await
                    .map_err(rest_failure)
                    .and_then(|list| list.providers.into_iter().map(account).collect()),
            ),
            ProvidersEffect::LoadEndpoints => ProvidersInput::EndpointsLoaded(
                api::load_endpoints(&self.rest)
                    .await
                    .map_err(rest_failure)
                    .and_then(endpoint_list),
            ),
            ProvidersEffect::CheckKey { env_var } => {
                let result = match secret.as_deref() {
                    Some(key) if !key.is_empty() => {
                        timed(api::check_key(&self.rest, &env_var, key), 20)
                            .await
                            .and_then(|check| {
                                Ok(KeyCheck {
                                    ok: check.ok.ok_or(Failure::BadReply)?,
                                    reachable: check.reachable.ok_or(Failure::BadReply)?,
                                    message: check.message.unwrap_or_default(),
                                })
                            })
                    }
                    _ => Err(missing_secret()),
                };
                ProvidersInput::KeyChecked(result)
            }
            ProvidersEffect::SaveKey { slug } => {
                let result = match secret.as_deref() {
                    Some(key) if !key.is_empty() => self
                        .client
                        .call_with::<model_save_key::Marker>(
                            ModelSaveKeyParams {
                                slug,
                                api_key: key.to_string(),
                                session_id: None,
                            },
                            options,
                        )
                        .await
                        .map(|reply| reply.provider)
                        .map_err(call_failure),
                    _ => Err(missing_secret()),
                };
                ProvidersInput::KeySaved(result)
            }
            ProvidersEffect::Disconnect { slug } => ProvidersInput::Disconnected(
                self.client
                    .call_with::<model_disconnect::Marker>(
                        ModelDisconnectParams {
                            slug,
                            session_id: None,
                        },
                        options,
                    )
                    .await
                    .map(|_| ())
                    .map_err(call_failure),
            ),
            ProvidersEffect::StartSignIn { id, attempt } => return self.start(&id, attempt).await,
            ProvidersEffect::Poll {
                id,
                session_id,
                after,
            } => return self.poll(&id, session_id, after).await,
            ProvidersEffect::CancelSignIn { session_id } => {
                let remove = self.sign_ins().sessions.remove(&session_id);
                if remove {
                    self.cancel(&session_id).await;
                }
                return Vec::new();
            }
            ProvidersEffect::SignOut { id } => ProvidersInput::SignedOut(
                api::sign_out(&self.rest, &id)
                    .await
                    .map_err(rest_failure)
                    .and_then(ok_reply),
            ),
            ProvidersEffect::CheckEndpoint(draft) => {
                let result = endpoint_body(&draft, secret.as_deref().map(String::as_str), false)
                    .map_err(|_| missing_secret());
                ProvidersInput::EndpointChecked(match result {
                    Ok(body) => timed(api::check_endpoint(&self.rest, &body), 20)
                        .await
                        .and_then(|check| {
                            Ok(EndpointCheck {
                                ok: check.ok.ok_or(Failure::BadReply)?,
                                reachable: check.reachable.ok_or(Failure::BadReply)?,
                                message: check.message.unwrap_or_default(),
                                models: check.models.unwrap_or_default(),
                                model_details: check.model_details.unwrap_or_default(),
                                resolved_base_url: check.resolved_base_url,
                            })
                        }),
                    Err(error) => Err(error),
                })
            }
            ProvidersEffect::SaveEndpoint(draft) => {
                let result = endpoint_body(&draft, secret.as_deref().map(String::as_str), true)
                    .map_err(|_| missing_secret());
                ProvidersInput::EndpointSaved(match result {
                    Ok(body) => api::save_endpoint(&self.rest, &body)
                        .await
                        .map_err(rest_failure)
                        .and_then(saved_endpoint_list),
                    Err(error) => Err(error),
                })
            }
            ProvidersEffect::DeleteEndpoint { id } => ProvidersInput::EndpointDeleted(
                api::delete_endpoint(&self.rest, &id)
                    .await
                    .map_err(rest_failure)
                    .and_then(saved_endpoint_list),
            ),
            ProvidersEffect::Close => {
                self.close().await;
                return Vec::new();
            }
            ProvidersEffect::ForgetSecret
            | ProvidersEffect::OpenUrl(_)
            | ProvidersEffect::CopyText(_) => return Vec::new(),
        };
        if let Some(secret) = secret.as_deref() {
            redact_secret_echo(&mut input, secret);
        }
        vec![input]
    }

    async fn start(&self, id: &str, attempt: u64) -> Vec<ProvidersInput> {
        let generation = {
            let mut state = self.sign_ins();
            if state.closed || attempt < state.newest_attempt {
                return Vec::new();
            }
            state.newest_attempt = attempt;
            state.generation
        };
        let result = match timed(api::start_sign_in(&self.rest, id), 20).await {
            Ok(start) => {
                let session_id = start.session_id.clone();
                let result = sign_in_start(start);
                if result.is_err()
                    && let Some(session_id) = session_id
                {
                    self.cancel(&session_id).await;
                }
                result
            }
            Err(error) => Err(error),
        };
        let stale = {
            let mut state = self.sign_ins();
            let stale =
                state.closed || state.generation != generation || state.newest_attempt != attempt;
            if !stale && let Ok(start) = &result {
                state.sessions.insert(start.session_id.clone());
            }
            stale
        };
        if stale {
            if let Ok(start) = result {
                self.cancel(&start.session_id).await;
            }
            return Vec::new();
        }
        vec![ProvidersInput::SignInStarted { attempt, result }]
    }

    async fn poll(&self, id: &str, session_id: String, after: Duration) -> Vec<ProvidersInput> {
        tokio::time::sleep(after).await;
        if !self.sign_ins().sessions.contains(&session_id) {
            return Vec::new();
        }
        let result = timed(api::poll_sign_in(&self.rest, id, &session_id), 10)
            .await
            .and_then(|reply| {
                if reply
                    .session_id
                    .as_ref()
                    .is_some_and(|returned| returned != &session_id)
                {
                    return Err(Failure::BadReply);
                }
                let status = reply.status.ok_or(Failure::BadReply)?;
                Ok(PollReply {
                    status: match status.as_str() {
                        "pending" => PollStatus::Pending,
                        "approved" => PollStatus::Approved,
                        "denied" => PollStatus::Denied,
                        "expired" => PollStatus::Expired,
                        "error" => PollStatus::Error,
                        "cancelled" => PollStatus::Cancelled,
                        _ => PollStatus::Other(status),
                    },
                    error_message: reply.error_message,
                    account_email: reply.account_email,
                    model: reply.model,
                    retryable: reply.retryable,
                    retry_after: reply.retry_after,
                })
            });
        {
            let mut state = self.sign_ins();
            if !state.sessions.contains(&session_id) {
                return Vec::new();
            }
            let terminal = match &result {
                Ok(reply) => !matches!(reply.status, PollStatus::Pending | PollStatus::Other(_)),
                Err(Failure::Http { status, .. }) => (400..500).contains(status) && *status != 429,
                _ => false,
            };
            if terminal {
                state.sessions.remove(&session_id);
            }
        }
        vec![ProvidersInput::SignInPolled { session_id, result }]
    }

    async fn cancel(&self, session_id: &str) {
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            api::cancel_sign_in(&self.rest, session_id),
        )
        .await;
    }

    /// Close now; the returned task only cancels the sessions captured by this call.
    /// It owns its data, so it may be spawned after another overlay has opened.
    pub fn close(&self) -> impl std::future::Future<Output = ()> + Send + 'static + use<> {
        let sessions = self.sign_ins().begin_close();
        let rest = self.rest.clone();
        async move {
            let mut cancels = tokio::task::JoinSet::new();
            for session in sessions {
                let rest = rest.clone();
                cancels.spawn(async move {
                    let _ = tokio::time::timeout(
                        Duration::from_secs(5),
                        api::cancel_sign_in(&rest, &session),
                    )
                    .await;
                });
            }
            while cancels.join_next().await.is_some() {}
        }
    }
}

fn redact_secret_echo(input: &mut ProvidersInput, secret: &str) {
    if secret.is_empty() {
        return;
    }
    match input {
        ProvidersInput::EnvLoaded(Err(error))
        | ProvidersInput::OptionsLoaded(Err(error))
        | ProvidersInput::OauthLoaded(Err(error))
        | ProvidersInput::EndpointsLoaded(Err(error))
        | ProvidersInput::KeyChecked(Err(error))
        | ProvidersInput::KeySaved(Err(error))
        | ProvidersInput::Disconnected(Err(error))
        | ProvidersInput::SignInStarted {
            result: Err(error), ..
        }
        | ProvidersInput::SignInPolled {
            result: Err(error), ..
        }
        | ProvidersInput::SignedOut(Err(error))
        | ProvidersInput::EndpointChecked(Err(error))
        | ProvidersInput::EndpointSaved(Err(error))
        | ProvidersInput::EndpointDeleted(Err(error)) => match error {
            Failure::Refused { message, .. } | Failure::NotSent(message) => {
                redact_text(message, secret)
            }
            Failure::Http {
                detail: Some(detail),
                ..
            } => redact_text(detail, secret),
            _ => {}
        },
        ProvidersInput::KeyChecked(Ok(check)) => redact_text(&mut check.message, secret),
        ProvidersInput::EndpointChecked(Ok(check)) => {
            redact_text(&mut check.message, secret);
            for model in &mut check.models {
                redact_text(model, secret);
            }
            for detail in &mut check.model_details {
                redact_json(detail, secret);
            }
            if let Some(url) = &mut check.resolved_base_url {
                redact_text(url, secret);
            }
        }
        ProvidersInput::KeySaved(result @ Ok(_)) => {
            // The contract row is open and may acquire fields; scrub every string before core.
            let safe = result
                .as_ref()
                .ok()
                .and_then(|row| serde_json::to_value(row).ok())
                .and_then(|mut value| {
                    redact_json(&mut value, secret);
                    serde_json::from_value(value).ok()
                });
            *result = safe.ok_or(Failure::BadReply);
        }
        ProvidersInput::EndpointSaved(Ok(list)) | ProvidersInput::EndpointDeleted(Ok(list)) => {
            redact_text(&mut list.current_provider, secret);
            redact_text(&mut list.current_model, secret);
            for endpoint in &mut list.endpoints {
                for text in [
                    &mut endpoint.id,
                    &mut endpoint.name,
                    &mut endpoint.base_url,
                    &mut endpoint.model,
                    &mut endpoint.source,
                ] {
                    redact_text(text, secret);
                }
                for model in &mut endpoint.models {
                    redact_text(model, secret);
                }
                if let Some(mode) = &mut endpoint.api_mode {
                    redact_text(mode, secret);
                }
            }
        }
        _ => {}
    }
}

fn redact_text(text: &mut String, secret: &str) {
    if text.contains(secret) {
        let safe = text.replace(secret, "[redacted]");
        text.zeroize();
        *text = safe;
    }
}

fn redact_json(value: &mut Value, secret: &str) {
    match value {
        Value::String(text) => redact_text(text, secret),
        Value::Array(values) => {
            for value in values {
                redact_json(value, secret);
            }
        }
        Value::Object(values) => {
            let original = std::mem::take(values);
            for (mut key, mut value) in original {
                redact_text(&mut key, secret);
                redact_json(&mut value, secret);
                values.insert(key, value);
            }
        }
        _ => {}
    }
}

fn missing_secret() -> Failure {
    Failure::NotSent("No key was supplied.".into())
}

async fn timed<T>(
    future: impl std::future::Future<Output = Result<T, RestError>>,
    seconds: u64,
) -> Result<T, Failure> {
    tokio::time::timeout(Duration::from_secs(seconds), future)
        .await
        .map_err(|_| Failure::NotSent(format!("no answer in {seconds} s")))?
        .map_err(rest_failure)
}

pub fn call_failure(error: CallError) -> Failure {
    match error {
        CallError::Rpc(error) => Failure::Refused {
            code: error.code,
            message: error.message,
        },
        CallError::NotConnected => Failure::NotSent("Not connected to Hermes.".into()),
        CallError::Encode => Failure::NotSent("Could not encode the request.".into()),
        CallError::InvalidTimeout => Failure::NotSent("Invalid request timeout.".into()),
        CallError::TimedOut { .. } | CallError::Unknown { .. } | CallError::BadResult { .. } => {
            Failure::Unknown
        }
    }
}

pub fn rest_failure(error: RestError) -> Failure {
    match error {
        RestError::Timeout { sent: true } | RestError::Dropped { sent: true } => Failure::Unknown,
        RestError::Unauthorized => Failure::Http {
            status: 401,
            detail: None,
        },
        RestError::Forbidden { detail } => Failure::Http {
            status: 403,
            detail: Some(detail),
        },
        RestError::NotFound { .. } => Failure::Http {
            status: 404,
            detail: None,
        },
        RestError::Refused { status, detail } => Failure::Http { status, detail },
        RestError::Server { status } => Failure::Http {
            status,
            detail: None,
        },
        RestError::BodyTooLarge | RestError::BadBody(_) => Failure::BadReply,
        RestError::NotLoopback
        | RestError::TlsNotSupported
        | RestError::Ticket(_)
        | RestError::Connect(_)
        | RestError::Timeout { sent: false }
        | RestError::Dropped { sent: false } => Failure::NotSent(error.to_string()),
    }
}

fn ok_reply(reply: Value) -> Result<(), Failure> {
    if reply.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(Failure::BadReply)
    }
}
fn saved_endpoint_list(reply: api::EndpointListWire) -> Result<EndpointList, Failure> {
    if reply.ok == Some(true) {
        endpoint_list(reply)
    } else {
        Err(Failure::BadReply)
    }
}

fn sign_in_start(start: api::OauthStartWire) -> Result<SignInStart, Failure> {
    if start
        .flow
        .as_deref()
        .is_some_and(|flow| flow != "device_code")
    {
        return Err(Failure::BadReply);
    }
    Ok(SignInStart {
        session_id: start
            .session_id
            .filter(|id| !id.is_empty())
            .ok_or(Failure::BadReply)?,
        user_code: start.user_code.ok_or(Failure::BadReply)?,
        verification_url: start.verification_url.ok_or(Failure::BadReply)?,
        expires_in: start.expires_in,
        poll_interval: start.poll_interval,
    })
}

fn endpoint_body<'a>(
    draft: &'a EndpointDraft,
    secret: Option<&'a str>,
    save: bool,
) -> Result<EndpointBodyWire<'a>, ()> {
    let api_key = match draft.api_key {
        ApiKeyChange::Keep => None,
        ApiKeyChange::Clear if save => Some(""),
        ApiKeyChange::Clear => None,
        ApiKeyChange::Typed => Some(secret.filter(|key| !key.is_empty()).ok_or(())?),
    };
    let api_mode = if !save || draft.id.is_empty() {
        Some(draft.api_mode.as_deref().unwrap_or(""))
    } else {
        draft.api_mode.as_deref()
    };
    Ok(EndpointBodyWire {
        id: save.then(|| draft.id.clone()),
        name: Some(draft.name.clone()),
        base_url: Some(draft.base_url.clone()),
        model: Some(draft.model.clone()),
        api_key,
        api_mode: api_mode.map(str::to_owned),
        context_length: save.then_some(draft.context_length).flatten(),
        discover_models: save.then_some(draft.discover_models),
        make_default: save.then_some(false),
        models: if save { draft.models.clone() } else { None },
        model_details: if save {
            draft.model_details.clone()
        } else {
            None
        },
    })
}

fn env_rows(rows: std::collections::BTreeMap<String, api::EnvVarWire>) -> Vec<EnvRow> {
    rows.into_iter()
        .map(|(env_var, row)| {
            let slug = row.provider.unwrap_or_default();
            let profiles: Vec<api::ProviderProfileWire> = row
                .provider_profiles
                .unwrap_or_default()
                .into_iter()
                .filter_map(|profile| serde_json::from_value(profile).ok())
                .collect();
            let shared_with = profiles
                .iter()
                .filter(|profile| profile.provider != slug)
                .map(|profile| (profile.provider.clone(), profile.provider_label.clone()))
                .collect();
            let profile_slugs = profiles
                .into_iter()
                .map(|profile| profile.provider)
                .collect();
            EnvRow {
                env_var,
                slug,
                label: row.provider_label.unwrap_or_default(),
                is_set: row.is_set.unwrap_or(false),
                tail: row.redacted_value.as_deref().and_then(key_tail),
                shared_with,
                primary: row.provider_primary.unwrap_or(false),
                is_password: row.is_password.unwrap_or(false),
                channel_managed: row.channel_managed.unwrap_or(false),
                custom: row.custom.unwrap_or(false),
                profile_slugs,
            }
        })
        .collect()
}

/// Only the exact Hermes long preview yields its final four characters.
pub fn key_tail(preview: &str) -> Option<String> {
    let mask = preview.strip_prefix("«redacted:")?.strip_suffix('»')?;
    let chars: Vec<_> = mask.chars().collect();
    if chars.len() != 11 || chars[4..7] != ['.', '.', '.'] || chars.iter().any(|ch| ch.is_control())
    {
        return None;
    }
    Some(chars[7..].iter().collect())
}

fn account(row: api::OauthProviderWire) -> Result<Account, Failure> {
    let details: api::OauthStatusWire = row
        .status
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| Failure::BadReply)?
        .unwrap_or_default();
    let status = if details.logged_in {
        AccountStatus::SignedIn {
            source_label: details.source_label.unwrap_or_default(),
            expires_in_s: details.expires_at.as_ref().and_then(|expiry| {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
                expires_in_s(expiry, i64::try_from(now).ok()?)
            }),
        }
    } else if let Some(error) = details.error {
        AccountStatus::Unknown(error)
    } else {
        AccountStatus::NotSignedIn
    };
    Ok(Account {
        id: row.id.ok_or(Failure::BadReply)?,
        name: row.name.ok_or(Failure::BadReply)?,
        device_code: row.flow.as_deref() == Some("device_code"),
        cli_command: row.cli_command,
        docs_url: row.docs_url,
        disconnect_hint: row.disconnect_hint,
        disconnect_command: row.disconnect_command,
        disconnectable: row.disconnectable.unwrap_or(false),
        status,
    })
}

fn endpoint_list(list: api::EndpointListWire) -> Result<EndpointList, Failure> {
    let current = list.current.unwrap_or_default();
    Ok(EndpointList {
        endpoints: list
            .endpoints
            .into_iter()
            .map(|row| {
                Ok(Endpoint {
                    id: row.id.ok_or(Failure::BadReply)?,
                    name: row.name.ok_or(Failure::BadReply)?,
                    base_url: row.base_url.ok_or(Failure::BadReply)?,
                    model: row.model.unwrap_or_default(),
                    models: row.models.unwrap_or_default(),
                    api_mode: row.api_mode,
                    context_length: row.context_length,
                    discover_models: row.discover_models.unwrap_or(true),
                    has_api_key: row.has_api_key.unwrap_or(false),
                    is_current: row.is_current.unwrap_or(false),
                    source: row.source.unwrap_or_default(),
                })
            })
            .collect::<Result<_, Failure>>()?,
        current_provider: current.provider.unwrap_or_default(),
        current_model: current.model.unwrap_or_default(),
    })
}

/// Convert a supported absolute expiry to seconds from the supplied wall clock.
pub fn expires_in_s(value: &Value, now_unix: i64) -> Option<i64> {
    expiry_epoch(value).map(|expiry| expiry.saturating_sub(now_unix))
}

/// Return an epoch second from a numeric timestamp or a complete RFC 3339 timestamp.
pub fn expiry_epoch(value: &Value) -> Option<i64> {
    if let Some(number) = value.as_f64() {
        let seconds = if number > 1_000_000_000_000.0 {
            number / 1000.0
        } else {
            number
        };
        return (seconds.is_finite() && seconds >= i64::MIN as f64 && seconds < i64::MAX as f64)
            .then_some(seconds as i64);
    }
    parse_timestamp(value.as_str()?)
}

fn parse_timestamp(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return None;
    }
    fn number(bytes: &[u8]) -> Option<i64> {
        bytes.iter().try_fold(0i64, |value, digit| {
            if !digit.is_ascii_digit() {
                return None;
            }
            Some(value * 10 + i64::from(digit - b'0'))
        })
    }
    let year = number(&bytes[0..4])?;
    let month = number(&bytes[5..7])?;
    let day = number(&bytes[8..10])?;
    let hour = number(&bytes[11..13])?;
    let minute = number(&bytes[14..16])?;
    let second = number(&bytes[17..19])?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if day < 1 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let mut pos = 19;
    if bytes.get(pos) == Some(&b'.') {
        pos += 1;
        let start = pos;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        if pos == start {
            return None;
        }
    }
    let offset = match bytes.get(pos..) {
        Some(b"Z") => 0,
        Some(zone) if zone.len() == 6 && matches!(zone[0], b'+' | b'-') && zone[3] == b':' => {
            let hours = number(&zone[1..3])?;
            let minutes = number(&zone[4..6])?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            (hours * 3600 + minutes * 60) * if zone[0] == b'-' { -1 } else { 1 }
        }
        _ => return None,
    };
    // Gregorian calendar decomposition; March starts the computational year.
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_in_era = year - era * 400;
    let month = month + if month > 2 { -3 } else { 9 };
    let day_in_year = (153 * month + 2) / 5 + day - 1;
    let day_in_era = year_in_era * 365 + year_in_era / 4 - year_in_era / 100 + day_in_year;
    Some(
        (era * 146_097 + day_in_era - 719_468) * 86_400 + hour * 3600 + minute * 60 + second
            - offset,
    )
}

/// Platform browser command with one literal argument, restricted to HTTPS.
pub fn opener_argv(url: &str) -> Option<Vec<String>> {
    let remainder = url.strip_prefix("https://")?;
    let authority = remainder.split(['/', '?', '#']).next()?;
    if authority.is_empty()
        || authority.contains('@')
        || url.chars().any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return None;
    }
    let command = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "linux") {
        "xdg-open"
    } else {
        return None;
    };
    Some(vec![command.into(), url.into()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_checks_never_deliver_echoed_secrets_to_core() {
        let secret = Zeroizing::new("opaque-example-credential".to_string());
        let mut input = ProvidersInput::KeyChecked(Ok(KeyCheck {
            ok: false,
            reachable: true,
            message: format!("Rejected {}. Try again.", secret.as_str()),
        }));
        redact_secret_echo(&mut input, &secret);
        assert!(!format!("{input:?}").contains(secret.as_str()));
        let ProvidersInput::KeyChecked(Ok(check)) = input else {
            panic!("expected key check");
        };
        assert_eq!(check.message, "Rejected [redacted]. Try again.");

        let mut input = ProvidersInput::EndpointChecked(Ok(EndpointCheck {
            ok: true,
            reachable: true,
            message: format!("Accepted {}.", secret.as_str()),
            models: vec![secret.to_string()],
            model_details: vec![serde_json::json!({"id": secret.as_str()})],
            resolved_base_url: Some(format!("https://example.invalid/{}", secret.as_str())),
        }));
        redact_secret_echo(&mut input, &secret);
        assert!(!format!("{input:?}").contains(secret.as_str()));
    }

    #[test]
    fn failures_redact_only_the_supplied_literal() {
        let secret = Zeroizing::new("opaque-example-credential".to_string());
        for error in [
            Failure::Refused {
                code: 4001,
                message: format!("Rejected {}.", secret.as_str()),
            },
            Failure::Http {
                status: 422,
                detail: Some(format!("Rejected {}.", secret.as_str())),
            },
            Failure::NotSent(format!("Rejected {}.", secret.as_str())),
        ] {
            let mut input = ProvidersInput::KeySaved(Err(error));
            redact_secret_echo(&mut input, &secret);
            let rendered = format!("{input:?}");
            assert!(!rendered.contains(secret.as_str()));
            assert!(rendered.contains("Rejected [redacted]."));
        }
        let mut input = ProvidersInput::EndpointSaved(Err(Failure::Http {
            status: 422,
            detail: Some("Exact refusal without a secret.".into()),
        }));
        redact_secret_echo(&mut input, &secret);
        assert!(
            matches!(input, ProvidersInput::EndpointSaved(Err(Failure::Http { status: 422, detail: Some(detail) })) if detail == "Exact refusal without a secret.")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn timed_out_checks_keep_the_exact_failure_text() {
        let result: Result<api::ProviderValidationWire, Failure> =
            timed(std::future::pending(), 20).await;
        let mut input = ProvidersInput::KeyChecked(result.map(|_| unreachable!()));
        redact_secret_echo(&mut input, "opaque-example-credential");
        assert!(
            matches!(input, ProvidersInput::KeyChecked(Err(Failure::NotSent(message))) if message == "no answer in 20 s")
        );
    }

    #[test]
    fn closing_future_can_outlive_its_service_borrow() {
        fn accepts_service(service: &ProviderService) {
            fn requires_detached_future(_: impl std::future::Future<Output = ()> + Send + 'static) {
            }
            requires_detached_future(service.close());
        }
        // This assertion also compiles when the service is only borrowed briefly.
        let _: fn(&ProviderService) = accepts_service;
    }

    #[tokio::test]
    async fn delayed_close_cancellation_keeps_the_reopened_generation() {
        let mut state = SignIns::default();
        state.open();
        let old_generation = state.generation;
        state.sessions.insert("old-session".into());
        let closing_sessions = state.begin_close();
        let cancellation = async move { closing_sessions };
        assert!(state.closed);
        assert!(state.sessions.is_empty());

        // Open and admit a new session before the previous cancellation is polled.
        state.open();
        state.sessions.insert("new-session".into());
        let new_generation = state.generation;
        assert_ne!(old_generation, new_generation);
        assert_eq!(cancellation.await, vec!["old-session"]);
        assert!(!state.closed);
        assert_eq!(state.generation, new_generation);
        assert!(state.sessions.contains("new-session"));

        // A dropped cancellation future also owns only the old snapshot.
        let closing_sessions = state.begin_close();
        let cancellation = async move { closing_sessions };
        state.open();
        state.sessions.insert("another-session".into());
        drop(cancellation);
        assert!(!state.closed);
        assert!(state.sessions.contains("another-session"));
    }

    #[test]
    fn expiry_accepts_seconds_milliseconds_and_calendar_offsets() {
        for value in [
            serde_json::json!(1_700_000_000),
            serde_json::json!(1_700_000_000_000u64),
            serde_json::json!("2023-11-14T22:13:20Z"),
            serde_json::json!("2023-11-14T23:13:20.123+01:00"),
        ] {
            assert_eq!(expiry_epoch(&value), Some(1_700_000_000));
        }
        assert_eq!(
            expiry_epoch(&serde_json::json!("1970-01-01T00:00:00Z")),
            Some(0)
        );
        assert_eq!(
            expiry_epoch(&serde_json::json!("2000-02-29T00:00:00Z")),
            Some(951_782_400)
        );
        for text in [
            "2023-02-29T00:00:00Z",
            "2023-01-00T00:00:00Z",
            "2023-01-01T24:00:00Z",
            "2023-01-01T00:00:00+24:00",
            "+023-01-01T00:00:00Z",
            "2023-01-01T00:00:00.Z",
            "2023-01-01T00:00:00Zjunk",
        ] {
            assert_eq!(expiry_epoch(&serde_json::json!(text)), None, "{text}");
        }
    }

    #[test]
    fn previews_export_only_exact_final_four() {
        assert_eq!(key_tail("«redacted:abcd...wxyz»").as_deref(), Some("wxyz"));
        for text in [
            "«redacted:***»",
            "abcd...wxyz",
            "«redacted:abc...wxyz»",
            "«redacted:abcd...wxyz»suffix",
            "«redacted:abcd...wxy\n»",
        ] {
            assert_eq!(key_tail(text), None);
        }
    }

    #[test]
    fn endpoint_key_changes_and_test_fields_are_exact() {
        let mut draft = EndpointDraft {
            name: "example".into(),
            base_url: "https://example.invalid/v1".into(),
            model: "example-model".into(),
            ..Default::default()
        };
        let body = endpoint_body(&draft, None, true).unwrap();
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["make_default"], false);
        assert!(json.get("api_key").is_none());
        draft.api_key = ApiKeyChange::Clear;
        assert_eq!(endpoint_body(&draft, None, true).unwrap().api_key, Some(""));
        assert_eq!(endpoint_body(&draft, None, false).unwrap().api_key, None);
        draft.api_key = ApiKeyChange::Typed;
        assert!(endpoint_body(&draft, None, true).is_err());
        let secret = Zeroizing::new("test-only-credential".to_string());
        let body = endpoint_body(&draft, Some(&secret), false).unwrap();
        assert_eq!(body.api_key, Some(secret.as_str()));
        assert!(!format!("{body:?}").contains(secret.as_str()));
        assert!(body.make_default.is_none());
        assert!(body.context_length.is_none());
    }

    #[test]
    fn browser_arguments_refuse_other_schemes_and_control_characters() {
        for text in [
            "http://example.invalid",
            "file:///example",
            "javascript:alert(1)",
            "https://",
            "https://example.invalid\n",
            "https://name@example.invalid",
        ] {
            assert!(opener_argv(text).is_none());
        }
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            let args = opener_argv("https://example.invalid/a?b=c&d=e").unwrap();
            assert_eq!(args.len(), 2);
            assert_eq!(args[1], "https://example.invalid/a?b=c&d=e");
        }
    }
}
