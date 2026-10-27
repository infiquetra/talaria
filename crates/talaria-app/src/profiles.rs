//! Runs profile effects against Hermes and keeps only the fields the screen needs.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use hermes_client::control_verbs::{
    ControlVerbError, RESCAN_TIMEOUT, rescan_profiles, scrub_reason, served_and_parked,
};
use hermes_client::rest::{NativeHttpMinter, RestClient, RestError, RestOptions};
use hermes_client::{CallError, CallOptions, Client, DiscoverOptions, EnsurePolicy, discover_with};
use hermes_contract::{method::Method, native::NativeMethod, schemas};
use serde_json::{Value, json};
use talaria_core::profiles::{
    Applied, CallFailure, CreatedProfile, Description, DiscoveryFailure, GatewayView, HostTopology,
    ProfilesEffect, ProfilesInput, RescanFailure, RescanSummary, RosterEntry, Section,
};

pub const LIST_TIMEOUT: Duration = Duration::from_secs(10);
pub const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(15);
pub const CREATE_TIMEOUT: Duration = Duration::from_secs(60);
pub const CONFIGURE_TIMEOUT: Duration = Duration::from_secs(15);

// Generated parameter types emit default keys beyond the permitted request keys.
// Roster and create replies are projected directly so private metadata is not retained.
macro_rules! profile_method {
    ($marker:ident, $name:literal, $result:ty) => {
        struct $marker;
        impl Method for $marker {
            const NAME: &'static str = $name;
            type Params = Value;
            type Result = $result;
        }
        impl NativeMethod for $marker {}
    };
}
profile_method!(ListProfiles, "profiles.list", Value);
profile_method!(
    DescribeProfile,
    "profiles.describe",
    schemas::ProfilesDescribeResult
);
profile_method!(CreateProfile, "profiles.create", Value);
profile_method!(
    ConfigureProfile,
    "profiles.configure",
    schemas::ProfilesConfigureResult
);

/// Runs independent effects using the existing foreground connection.
#[derive(Clone)]
pub struct ProfilesService {
    client: Client,
    home: PathBuf,
    profile: String,
}

impl std::fmt::Debug for ProfilesService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfilesService").finish_non_exhaustive()
    }
}

impl ProfilesService {
    #[must_use]
    pub fn new(client: Client, home: PathBuf, profile: String) -> Self {
        Self {
            client,
            home,
            profile,
        }
    }

