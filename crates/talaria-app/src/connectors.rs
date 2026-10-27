//! Connector socket calls and profile-scoped messaging calls.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hermes_client::messaging_api::{self, PlatformUpdate};
use hermes_client::rest::{RestClient, RestError};
use hermes_client::{CallError, CallOptions, Client};
use hermes_contract::{methods::legacy, native::NativeMethod};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use talaria_core::connectors::{
    self as core, ConnectorsEffect, ConnectorsFailure, ConnectorsInput,
};
use zeroize::{Zeroize, Zeroizing};

/// Consent is shared by all callers in one process and keyed by profile.
#[derive(Clone, Default)]
pub struct ConnectorsConsent(Arc<Mutex<BTreeSet<String>>>);

impl ConnectorsConsent {
    #[must_use]
    pub fn granted(&self, profile: &str) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .contains(profile)
    }

    pub fn grant(&self, profile: &str) {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(profile.to_owned());
    }
}

/// Executes effects without owning overlay state or issuing automatic retries.
pub struct ConnectorsService {
    client: Client,
    rest: RestClient,
    profile: String,
    consent: ConnectorsConsent,
}

/// Complete each rejected effect without minting a ticket or contacting a gateway.
pub(crate) fn unavailable(effect: ConnectorsEffect) -> Vec<ConnectorsInput> {
    use ConnectorsEffect as E;
    use ConnectorsInput as I;
    let failure = || ConnectorsFailure::NotSent("Not connected to Hermes.".into());
    let input = match effect {
        E::ListConnectors => I::ListReply(Err(failure())),
        E::LoadCatalog => I::CatalogReply(Err(failure())),
        E::LoadAccounts => I::AccountsReply(Err(failure())),
        E::LoadPolicy => I::PolicyReply(Err(failure())),
        E::LoadTools { slug, .. } => I::ToolsReply {
            slug,
            reply: Err(failure()),
        },
        E::Connect { .. } => I::ConnectReply(Err(failure())),
        E::PollOperation { op_id } => I::OperationReply {
            op_id,
            reply: Err(failure()),
        },
        E::WakeOperation { .. } => I::WakeReply(Err(failure())),
        E::RenameAccount { .. } => I::RenameReply(Err(failure())),
        E::RemoveAccount { .. } => I::RemoveReply(Err(failure())),
        E::SetConnector { .. } | E::SetTools { .. } => I::PolicySetReply(Err(failure())),
        E::LoadPlatforms => I::PlatformsLoaded(Err(failure())),
        E::LoadPairing => I::PairingReply(Err(failure())),
        E::UpdatePlatform { id, .. } => I::UpdateReply {
            id,
            reply: Err(failure()),
        },
        E::TestPlatform { id } => I::TestReply {
            id,
            reply: Err(failure()),
        },
        E::ApprovePairing { .. } => I::ApproveReply(Err(failure())),
        E::RevokePairing { .. } => I::RevokeReply(Err(failure())),
        _ => return Vec::new(),
    };
    vec![input]
}

impl ConnectorsService {
    #[must_use]
    pub fn new(
        client: Client,
        rest: RestClient,
        profile: String,
        consent: ConnectorsConsent,
    ) -> Self {
        Self {
            client,
            rest,
            profile,
            consent,
        }
    }

    /// A connector call checks consent immediately before entering the socket client.
    async fn call<M, R>(&self, params: Value, seconds: u64) -> Result<R, ConnectorsFailure>
    where
        M: NativeMethod,
        M::Params: DeserializeOwned,
        M::Result: Serialize,
        R: DeserializeOwned,
    {
        if !self.consent.granted(&self.profile) {
            return Err(ConnectorsFailure::NotSent("consent not given".into()));
        }
        let params = serde_json::from_value(params)
            .map_err(|_| ConnectorsFailure::NotSent("invalid connector parameters".into()))?;
        let result = self
            .client
            .call_with::<M>(
                params,
                CallOptions {
                    timeout: Some(Duration::from_secs(seconds)),
                },
            )
            .await
            .map_err(call_failure)?;
        let value = serde_json::to_value(result).map_err(|_| ConnectorsFailure::BadReply)?;
        serde_json::from_value(value).map_err(|_| ConnectorsFailure::BadReply)
    }

