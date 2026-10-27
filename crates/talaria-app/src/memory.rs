//! Executes memory effects for a single connection profile.

use std::{path::PathBuf, sync::Arc};

use hermes_client::{
    memory_api::{self as api, MemoryApiError},
    rest::{NativeHttpMinter, RestClient, RestError, RestOptions},
};
use serde_json::{Map, Value, json};
use talaria_core::memory::{self as core, FailText, MemoryEffect, MemoryInput, WriteOutcome};

pub struct MemoryService {
    rest: Arc<RestClient>,
    profile: String,
}

impl MemoryService {
    pub fn new(rest: Arc<RestClient>, profile: String) -> Self {
        Self { rest, profile }
    }

    pub async fn execute(&self, effect: MemoryEffect) -> Vec<MemoryInput> {
        let input = match effect {
            MemoryEffect::LoadStatus => MemoryInput::StatusLoaded(
                api::memory_status(&self.rest)
                    .await
                    .map(status_view)
                    .map_err(|error| self.read_failure(error, 45)),
            ),
            MemoryEffect::LoadCurator => MemoryInput::CuratorLoaded(
                api::curator_status(&self.rest)
                    .await
                    .map(curator_view)
                    .map_err(|error| self.read_failure(error, 15)),
            ),
            MemoryEffect::LoadConfig { name } => {
                let result = api::provider_config(&self.rest, &name)
                    .await
                    .map(config_view)
                    .map_err(|error| match error {
                        MemoryApiError::Rest(RestError::Timeout { .. }) => {
                            FailText("Hermes took too long to answer the settings request.".into())
                        }
                        error => self.read_failure(error, 45),
                    });
                MemoryInput::ConfigLoaded { name, result }
            }
            MemoryEffect::SetProvider { name } => MemoryInput::ProviderSet(
                self.write_result(
                    api::set_memory_provider(&self.rest, &name)
                        .await
                        .map(|active| json!({"ok":true,"active":active})),
                    false,
                    false,
                ),
            ),
            MemoryEffect::SaveConfig {
                name,
                surface,
                values,
            } => {
                let has_secrets = !values.secrets.is_empty();
                let mut payload: Map<String, Value> = values
                    .plain
                    .into_iter()
                    .map(|(key, value)| {
                        let value = match value {
                            Value::Bool(_) => value,
                            Value::String(text) => Value::String(text.trim().into()),
                            value => Value::String(value.to_string()),
                        };
                        (key, value)
                    })
                    .collect();
                // Sensitive text is revealed only while constructing the request payload.
                for (key, value) in &values.secrets {
                    if !value.expose().trim().is_empty() {
                        payload.insert(key.clone(), Value::String(value.expose().trim().into()));
                    }
                }
                let result =
                    api::save_provider_config(&self.rest, &name, api_surface(surface), &payload)
                        .await;
                let result = result.map(|_| match surface {
                    core::ConfigSurface::Declared => json!({"ok":true}),
                    core::ConfigSurface::Plain => json!({"ok":true,"active":name}),
                });
                MemoryInput::ConfigSaved(self.write_result(result, false, has_secrets))
            }
            MemoryEffect::Reset { target } => {
                let target = match target {
                    core::ResetTarget::Memory => api::ResetTarget::Memory,
                    core::ResetTarget::User => api::ResetTarget::User,
                    core::ResetTarget::All => api::ResetTarget::All,
                };
                MemoryInput::ResetDone(
                    self.write_result(
                        api::reset_memory(&self.rest, &self.profile, target)
                            .await
                            .map(|deleted| json!({"ok":true,"deleted":deleted})),
                        true,
                        false,
                    ),
                )
            }
            MemoryEffect::SetPaused(paused) => MemoryInput::PausedSet(
                self.write_result(
                    api::set_curator_paused(&self.rest, paused)
                        .await
                        .map(|paused| json!({"ok":true,"paused":paused})),
                    false,
                    false,
                ),
            ),
            MemoryEffect::RunCurator => MemoryInput::RunStarted(
                self.write_result(
                    api::run_curator(&self.rest, &self.profile)
                        .await
                        .map(|started| json!({"ok":true,"name":started.name,"pid":started.pid})),
                    false,
                    false,
                ),
            ),
            MemoryEffect::PollAction { follow, after } => {
                tokio::time::sleep(after).await;
                MemoryInput::ActionPolled {
                    follow,
                    result: api::action_status(&self.rest, "curator-run", 40)
                        .await
                        .map(action_view)
                        .map_err(|error| self.read_failure(error, 5)),
                }
            }
            _ => return Vec::new(),
        };
        vec![input]
    }

