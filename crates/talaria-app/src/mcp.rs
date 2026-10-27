use std::{
    collections::BTreeSet,
    future::Future,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use hermes_client::{
    CallError, CallOptions, Client,
    mcp_api::set_server_enabled,
    rest::{RestClient, RestError},
};
use hermes_contract::{
    methods::legacy,
    native::NativeMethod,
    overrides::reload_mcp,
    schemas::{self, McpOauthPollStatus, McpServerRuntimeRow},
};
use serde::Deserialize;
use serde_json::Value;
use talaria_core::mcp::{Failure, McpEffect, McpInput, parse_redirect};
use zeroize::Zeroizing;

type Flow = (String, String);

/// Decode each runtime row independently, preserving rows from the pinned contract.
struct ServerStatus;

impl hermes_contract::method::Method for ServerStatus {
    const NAME: &'static str = legacy::mcp_servers_status::NAME;
    type Params = legacy::mcp_servers_status::Params;
    type Result = ServerStatusResult;
}

impl NativeMethod for ServerStatus {}

#[derive(Debug)]
struct ServerStatusResult(schemas::McpServersStatusResult);

impl<'de> Deserialize<'de> for ServerStatusResult {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = Value::deserialize(deserializer)?;
        let rows = value
            .get_mut("servers")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| <D::Error as serde::de::Error>::custom("invalid MCP status reply"))?;
        rows.retain(|row| serde_json::from_value::<McpServerRuntimeRow>(row.clone()).is_ok());
        serde_json::from_value(value)
            .map(Self)
            .map_err(<D::Error as serde::de::Error>::custom)
    }
}

#[derive(Default)]
struct SignIns {
    open: BTreeSet<Flow>,
    retired: BTreeSet<Flow>,
    closed: bool,
    generation: u64,
}

pub struct McpService {
    client: Client,
    rest: RestClient,
    sign_ins: Mutex<SignIns>,
}

impl McpService {
    pub fn new(client: Client, rest: RestClient, _profile_name: String) -> Self {
        Self {
            client,
            rest,
            sign_ins: Mutex::new(SignIns::default()),
        }
    }

    fn sign_ins(&self) -> MutexGuard<'_, SignIns> {
        // A panic cannot corrupt a flow identifier or undo a cancellation claim.
        self.sign_ins
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Start another overlay lifetime on the same profile connection.
    pub fn reopen(&self) {
        let mut sign_ins = self.sign_ins();
        sign_ins.closed = false;
        sign_ins.generation = sign_ins.generation.wrapping_add(1);
    }

    async fn call<M: NativeMethod>(
        &self,
        params: M::Params,
        options: CallOptions,
    ) -> Result<M::Result, Failure> {
        self.client
            .call_with::<M>(params, options)
            .await
            .map_err(|error| failure_from_call(&error))
    }

