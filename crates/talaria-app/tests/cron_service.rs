#[path = "support/mock_gateway.rs"]
mod mock_gateway;
#[path = "../../hermes-client/tests/support/mock_rest.rs"]
mod mock_rest;

use std::time::{Duration, Instant};

use hermes_client::{
    cron_api::{self, TRIGGER_DEADLINE},
    rest::{RequestOpts, RestClient, RestError, RestOptions},
};
use hermes_contract::events::EventFrame;
use mock_gateway::{Behavior, MockGateway};
use mock_rest::{CountingMinter, MockRest};
use serde_json::{Value, json};
use std::sync::Arc;
use talaria_app::cron::{CronService, classify_rest, hide_secrets};
use talaria_core::cron::{
    AddRequest, CallFailure, CronEffect, CronInput, FireError, RestFailure, TriggerOutcome,
};
use talaria_core::session::{CellId, Event, NoticeCell, NoticeKind, SessionId, SessionState};

const LIST: &str = include_str!("../../../fixtures/socket/cron-manage-list.json");
const SECRET: &str = "sk-test-sentinel-0123456789abcd";

fn non_null_keys(params: &Value) -> Vec<&str> {
    let mut keys: Vec<_> = params
        .as_object()
        .expect("params object")
        .iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, _)| key.as_str())
        .collect();
    keys.sort_unstable();
    keys
}

async fn service() -> (MockGateway, CronService) {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let service = CronService::new(client, None, "fixture".into(), 1);
    (gateway, service)
}

#[tokio::test]
async fn socket_params_are_exact() {
    let (gateway, service) = service().await;
    let samples = [
        CronEffect::List { seq: 1 },
        CronEffect::Add(AddRequest {
            name: Some("daily digest".into()),
            schedule: "every day 9am".into(),
            prompt: "Summarise yesterday's merged pull requests in five lines.".into(),
            repeat: Some(3),
            deliver: "local".into(),
        }),
        CronEffect::Add(AddRequest {
            name: None,
            schedule: "every day 9am".into(),
            prompt: "Run the checks.".into(),
            repeat: None,
            deliver: "local".into(),
        }),
        CronEffect::SetPaused {
            job_id: "a1b2c3d4e5f6".into(),
            paused: true,
        },
        CronEffect::SetPaused {
            job_id: "a1b2c3d4e5f6".into(),
            paused: false,
        },
        CronEffect::Remove {
            job_id: "a1b2c3d4e5f6".into(),
        },
    ];
    for effect in samples {
        gateway.push_result(json!({"success": true, "jobs": [], "count": 0}));
        let before = gateway.requests("cron.manage").len();
        let _inputs = service.execute(effect.clone()).await;
        let frames = gateway.requests("cron.manage");
        assert_eq!(frames.len(), before + 1, "one cron.manage frame");
        let params = &frames[before].params;
        assert!(params.get("profile").is_none() || params["profile"].is_null());
        match effect {
            CronEffect::List { .. } => {
                assert_eq!(params["action"], "list");
                assert_eq!(params["include_disabled"], true);
                assert_eq!(non_null_keys(params), vec!["action", "include_disabled"]);
            }
            CronEffect::Add(request) => {
                assert_eq!(params["action"], "add");
                assert_eq!(params["schedule"], request.schedule);
                assert_eq!(params["prompt"], request.prompt);
                assert_eq!(params["deliver"], request.deliver);
                match request.name {
                    Some(name) => assert_eq!(params["name"], name),
                    None => assert!(params.get("name").is_none() || params["name"].is_null()),
                }
                match request.repeat {
                    Some(times) => assert_eq!(params["repeat"], json!(times)),
                    None => assert!(params.get("repeat").is_none() || params["repeat"].is_null()),
                }
                for key in non_null_keys(params) {
                    assert!(
                        matches!(
                            key,
                            "action" | "name" | "schedule" | "prompt" | "repeat" | "deliver"
                        ),
                        "unexpected {key}"
                    );
                }
            }
            CronEffect::SetPaused { paused, job_id } => {
                assert_eq!(params["action"], if paused { "pause" } else { "resume" });
                assert_eq!(params["name"], job_id);
                assert_eq!(non_null_keys(params), vec!["action", "name"]);
            }
            CronEffect::Remove { job_id } => {
                assert_eq!(params["action"], "remove");
                assert_eq!(params["name"], job_id);
                assert_eq!(non_null_keys(params), vec!["action", "name"]);
            }
            other => panic!("not a socket effect: {other:?}"),
        }
    }
}