    fn read_failure(&self, error: MemoryApiError, seconds: u64) -> FailText {
        let reason = match error {
            MemoryApiError::Rest(error) => match error {
                RestError::Unauthorized => {
                    "Hermes refused Talaria's ticket. Reconnect, then try again.".into()
                }
                RestError::Forbidden { detail } => format!(
                    "Hermes refused this for profile {} ({}).",
                    sanitize(&self.profile),
                    safe_detail(&detail)
                ),
                RestError::NotFound { route } => format!(
                    "This Hermes has no {}. Talaria needs Hermes v0.22.4 or newer.",
                    sanitize(&route)
                ),
                RestError::Refused { detail, .. } => format!(
                    "Hermes refused: {}",
                    detail
                        .as_deref()
                        .map(safe_detail)
                        .unwrap_or_else(|| "request refused".into())
                ),
                RestError::Server { status } => format!("Hermes failed with status {status}."),
                RestError::Timeout { .. } if seconds == 45 => {
                    "Hermes took more than 45 s (it runs each provider's dependency check)".into()
                }
                RestError::Timeout { .. } => {
                    format!("Could not reach Hermes: request timed out after {seconds} s.")
                }
                RestError::Dropped { .. } => "Could not reach Hermes: connection dropped.".into(),
                RestError::Connect(_) => "Could not reach Hermes: connection failed.".into(),
                RestError::Ticket(_) => {
                    "Hermes refused Talaria's ticket. Reconnect, then try again.".into()
                }
                RestError::BodyTooLarge | RestError::BadBody(_) => {
                    "Hermes returned an invalid reply.".into()
                }
                RestError::NotLoopback | RestError::TlsNotSupported => {
                    "Could not reach Hermes: the gateway must use a local HTTP connection.".into()
                }
            },
            MemoryApiError::BadProviderName => "Talaria will not send this provider name.".into(),
            MemoryApiError::MissingProfile => "A connection profile is required.".into(),
            MemoryApiError::BadReply(path) => format!(
                "Hermes returned an invalid reply field: {}.",
                sanitize(&path)
            ),
        };
        FailText(reason)
    }

    fn write_result(
        &self,
        result: Result<Value, MemoryApiError>,
        reset: bool,
        has_secrets: bool,
    ) -> WriteOutcome {
        match result {
            Ok(value) => WriteOutcome::Ok(value),
            Err(MemoryApiError::Rest(error)) if error.outcome_unknown() => WriteOutcome::Unknown,
            Err(MemoryApiError::Rest(RestError::Unauthorized)) => WriteOutcome::Refused {
                status: 401,
                detail: None,
            },
            Err(MemoryApiError::Rest(RestError::Forbidden { detail })) => WriteOutcome::Refused {
                status: 403,
                detail: if has_secrets && detail != "profile_scope_mismatch" {
                    None
                } else {
                    Some(safe_detail(&detail))
                },
            },
            Err(MemoryApiError::Rest(RestError::NotFound { route })) => WriteOutcome::Refused {
                status: 404,
                detail: Some(sanitize(&route)),
            },
            Err(MemoryApiError::Rest(RestError::Refused { status, detail })) => {
                WriteOutcome::Refused {
                    status,
                    // A server may reflect even a fragment of a submitted credential in its detail.
                    detail: if has_secrets {
                        None
                    } else {
                        detail.as_deref().map(safe_detail)
                    },
                }
            }
            Err(MemoryApiError::Rest(RestError::Server { .. })) if reset => {
                WriteOutcome::Failed(FailText("Hermes could not delete the file.".into()))
            }
            Err(error @ MemoryApiError::Rest(RestError::Server { .. })) => {
                WriteOutcome::Failed(self.read_failure(error, 15))
            }
            Err(
                MemoryApiError::BadReply(_)
                | MemoryApiError::Rest(RestError::BadBody(_) | RestError::BodyTooLarge),
            ) => WriteOutcome::Unknown,
            Err(error) => WriteOutcome::NotSent(self.read_failure(error, 15)),
        }
    }
}