    pub async fn execute(
        &self,
        effect: McpEffect,
        secret: Option<Zeroizing<String>>,
    ) -> Vec<McpInput> {
        let options = CallOptions {
            timeout: timeout_for(&effect),
        };
        let generation = {
            let sign_ins = self.sign_ins();
            if sign_ins.closed && !matches!(effect, McpEffect::CancelSignIn { .. }) {
                return not_sent_reply(effect);
            }
            sign_ins.generation
        };
        let input = match effect {
            McpEffect::LoadList => McpInput::ListLoaded(
                self.call::<legacy::mcp_servers_list::Marker>(
                    schemas::ProfileParams { profile: None },
                    options,
                )
                .await
                .map(|reply| reply.servers),
            ),
            McpEffect::LoadStatus => McpInput::StatusLoaded(
                self.call::<ServerStatus>(schemas::ProfileParams { profile: None }, options)
                    .await
                    .map(|reply| reply.0.servers),
            ),
            McpEffect::LoadCatalog => McpInput::CatalogLoaded(
                self.call::<legacy::mcp_catalog::Marker>(
                    schemas::ProfileParams { profile: None },
                    options,
                )
                .await
                .map(|reply| reply.servers),
            ),
            McpEffect::AddServer {
                name,
                preset,
                config,
                token,
            } => {
                let bearer_token = if token {
                    match secret.as_deref().filter(|value| !value.is_empty()) {
                        Some(value) => Some(value.to_string()),
                        None => {
                            return vec![McpInput::Added {
                                name,
                                result: Err(missing_secret()),
                            }];
                        }
                    }
                } else {
                    None
                };
                let result = self
                    .call::<legacy::mcp_servers_add::Marker>(
                        schemas::McpServersAddParams {
                            name: name.clone(),
                            preset,
                            config,
                            bearer_token,
                            profile: None,
                        },
                        options,
                    )
                    .await
                    .and_then(|reply| {
                        if reply.ok {
                            Ok(reply.server)
                        } else {
                            Err(Failure::BadReply)
                        }
                    })
                    .map_err(|failure| {
                        redact_failure(
                            failure,
                            &[if token {
                                secret.as_ref().map_or("", |value| value.as_str())
                            } else {
                                ""
                            }],
                        )
                    });
                McpInput::Added { name, result }
            }
            McpEffect::SetKey { name, env_var, .. } => {
                let Some(value) = secret.as_deref().filter(|value| !value.is_empty()) else {
                    return vec![McpInput::KeySaved {
                        name,
                        env_var,
                        result: Err(missing_secret()),
                    }];
                };
                let result = self
                    .call::<legacy::mcp_servers_set_api_key::Marker>(
                        schemas::McpServersSetApiKeyParams {
                            name: name.clone(),
                            env_var: env_var.clone(),
                            value: value.to_string(),
                            profile: None,
                        },
                        options,
                    )
                    .await
                    .and_then(|reply| {
                        if reply.ok {
                            Ok(reply.env_var)
                        } else {
                            Err(Failure::BadReply)
                        }
                    })
                    .map_err(|failure| redact_failure(failure, &[value]));
                McpInput::KeySaved {
                    name,
                    env_var,
                    result,
                }
            }
            McpEffect::Test { name } => {
                let result = self
                    .call::<legacy::mcp_servers_test::Marker>(name_params(&name), options)
                    .await;
                McpInput::Tested { name, result }
            }
            McpEffect::SetEnabled { name, enabled } => {
                let result = set_server_enabled(&self.rest, &name, enabled)
                    .await
                    .map_err(|error| failure_from_rest(&error))
                    .and_then(|reply| {
                        if reply.ok && reply.name == name {
                            Ok(reply.enabled)
                        } else {
                            Err(Failure::BadReply)
                        }
                    });
                McpInput::Toggled { name, result }
            }
            McpEffect::Remove { name } => {
                let result = self
                    .call::<legacy::mcp_servers_remove::Marker>(name_params(&name), options)
                    .await
                    .and_then(|reply| {
                        if reply.ok && reply.removed {
                            Ok(())
                        } else {
                            Err(Failure::BadReply)
                        }
                    });
                McpInput::Removed { name, result }
            }
            McpEffect::StartSignIn { name } => {
                let result = self
                    .call::<legacy::mcp_servers_oauth_start::Marker>(
                        schemas::McpOauthStartParams {
                            name: name.clone(),
                            client_redirect_uri: None,
                            profile: None,
                        },
                        options,
                    )
                    .await
                    .and_then(|reply| {
                        if reply.ok && !reply.session_id.is_empty() {
                            Ok((reply.session_id, reply.auth_url))
                        } else {
                            Err(Failure::BadReply)
                        }
                    });
                let (stale, cancel) = {
                    let mut sign_ins = self.sign_ins();
                    let stale = sign_ins.closed || sign_ins.generation != generation;
                    let cancel = if let Ok((flow, _)) = &result {
                        let key = (name.clone(), flow.clone());
                        if stale {
                            sign_ins.retired.insert(key)
                        } else {
                            if !sign_ins.retired.contains(&key) {
                                sign_ins.open.insert(key);
                            }
                            false
                        }
                    } else {
                        false
                    };
                    (stale, cancel)
                };
                if stale {
                    if cancel && let Ok((flow, _)) = &result {
                        let _ = tokio::time::timeout(
                            Duration::from_secs(1),
                            send_cancel(self.client.clone(), name.clone(), flow.clone()),
                        )
                        .await;
                    }
                    return Vec::new();
                }
                McpInput::SignInStarted { name, result }
            }
            McpEffect::Poll { name, flow } => {
                let result = self
                    .call::<legacy::mcp_servers_oauth_poll::Marker>(
                        flow_params(&name, &flow),
                        options,
                    )
                    .await;
                if result
                    .as_ref()
                    .is_ok_and(|reply| reply.ok && reply.status != McpOauthPollStatus::Pending)
                {
                    let mut sign_ins = self.sign_ins();
                    let key = (name.clone(), flow);
                    sign_ins.open.remove(&key);
                    sign_ins.retired.insert(key);
                }
                McpInput::Polled { name, result }
            }
            McpEffect::CancelSignIn { name, flow } => {
                let send = {
                    let mut sign_ins = self.sign_ins();
                    let key = (name.clone(), flow.clone());
                    sign_ins.open.remove(&key);
                    sign_ins.retired.insert(key)
                };
                if send {
                    send_cancel(self.client.clone(), name, flow).await;
                }
                return Vec::new();
            }
            McpEffect::RelayCallback { name, flow } => {
                let result = self.relay_callback(&name, &flow, secret, options).await;
                McpInput::CallbackRelayed { name, result }
            }
            McpEffect::Reload {
                session_id,
                confirm,
            } => McpInput::Reloaded(
                self.call::<reload_mcp::Marker>(
                    reload_mcp::Params {
                        session_id,
                        confirm: confirm.then_some(true),
                    },
                    options,
                )
                .await,
            ),
            McpEffect::ForgetSecrets
            | McpEffect::OpenUrl(_)
            | McpEffect::CopyText(_)
            | McpEffect::Close => return Vec::new(),
        };
        vec![input]
    }

