#[path = "support/capabilities_fixture.rs"]
mod fixture;

use fixture::{Harness, mock_rest::capability_fixture};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use talaria_app::capabilities::ChangeOutcome;
use talaria_core::capabilities::{CapEffect, CapInput, CapTab, CapabilityChange, InstallTarget};

fn forbidden_key(value: &Value) -> Option<&str> {
    match value {
        Value::Object(object) => object.iter().find_map(|(key, value)| {
            if [
                "force",
                "accept_capabilities",
                "ref",
                "repo",
                "values",
                "unattended",
            ]
            .contains(&key.as_str())
            {
                Some(key.as_str())
            } else {
                forbidden_key(value)
            }
        }),
        Value::Array(array) => array.iter().find_map(forbidden_key),
        _ => None,
    }
}
fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

fn forbidden_key_check_includes_nested_nulls() {
    for key in [
        "force",
        "accept_capabilities",
        "ref",
        "repo",
        "values",
        "unattended",
    ] {
        let nested = json!({"outer":[{(key):Value::Null}]});
        assert_eq!(forbidden_key(&nested), Some(key));
    }
}

#[tokio::test]
async fn wire_requests_are_exact() {
    forbidden_key_check_includes_nested_nulls();
    execute_correlates_errors_and_read_backs().await;
    all_socket_calls_have_the_prescribed_timeout().await;

    let harness = Harness::new().await;
    let service = harness.service();
    let profile = "fixture".to_owned();
    let effects = vec![
        CapEffect::Fetch {
            profile: profile.clone(),
            tab: CapTab::Skills,
            generation: 1,
        },
        CapEffect::FetchDetail {
            profile: profile.clone(),
            tab: CapTab::Skills,
            key: "fixture-skill-a".into(),
            generation: 2,
        },
        CapEffect::HubSearch {
            profile: profile.clone(),
            q: "fixture".into(),
            generation: 3,
        },
        CapEffect::HubPreview {
            profile: profile.clone(),
            identifier: "official/fixture-skill-b".into(),
        },
        CapEffect::HubScan {
            profile: profile.clone(),
            identifier: "official/fixture-allow".into(),
        },
        CapEffect::Fetch {
            profile: profile.clone(),
            tab: CapTab::Toolsets,
            generation: 4,
        },
        CapEffect::FetchDetail {
            profile: profile.clone(),
            tab: CapTab::Toolsets,
            key: "web".into(),
            generation: 5,
        },
        CapEffect::ToggleSkill {
            profile: profile.clone(),
            op_id: 1,
            name: "fixture-skill-a".into(),
            enabled: false,
        },
        CapEffect::ToggleToolset {
            profile: profile.clone(),
            op_id: 2,
            name: "web".into(),
            enabled: false,
        },
        CapEffect::FetchCatalog {
            profile: profile.clone(),
            generation: 6,
        },
        CapEffect::HubBrowse {
            profile: profile.clone(),
            page: 2,
            generation: 7,
        },
        CapEffect::Fetch {
            profile: profile.clone(),
            tab: CapTab::Plugins,
            generation: 8,
        },
        CapEffect::LookupPlugin {
            profile: profile.clone(),
            name: "fixture-plugin".into(),
        },
        CapEffect::TogglePlugin {
            profile: profile.clone(),
            op_id: 3,
            key: "fixture-plugin".into(),
            enable: false,
        },
        CapEffect::InstallSkill {
            profile: profile.clone(),
            op_id: 4,
            install_key: "official/fixture-skill-b".into(),
        },
        CapEffect::ReloadSkills {
            session_id: "local-session-1".into(),
        },
        CapEffect::InstallPlugin {
            profile: profile.clone(),
            op_id: 5,
            target: InstallTarget::CatalogName("fixture-plugin".into()),
        },
        CapEffect::InstallPlugin {
            profile: profile.clone(),
            op_id: 6,
            target: InstallTarget::Repository("https://example.invalid/fixture/plugin".into()),
        },
        CapEffect::RemovePlugin {
            profile: profile.clone(),
            op_id: 7,
            name: "fixture-plugin".into(),
        },
    ];
    for effect in effects {
        assert_eq!(service.execute(effect).await.len(), 1);
    }
    let expected_rest = [
        ("GET", "/api/skills?profile=fixture", Value::Null),
        (
            "GET",
            "/api/skills/content?name=fixture-skill-a&profile=fixture",
            Value::Null,
        ),
        (
            "GET",
            "/api/skills/hub/search?q=fixture&source=all&limit=20&profile=fixture",
            Value::Null,
        ),
        (
            "GET",
            "/api/skills/hub/preview?identifier=official%2Ffixture-skill-b&profile=fixture",
            Value::Null,
        ),
        (
            "GET",
            "/api/skills/hub/scan?identifier=official%2Ffixture-allow&profile=fixture",
            Value::Null,
        ),
        ("GET", "/api/tools/toolsets?profile=fixture", Value::Null),
        (
            "GET",
            "/api/tools/toolsets/web/config?profile=fixture",
            Value::Null,
        ),
        (
            "PUT",
            "/api/skills/toggle",
            json!({"name":"fixture-skill-a","enabled":false,"profile":"fixture"}),
        ),
        (
            "PUT",
            "/api/tools/toolsets/web",
            json!({"enabled":false,"profile":"fixture"}),
        ),
        ("GET", "/api/dashboard/plugins/catalog", Value::Null),
    ];
    let requests = harness.rest.requests().await;
    assert_eq!(requests.len(), expected_rest.len());
    assert_eq!(harness.minter.count(), requests.len());
    for (actual, (method, path, body)) in requests.iter().zip(expected_rest) {
        assert_eq!(
            (actual.method.as_str(), actual.path.as_str()),
            (method, path)
        );
        let actual_body = if actual.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&actual.body).unwrap()
        };
        assert_eq!(actual_body, body);
        assert!(forbidden_key(&actual_body).is_none());
        for banned in ["/api/skills/hub/install", "/uninstall", "/update"] {
            assert!(!actual.path.contains(banned));
        }
        assert_ne!((method, path), ("POST", "/api/skills"));
        assert_ne!((method, path), ("PUT", "/api/skills/content"));
        if actual_body.is_object() {
            assert_eq!(keys(&actual_body), keys(&body));
        }
    }
    let expected_socket = [
        (
            "skills.manage",
            json!({"action":"browse","page":2,"page_size":20,"profile":"fixture"}),
        ),
        (
            "plugins.manage",
            json!({"action":"list","profile":"fixture"}),
        ),
        (
            "plugins.catalog.lookup",
            json!({"name":"fixture-plugin","profile":"fixture"}),
        ),
        (
            "plugins.manage",
            json!({"action":"toggle","key":"fixture-plugin","enable":false,"profile":"fixture"}),
        ),
        (
            "skills.manage",
            json!({"action":"install","query":"official/fixture-skill-b","profile":"fixture"}),
        ),
        ("skills.reload", json!({"session_id":"local-session-1"})),
        (
            "plugins.manage",
            json!({"action":"install","catalog_name":"fixture-plugin","enable":true,"profile":"fixture"}),
        ),
        (
            "plugins.manage",
            json!({"action":"install","identifier":"https://example.invalid/fixture/plugin","enable":true,"profile":"fixture"}),
        ),
        (
            "plugins.manage",
            json!({"action":"remove","name":"fixture-plugin","profile":"fixture"}),
        ),
    ];
    let frames = harness.gateway.seen();
    assert_eq!(frames.len(), expected_socket.len());
    for (frame, (method, params)) in frames.iter().zip(expected_socket) {
        assert_eq!(frame.method.as_deref(), Some(method));
        assert_eq!(frame.params, params);
        assert_eq!(keys(&frame.params), keys(&params));
        assert!(forbidden_key(&frame.raw).is_none());
        assert!(
            ![
                "tools.configure",
                "tools.list",
                "toolsets.list",
                "reload.mcp"
            ]
            .contains(&method)
        );
        assert!(
            !["update", "settings", "onboarding", "presence"]
                .contains(&frame.params["action"].as_str().unwrap_or(""))
        );
    }
}