    /// Carry out one effect. Only a platform save consumes typed secret values.
    pub async fn execute(
        &self,
        effect: ConnectorsEffect,
        secrets: Option<BTreeMap<String, Zeroizing<String>>>,
    ) -> Vec<ConnectorsInput> {
        use ConnectorsEffect as E;
        use ConnectorsInput as I;
        let input = match effect {
            E::GrantConsent => {
                self.consent.grant(&self.profile);
                return Vec::new();
            }
            E::ListConnectors => {
                let reply = self
                    .call::<legacy::connectors_list::Marker, _>(
                        json!({"owner": {"type": "account"}}),
                        15,
                    )
                    .await;
                I::ListReply(reply)
            }
            E::LoadCatalog => {
                let reply = self
                    .call::<legacy::connectors_catalog::Marker, core::CatalogReply>(json!({}), 15)
                    .await;
                I::CatalogReply(reply.map(|reply| reply.connectors))
            }
            E::LoadAccounts => {
                #[derive(Deserialize)]
                struct Accounts {
                    accounts: Vec<core::AccountRow>,
                }
                let reply = self
                    .call::<legacy::connectors_accounts::Marker, Accounts>(json!({}), 15)
                    .await;
                I::AccountsReply(reply.map(|reply| reply.accounts))
            }
            E::LoadPolicy => {
                let reply = self
                    .call::<legacy::connectors_policy_get::Marker, _>(json!({}), 15)
                    .await;
                I::PolicyReply(reply)
            }
            E::LoadTools { slug, refresh } => {
                let reply = self
                    .call::<legacy::connectors_tools::Marker, _>(
                        json!({"slug": slug, "refresh": refresh}),
                        30,
                    )
                    .await;
                I::ToolsReply { slug, reply }
            }
            E::Connect { slug, address } => {
                let params = match address {
                    core::AccountAddress::Alias(alias) => json!({
                        "owner": {"type": "account"}, "connectors": [slug],
                        "alias": alias, "reconnect": false,
                    }),
                    core::AccountAddress::Repair { connection_id } => json!({
                        "owner": {"type": "account"}, "connectors": [slug],
                        "connection_id": connection_id, "reconnect": true,
                    }),
                };
                let reply = self
                    .call::<legacy::connectors_connect::Marker, OperationWire>(params, 45)
                    .await;
                I::ConnectReply(reply.map(operation))
            }
            E::PollOperation { op_id } => {
                let params = json!({"owner": {"type": "account"}, "op_id": op_id});
                let reply = self
                    .call::<legacy::connectors_operation_status::Marker, OperationWire>(params, 10)
                    .await;
                I::OperationReply {
                    op_id,
                    reply: reply.map(operation),
                }
            }
            E::WakeOperation { op_id } => {
                let params = json!({"owner": {"type": "account"}, "op_id": op_id});
                let reply = self
                    .call::<legacy::connectors_operation_wake::Marker, Value>(params, 5)
                    .await;
                I::WakeReply(reply.map(|_| ()))
            }
            E::RenameAccount {
                connection_id,
                alias,
            } => {
                let params = json!({"connection_id": connection_id, "alias": alias});
                let reply = self
                    .call::<legacy::connectors_accounts_rename::Marker, _>(params, 30)
                    .await;
                I::RenameReply(reply)
            }
            E::RemoveAccount { connection_id } => {
                #[derive(Deserialize)]
                struct Removed {
                    connection_id: String,
                }
                let reply = self
                    .call::<legacy::connectors_accounts_remove::Marker, Removed>(
                        json!({"connection_id": connection_id}),
                        30,
                    )
                    .await;
                I::RemoveReply(reply.map(|reply| core::RemovedReply {
                    connection_id: reply.connection_id,
                }))
            }
            E::SetConnector {
                slug,
                enabled,
                expected_revision,
            } => {
                let params = json!({
                    "change": {"type": "connector", "connector": slug, "enabled": enabled},
                    "expected_revision": expected_revision,
                });
                I::PolicySetReply(
                    self.call::<legacy::connectors_policy_set::Marker, _>(params, 30)
                        .await,
                )
            }
            E::SetTools {
                slug,
                mut disabled_tools,
                expected_revision,
            } => {
                disabled_tools.sort();
                disabled_tools.dedup();
                let params = json!({
                    "change": {"type": "tools", "connector": slug, "disabled_tools": disabled_tools},
                    "expected_revision": expected_revision,
                });
                I::PolicySetReply(
                    self.call::<legacy::connectors_policy_set::Marker, _>(params, 30)
                        .await,
                )
            }
            E::LoadPlatforms => {
                let reply = messaging_api::list_platforms(&self.rest, &self.profile)
                    .await
                    .map(|reply| core::PlatformsReply {
                        gateway_start_command: reply.gateway_start_command,
                        platforms: reply.platforms.into_iter().map(platform).collect(),
                    })
                    .map_err(rest_failure);
                I::PlatformsLoaded(reply)
            }
            E::LoadPairing => {
                let reply = messaging_api::list_pairing(&self.rest, &self.profile)
                    .await
                    .map(pairing)
                    .map_err(rest_failure);
                I::PairingReply(reply)
            }
            E::UpdatePlatform {
                id,
                enabled,
                typed_keys,
                lists,
                clear_env,
            } => {
                let list_entries: Vec<_> = lists
                    .values()
                    .flatten()
                    .map(|entry| Zeroizing::new(entry.clone()))
                    .collect();
                let reply = match platform_update(enabled, typed_keys, lists, clear_env, secrets) {
                    Ok(body) => {
                        messaging_api::update_platform(&self.rest, &self.profile, &id, &body)
                            .await
                            .map_err(|error| update_failure(error, &body, &list_entries))
                            .and_then(|reply| {
                                if reply.ok {
                                    Ok(core::UpdateReply {
                                        hot_served: reply.hot_served,
                                    })
                                } else {
                                    Err(ConnectorsFailure::BadReply)
                                }
                            })
                    }
                    Err(error) => Err(error),
                };
                I::UpdateReply { id, reply }
            }
            E::TestPlatform { id } => {
                let reply = messaging_api::test_platform(&self.rest, &self.profile, &id)
                    .await
                    .map(|reply| core::TestReply {
                        ok: reply.ok,
                        state: reply.state,
                        message: reply.message,
                        auth_posture: reply.auth_posture,
                    })
                    .map_err(rest_failure);
                I::TestReply { id, reply }
            }
            E::ApprovePairing {
                platform,
                request_id,
            } => {
                let reply = messaging_api::approve_pairing(
                    &self.rest,
                    &self.profile,
                    &platform,
                    &request_id,
                )
                .await
                .map_err(rest_failure)
                .and_then(|reply| {
                    if reply.ok {
                        Ok(())
                    } else {
                        Err(ConnectorsFailure::BadReply)
                    }
                });
                I::ApproveReply(reply)
            }
            E::RevokePairing { platform, user_id } => {
                let reply =
                    messaging_api::revoke_pairing(&self.rest, &self.profile, &platform, &user_id)
                        .await
                        .map_err(rest_failure);
                I::RevokeReply(reply)
            }
            E::OpenUrl(_)
            | E::CopyText(_)
            | E::OpenDoctor
            | E::EditSecret { .. }
            | E::ForgetSecrets
            | E::Notice(_)
            | E::Close => return Vec::new(),
        };
        vec![input]
    }
}