    /// Run one effect. The loop owns timers, notices, drawing and editor hand-off.
    pub async fn execute(&self, effect: ProfilesEffect) -> Option<ProfilesInput> {
        Some(match effect {
            ProfilesEffect::Discover { generation } => {
                let result = discover_with(&self.home, &discovery_options())
                    .await
                    .map(|endpoint| gateway_view(&endpoint, &self.home))
                    .map_err(|error| DiscoveryFailure {
                        message: error.to_string(),
                        next_step: error.next_step().to_owned(),
                    });
                ProfilesInput::Discovered(generation, result)
            }
            ProfilesEffect::List { generation } => {
                let result = self
                    .client
                    .call_with::<ListProfiles>(
                        json!({"include_sessions": false}),
                        options(LIST_TIMEOUT),
                    )
                    .await
                    .map_err(|error| list_failure(&error, &self.home))
                    .and_then(|reply| parse_roster(&reply));
                ProfilesInput::Listed(generation, result)
            }
            ProfilesEffect::Topology { generation } => {
                ProfilesInput::Topology(generation, load_topology(&self.home, &self.profile).await)
            }
            ProfilesEffect::Describe { generation, name } => {
                let result = self
                    .client
                    .call_with::<DescribeProfile>(json!({"name": name}), options(DESCRIBE_TIMEOUT))
                    .await
                    .map(description_from_contract)
                    .map_err(|error| call_failure(error, &self.home));
                ProfilesInput::Described(generation, name, result)
            }
            ProfilesEffect::Create(request) => {
                let mut params =
                    json!({"name": request.name, "mirror_credentials": request.mirror_credentials});
                if let Some(description) = request.description {
                    params["description"] = description.into();
                }
                if let Some(source) = request.clone_from {
                    params["clone_from"] = source.into();
                }
                let result = self
                    .client
                    .call_with::<CreateProfile>(params, options(CREATE_TIMEOUT))
                    .await
                    .map_err(|error| call_failure(error, &self.home))
                    .and_then(|reply| {
                        if reply["ok"].as_bool() != Some(true) {
                            return Err(CallFailure::Unreadable);
                        }
                        let name = reply["name"]
                            .as_str()
                            .ok_or(CallFailure::Unreadable)?
                            .to_owned();
                        let copied =
                            reply["mirrored"]["env"] == true || reply["mirrored"]["auth"] == true;
                        Ok(CreatedProfile {
                            name,
                            keys_copied: copied,
                        })
                    });
                ProfilesInput::CreateAnswered(result)
            }
            ProfilesEffect::Configure {
                name,
                section,
                value,
            } => {
                let mut params = json!({"name": name});
                let key = match section {
                    Section::Description => "description",
                    Section::Soul => "soul",
                };
                params[key] = value.into();
                let result = self
                    .client
                    .call_with::<ConfigureProfile>(params, options(CONFIGURE_TIMEOUT))
                    .await
                    .map(|reply| Applied {
                        ok: reply.ok,
                        applied: match section {
                            Section::Description => reply.applied.description.unwrap_or(false),
                            Section::Soul => reply.applied.soul.unwrap_or(false),
                        },
                    })
                    .map_err(|error| call_failure(error, &self.home));
                ProfilesInput::ConfigureAnswered(name, section, result)
            }
            ProfilesEffect::Rescan => {
                let result = rescan_profiles(&self.home, RESCAN_TIMEOUT)
                    .await
                    .map(|reply| {
                        if !reply.multiplex {
                            RescanSummary::SingleProfile
                        } else if reply.pending {
                            RescanSummary::Pending
                        } else {
                            RescanSummary::Finished {
                                added: reply.added.len(),
                                parked: reply.parked.len(),
                                removed: reply.removed.len(),
                            }
                        }
                    })
                    .map_err(rescan_failure);
                ProfilesInput::RescanAnswered(result)
            }
            ProfilesEffect::OpenEditor(_)
            | ProfilesEffect::ScheduleTick { .. }
            | ProfilesEffect::CancelTicks
            | ProfilesEffect::Notice(_)
            | ProfilesEffect::Redraw
            | ProfilesEffect::Close => return None,
        })
    }
}

/// Read optional host topology independently of the foreground socket connection.
pub async fn load_topology(home: &Path, profile: &str) -> Result<HostTopology, String> {
    let endpoint = discover_with(home, &discovery_options())
        .await
        .map_err(|_| "no ticket")?;
    let profile = endpoint.resolve_profile(profile).map_err(|_| "no ticket")?;
    let origin = endpoint.api_origin().to_owned();
    let minter = Arc::new(NativeHttpMinter::new(
        home.to_path_buf(),
        Arc::new(endpoint),
        profile,
    ));
    let rest = RestClient::new(&origin, minter, RestOptions::default())
        .map_err(|error| rest_failure(&error))?;
    let reply = hermes_client::gateway_api::gateway_status(&rest)
        .await
        .map_err(|error| rest_failure(&error))?;
    parse_topology(&reply, home)
}

fn discovery_options() -> DiscoverOptions {
    DiscoverOptions {
        ensure: EnsurePolicy::Never,
        ..DiscoverOptions::default()
    }
}

fn options(timeout: Duration) -> CallOptions {
    CallOptions {
        timeout: Some(timeout),
    }
}