fn change(kind: CapTab, enabled: bool) -> CapabilityChange {
    match kind {
        CapTab::Skills => CapabilityChange::Skill {
            name: "fixture-skill-a".into(),
            enabled,
        },
        CapTab::Toolsets => CapabilityChange::Toolset {
            name: "web".into(),
            enabled,
        },
        _ => CapabilityChange::Plugin {
            key: "fixture-plugin".into(),
            enabled,
        },
    }
}

#[tokio::test]
async fn apply_reports_outcomes() {
    apply_never_resends_unknown_writes_and_reports_read_back_failure().await;
    apply_maps_pre_read_and_write_refusals_for_every_kind().await;
    apply_read_back_matrix_covers_every_kind().await;
    apply_refuses_unknown_plugin_status_and_other_profile_socket_routing().await;
    apply_reports_a_rest_write_not_sent_after_successful_pre_read().await;

    for kind in [CapTab::Skills, CapTab::Toolsets, CapTab::Plugins] {
        let harness = Harness::new().await;
        let service = harness.service();
        assert_eq!(
            service.apply("fixture", change(kind, true)).await,
            ChangeOutcome::Unchanged
        );
        assert_eq!(
            service.apply("fixture", change(kind, false)).await,
            ChangeOutcome::Applied
        );
        assert_eq!(
            service.apply("fixture", change(kind, false)).await,
            ChangeOutcome::Unchanged
        );
        let before = (
            harness.rest.requests().await.len(),
            harness.gateway.frame_count(),
        );
        assert_eq!(service.apply("absent",change(kind,true)).await,ChangeOutcome::Refused("Profile absent is not served by the gateway, so its capabilities cannot be read or changed.".into()));
        assert_eq!(
            (
                harness.rest.requests().await.len(),
                harness.gateway.frame_count()
            ),
            before
        );
        let missing = match kind {
            CapTab::Skills => CapabilityChange::Skill {
                name: "nosuch".into(),
                enabled: true,
            },
            CapTab::Toolsets => CapabilityChange::Toolset {
                name: "nosuch".into(),
                enabled: true,
            },
            _ => CapabilityChange::Plugin {
                key: "nosuch".into(),
                enabled: true,
            },
        };
        assert_eq!(
            service.apply("fixture", missing).await,
            ChangeOutcome::Refused("nosuch is not listed in profile fixture.".into())
        );
    }
    let harness = Harness::new().await;
    harness.gateway.unchanged_capability_toggle();
    assert_eq!(
        harness
            .service()
            .apply("fixture", change(CapTab::Plugins, false))
            .await,
        ChangeOutcome::Unchanged
    );
    assert_eq!(
        harness.gateway.requests("plugins.manage").len(),
        2,
        "unchanged reply needs no read-back"
    );
}