fn platform_update(
    enabled: Option<bool>,
    typed_keys: Vec<String>,
    lists: BTreeMap<String, Vec<String>>,
    clear_env: Vec<String>,
    secrets: Option<BTreeMap<String, Zeroizing<String>>>,
) -> Result<PlatformUpdate, ConnectorsFailure> {
    let mut secrets = secrets.unwrap_or_default();
    let mut env = BTreeMap::new();
    let mut clear: BTreeSet<String> = clear_env.into_iter().collect();
    for key in typed_keys {
        if clear.contains(&key) {
            continue;
        }
        let value = secrets
            .remove(&key)
            .ok_or_else(|| ConnectorsFailure::NotSent("secret draft is missing".into()))?;
        env.insert(key, value);
    }
    for (key, values) in lists {
        if values.is_empty() {
            clear.insert(key.clone());
            env.remove(&key);
        } else if !clear.contains(&key) {
            env.insert(key, Zeroizing::new(values.join(",")));
        }
    }
    Ok(PlatformUpdate {
        enabled,
        env,
        clear_env: clear.into_iter().collect(),
    })
}

#[derive(Deserialize)]
struct OperationWire {
    op_id: String,
    deadline_at: f64,
    settled: bool,
    #[serde(default)]
    targets: Vec<core::Target>,
}