/// Discover an existing gateway without starting it or changing its configuration.
pub(crate) async fn discover_service(home: PathBuf, profile: &str) -> Option<Arc<MemoryService>> {
    use hermes_client::{DiscoverOptions, EnsurePolicy, discover_with};
    let endpoint = Arc::new(
        discover_with(
            &home,
            &DiscoverOptions {
                ensure: EnsurePolicy::Never,
                ..DiscoverOptions::default()
            },
        )
        .await
        .ok()?,
    );
    let profile_id = endpoint.resolve_profile(profile).ok()?;
    let minter = Arc::new(NativeHttpMinter::new(home, endpoint.clone(), profile_id));
    let rest = RestClient::new(endpoint.api_origin(), minter, RestOptions::default()).ok()?;
    Some(Arc::new(MemoryService::new(Arc::new(rest), profile.into())))
}

fn api_surface(surface: core::ConfigSurface) -> api::ConfigSurface {
    match surface {
        core::ConfigSurface::Declared => api::ConfigSurface::Declared,
        core::ConfigSurface::Plain => api::ConfigSurface::Plain,
    }
}
fn status_view(status: api::MemoryStatus) -> core::StatusView {
    core::StatusView {
        active: status.active,
        providers: status
            .providers
            .into_iter()
            .map(|row| core::ProviderRowView {
                name: row.name,
                description: row.description,
                available: row.available,
                configured: row.configured,
                status: row.status,
                setup: core::ProviderSetup {
                    pip_dependencies: row.setup.pip_dependencies,
                    python_dependencies_declared: row.setup.python_dependencies_declared,
                    external_dependencies: row
                        .setup
                        .external_dependencies
                        .into_iter()
                        .map(|dep| core::ExternalDependency {
                            name: dep.name,
                            install: dep.install,
                            check: dep.check,
                        })
                        .collect(),
                    required_env: row.setup.required_env,
                    dependencies_installed: row.setup.dependencies_installed,
                },
            })
            .collect(),
        builtin_files: core::BuiltinFiles {
            memory: status.builtin_files.memory,
            user: status.builtin_files.user,
        },
    }
}
fn curator_view(status: api::CuratorStatus) -> core::CuratorView {
    core::CuratorView {
        enabled: status.enabled,
        paused: status.paused,
        interval_hours: status.interval_hours,
        last_run_at: status.last_run_at,
        min_idle_hours: status.min_idle_hours,
        stale_after_days: status.stale_after_days,
        archive_after_days: status.archive_after_days,
    }
}
fn config_view(config: api::ProviderConfig) -> core::ConfigView {
    core::ConfigView {
        name: config.name,
        label: config.label,
        surface: match config.surface {
            api::ConfigSurface::Declared => core::ConfigSurface::Declared,
            api::ConfigSurface::Plain => core::ConfigSurface::Plain,
        },
        fields: config
            .fields
            .into_iter()
            .map(|field| core::FieldView {
                key: field.key,
                label: field.label,
                kind: match field.kind {
                    api::FieldKind::Text => core::FieldKind::Text,
                    api::FieldKind::Secret => core::FieldKind::Secret,
                    api::FieldKind::Select => core::FieldKind::Select,
                    api::FieldKind::Bool => core::FieldKind::Bool,
                    api::FieldKind::Integer => core::FieldKind::Integer,
                    api::FieldKind::Number => core::FieldKind::Number,
                    api::FieldKind::Json => core::FieldKind::Json,
                    api::FieldKind::Other(kind) => core::FieldKind::Other(kind),
                },
                description: field.description,
                placeholder: field.placeholder,
                required: field.required,
                value: field.value,
                is_set: field.is_set,
                options: field
                    .options
                    .into_iter()
                    .map(|option| core::FieldOption {
                        value: option.value,
                        label: option.label,
                        description: option.description,
                    })
                    .collect(),
                when: field.when,
                minimum: field.minimum,
                maximum: field.maximum,
            })
            .collect(),
    }
}
fn action_view(status: api::ActionStatus) -> core::ActionView {
    core::ActionView {
        name: status.name,
        running: status.running,
        exit_code: status.exit_code,
        pid: status.pid,
        lines: status.lines,
    }
}
fn safe_detail(text: &str) -> String {
    sanitize(text).chars().take(200).collect()
}
fn sanitize(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        let code = u32::from(ch);
        match code {
            0x09 => result.push_str("    "),
            0x00..=0x1f => {
                if let Some(picture) = char::from_u32(0x2400 + code) {
                    result.push(picture);
                }
            }
            0x7f => result.push('\u{2421}'),
            0x80..=0x9f
            | 0x061c
            | 0x200e
            | 0x200f
            | 0x2028..=0x202e
            | 0x2066..=0x2069
            | 0xe0000..=0xe007f => result.push_str(&format!("‹U+{code:04X}›")),
            _ => result.push(ch),
        }
    }
    result
}