async fn apply_never_resends_unknown_writes_and_reports_read_back_failure() {
    for kind in [CapTab::Skills, CapTab::Toolsets, CapTab::Plugins] {
        let harness = Harness::new().await;
        if kind == CapTab::Plugins {
            harness.gateway.set_silent("plugins.manage", true);
        } else {
            harness
                .rest
                .drop_after_request(
                    "PUT",
                    if kind == CapTab::Skills {
                        "/api/skills/toggle"
                    } else {
                        "/api/tools/toolsets/web"
                    },
                )
                .await;
        }
        if kind == CapTab::Plugins {
            harness.gateway.push_matched_result(
                "plugins.manage",
                json!({"action":"list","profile":"fixture"}),
                capability_fixture("socket/plugins-manage-list.json"),
            );
        }
        let service = harness.service();
        let result = service.apply("fixture", change(kind, false));
        let outcome = if kind == CapTab::Plugins {
            tokio::pin!(result);
            tokio::select! { outcome = &mut result => outcome, _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                tokio::time::pause(); tokio::time::advance(std::time::Duration::from_secs(31)).await;
                let outcome = result.await; tokio::time::resume(); outcome
            }}
        } else {
            result.await
        };
        assert_eq!(outcome, ChangeOutcome::Unknown);
        if kind == CapTab::Plugins {
            assert_eq!(
                harness
                    .gateway
                    .requests("plugins.manage")
                    .iter()
                    .filter(|frame| frame.params["action"] == "toggle")
                    .count(),
                1
            );
        } else {
            assert_eq!(
                harness
                    .rest
                    .requests()
                    .await
                    .iter()
                    .filter(|request| request.method == "PUT")
                    .count(),
                1
            );
        }
    }
    for failed_read in [false, true] {
        let mut harness = Harness::new().await;
        let reads = Arc::new(AtomicUsize::new(0));
        let count = reads.clone();
        harness.rest = harness
            .rest
            .with_route("GET", "/api/skills?profile=fixture", move |_| {
                let attempt = count.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    (200, capability_fixture("rest/skills.json"))
                } else if failed_read {
                    (502, json!({"detail":"Unavailable"}))
                } else {
                    (200, json!([]))
                }
            });
        let expected = if failed_read {
            ChangeOutcome::Unknown
        } else {
            ChangeOutcome::ReadBackMismatch
        };
        assert_eq!(
            harness
                .service()
                .apply("fixture", change(CapTab::Skills, false))
                .await,
            expected
        );
        assert_eq!(reads.load(Ordering::SeqCst), 2);
    }
}