fn operation(reply: OperationWire) -> core::Operation {
    let now = Instant::now();
    let epoch_now = SystemTime::now();
    core::Operation {
        op_id: reply.op_id,
        deadline: deadline(reply.deadline_at, epoch_now, now),
        settled: reply.settled,
        targets: reply.targets,
    }
}

fn deadline(epoch_seconds: f64, epoch_now: SystemTime, now: Instant) -> Instant {
    let epoch_now = epoch_now
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let seconds = (epoch_seconds.trunc() - epoch_now).clamp(0.0, 300.0);
    now + Duration::from_secs_f64(seconds)
}

fn platform(wire: messaging_api::PlatformWire) -> core::PlatformRow {
    core::PlatformRow {
        id: wire.id,
        name: wire.name,
        description: wire.description,
        docs_url: wire.docs_url,
        enabled: wire.enabled.unwrap_or(false),
        state: wire.state,
        error_message: wire.error_message,
        home_channel: wire
            .home_channel
            .map(|channel| core::HomeChannel { name: channel.name }),
        ingress_url: wire.ingress_url,
        env_vars: wire
            .env_vars
            .into_iter()
            .map(|field| core::EnvVar {
                key: field.key,
                prompt: field.prompt,
                required: field.required,
                is_set: field.is_set,
                is_list: field.is_list,
                is_password: field.is_password,
                advanced: field.advanced,
                value: field.value.filter(|_| field.is_list),
                help: field.help,
                description: field.description,
                url: field.url,
            })
            .collect(),
    }
}

fn pairing(reply: messaging_api::PairingReply) -> core::Pairing {
    core::Pairing {
        pending: reply
            .pending
            .into_iter()
            .map(|row| core::PendingPair {
                platform: row.platform,
                request_id: row.request_id,
                user_id: row.user_id,
                user_name: row.user_name,
                age_minutes: row.age_minutes.unwrap_or_default(),
            })
            .collect(),
        approved: reply
            .approved
            .into_iter()
            .map(|row| core::ApprovedPair {
                platform: row.platform,
                user_id: row.user_id,
                user_name: row.user_name,
            })
            .collect(),
    }
}

fn call_failure(error: CallError) -> ConnectorsFailure {
    match error {
        CallError::Rpc(error) => ConnectorsFailure::Refused {
            code: error.code,
            reason: error.reason,
            message: error.message,
        },
        CallError::NotConnected => ConnectorsFailure::NotSent("Not connected to Hermes.".into()),
        CallError::Encode | CallError::InvalidTimeout => {
            ConnectorsFailure::NotSent("Talaria could not send the request.".into())
        }
        CallError::TimedOut { .. } | CallError::Unknown { .. } | CallError::BadResult { .. } => {
            ConnectorsFailure::Unknown
        }
    }
}

fn rest_failure(error: RestError) -> ConnectorsFailure {
    if error.outcome_unknown() {
        return ConnectorsFailure::Unknown;
    }
    match error {
        RestError::Unauthorized => ConnectorsFailure::Http {
            status: 401,
            detail: None,
        },
        RestError::Forbidden { detail } => ConnectorsFailure::Http {
            status: 403,
            detail: Some(detail),
        },
        RestError::NotFound { .. } => ConnectorsFailure::Http {
            status: 404,
            detail: None,
        },
        RestError::Refused { status, detail } => ConnectorsFailure::Http { status, detail },
        RestError::Server { status } => ConnectorsFailure::Http {
            status,
            detail: None,
        },
        RestError::BodyTooLarge | RestError::BadBody(_) => ConnectorsFailure::BadReply,
        other => ConnectorsFailure::NotSent(other.to_string()),
    }
}

