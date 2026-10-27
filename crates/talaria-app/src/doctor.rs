//! Gather gateway diagnostics without starting or changing the gateway.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::SinkExt;
use hermes_client::gateway_api::{gateway_status, restart_gateway};
use hermes_client::rest::{NativeHttpMinter, RestClient, RestError, RestOptions};
use hermes_client::{
    DiscoverOptions, DiscoveryError, Endpoint, EnsurePolicy, GatewayState, ProfileId, Purpose,
    ReadVerb, Scope, connect, control_query, handshake, mint_ticket,
};
use hermes_contract::pinned;
use serde_json::Value;
use talaria_core::doctor::{
    AttachFacts, DoctorFacts, GatewayFacts, IdentityFacts, Pin, PostResult, ProbeResult,
    StatusFacts, StatusPage, WorkSummary, scrub_text,
};

pub struct DoctorService {
    pub home: PathBuf,
    pub options: DiscoverOptions,
}

impl DoctorService {
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            options: DiscoverOptions {
                ensure: EnsurePolicy::Never,
                ..DiscoverOptions::default()
            },
        }
    }

    fn scrub(&self, text: &str) -> String {
        let supplied = self.home.to_string_lossy();
        let scrubbed = scrub_text(text, Some(&supplied));
        match self.home.canonicalize() {
            Ok(canonical) => scrub_text(&scrubbed, Some(&canonical.to_string_lossy())),
            Err(_) => scrubbed,
        }
    }

    fn string(&self, value: &Value) -> Option<String> {
        value.as_str().map(|text| self.scrub(text))
    }

    fn unreachable(&self, error: &DiscoveryError) -> GatewayFacts {
        GatewayFacts::Unreachable {
            message: self.scrub(&error.to_string()),
            next_step: self.scrub(error.next_step()),
        }
    }

    /// Read identity once, then independently check attach and public health.
    pub async fn gather(&self, profile: &str) -> DoctorFacts {
        let started = Instant::now();
        let mut facts = DoctorFacts {
            talaria_version: self.scrub(env!("CARGO_PKG_VERSION")),
            pin: Pin {
                tag: self.scrub(pinned::TAG),
                // This trusted build constant is retained for the dedicated JSON metadata field.
                commit: pinned::COMMIT.to_owned(),
            },
            profile: self.scrub(profile.trim()),
            gateway: GatewayFacts::Incompatible,
            identity: None,
            attach: AttachFacts::NotTried,
            status: StatusFacts::NotTried,
        };
        let result = match control_query(&self.home, ReadVerb::Identify, &self.options).await {
            Ok(result) => result,
            Err(error) => {
                facts.gateway = self.unreachable(&error);
                return facts;
            }
        };
        let identify_ms = milliseconds(started.elapsed());
        let endpoint = Endpoint::from_identify(&result, &self.home);
        facts.identity = Some(self.identity(&result, endpoint.as_ref().ok()));
        let endpoint = match endpoint {
            Ok(endpoint) => endpoint,
            Err(DiscoveryError::NotReady { .. } | DiscoveryError::InvalidState) => {
                facts.gateway = GatewayFacts::NotReady {
                    state: self
                        .string(&result["state"])
                        .unwrap_or_default()
                        .chars()
                        .take(40)
                        .collect(),
                    drain_reason: self
                        .string(&result["drain_reason"])
                        .map(|s| s.chars().take(40).collect()),
                };
                return facts;
            }
            Err(DiscoveryError::Incompatible) => return facts,
            Err(error) => {
                facts.gateway = self.unreachable(&error);
                return facts;
            }
        };
        facts.gateway = match endpoint.state() {
            GatewayState::Ready => GatewayFacts::Ready {
                instance_id: self.scrub(endpoint.instance_id()),
                epoch: endpoint.authority_epoch(),
                launch: self.string(&result["launch_supervisor"]),
            },
            GatewayState::DrainingForRestart => GatewayFacts::RestartWait {
                instance_id: self.scrub(endpoint.instance_id()),
                epoch: endpoint.authority_epoch(),
            },
        };
        let endpoint = Arc::new(endpoint);
        let (attach, status) = tokio::join!(
            self.attach(&endpoint, profile, identify_ms),
            self.status(endpoint.clone(), profile),
        );
        facts.attach = attach;
        facts.status = status;
        facts
    }

    fn identity(&self, result: &Value, endpoint: Option<&Endpoint>) -> IdentityFacts {
        let served = match endpoint {
            Some(endpoint) => endpoint
                .profiles()
                .iter()
                .map(|p| self.scrub(&p.name))
                .collect(),
            None => result["served_profiles"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.as_object()?.get("home")?.as_str())
                .filter_map(|home| {
                    let home = Path::new(home);
                    if home
                        .parent()
                        .and_then(Path::file_name)
                        .is_some_and(|name| name == "profiles")
                    {
                        home.file_name()
                            .and_then(|name| name.to_str())
                            .map(|name| self.scrub(&name.to_lowercase()))
                    } else {
                        Some(self.scrub("default"))
                    }
                })
                .collect(),
        };
        let parked = result["parked_profiles"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(name, reason)| {
                reason
                    .as_str()
                    .map(|reason| (self.scrub(name), self.scrub(reason)))
            })
            .collect();
        IdentityFacts {
            code_version: self.string(&result["code_version"]),
            code_sha: result["code_sha"]
                .as_str()
                .map(|sha| self.scrub(&sha.chars().take(8).collect::<String>())),
            capabilities: result["capabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|cap| self.string(cap))
                .collect(),
            served,
            parked,
        }
    }

    async fn attach(&self, endpoint: &Endpoint, profile: &str, identify_ms: u64) -> AttachFacts {
        let Ok(profile_id) = endpoint.resolve_profile(profile) else {
            return AttachFacts::NotTried;
        };
        let started = Instant::now();
        let ticket = match mint_ticket(
            &self.home,
            endpoint,
            &profile_id,
            Purpose::Interactive,
            Scope::profile(),
        )
        .await
        {
            Ok(ticket) => ticket,
            Err(error) => {
                return AttachFacts::Failed {
                    message: self.scrub(&error.to_string()),
                    next_step: self.scrub(error.next_step()),
                };
            }
        };
        let ticket_ms = milliseconds(started.elapsed());
        let started = Instant::now();
        let mut connection = match connect(endpoint, ticket).await {
            Ok(connection) => connection,
            Err(error) => {
                return AttachFacts::Failed {
                    message: self.scrub(&error.to_string()),
                    next_step: self.scrub(error.next_step()),
                };
            }
        };
        let upgrade_ms = milliseconds(started.elapsed());
        let result = handshake(&mut connection).await;
        let mut parts = connection.into_parts();
        // Closing is best effort and bounded; a successful handshake remains a fact.
        let _ = tokio::time::timeout(Duration::from_secs(1), parts.sink.close()).await;
        match result {
            Ok(handshake) => AttachFacts::Attached {
                describe_capabilities: handshake
                    .describe
                    .capabilities
                    .iter()
                    .map(|s| self.scrub(s))
                    .collect(),
                identify_ms,
                ticket_ms,
                upgrade_ms,
                ready_ms: milliseconds(handshake.timings.ready),
                capabilities_ms: milliseconds(handshake.timings.capabilities),
                describe_ms: milliseconds(handshake.timings.describe),
            },
            Err(error) => AttachFacts::Failed {
                message: self.scrub(&error.to_string()),
                next_step: self.scrub(error.next_step()),
            },
        }
    }

    fn rest(&self, endpoint: Arc<Endpoint>, selected: &str) -> Result<RestClient, RestError> {
        let profile = root_profile(&endpoint, selected).ok_or(RestError::NotLoopback)?;
        let minter = Arc::new(NativeHttpMinter::new(
            self.home.clone(),
            endpoint.clone(),
            profile,
        ));
        RestClient::new(endpoint.api_origin(), minter, RestOptions::default())
    }

    async fn status(&self, endpoint: Arc<Endpoint>, profile: &str) -> StatusFacts {
        let result = match self.rest(endpoint, profile) {
            Ok(rest) => gateway_status(&rest).await,
            Err(error) => Err(error),
        };
        match result {
            Ok(value) if value.is_object() => StatusFacts::Read(self.status_page(&value)),
            Ok(_) | Err(RestError::BadBody(_) | RestError::BodyTooLarge) => unreadable("not JSON"),
            Err(RestError::Server { status: 503 }) => StatusFacts::NotReady,
            Err(RestError::Timeout { .. }) => unreadable("timed out after 8 s"),
            Err(error) => match http_status(&error) {
                Some(status) => unreadable(&format!("HTTP {status}")),
                None => unreadable("connection failed"),
            },
        }
    }

    fn status_page(&self, value: &Value) -> StatusPage {
        let components = &value["components"];
        StatusPage {
            config_version: value["config_version"].as_i64(),
            latest_config_version: value["latest_config_version"].as_i64(),
            heartbeat_stale_s: value["gateway_heartbeat_stale_s"].as_f64(),
            active_agents: value["active_agents"].as_u64(),
            storage_status: self.string(&components["storage"]["status"]),
            storage_reason: self.string(&components["storage"]["reason"]),
            platforms_status: self.string(&components["platforms"]["status"]),
            platforms_configured: components["platforms"]["configured"].as_u64(),
            platforms_connected: components["platforms"]["connected"].as_u64(),
            dashboard_status: self.string(&components["dashboard"]["status"]),
            dashboard_errors: components["dashboard"]["recent_unhandled_errors"].as_u64(),
            dashboard_selftest: self.string(&components["dashboard"]["selftest"]),
            standalone_reason: self.string(&value["multiplex_standalone_reason"]),
        }
    }

    /// Count only work kinds; discard session keys, model names and goals.
    // The shared confirmation interface intentionally carries only known or unknown counts.
    #[allow(clippy::result_unit_err)]
    pub async fn active_work(&self) -> Result<WorkSummary, ()> {
        let options = DiscoverOptions {
            control_timeout: Duration::from_secs(8),
            ensure: EnsurePolicy::Never,
        };
        let result = control_query(&self.home, ReadVerb::ActiveWork, &options)
            .await
            .map_err(|_| ())?;
        work_summary(&result)
    }

    /// Re-check identity before requesting the already-confirmed restart.
    pub async fn post_restart(&self, old_instance: &str, selected_profile: &str) -> PostResult {
        let endpoint = match control_query(&self.home, ReadVerb::Identify, &self.options)
            .await
            .and_then(|result| Endpoint::from_identify(&result, &self.home))
        {
            Ok(endpoint)
                if endpoint.state() == GatewayState::Ready
                    && endpoint.instance_id() == old_instance =>
            {
                Arc::new(endpoint)
            }
            _ => return PostResult::Changed,
        };
        let result = match self.rest(endpoint, selected_profile) {
            Ok(rest) => restart_gateway(&rest).await,
            Err(error) => Err(error),
        };
        self.post_result(result)
    }

    fn post_result(&self, result: Result<Value, RestError>) -> PostResult {
        match result {
            Ok(value) if value["ok"] == true => PostResult::Accepted,
            Ok(_) | Err(RestError::BadBody(_) | RestError::BodyTooLarge) => PostResult::Unknown,
            Err(error) if error.outcome_unknown() => PostResult::Unknown,
            Err(error) => {
                let status = http_status(&error);
                let detail = match &error {
                    RestError::Unauthorized => "Unauthorized".to_owned(),
                    RestError::Forbidden { detail } => self.scrub(detail),
                    RestError::Refused { detail, .. } => {
                        self.scrub(detail.as_deref().unwrap_or_default())
                    }
                    RestError::NotFound { .. } | RestError::Server { .. } => String::new(),
                    _ => self.scrub(&error.to_string()),
                };
                PostResult::Refused {
                    http_status: status,
                    detail,
                }
            }
        }
    }

    pub async fn probe(&self) -> ProbeResult {
        let options = DiscoverOptions {
            control_timeout: Duration::from_secs(2),
            ensure: EnsurePolicy::Never,
        };
        match control_query(&self.home, ReadVerb::Identify, &options)
            .await
            .and_then(|result| Endpoint::from_identify(&result, &self.home))
        {
            Ok(endpoint) if endpoint.state() == GatewayState::Ready => ProbeResult::Ready {
                instance_id: self.scrub(endpoint.instance_id()),
            },
            Err(DiscoveryError::NoGateway | DiscoveryError::ControlTimeout) => {
                ProbeResult::NotAnswering
            }
            _ => ProbeResult::NotReady,
        }
    }
}