    async fn relay_callback(
        &self,
        name: &str,
        flow: &str,
        secret: Option<Zeroizing<String>>,
        options: CallOptions,
    ) -> Result<(), Failure> {
        let secret = secret.ok_or_else(missing_secret)?;
        let parts = parse_redirect(&secret).map_err(Failure::BadPaste)?;
        let code = parts
            .code
            .map(decode_component)
            .transpose()?
            .map(Zeroizing::new);
        let state = Zeroizing::new(decode_component(parts.state)?);
        let error = parts
            .error
            .map(decode_component)
            .transpose()?
            .map(Zeroizing::new);
        let iss = parts
            .iss
            .map(decode_component)
            .transpose()?
            .map(Zeroizing::new);
        let sensitive: Vec<&str> = [
            Some(secret.as_str()),
            parts.code,
            Some(parts.state),
            parts.error,
            parts.iss,
            code.as_ref().map(|value| value.as_str()),
            Some(state.as_str()),
            error.as_ref().map(|value| value.as_str()),
            iss.as_ref().map(|value| value.as_str()),
        ]
        .into_iter()
        .flatten()
        .collect();
        let reply = self
            .call::<legacy::mcp_servers_oauth_callback::Marker>(
                schemas::McpOauthCallbackParams {
                    name: name.to_string(),
                    session_id: flow.to_string(),
                    profile: None,
                    code: code.as_ref().map(|value| value.to_string()),
                    state: Some(state.to_string()),
                    error: error.as_ref().map(|value| value.to_string()),
                    iss: iss.as_ref().map(|value| value.to_string()),
                },
                options,
            )
            .await
            .map_err(|failure| redact_failure(failure, &sensitive))?;
        if reply.ok {
            Ok(())
        } else {
            Err(redact_failure(
                Failure::Rpc {
                    code: 0,
                    message: reply
                        .error_message
                        .unwrap_or_else(|| "Sign-in callback was refused.".into()),
                    reason: None,
                    detail: None,
                },
                &sensitive,
            ))
        }
    }