/// A refused save may echo its input. Withhold only values known from this draft.
fn update_failure(
    error: RestError,
    body: &PlatformUpdate,
    list_entries: &[Zeroizing<String>],
) -> ConnectorsFailure {
    let mut failure = rest_failure(error);
    if let ConnectorsFailure::Http {
        detail: Some(detail),
        ..
    } = &mut failure
    {
        let may_be_truncated = detail.chars().count() == 200;
        let mut literals: Vec<_> = body
            .env
            .values()
            .chain(list_entries)
            .flat_map(|value| [value.as_str(), value.trim()])
            .filter(|value| !value.is_empty())
            .collect();
        literals.sort_unstable_by_key(|value| std::cmp::Reverse(value.len()));
        literals.dedup();
        for literal in literals {
            // The transport caps refusal text before this layer sees it. An echoed
            // value may therefore end partway through the last character run.
            if may_be_truncated
                && let Some((start, _)) = detail
                    .char_indices()
                    .find(|(start, _)| literal.starts_with(&detail[*start..]))
            {
                let safe = format!("{}[redacted]", &detail[..start]);
                detail.zeroize();
                *detail = safe;
            }
            if detail.contains(literal) {
                let safe = detail.replace(literal, "[redacted]");
                detail.zeroize();
                *detail = safe;
            }
        }
    }
    failure
}