#[tokio::test]
async fn replies_decode_v0224_shapes() {
    let object = json!({"at": "2026-10-22T03:00:01-04:00", "detail": "forward to gateway failed"});
    let (at, detail) = hermes_contract::overrides::last_fire_error_parts(&object).expect("object");
    assert_eq!(at.as_deref(), Some("2026-10-22T03:00:01-04:00"));
    assert_eq!(detail, "forward to gateway failed");
    let (at, detail) =
        hermes_contract::overrides::last_fire_error_parts(&json!("provider returned 429"))
            .expect("string");
    assert!(at.is_none());
    assert_eq!(detail, "provider returned 429");

    let (gateway, service) = service().await;
    gateway.push_result(serde_json::from_str(LIST).unwrap());
    let inputs = service.execute(CronEffect::List { seq: 1 }).await;
    let CronInput::Listed {
        seq: 1,
        result: Ok(reply),
    } = &inputs[0]
    else {
        panic!("list reply: {inputs:?}");
    };
    assert_eq!(reply.jobs.len(), 7);
    assert_eq!(
        reply.warning.as_deref(),
        Some("The scheduler is not running.")
    );
    assert_eq!(
        reply.jobs[4].last_fire_error,
        Some(FireError {
            at: Some("2026-10-22T03:00:01-04:00".into()),
            detail: "forward to gateway failed".into(),
        })
    );
    assert!(reply.jobs.iter().any(|job| job.job_id == "e5f6a1b2c3d4"));

    gateway.push_result(json!({
        "success": false,
        "error": "nope",
        "job_id": "a1b2c3d4e5f6",
        "job_saved": true
    }));
    let inputs = service
        .execute(CronEffect::Add(AddRequest {
            name: Some("daily digest".into()),
            schedule: "blue moon".into(),
            prompt: "Run the checks.".into(),
            repeat: None,
            deliver: "local".into(),
        }))
        .await;
    assert!(matches!(
        &inputs[0],
        CronInput::Added(Err(CallFailure::Tool {
            error,
            job_saved: true,
            job_id: Some(id),
            matches: 0,
        })) if error == "nope" && id == "a1b2c3d4e5f6"
    ));
    gateway.push_result(json!({
        "success": false,
        "error": "two names",
        "matches": ["daily digest", "daily digest"]
    }));
    let inputs = service
        .execute(CronEffect::Remove {
            job_id: "daily digest".into(),
        })
        .await;
    assert!(matches!(
        &inputs[0],
        CronInput::Removed {
            result: Err(CallFailure::Tool {
                matches: 2,
                job_saved: false,
                ..
            }),
            ..
        }
    ));
}

#[tokio::test]
async fn secret_shaped_errors_are_hidden() {
    assert!(!hermes_client::redact::scan_for_secrets(SECRET).is_empty());
    let hidden = hide_secrets(&format!("boom {SECRET}"));
    assert_eq!(hidden, "[hidden: looked like a credential]");
    assert!(!hidden.contains(SECRET));
    let (gateway, service) = service().await;
    let mut list: Value = serde_json::from_str(LIST).unwrap();
    list["jobs"][2]["last_delivery_error"] = json!(SECRET);
    list["jobs"][2]["paused_reason"] = json!(format!("held {SECRET}"));
    list["jobs"][2]["prompt_preview"] = json!(SECRET);
    list["jobs"][4]["last_fire_error"]["detail"] = json!(SECRET);
    gateway.push_result(list);
    let inputs = service.execute(CronEffect::List { seq: 1 }).await;
    let dumped = format!("{inputs:?}");
    assert!(!dumped.contains(SECRET));
    assert!(dumped.contains("[hidden: looked like a credential]"));
    gateway.push_result(json!({"success": false, "error": SECRET}));
    let inputs = service
        .execute(CronEffect::Remove {
            job_id: "a1b2c3d4e5f6".into(),
        })
        .await;
    assert!(!format!("{inputs:?}").contains(SECRET));
}