fn milliseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn work_summary(result: &Value) -> Result<WorkSummary, ()> {
    let Some(work) = result["active_work"].as_array() else {
        return result["active_agents"]
            .as_u64()
            .map(|chat| WorkSummary { chat, other: 0 })
            .ok_or(());
    };
    let mut summary = WorkSummary { chat: 0, other: 0 };
    for item in work {
        if item["kind"].as_str() == Some("chat") {
            summary.chat = summary.chat.saturating_add(1);
        } else {
            summary.other = summary.other.saturating_add(1);
        }
    }
    Ok(summary)
}

fn root_profile(endpoint: &Endpoint, selected: &str) -> Option<ProfileId> {
    endpoint
        .resolve_profile("default")
        .ok()
        .or_else(|| endpoint.resolve_profile(selected).ok())
        .or_else(|| {
            endpoint
                .profiles()
                .first()
                .map(|profile| profile.id.clone())
        })
}

fn unreadable(reason: &str) -> StatusFacts {
    StatusFacts::Unreadable {
        reason: reason.to_owned(),
    }
}

fn http_status(error: &RestError) -> Option<u16> {
    match error {
        RestError::Unauthorized => Some(401),
        RestError::Forbidden { .. } => Some(403),
        RestError::NotFound { .. } => Some(404),
        RestError::Refused { status, .. } | RestError::Server { status } => Some(*status),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn work_counts_fall_back_without_exposing_job_fields() {
        assert_eq!(
            work_summary(&json!({"active_agents": 3})),
            Ok(WorkSummary { chat: 3, other: 0 })
        );
        assert_eq!(
            work_summary(&json!({"active_work": false, "active_agents": 2})),
            Ok(WorkSummary { chat: 2, other: 0 })
        );
        assert_eq!(
            work_summary(&json!({"active_work": [{"kind": "chat"}, {"kind": "delegation"}, {}]})),
            Ok(WorkSummary { chat: 1, other: 2 })
        );
        assert_eq!(work_summary(&json!({})), Err(()));
    }

    #[tokio::test]
    async fn missing_gateway_never_creates_files() {
        let home = tempfile::tempdir().unwrap();
        let service = DoctorService::new(home.path().to_path_buf());
        assert!(matches!(service.options.ensure, EnsurePolicy::Never));
        let facts = service.gather(" fixture ").await;
        assert_eq!(facts.profile, "fixture");
        assert!(matches!(facts.gateway, GatewayFacts::Unreachable { .. }));
        assert!(facts.identity.is_none());
        assert!(matches!(facts.attach, AttachFacts::NotTried));
        assert!(matches!(facts.status, StatusFacts::NotTried));
        assert!(matches!(service.probe().await, ProbeResult::NotAnswering));
        assert!(matches!(
            service.post_restart("old", "fixture").await,
            PostResult::Changed
        ));
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    }

    #[test]
    fn identity_uses_safe_names_when_endpoint_is_not_ready() {
        let home = tempfile::tempdir().unwrap();
        let service = DoctorService::new(home.path().to_path_buf());
        let named = home.path().join("profiles").join("fixture");
        let reason = format!("{} did not open", home.path().join("state.db").display());
        let identity = service.identity(
            &json!({
                "code_version": "0.22.4", "code_sha": 42,
                "capabilities": ["session-authority-v1", false],
                "served_profiles": [{"home": home.path()}, {"home": named}, null],
                "parked_profiles": {"fixture": reason, "default": false},
            }),
            None,
        );
        assert_eq!(identity.served, ["default", "fixture"]);
        assert_eq!(
            identity.parked,
            [("fixture".to_owned(), "<path> did not open".to_owned())]
        );
        assert!(identity.code_sha.is_none());
        assert_eq!(identity.capabilities, ["session-authority-v1"]);
    }

    #[test]
    fn status_fields_are_typed_and_scrubbed() {
        let home = tempfile::tempdir().unwrap();
        let service = DoctorService::new(home.path().to_path_buf());
        let page = service.status_page(&json!({
            "config_version": 1, "latest_config_version": "2",
            "gateway_heartbeat_stale_s": 125.7, "active_agents": 3,
            "components": {
                "storage": {"status":"degraded", "reason": home.path().join("state.db")},
                "platforms": {"status":"degraded", "configured":2, "connected":1},
                "dashboard": {"status":"degraded", "recent_unhandled_errors":0, "selftest":"failing"}
            },
            "multiplex_standalone_reason": home.path().join("state.db"),
            "install_id": "private identity", "gateway_pid":42,
        }));
        assert_eq!(page.config_version, Some(1));
        assert_eq!(page.latest_config_version, None);
        assert_eq!(page.heartbeat_stale_s, Some(125.7));
        assert_eq!(page.storage_reason.as_deref(), Some("<path>"));
        assert_eq!(page.standalone_reason.as_deref(), Some("<path>"));
        assert_eq!(page.dashboard_selftest.as_deref(), Some("failing"));
        assert_eq!(page.platforms_connected, Some(1));
    }

    #[test]
    fn restart_mapping_preserves_uncertainty_and_scrubs_refusals() {
        let home = tempfile::tempdir().unwrap();
        let service = DoctorService::new(home.path().to_path_buf());
        for result in [
            Ok(json!({"ok":false})),
            Err(RestError::Timeout { sent: true }),
            Err(RestError::Dropped { sent: true }),
            Err(RestError::BadBody("private response".to_owned())),
            Err(RestError::BodyTooLarge),
        ] {
            assert!(matches!(service.post_result(result), PostResult::Unknown));
        }
        assert!(matches!(
            service.post_result(Ok(json!({"ok":true}))),
            PostResult::Accepted
        ));
        let result = service.post_result(Err(RestError::Refused {
            status: 409,
            detail: Some(format!(
                "busy at {}",
                home.path().join("state.db").display()
            )),
        }));
        assert!(
            matches!(result, PostResult::Refused { http_status: Some(409), detail } if detail == "busy at <path>")
        );
        assert!(
            matches!(service.post_result(Err(RestError::Server { status:500 })), PostResult::Refused { http_status:Some(500), detail } if detail.is_empty())
        );
        assert!(matches!(
            service.post_result(Err(RestError::Timeout { sent: false })),
            PostResult::Refused {
                http_status: None,
                ..
            }
        ));
    }
}