/// Discover only the already-running endpoint; never start or change Hermes.
pub(crate) async fn discover_service(
    home: PathBuf,
    profile: &str,
    client: Client,
    consent: ConnectorsConsent,
) -> Option<Arc<ConnectorsService>> {
    use hermes_client::rest::{NativeHttpMinter, RestOptions};
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
    Some(Arc::new(ConnectorsService::new(
        client,
        rest,
        profile.to_owned(),
        consent,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_discards_fraction_and_clamps_past_and_future() {
        let now = Instant::now();
        let epoch = UNIX_EPOCH + Duration::from_millis(100_250);
        assert_eq!(
            deadline(120.99, epoch, now),
            now + Duration::from_millis(19_750)
        );
        assert_eq!(deadline(99.99, epoch, now), now);
        assert_eq!(deadline(900.0, epoch, now), now + Duration::from_secs(300));
    }

    #[test]
    fn consent_is_shared_but_profile_specific() {
        let consent = ConnectorsConsent::default();
        let second_caller = consent.clone();
        assert!(!second_caller.granted("fixture"));
        consent.grant("fixture");
        assert!(second_caller.granted("fixture"));
        assert!(!second_caller.granted("default"));
    }

    #[test]
    fn body_selects_only_edited_keys_and_keeps_clears_out_of_env() {
        let secrets = BTreeMap::from([
            ("TOKEN".into(), Zeroizing::new("typed-value".into())),
            ("UNTOUCHED".into(), Zeroizing::new("unused-value".into())),
        ]);
        let body = platform_update(
            None,
            vec!["TOKEN".into()],
            BTreeMap::from([
                ("ALLOWED".into(), vec!["one".into(), "two".into()]),
                ("EMPTY".into(), Vec::new()),
                ("CLEAR".into(), vec!["ignored".into()]),
            ]),
            vec!["CLEAR".into()],
            Some(secrets),
        )
        .expect("draft body");
        assert_eq!(body.env["TOKEN"].as_str(), "typed-value");
        assert_eq!(body.env["ALLOWED"].as_str(), "one,two");
        assert_eq!(body.env.len(), 2);
        assert_eq!(body.clear_env, ["CLEAR", "EMPTY"]);
        assert!(matches!(
            platform_update(None, vec!["MISSING".into()], BTreeMap::new(), vec![], None),
            Err(ConnectorsFailure::NotSent(_))
        ));
    }

    #[test]
    fn errors_distinguish_unsent_and_unknown_and_keep_refusal_reason() {
        assert!(matches!(
            call_failure(CallError::NotConnected),
            ConnectorsFailure::NotSent(_)
        ));
        assert!(matches!(
            call_failure(CallError::Encode),
            ConnectorsFailure::NotSent(_)
        ));
        assert!(matches!(
            call_failure(CallError::InvalidTimeout),
            ConnectorsFailure::NotSent(_)
        ));
        assert!(matches!(
            call_failure(CallError::BadResult {
                method: "connectors.connect".into(),
                reason: "result did not decode"
            }),
            ConnectorsFailure::Unknown
        ));
        for sent in [false, true] {
            for error in [RestError::Timeout { sent }, RestError::Dropped { sent }] {
                let failure = rest_failure(error);
                if sent {
                    assert_eq!(failure, ConnectorsFailure::Unknown);
                } else {
                    assert!(matches!(failure, ConnectorsFailure::NotSent(_)));
                }
            }
        }
        assert_eq!(
            rest_failure(RestError::NotFound {
                route: "/api/pairing".into()
            }),
            ConnectorsFailure::Http {
                status: 404,
                detail: None
            }
        );
        assert_eq!(
            rest_failure(RestError::Server { status: 503 }),
            ConnectorsFailure::Http {
                status: 503,
                detail: None
            }
        );
        let rpc = hermes_client::RpcError::from_value(
            &json!({"code": 4090, "message": "conflict", "data": {"reason": "POLICY_CONFLICT"}}),
        );
        assert_eq!(
            call_failure(CallError::Rpc(rpc)),
            ConnectorsFailure::Refused {
                code: 4090,
                reason: Some("POLICY_CONFLICT".into()),
                message: "conflict".into()
            }
        );
    }

    #[test]
    fn refused_save_redacts_a_value_cut_at_the_transport_detail_limit() {
        let credential = "secret-sentinel-long-value";
        let prefix = "x".repeat(190);
        let detail: String = format!("{prefix}{credential}").chars().take(200).collect();
        let body = PlatformUpdate {
            enabled: None,
            env: BTreeMap::from([("TOKEN".into(), Zeroizing::new(credential.into()))]),
            clear_env: Vec::new(),
        };
        let failure = update_failure(
            RestError::Refused {
                status: 400,
                detail: Some(detail),
            },
            &body,
            &[],
        );
        let ConnectorsFailure::Http {
            detail: Some(detail),
            ..
        } = failure
        else {
            panic!("expected refusal");
        };
        assert_eq!(detail, format!("{prefix}[redacted]"));
    }

    #[test]
    fn refused_save_redacts_typed_values_and_individual_list_entries() {
        let credential = "  secret-sentinel-full-value  ";
        let first_id = "allowlist-first-person";
        let second_id = "allowlist-second-person";
        let body = PlatformUpdate {
            enabled: None,
            env: BTreeMap::from([
                ("CREDENTIAL".into(), Zeroizing::new(credential.into())),
                (
                    "ALLOWED".into(),
                    Zeroizing::new(format!("{first_id},{second_id}")),
                ),
            ]),
            clear_env: Vec::new(),
        };
        let list_entries = [
            Zeroizing::new(first_id.into()),
            Zeroizing::new(second_id.into()),
        ];
        for status in [400, 409] {
            let error = RestError::Refused {
                status,
                detail: Some(format!(
                    "Invalid settings: {credential}; normalized {}; list {first_id},{second_id}; rejected person {second_id}. Re-enter the fields.",
                    credential.trim(),
                )),
            };
            let failure = update_failure(error, &body, &list_entries);
            let debug = format!("{failure:?}");
            for private in [credential.trim(), first_id, second_id] {
                assert!(!debug.contains(private));
            }
            let ConnectorsFailure::Http {
                status: actual,
                detail: Some(detail),
            } = failure
            else {
                panic!("expected HTTP refusal");
            };
            assert_eq!(actual, status);
            assert!(detail.starts_with("Invalid settings:"));
            assert!(detail.ends_with("Re-enter the fields."));
            assert!(detail.contains("[redacted]"));
            for private in [credential.trim(), first_id, second_id] {
                assert!(!detail.contains(private));
            }
        }
        for error in [
            RestError::Timeout { sent: false },
            RestError::Dropped { sent: false },
        ] {
            assert!(matches!(
                update_failure(error, &body, &list_entries),
                ConnectorsFailure::NotSent(_)
            ));
        }
        for error in [
            RestError::Timeout { sent: true },
            RestError::Dropped { sent: true },
        ] {
            assert_eq!(
                update_failure(error, &body, &list_entries),
                ConnectorsFailure::Unknown
            );
        }
    }
}