async fn execute_correlates_errors_and_read_backs() {
    let harness = Harness::new().await;
    let service = harness.service();
    let replies = service
        .execute(CapEffect::ReadBack {
            profile: "fixture".into(),
            tab: CapTab::Skills,
            op_id: 17,
        })
        .await;
    assert!(matches!(
        replies.as_slice(),
        [CapInput::ReadBack {
            op_id: 17,
            result: Ok(_)
        }]
    ));
    harness.gateway.fail_capability(
        "skills.manage",
        "install",
        5031,
        "skill install failed",
        json!({"log":"line one\nline two\nline three"}),
    );
    let replies = service
        .execute(CapEffect::InstallSkill {
            profile: "fixture".into(),
            op_id: 18,
            install_key: "fixture".into(),
        })
        .await;
    assert!(
        matches!(replies.as_slice(),[CapInput::WriteDone { op_id:18,result:Err(talaria_core::capabilities::CapError::Rpc { code:5031,log:Some(log),.. }) }] if log.lines().count() == 3)
    );
    harness.gateway.script_error(
        "plugins.manage",
        Some("fixture"),
        4064,
        "profile 'fixture' not found",
    );
    let replies = service
        .execute(CapEffect::Fetch {
            profile: "fixture".into(),
            tab: CapTab::Plugins,
            generation: 19,
        })
        .await;
    assert!(matches!(
        replies.as_slice(),
        [CapInput::Loaded {
            tab: CapTab::Plugins,
            generation: 19,
            result: Err(_)
        }]
    ));
}

async fn all_socket_calls_have_the_prescribed_timeout() {
    let profile = "fixture".to_owned();
    let cases = [
        (
            "skills.manage",
            45,
            CapEffect::HubBrowse {
                profile: profile.clone(),
                page: 1,
                generation: 1,
            },
        ),
        (
            "plugins.manage",
            30,
            CapEffect::Fetch {
                profile: profile.clone(),
                tab: CapTab::Plugins,
                generation: 2,
            },
        ),
        (
            "plugins.catalog.lookup",
            20,
            CapEffect::LookupPlugin {
                profile: profile.clone(),
                name: "fixture-plugin".into(),
            },
        ),
        (
            "plugins.manage",
            30,
            CapEffect::TogglePlugin {
                profile: profile.clone(),
                op_id: 3,
                key: "fixture-plugin".into(),
                enable: false,
            },
        ),
        (
            "skills.manage",
            120,
            CapEffect::InstallSkill {
                profile: profile.clone(),
                op_id: 4,
                install_key: "fixture-skill-b".into(),
            },
        ),
        (
            "skills.reload",
            30,
            CapEffect::ReloadSkills {
                session_id: "local-session-1".into(),
            },
        ),
        (
            "plugins.manage",
            120,
            CapEffect::InstallPlugin {
                profile: profile.clone(),
                op_id: 5,
                target: InstallTarget::CatalogName("fixture-plugin".into()),
            },
        ),
        (
            "plugins.manage",
            120,
            CapEffect::InstallPlugin {
                profile: profile.clone(),
                op_id: 6,
                target: InstallTarget::Repository("https://example.invalid/fixture/plugin".into()),
            },
        ),
        (
            "plugins.manage",
            30,
            CapEffect::RemovePlugin {
                profile: profile.clone(),
                op_id: 7,
                name: "fixture-plugin".into(),
            },
        ),
    ];
    for (method, seconds, effect) in cases {
        let harness = Harness::new().await;
        harness.gateway.set_silent(method, true);
        let service = harness.service();
        let call = tokio::spawn(async move { service.execute(effect).await });
        harness.gateway.wait_for_request(method, 0).await;
        tokio::time::pause();
        tokio::time::advance(std::time::Duration::from_secs(seconds - 1)).await;
        tokio::task::yield_now().await;
        assert!(
            !call.is_finished(),
            "socket deadline expired early for {method}"
        );
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
        let events = call.await.unwrap();
        assert!(matches!(
            events.as_slice(),
            [CapInput::Loaded {
                result: Err(talaria_core::capabilities::CapError::TimedOut),
                ..
            }] | [CapInput::WriteDone {
                result: Err(talaria_core::capabilities::CapError::TimedOut),
                ..
            }] | [CapInput::LookupDone(Err(
                talaria_core::capabilities::CapError::TimedOut
            ))] | [CapInput::ReloadDone(Err(
                talaria_core::capabilities::CapError::TimedOut
            ))]
        ));
        assert_eq!(
            harness.gateway.frame_count(),
            1,
            "a timed-out call must never be resent"
        );
        tokio::time::resume();
    }
}

