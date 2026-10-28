#![cfg(feature = "fake-gateway")]
mod support;
use hermes_client::{ReplayFrame, ReplaySource};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};
use support::harness::*;
use talaria_workflow::journal::{
    AdmissionState, CreateSession, GatewayPort, PortError, SubmitTurn,
    fake_gateway::{FakeGateway, FakeOutcome},
};

fn create(id: &str) -> CreateSession {
    CreateSession {
        request_id: id.into(),
        launch: launch(),
        label: "review".into(),
    }
}
fn submit(session: &str, id: &str, text: &str) -> SubmitTurn {
    SubmitTurn {
        profile: "fixture".into(),
        session_id: session.into(),
        submission_id: id.into(),
        text: text.into(),
    }
}
fn reason<T>(value: Result<T, PortError>, expected: &str) {
    match value {
        Err(PortError::Refused { reason, .. }) => assert_eq!(reason, expected),
        _ => panic!("expected refusal {expected}"),
    }
}
fn answer<'a>(request: &ReplayFrame, inbound: &'a [&ReplayFrame]) -> &'a Value {
    &inbound
        .iter()
        .find(|response| {
            response.seq > request.seq
                && response.generation == request.generation
                && response.frame.get("id") == request.frame.get("id")
        })
        .expect("response to recorded request")
        .frame
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn matches_recorded_idempotency_fixture() {
    let name = ["p", "5", "-", "02", "-idempotency.jsonl"].concat();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("frames")
        .join(name);
    if !path.exists() {
        assert_ne!(
            std::env::var("TALARIA_REQUIRE_FIXTURE").ok().as_deref(),
            Some("1"),
            "recorded idempotency fixture is required"
        );
        println!("recorded idempotency fixture is absent");
        return;
    }
    let source = ReplaySource::open(&path).unwrap();
    let outbound = source.outbound().collect::<Vec<_>>();
    let inbound = source.inbound().collect::<Vec<_>>();
    let creates = outbound
        .iter()
        .copied()
        .filter(|f| f.frame["method"] == "session.create")
        .collect::<Vec<_>>();
    let submits = outbound
        .iter()
        .copied()
        .filter(|f| f.frame["method"] == "prompt.submit")
        .collect::<Vec<_>>();
    let receipts = outbound
        .iter()
        .copied()
        .filter(|f| {
            f.frame["method"] == "prompt.receipt" && f.frame["params"]["include_result"] == true
        })
        .collect::<Vec<_>>();
    let duplicate_create = creates
        .iter()
        .enumerate()
        .find_map(|(i, a)| {
            creates
                .iter()
                .skip(i + 1)
                .find(|b| {
                    a.frame["params"]["request_id"] == b.frame["params"]["request_id"]
                        && a.frame["params"] == b.frame["params"]
                })
                .map(|b| (*a, *b))
        })
        .expect("duplicate create missing");
    let conflict_create = creates
        .iter()
        .enumerate()
        .find_map(|(i, a)| {
            creates
                .iter()
                .skip(i + 1)
                .find(|b| {
                    a.frame["params"]["request_id"] == b.frame["params"]["request_id"]
                        && a.frame["params"]["model"] != b.frame["params"]["model"]
                })
                .map(|b| (*a, *b))
        })
        .expect("changed create missing");
    let duplicate_submit = submits
        .iter()
        .enumerate()
        .find_map(|(i, a)| {
            submits
                .iter()
                .skip(i + 1)
                .find(|b| {
                    a.frame["params"]["submission_id"] == b.frame["params"]["submission_id"]
                        && a.frame["params"]["text"] == b.frame["params"]["text"]
                })
                .map(|b| (*a, *b))
        })
        .expect("duplicate submit missing");
    let conflict_submit = submits
        .iter()
        .enumerate()
        .find_map(|(i, a)| {
            submits
                .iter()
                .skip(i + 1)
                .find(|b| {
                    a.frame["params"]["submission_id"] == b.frame["params"]["submission_id"]
                        && a.frame["params"]["text"] != b.frame["params"]["text"]
                })
                .map(|b| (*a, *b))
        })
        .expect("changed submit missing");
    let terminal = receipts
        .iter()
        .find(|r| {
            answer(r, &inbound)["result"]["status"] == "terminal"
                && answer(r, &inbound)["result"].get("result").is_some()
                && answer(r, &inbound)["result"]["usage"].is_object()
        })
        .expect("terminal result and usage missing");
    assert_eq!(
        answer(duplicate_create.0, &inbound)["result"]["session_id"],
        answer(duplicate_create.1, &inbound)["result"]["session_id"]
    );
    assert_eq!(
        answer(duplicate_submit.0, &inbound)["result"]["admission_id"],
        answer(duplicate_submit.1, &inbound)["result"]["admission_id"]
    );
    assert_eq!(
        answer(conflict_create.1, &inbound)["error"]["data"]["reason"],
        "invalid_params"
    );
    assert_eq!(
        answer(conflict_submit.1, &inbound)["error"]["data"]["reason"],
        "admission_conflict"
    );
    assert!(answer(terminal, &inbound)["result"]["result"].is_object());
    let fake = FakeGateway::new();
    let req = create("fixture-id");
    let a = fake.create_session(req.clone()).await.unwrap();
    let b = fake.create_session(req.clone()).await.unwrap();
    assert_eq!(a.session_id, b.session_id);
    let mut changed = req;
    changed.launch.model = "other".into();
    reason(fake.create_session(changed).await, "invalid_params");
    let turn = submit(&a.session_id, "submission", "hello");
    let first = fake.submit(turn.clone()).await.unwrap();
    let duplicate = fake.submit(turn.clone()).await.unwrap();
    assert_eq!(first.admission_id, duplicate.admission_id);
    let mut changed = turn;
    changed.text = "different".into();
    reason(fake.submit(changed).await, "admission_conflict");
    let saved = fake
        .wait_settled("fixture".into(), a.session_id, first.admission_id)
        .await
        .unwrap();
    assert!(saved.result.is_some());
    assert!(saved.usage.is_some());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn rules_f1_to_f14() {
    let fake = FakeGateway::new();
    for id in [String::new(), "x".repeat(257)] {
        reason(fake.create_session(create(&id)).await, "invalid_params");
    }
    let bound = fake.create_session(create("fixture-id")).await.unwrap();
    assert_eq!(
        bound.session_id,
        "local-391ebe46700a4e7e88892c7f06d63bbb3d75ccd037507f70c59bdfdfda4a7036"
    );
    let repeated = fake.create_session(create("fixture-id")).await.unwrap();
    assert_eq!(bound, repeated);
    assert_eq!(fake.stats().sessions, 1);
    let mut changed = create("fixture-id");
    changed.launch.model = "other".into();
    reason(fake.create_session(changed).await, "invalid_params");
    reason(
        fake.attach("fixture".into(), "missing".into()).await,
        "not_found",
    );
    fake.disconnect();
    reason(
        fake.submit(submit(&bound.session_id, "one", "hello")).await,
        "permission_denied",
    );
    fake.attach("fixture".into(), bound.session_id.clone())
        .await
        .unwrap();
    reason(
        fake.submit(submit(&bound.session_id, "", "hello")).await,
        "invalid_params",
    );
    let first = fake
        .submit(submit(&bound.session_id, "one", "hello"))
        .await
        .unwrap();
    let again = fake
        .submit(submit(&bound.session_id, "one", "hello"))
        .await
        .unwrap();
    assert_eq!(first.admission_id, again.admission_id);
    assert!(again.result.is_none() && again.usage.is_none());
    reason(
        fake.submit(submit(&bound.session_id, "one", "different"))
            .await,
        "admission_conflict",
    );
    let done = fake
        .wait_settled(
            "fixture".into(),
            bound.session_id.clone(),
            first.admission_id.clone(),
        )
        .await
        .unwrap();
    assert_eq!(done.status, AdmissionState::Terminal);
    assert!(done.result.is_some() && done.usage.is_some());
    reason(
        fake.resolve_unknown(
            "fixture".into(),
            bound.session_id.clone(),
            first.admission_id.clone(),
            done.execution_generation.unwrap(),
        )
        .await,
        "stale_generation",
    );
    let terminal_retry = fake
        .submit(submit(&bound.session_id, "one", "hello"))
        .await
        .unwrap();
    assert_eq!(terminal_retry.status, AdmissionState::Terminal);
    assert!(terminal_retry.result.is_none() && terminal_retry.usage.is_none());
    reason(
        fake.receipt("fixture".into(), bound.session_id.clone(), "missing".into())
            .await,
        "not_found",
    );
    let other = fake.create_session(create("other")).await.unwrap();
    reason(
        fake.receipt(
            "fixture".into(),
            other.session_id,
            first.admission_id.clone(),
        )
        .await,
        "not_found",
    );
    fake.disconnect();
    fake.attach("fixture".into(), bound.session_id.clone())
        .await
        .unwrap();
    assert_eq!(
        fake.receipt(
            "fixture".into(),
            bound.session_id.clone(),
            first.admission_id.clone()
        )
        .await
        .unwrap()
        .status,
        AdmissionState::Terminal
    );
    fake.delete_session(&bound.session_id);
    reason(
        fake.attach("fixture".into(), bound.session_id.clone())
            .await,
        "not_found",
    );
    reason(
        fake.receipt("fixture".into(), bound.session_id, first.admission_id)
            .await,
        "not_found",
    );

    let continuing = FakeGateway::new().with_script(vec![rule("review", "continued", 100, None)]);
    let continued_session = continuing
        .create_session(create("disconnect"))
        .await
        .unwrap()
        .session_id;
    let continued = continuing
        .submit(submit(&continued_session, "continued", "work"))
        .await
        .unwrap();
    continuing.disconnect();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(200)).await;
    tokio::task::yield_now().await;
    continuing
        .attach("fixture".into(), continued_session.clone())
        .await
        .unwrap();
    let continued = continuing
        .receipt("fixture".into(), continued_session, continued.admission_id)
        .await
        .unwrap();
    assert_eq!(continued.status, AdmissionState::Terminal);
    assert!(continued.result.is_some());

    let mut held = rule("review", "ok", 10, Some("hold"));
    held.duration = Duration::from_millis(10);
    let queue = FakeGateway::new().with_script(vec![held]);
    let session = queue
        .create_session(create("queue"))
        .await
        .unwrap()
        .session_id;
    let active = queue
        .submit(submit(&session, "active", "active"))
        .await
        .unwrap();
    let mut started = None;
    for _ in 0..100 {
        let receipt = queue
            .receipt(
                "fixture".into(),
                session.clone(),
                active.admission_id.clone(),
            )
            .await
            .unwrap();
        if receipt.status == AdmissionState::Started {
            started = Some(receipt);
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert!(started.unwrap().execution_generation.is_some());
    let mut first_queued = None;
    for i in 0..32 {
        let admission = queue
            .submit(submit(&session, &format!("queued-{i}"), "queued"))
            .await
            .unwrap();
        assert_eq!(admission.status, AdmissionState::Queued);
        if i == 0 {
            first_queued = Some(admission.admission_id);
        }
    }
    reason(
        queue.submit(submit(&session, "overflow", "queued")).await,
        "queue_full",
    );
    queue.restart();
    queue
        .attach("fixture".into(), session.clone())
        .await
        .unwrap();
    let unknown = queue
        .receipt(
            "fixture".into(),
            session.clone(),
            active.admission_id.clone(),
        )
        .await
        .unwrap();
    assert_eq!(unknown.status, AdmissionState::Unknown);
    let queued_id = first_queued.unwrap();
    assert_eq!(
        queue
            .receipt("fixture".into(), session.clone(), queued_id.clone())
            .await
            .unwrap()
            .status,
        AdmissionState::Queued
    );
    assert_eq!(
        queue
            .create_session(create("queue"))
            .await
            .unwrap()
            .session_id,
        session
    );
    let generation = unknown.execution_generation.unwrap();
    reason(
        queue
            .resolve_unknown(
                "fixture".into(),
                session.clone(),
                active.admission_id.clone(),
                generation + 1,
            )
            .await,
        "stale_generation",
    );
    let resolved = queue
        .resolve_unknown(
            "fixture".into(),
            session.clone(),
            active.admission_id.clone(),
            generation,
        )
        .await
        .unwrap();
    assert_eq!(resolved.status, AdmissionState::Terminal);
    assert_eq!(resolved.outcome.as_deref(), Some("interrupted"));
    assert!(resolved.result.is_none());
    let repeat = queue
        .resolve_unknown(
            "fixture".into(),
            session.clone(),
            active.admission_id.clone(),
            generation,
        )
        .await
        .unwrap();
    assert_eq!(repeat.admission_id, resolved.admission_id);
    queue.release("hold");
    assert_eq!(
        queue
            .wait_settled("fixture".into(), session, queued_id)
            .await
            .unwrap()
            .status,
        AdmissionState::Terminal
    );

    let mut lost = rule("review", "kept", 10, None);
    lost.settlement_lost = true;
    lost.outcome = FakeOutcome::Completed;
    let fake = FakeGateway::new().with_script(vec![lost]);
    let session = fake
        .create_session(create("lost"))
        .await
        .unwrap()
        .session_id;
    let admission = fake.submit(submit(&session, "lost", "x")).await.unwrap();
    tokio::time::advance(Duration::from_millis(20)).await;
    tokio::task::yield_now().await;
    let unknown = fake
        .receipt(
            "fixture".into(),
            session.clone(),
            admission.admission_id.clone(),
        )
        .await
        .unwrap();
    assert_eq!(unknown.status, AdmissionState::Unknown);
    let settled = fake
        .resolve_unknown(
            "fixture".into(),
            session.clone(),
            admission.admission_id.clone(),
            unknown.execution_generation.unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(settled.outcome.as_deref(), Some("completed"));
    assert!(settled.result.is_none());
    let receipt = fake
        .receipt("fixture".into(), session, admission.admission_id)
        .await
        .unwrap();
    assert!(receipt.result.is_some());
}