    /// Claim cancellations before spawning cleanup, so a later reopen stays open.
    pub fn closing(&self) -> impl Future<Output = ()> + Send + 'static + use<> {
        let open = {
            let mut sign_ins = self.sign_ins();
            sign_ins.closed = true;
            let open = std::mem::take(&mut sign_ins.open);
            sign_ins.retired.extend(open.iter().cloned());
            open
        };
        let client = self.client.clone();
        async move {
            let mut tasks = tokio::task::JoinSet::new();
            for (name, flow) in open {
                tasks.spawn(send_cancel(client.clone(), name, flow));
            }
            let _ = tokio::time::timeout(Duration::from_secs(1), async {
                while tasks.join_next().await.is_some() {}
            })
            .await;
        }
    }

    /// Cancel every known sign-in, allowing at most one second for shutdown.
    pub async fn close(&self) {
        self.closing().await;
    }
}

fn name_params(name: &str) -> schemas::McpServerNameParams {
    schemas::McpServerNameParams {
        name: name.to_string(),
        profile: None,
    }
}

fn flow_params(name: &str, flow: &str) -> schemas::McpOauthFlowParams {
    schemas::McpOauthFlowParams {
        name: name.to_string(),
        session_id: flow.to_string(),
        profile: None,
    }
}

async fn send_cancel(client: Client, name: String, flow: String) {
    let _ = client
        .call_with::<legacy::mcp_servers_oauth_cancel::Marker>(
            flow_params(&name, &flow),
            CallOptions {
                timeout: Some(Duration::from_secs(5)),
            },
        )
        .await;
}

fn missing_secret() -> Failure {
    Failure::NotSent("Enter a value before saving.".into())
}

fn not_sent_reply(effect: McpEffect) -> Vec<McpInput> {
    let failure = || Failure::NotSent("The MCP screen closed before the request was sent.".into());
    let input = match effect {
        McpEffect::LoadList => McpInput::ListLoaded(Err(failure())),
        McpEffect::LoadStatus => McpInput::StatusLoaded(Err(failure())),
        McpEffect::LoadCatalog => McpInput::CatalogLoaded(Err(failure())),
        McpEffect::AddServer { name, .. } => McpInput::Added {
            name,
            result: Err(failure()),
        },
        McpEffect::SetKey { name, env_var, .. } => McpInput::KeySaved {
            name,
            env_var,
            result: Err(failure()),
        },
        McpEffect::Test { name } => McpInput::Tested {
            name,
            result: Err(failure()),
        },
        McpEffect::SetEnabled { name, .. } => McpInput::Toggled {
            name,
            result: Err(failure()),
        },
        McpEffect::Remove { name } => McpInput::Removed {
            name,
            result: Err(failure()),
        },
        McpEffect::StartSignIn { name } => McpInput::SignInStarted {
            name,
            result: Err(failure()),
        },
        McpEffect::Poll { name, .. } => McpInput::Polled {
            name,
            result: Err(failure()),
        },
        McpEffect::RelayCallback { name, .. } => McpInput::CallbackRelayed {
            name,
            result: Err(failure()),
        },
        McpEffect::Reload { .. } => McpInput::Reloaded(Err(failure())),
        McpEffect::CancelSignIn { .. }
        | McpEffect::ForgetSecrets
        | McpEffect::OpenUrl(_)
        | McpEffect::CopyText(_)
        | McpEffect::Close => return Vec::new(),
    };
    vec![input]
}