fn rest_routes(kind: CapTab) -> (&'static str, &'static str, &'static str) {
    if kind == CapTab::Skills {
        (
            "/api/skills?profile=fixture",
            "/api/skills/toggle",
            "rest/skills.json",
        )
    } else {
        (
            "/api/tools/toolsets?profile=fixture",
            "/api/tools/toolsets/web",
            "rest/toolsets.json",
        )
    }
}
fn plugin_read() -> Value {
    json!({"action":"list","profile":"fixture"})
}
fn plugin_write() -> Value {
    json!({"action":"toggle","key":"fixture-plugin","enable":false,"profile":"fixture"})
}
fn assert_no_write(harness: &Harness, requests: &[fixture::mock_rest::RecordedRequest]) {
    assert!(requests.iter().all(|request| request.method == "GET"));
    assert!(
        harness
            .gateway
            .seen()
            .iter()
            .all(|frame| frame.params["action"] != "toggle")
    );
}

async fn apply_maps_pre_read_and_write_refusals_for_every_kind() {
    for kind in [CapTab::Skills, CapTab::Toolsets, CapTab::Plugins] {
        for before_write in [true, false] {
            let harness = Harness::new().await;
            if kind == CapTab::Plugins {
                harness.gateway.fail_capability(
                    "plugins.manage",
                    if before_write { "list" } else { "toggle" },
                    5026,
                    "Synthetic refusal",
                    Value::Null,
                );
            } else {
                let (read, write, _) = rest_routes(kind);
                harness
                    .rest
                    .fail_status(
                        if before_write { "GET" } else { "PUT" },
                        if before_write { read } else { write },
                        400,
                        Some("Synthetic refusal"),
                    )
                    .await;
            }
            assert_eq!(
                harness
                    .service()
                    .apply("fixture", change(kind, false))
                    .await,
                ChangeOutcome::Refused("Hermes refused this request: Synthetic refusal".into())
            );
            let requests = harness.rest.requests().await;
            if before_write {
                assert_no_write(&harness, &requests);
            } else if kind == CapTab::Plugins {
                assert_eq!(harness.gateway.requests("plugins.manage").len(), 2);
            } else {
                assert_eq!(
                    requests
                        .iter()
                        .filter(|request| request.method == "PUT")
                        .count(),
                    1
                );
            }
        }
        let harness = Harness::new().await;
        if kind == CapTab::Plugins {
            harness.socket.close().await;
        } else {
            harness
                .rest
                .drop_after_request("GET", rest_routes(kind).0)
                .await;
        }
        let outcome = harness
            .service()
            .apply("fixture", change(kind, false))
            .await;
        if kind == CapTab::Plugins {
            assert_eq!(
                outcome,
                ChangeOutcome::Refused(
                    "Talaria could not reach the gateway. Press r to retry.".into()
                )
            );
        } else {
            assert_eq!(outcome, ChangeOutcome::Unknown);
        }
        assert_no_write(&harness, &harness.rest.requests().await);
    }
}