/// Parse roster fields directly without retaining private metadata.
pub fn parse_roster(result: &Value) -> Result<Vec<RosterEntry>, String> {
    let rows = result
        .get("profiles")
        .and_then(Value::as_array)
        .ok_or("unreadable reply")?;
    rows.iter()
        .map(|row| {
            let name = row["name"].as_str().ok_or("unreadable reply")?.to_owned();
            Ok(RosterEntry {
                display_name: row["display_name"].as_str().unwrap_or(&name).to_owned(),
                name,
                is_default: row["is_default"].as_bool().unwrap_or(false),
                model: row["model"].as_str().unwrap_or_default().to_owned(),
                provider: row["provider"].as_str().unwrap_or_default().to_owned(),
                description: row["description"].as_str().unwrap_or_default().to_owned(),
                skill_count: row["skill_count"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(0),
            })
        })
        .collect()
}

/// Parse the generated description result, preserving the soul exactly.
pub fn parse_description(result: &Value) -> Result<Description, String> {
    serde_json::from_value::<schemas::ProfilesDescribeResult>(result.clone())
        .map(description_from_contract)
        .map_err(|_| "unreadable reply".into())
}

fn description_from_contract(reply: schemas::ProfilesDescribeResult) -> Description {
    Description {
        name: reply.name,
        description: reply.description,
        soul: reply.soul,
        model_provider: reply.model.provider,
        model_default: reply.model.default,
        skills: reply
            .skills
            .into_iter()
            .map(|entry| (entry.name, entry.enabled))
            .collect(),
        toolsets: reply
            .toolsets
            .into_iter()
            .map(|entry| (entry.label, entry.enabled))
            .collect(),
        toolsets_pinned: reply.toolsets_pinned,
        mcp_servers: reply
            .mcp_servers
            .into_iter()
            .map(|entry| (entry.name, entry.enabled, entry.transport))
            .collect(),
    }
}

/// Read only the four permitted topology fields and scrub the reason.
pub fn parse_topology(reply: &Value, home: &Path) -> Result<HostTopology, String> {
    let reply = reply.as_object().ok_or("unreadable reply")?;
    let stopped = names(reply.get("parked_profiles").ok_or("unreadable reply")?)?;
    let own_gateway = match reply.get("gateways") {
        None => None,
        Some(value) => Some(
            value
                .as_array()
                .ok_or("unreadable reply")?
                .iter()
                .map(|gateway| {
                    gateway["profile"]
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "unreadable reply".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    Ok(HostTopology {
        stopped,
        own_gateway,
        gateway_mode: reply
            .get("gateway_mode")
            .and_then(Value::as_str)
            .map(str::to_owned),
        single_reason: reply
            .get("multiplex_standalone_reason")
            .and_then(Value::as_str)
            .map(|reason| scrub_reason(reason, home)),
    })
}

fn names(value: &Value) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or("unreadable reply")?
        .iter()
        .map(|name| {
            name.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "unreadable reply".to_owned())
        })
        .collect()
}

#[must_use]
pub fn gateway_view(endpoint: &hermes_client::Endpoint, home: &Path) -> GatewayView {
    let (served, parked) = served_and_parked(endpoint, home);
    GatewayView { served, parked }
}

fn call_failure(error: CallError, home: &Path) -> CallFailure {
    match error {
        CallError::TimedOut { .. } | CallError::Unknown { .. } => CallFailure::Unknown,
        CallError::NotConnected | CallError::Encode | CallError::InvalidTimeout => {
            CallFailure::NotSent
        }
        CallError::BadResult { .. } => CallFailure::Unreadable,
        CallError::Rpc(error) => CallFailure::Refused {
            code: error.code,
            message: scrub_reason(&error.message, home),
            reason: error.reason.map(|reason| scrub_reason(&reason, home)),
        },
    }
}

fn list_failure(error: &CallError, home: &Path) -> String {
    match error {
        CallError::TimedOut { .. } | CallError::Unknown { .. } => "no answer".into(),
        CallError::Rpc(error) => format!(
            "{} {}",
            error.code,
            scrub_reason(error.reason.as_deref().unwrap_or(&error.message), home)
        ),
        CallError::BadResult { .. } => "unreadable reply".into(),
        _ => "not connected".into(),
    }
}

fn rest_failure(error: &RestError) -> String {
    match error {
        RestError::Timeout { .. } => "no answer within 8 s".into(),
        RestError::NotFound { .. } => "not found".into(),
        RestError::Unauthorized | RestError::Forbidden { .. } => "refused".into(),
        RestError::Server { status } => format!("server error {status}"),
        RestError::BadBody(_) | RestError::BodyTooLarge => "unreadable reply".into(),
        RestError::Connect(_) | RestError::Dropped { .. } => "connection failed".into(),
        RestError::Ticket(_) => "no ticket".into(),
        _ => "request failed".into(),
    }
}

fn rescan_failure(error: ControlVerbError) -> RescanFailure {
    match error {
        ControlVerbError::UnknownVerb => RescanFailure::UnknownVerb,
        ControlVerbError::Timeout => RescanFailure::Timeout,
        ControlVerbError::Refused(text) => RescanFailure::Other(text),
        ControlVerbError::InvalidReply => RescanFailure::Other("unreadable reply".into()),
        ControlVerbError::Io(_) => RescanFailure::Other("connection failed".into()),
        ControlVerbError::UnsafeNode { .. } => {
            RescanFailure::Other("the control socket failed a safety check".into())
        }
    }
}
