#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use hermes_client::{DiscoveryError, EnsurePolicy};
use mock_gateway::{INSTANCE_ID, StandaloneGateway};
use std::time::Duration;
use talaria_app::doctor::DoctorService;
use talaria_core::doctor::{
    CheckId, GatewayFacts, PostResult, ProbeResult, RestartEffect, RestartInput, RestartOutcome,
    RestartState, Severity, StatusFacts, WorkSummary, diagnose, render_text, restart_update,
};
use tempfile::TempDir;

fn gateway() -> (TempDir, StandaloneGateway) {
    let temp = TempDir::new().expect("temporary home");
    let gateway = StandaloneGateway::start(temp.path());
    gateway.doctor_set_revision(hermes_contract::pinned::COMMIT);
    (temp, gateway)
}
fn row(report: &talaria_core::doctor::Report, id: CheckId) -> &talaria_core::doctor::Check {
    report
        .checks
        .iter()
        .find(|row| row.id == id)
        .expect("check present")
}
fn header_count(request: &mock_gateway::DoctorHttpSeen, name: &str) -> usize {
    request
        .headers
        .iter()
        .filter(|(key, _)| key == name)
        .count()
}

#[tokio::test]
async fn gather_ready_gateway() {
    let (_temp, server) = gateway();
    let facts = DoctorService::new(server.home().to_path_buf())
        .gather("fixture")
        .await;
    let report = diagnose(&facts);
    assert_eq!(report.checks.len(), 7);
    assert!(report.checks.iter().all(|row| row.severity == Severity::Ok));
    assert!(matches!(facts.status, StatusFacts::Read(_)));
    let control = server.control_requests();
    assert_eq!(control.iter().filter(|r| r.verb == "identify").count(), 1);
    assert_eq!(
        control
            .iter()
            .filter(|r| r.verb == "session-ticket")
            .count(),
        1
    );
    let tickets = server.ticket_records();
    assert_eq!(tickets.len(), 1);
    assert_eq!(tickets[0].purpose, "interactive");
    assert_eq!(server.websocket_connections(), 1);
    let http = server.doctor_http_requests();
    assert_eq!(http.len(), 1);
    assert_eq!(http[0].method, "GET");
    assert_eq!(http[0].target, "/api/status");
    assert_eq!(header_count(&http[0], "x-hermes-gateway-ticket"), 0);
    assert_eq!(header_count(&http[0], "origin"), 0);
}

#[tokio::test]
async fn gather_not_ready_gateway() {
    for state in ["starting", "failed"] {
        let (_temp, server) = gateway();
        server.doctor_set_state(state);
        let facts = DoctorService::new(server.home().to_path_buf())
            .gather("fixture")
            .await;
        assert!(
            matches!(&facts.gateway, GatewayFacts::NotReady { state: got, .. } if got == state)
        );
        let served = &facts
            .identity
            .as_ref()
            .expect("raw identity retained")
            .served;
        assert_eq!(served, &["default", "fixture"]);
        let report = diagnose(&facts);
        assert_eq!(row(&report, CheckId::HermesVersion).severity, Severity::Ok);
        assert_eq!(
            server
                .control_requests()
                .iter()
                .filter(|r| r.verb == "identify")
                .count(),
            1
        );
        assert_eq!(server.tickets_issued(), 0);
        assert_eq!(server.websocket_connections(), 0);
        assert!(server.doctor_http_requests().is_empty());
    }
}

#[tokio::test]
async fn gather_no_gateway() {
    let temp = TempDir::new().expect("temporary home");
    let service = DoctorService::new(temp.path().to_owned());
    assert!(matches!(service.options.ensure, EnsurePolicy::Never));
    let facts = service.gather("fixture").await;
    let report = diagnose(&facts);
    let gateway = row(&report, CheckId::Gateway);
    assert_eq!(gateway.severity, Severity::Fail);
    assert_eq!(gateway.summary, DiscoveryError::NoGateway.to_string());
    assert_eq!(
        gateway.fix.as_deref(),
        Some(DiscoveryError::NoGateway.next_step())
    );
    assert_eq!(
        report
            .checks
            .iter()
            .filter(|check| check.severity == Severity::Skip)
            .count(),
        5
    );
}