async fn apply_read_back_matrix_covers_every_kind() {
    for kind in [CapTab::Skills, CapTab::Toolsets, CapTab::Plugins] {
        for situation in ["different", "vanished", "failed"] {
            let mut harness = Harness::new().await;
            if kind == CapTab::Plugins {
                harness.gateway.push_matched_result(
                    "plugins.manage",
                    plugin_read(),
                    capability_fixture("socket/plugins-manage-list.json"),
                );
                let mut reply = capability_fixture("socket/plugins-manage-toggle.json");
                if situation == "different" {
                    reply["plugin"]["status"] = json!("enabled");
                } else {
                    reply["plugin"] = Value::Null;
                    if situation == "vanished" {
                        harness.gateway.push_matched_result(
                            "plugins.manage",
                            plugin_read(),
                            json!({"plugins":[]}),
                        );
                    } else {
                        harness.gateway.fail_capability(
                            "plugins.manage",
                            "list",
                            5026,
                            "read-back failed",
                            Value::Null,
                        );
                    }
                }
                harness
                    .gateway
                    .push_matched_result("plugins.manage", plugin_write(), reply);
            } else {
                let (read, _, file) = rest_routes(kind);
                let count = Arc::new(AtomicUsize::new(0));
                harness.rest = harness.rest.with_route("GET", read, move |_| {
                    let first = count.fetch_add(1, Ordering::SeqCst) == 0;
                    if first || situation == "different" {
                        (200, capability_fixture(file))
                    } else if situation == "vanished" {
                        (200, json!([]))
                    } else {
                        (502, json!({"detail":"read-back failed"}))
                    }
                });
            }
            let expected = if situation == "failed" {
                ChangeOutcome::Unknown
            } else {
                ChangeOutcome::ReadBackMismatch
            };
            assert_eq!(
                harness
                    .service()
                    .apply("fixture", change(kind, false))
                    .await,
                expected,
                "kind {kind:?}, {situation}"
            );
        }
    }
}

async fn apply_refuses_unknown_plugin_status_and_other_profile_socket_routing() {
    let harness = Harness::new().await;
    let mut list = capability_fixture("socket/plugins-manage-list.json");
    list["plugins"][2]["status"] = json!("unexpected");
    harness
        .gateway
        .push_matched_result("plugins.manage", plugin_read(), list);
    assert_eq!(harness.service().apply("fixture",change(CapTab::Plugins,false)).await,ChangeOutcome::Refused("Talaria shows fixture-plugin read-only: the gateway listed it without a key or with a status Talaria does not know.".into()));
    assert_eq!(harness.gateway.requests("plugins.manage").len(), 1);

    let minter = harness.minter.clone();
    let factory: talaria_app::settings::MinterFactory = Arc::new(move |_| minter.clone());
    let service = talaria_app::capabilities::CapabilitiesService::new(
        Some(harness.rest.origin()),
        vec!["fixture".into(), "other".into()],
        factory,
        harness.socket.clone(),
        String::new(),
        String::new(),
    );
    assert_eq!(service.apply("other",change(CapTab::Plugins,false)).await,ChangeOutcome::Refused("Profile other is not served by the gateway, so its capabilities cannot be read or changed.".into()));
    let frames = harness.gateway.requests("plugins.manage");
    assert_eq!(
        frames.last().unwrap().params,
        json!({"action":"list","profile":"other"})
    );
    assert!(
        harness.rest.requests().await.is_empty(),
        "socket routing must not fall back to REST"
    );
}

struct FailSecondTicket(AtomicUsize);
impl hermes_client::rest::TicketMinter for FailSecondTicket {
    fn mint(
        &self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<hermes_client::Ticket, hermes_client::TicketError>,
                > + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let index = self.0.fetch_add(1, Ordering::SeqCst);
            if index == 1 {
                Err(hermes_client::TicketError::Malformed)
            } else {
                hermes_client::Ticket::parse(format!("TICKET-SENTINEL-{index}"))
            }
        })
    }
}
async fn apply_reports_a_rest_write_not_sent_after_successful_pre_read() {
    for kind in [CapTab::Skills, CapTab::Toolsets] {
        let harness = Harness::new().await;
        let minter = Arc::new(FailSecondTicket(AtomicUsize::new(0)));
        let factory: talaria_app::settings::MinterFactory = Arc::new(move |_| minter.clone());
        let service = talaria_app::capabilities::CapabilitiesService::new(
            Some(harness.rest.origin()),
            vec!["fixture".into()],
            factory,
            harness.socket.clone(),
            String::new(),
            String::new(),
        );
        assert_eq!(
            service.apply("fixture", change(kind, false)).await,
            ChangeOutcome::Refused("Talaria could not reach the gateway. Press r to retry.".into())
        );
        let requests = harness.rest.requests().await;
        assert_eq!(requests.len(), 1);
        assert_no_write(&harness, &requests);
    }
}