#[tokio::test]
async fn cron_changed_reaches_the_reducer() {
    let (gateway, service) = service().await;
    let input = service.input_for_event("cron.changed");
    assert_eq!(input, Some(CronInput::Changed));
    assert_eq!(service.input_for_event("skin.changed"), None);
    gateway.send_event("cron.changed", json!({})).await;
    let raw = json!({
        "type": "cron.changed",
        "session_id": "",
        "payload": {}
    });
    let frame = EventFrame::from_params(&raw).expect("cron.changed frame");
    assert_eq!(frame.type_name(), Some("cron.changed"));
    let mut session = SessionState::empty(SessionId::new("local-session-1"));
    let before = session.unknown_event_types().len();
    talaria_core::session::update(
        &mut session,
        Event::Session {
            session_id: SessionId::new("local-session-1"),
            frame,
            raw,
        },
    );
    assert_eq!(session.unknown_event_types().len(), before);
    assert!(
        !session
            .unknown_event_types()
            .iter()
            .any(|name| name == "cron.changed")
    );
}

#[tokio::test]
async fn dropped_add_is_sent_once() {
    let (gateway, service) = service().await;
    gateway.set_behavior("cron.manage", Behavior::DropAfterFrame);
    let inputs = service
        .execute(CronEffect::Add(AddRequest {
            name: Some("daily digest".into()),
            schedule: "every day 9am".into(),
            prompt: "Run the checks.".into(),
            repeat: None,
            deliver: "local".into(),
        }))
        .await;
    let frames = gateway.requests("cron.manage");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].params["action"], "add");
    assert_eq!(inputs, vec![CronInput::Added(Err(CallFailure::Unknown))]);
}