#[tokio::test]
async fn restart_against_mock() {
    let (_temp, server) = gateway();
    let service = DoctorService::new(server.home().to_path_buf());
    assert_eq!(
        service.active_work().await,
        Ok(WorkSummary { chat: 2, other: 1 })
    );
    assert_eq!(
        service.post_restart(INSTANCE_ID, "fixture").await,
        PostResult::Accepted
    );
    let controls = server.control_requests();
    assert_eq!(controls.iter().filter(|r| r.verb == "identify").count(), 1);
    let tickets = server.ticket_records();
    assert_eq!(tickets.len(), 1);
    assert_eq!(tickets[0].purpose, "native-http");
    assert_eq!(tickets[0].profile_id, server.home().to_string_lossy());
    let http = server.doctor_http_requests();
    assert_eq!(http.len(), 1);
    assert_eq!(
        (&*http[0].method, &*http[0].target, &*http[0].body),
        ("POST", "/api/gateway/restart", "")
    );
    assert_eq!(header_count(&http[0], "x-hermes-gateway-ticket"), 1);
    assert_eq!(header_count(&http[0], "origin"), 0);
    server.doctor_new_after(2);
    let old = INSTANCE_ID.to_owned();
    let (mut state, _) = restart_update(
        RestartState::Requesting {
            old_instance: old.clone(),
        },
        RestartInput::Posted(PostResult::Accepted),
    );
    for second in 1..=2 {
        (state, _) = restart_update(state, RestartInput::Tick(Duration::from_secs(second)));
        let probe = service.probe().await;
        let (next, effects) = restart_update(state, RestartInput::Probe(probe));
        state = next;
        if second == 2 {
            assert!(effects.contains(&RestartEffect::RerunChecks));
        }
    }
    assert!(matches!(
        state,
        RestartState::Done(RestartOutcome::Restarted { .. })
    ));
    let facts = service.gather("fixture").await;
    let output = format!("{} {state:?}", render_text(&diagnose(&facts)));
    assert!(!output.contains("private goal marker"));
    assert!(!output.contains("agent:main:x"));
    assert!(!output.contains("agent:main:y"));

    let (_temp, changed) = gateway();
    changed.doctor_set_instance("33333333333333333333333333333333");
    let result = DoctorService::new(changed.home().to_owned())
        .post_restart(INSTANCE_ID, "fixture")
        .await;
    assert_eq!(result, PostResult::Changed);
    assert!(changed.doctor_http_requests().is_empty());
}

#[tokio::test]
async fn restart_refusals() {
    for status in [401, 503, 500] {
        let (_temp, server) = gateway();
        server.doctor_restart_status(status);
        let result = DoctorService::new(server.home().to_owned())
            .post_restart(INSTANCE_ID, "fixture")
            .await;
        match result {
            PostResult::Refused {
                http_status,
                detail,
            } => {
                assert_eq!(http_status, Some(status));
                assert!(!detail.contains(server.home().to_string_lossy().as_ref()));
            }
            other => panic!("expected refusal, got {other:?}"),
        }
        assert_eq!(
            server.doctor_http_requests().len(),
            if status == 401 { 2 } else { 1 }
        );
    }
    let (_temp, server) = gateway();
    server.doctor_drop_restart(true);
    let result = DoctorService::new(server.home().to_owned())
        .post_restart(INSTANCE_ID, "fixture")
        .await;
    assert_eq!(result, PostResult::Unknown);
    let (state, effects) = restart_update(
        RestartState::Requesting {
            old_instance: INSTANCE_ID.into(),
        },
        RestartInput::Posted(result),
    );
    assert!(matches!(
        state,
        RestartState::Waiting {
            unknown_request: true,
            ..
        }
    ));
    assert!(effects.contains(&RestartEffect::ProbeAfter(Duration::from_secs(1))));
    assert_ne!(
        DoctorService::new(server.home().to_owned()).probe().await,
        ProbeResult::NotAnswering
    );
}