fn redact_failure(mut failure: Failure, sensitive: &[&str]) -> Failure {
    let redact = |text: &mut String| {
        for secret in sensitive.iter().copied().filter(|value| !value.is_empty()) {
            if text.contains(secret) {
                *text = text.replace(secret, "[hidden]");
            }
        }
    };
    match &mut failure {
        Failure::Rpc {
            message,
            reason,
            detail,
            ..
        } => {
            redact(message);
            if let Some(reason) = reason {
                redact(reason);
            }
            if let Some(detail) = detail {
                redact(detail);
            }
        }
        Failure::Http {
            detail: Some(detail),
            ..
        }
        | Failure::NotSent(detail) => redact(detail),
        _ => {}
    }
    failure
}

fn decode_component(input: &str) -> Result<String, Failure> {
    let mut bytes = input.bytes();
    let mut decoded = Zeroizing::new(Vec::with_capacity(input.len()));
    while let Some(byte) = bytes.next() {
        match byte {
            b'%' => {
                let high = bytes.next().and_then(|byte| char::from(byte).to_digit(16));
                let low = bytes.next().and_then(|byte| char::from(byte).to_digit(16));
                let (Some(high), Some(low)) = (high, low) else {
                    return Err(Failure::BadReply);
                };
                decoded.push((high * 16 + low) as u8);
            }
            b'+' => decoded.push(b' '),
            byte => decoded.push(byte),
        }
    }
    String::from_utf8(decoded.to_vec()).map_err(|_| Failure::BadReply)
}

pub fn timeout_for(effect: &McpEffect) -> Option<Duration> {
    let seconds = match effect {
        McpEffect::LoadList | McpEffect::LoadStatus | McpEffect::LoadCatalog => 15,
        McpEffect::AddServer { .. } | McpEffect::SetKey { .. } | McpEffect::Remove { .. } => 30,
        McpEffect::Test { .. } => 60,
        McpEffect::StartSignIn { .. } => 45,
        McpEffect::Poll { .. } | McpEffect::RelayCallback { .. } => 10,
        McpEffect::CancelSignIn { .. } => 5,
        McpEffect::Reload { .. } => 120,
        McpEffect::SetEnabled { .. }
        | McpEffect::ForgetSecrets
        | McpEffect::OpenUrl(_)
        | McpEffect::CopyText(_)
        | McpEffect::Close => return None,
    };
    Some(Duration::from_secs(seconds))
}

pub fn failure_from_call(error: &CallError) -> Failure {
    match error {
        CallError::Rpc(error) => Failure::Rpc {
            code: error.code,
            message: error.message.clone(),
            reason: error.reason.clone(),
            detail: error
                .data
                .as_ref()
                .and_then(|data| data.get("detail"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        },
        CallError::NotConnected | CallError::Encode | CallError::InvalidTimeout => {
            Failure::NotSent(error.next_step().into())
        }
        CallError::TimedOut { .. } | CallError::Unknown { .. } | CallError::BadResult { .. } => {
            Failure::Unknown
        }
    }
}

pub fn failure_from_rest(error: &RestError) -> Failure {
    match error {
        RestError::NotLoopback
        | RestError::TlsNotSupported
        | RestError::Ticket(_)
        | RestError::Connect(_)
        | RestError::Timeout { sent: false }
        | RestError::Dropped { sent: false } => {
            Failure::NotSent("Request was not sent. Check the connection before retrying.".into())
        }
        RestError::Timeout { sent: true } | RestError::Dropped { sent: true } => Failure::Unknown,
        RestError::Unauthorized => Failure::Http {
            status: 401,
            detail: None,
        },
        RestError::Forbidden { detail } => Failure::Http {
            status: 403,
            detail: Some(detail.clone()),
        },
        RestError::NotFound { .. } => Failure::Http {
            status: 404,
            detail: None,
        },
        RestError::Refused { status, detail } => Failure::Http {
            status: *status,
            detail: detail.clone(),
        },
        RestError::Server { status } => Failure::Http {
            status: *status,
            detail: None,
        },
        RestError::BodyTooLarge | RestError::BadBody(_) => Failure::BadReply,
    }
}

pub fn decode_mcp_runtime_rows(value: &Value) -> Vec<McpServerRuntimeRow> {
    hermes_client::mcp_api::decode_runtime_rows(value)
}