#[tokio::test]
async fn run_now_does_not_block() {
    let path = "/api/cron/jobs/a1b2c3d4e5f6/trigger?profile=fixture";
    let mock = MockRest::start()
        .await
        .with_route("POST", path, |_request| (200, json!({"ok": true})));
    mock.delay("POST", path, Duration::from_secs(3)).await;
    let rest = RestClient::new(
        &mock.origin(),
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .unwrap();
    let (gateway, client, _inbound) = MockGateway::start().await;
    gateway.push_result(json!({"success": true, "jobs": [], "count": 0, "gateway_running": true}));
    let service = CronService::new(client, Some(rest), "fixture".into(), 1);
    let started = Instant::now();
    let trigger = service.execute(CronEffect::Trigger {
        job_id: "a1b2c3d4e5f6".into(),
        name: "daily digest".into(),
    });
    let list = service.execute(CronEffect::List { seq: 1 });
    tokio::pin!(trigger);
    tokio::pin!(list);
    tokio::select! {
        listed = &mut list => {
            assert!(started.elapsed() < Duration::from_secs(2));
            assert!(matches!(
                listed.first(),
                Some(CronInput::Listed { result: Ok(_), .. })
            ));
        }
        _ = tokio::time::sleep(Duration::from_millis(500)) => {
            panic!("the list did not finish while the trigger was delayed");
        }
        early = &mut trigger => {
            panic!("the trigger returned before the list: {early:?}");
        }
    }
    let triggered = trigger.await;
    assert!(matches!(
        triggered.first(),
        Some(CronInput::Triggered {
            outcome: TriggerOutcome::Accepted,
            ..
        })
    ));
}

#[tokio::test]
async fn stale_generation_is_dropped() {
    let (gateway, client, _inbound) = MockGateway::start().await;
    let first = CronService::new(client, None, "fixture".into(), 1);
    let tag = first.tag_for(&CronEffect::Trigger {
        job_id: "0f9e8d7c6b5a".into(),
        name: "nightly check".into(),
    });
    assert_eq!(tag.generation, 1);
    assert_eq!(tag.profile, "fixture");
    assert_eq!(tag.run_name.as_deref(), Some("nightly check"));
    let (_other_gateway, other_client, _other_in) = MockGateway::start().await;
    let second = CronService::new(other_client, None, "default".into(), 2);
    assert_ne!(tag.generation, second.generation());
    let old_list_applies = tag.generation == second.generation();
    assert!(!old_list_applies);
    let notice = talaria_core::cron::stale_run_notice(
        "nightly check",
        &tag.profile,
        false,
        &TriggerOutcome::Accepted,
    );
    assert_eq!(
        notice,
        "The run of nightly check in profile fixture ended. Switch back to fixture and open /cron to see its result."
    );
    let cell = NoticeCell {
        id: CellId::new(1),
        kind: NoticeKind::System,
        text: format!("Scheduled jobs: {notice}"),
        detail: None,
    };
    assert!(cell.text.contains("nightly check"));
    assert!(cell.text.contains("profile fixture"));
    let back = talaria_core::cron::stale_run_notice(
        "nightly check",
        "fixture",
        true,
        &TriggerOutcome::Failed(RestFailure::Server(500)),
    );
    assert!(back.ends_with("Open /cron to see its result."));
    assert!(back.contains("did not complete:"));
    let (_again_gateway, again_client, _again_in) = MockGateway::start().await;
    let reconnected = CronService::new(again_client, None, "fixture".into(), 1);
    assert_eq!(reconnected.generation(), tag.generation);
    let _ = gateway;
}

fn client(origin: &str) -> RestClient {
    RestClient::new(
        origin,
        Arc::new(CountingMinter::default()),
        RestOptions::default(),
    )
    .expect("loopback origin")
}

#[tokio::test]
async fn failures_classified() {
    let path = "/api/cron/jobs/a1b2c3d4e5f6/trigger?profile=fixture";
    let cases = [
        (400_u16, Some("invalid profile name"), "refused"),
        (403, Some("profile_scope_mismatch"), "forbidden"),
        (404, Some("Job not found"), "missing"),
        (
            409,
            Some("Job is already running or was claimed by another scheduler"),
            "claimed",
        ),
        (
            409,
            Some("Cron provider 'file' does not support atomic forced firing of paused jobs"),
            "forced",
        ),
        (500, Some("boom"), "server"),
    ];
    for (status, detail, label) in cases {
        let mock = MockRest::start().await;
        mock.fail_status("POST", path, status, detail).await;
        let rest = client(&mock.origin());
        let error = cron_api::trigger_job(
            &rest,
            "fixture",
            "a1b2c3d4e5f6",
            RequestOpts {
                ticket: true,
                deadline: Some(TRIGGER_DEADLINE),
            },
        )
        .await
        .expect_err(label);
        let failure = classify_rest(&error);
        match status {
            400 | 409 => {
                assert!(matches!(error, RestError::Refused { status: got, .. } if got == status));
                assert_eq!(
                    failure,
                    RestFailure::Refused {
                        status,
                        detail: detail.unwrap_or("").into(),
                    }
                );
            }
            403 => {
                assert!(matches!(error, RestError::Forbidden { .. }));
                assert_eq!(failure, RestFailure::Forbidden(detail.unwrap_or("").into()));
            }
            404 => {
                assert!(matches!(error, RestError::NotFound { .. }));
                assert_eq!(failure, RestFailure::NotFound);
            }
            500 => {
                assert!(matches!(error, RestError::Server { status: 500 }));
                assert_eq!(failure, RestFailure::Server(500));
            }
            _ => unreachable!(),
        }
    }

    let refused = client("http://127.0.0.1:1");
    let error = cron_api::trigger_job(
        &refused,
        "fixture",
        "a1b2c3d4e5f6",
        RequestOpts {
            ticket: true,
            deadline: Some(Duration::from_millis(200)),
        },
    )
    .await
    .expect_err("connection refused");
    assert!(matches!(error, RestError::Connect(_)));
    assert!(matches!(classify_rest(&error), RestFailure::NotSent(_)));

    let mock = MockRest::start().await;
    mock.delay("POST", path, Duration::from_millis(500)).await;
    mock.response_body("POST", path, br#"{"ok":true}"#.to_vec())
        .await;
    let rest = client(&mock.origin());
    let error = cron_api::trigger_job(
        &rest,
        "fixture",
        "a1b2c3d4e5f6",
        RequestOpts {
            ticket: true,
            deadline: Some(Duration::from_millis(200)),
        },
    )
    .await
    .expect_err("timed out after the request was sent");
    assert!(matches!(error, RestError::Timeout { sent: true }));
    assert!(error.outcome_unknown());
    assert_eq!(classify_rest(&error), RestFailure::Unknown);
}
