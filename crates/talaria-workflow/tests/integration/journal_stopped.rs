//! An agent whose turn a stop request ended parks for the operator instead of failing, unless
//! the stop was part of stopping the run.
#![cfg(feature = "fake-gateway")]
use crate::support;
use futures::future::BoxFuture;
use serde_json::json;
use std::time::Duration;
use support::harness::*;
use talaria_workflow::{
    engine::{AgentRequest, BudgetSnapshot, EngineEvent, RunOutcome},
    journal::{
        AdmissionGate, AttentionCause, CallOutcome, GateDecision, GatePermit, GateReason,
        JournalNotice, Record, Resolution, ResolvedLaunch,
        fake_gateway::{FakeGateway, FakeOutcome, ScriptRule},
    },
};

const SCRIPT: &str = "return await agent('slow',{label:'slow'});";

/// A turn that Hermes ends as interrupted, the way a key press in a session window ends it.
fn stopped(label: &str) -> ScriptRule {
    ScriptRule {
        outcome: FakeOutcome::Interrupted,
        ..rule(label, "Operation interrupted.", 10, None)
    }
}

fn harness(rules: Vec<ScriptRule>) -> (String, Harness) {
    let script = source(SCRIPT);
    let mut harness = Harness::new(&script);
    harness.gateway = FakeGateway::new().with_script(rules);
    (script, harness)
}

fn finished_reason(harness: &Harness) -> String {
    harness
        .records()
        .iter()
        .rev()
        .find_map(|line| match &line.record {
            Record::CallFinished {
                outcome: CallOutcome::Failed { reason },
                ..
            } => Some(reason.clone()),
            _ => None,
        })
        .expect("a failed finish")
}

fn first_session(gateway: &FakeGateway) -> String {
    gateway
        .stats()
        .admissions_by_session
        .keys()
        .next()
        .cloned()
        .expect("a session")
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_stopped_agent_parks_without_a_time_limit_and_fail_ends_it() {
    let (script, harness) = harness(vec![stopped("slow")]);
    // Parked for a long time: nothing ends it but an answer.
    let (run, notice) = harness
        .run_parked(
            &script,
            Some(Resolution::Fail),
            Duration::from_secs(24 * 3600),
        )
        .await;
    let JournalNotice::NeedsOperator {
        cause,
        episode,
        detail,
        ..
    } = &notice
    else {
        unreachable!()
    };
    assert_eq!((*cause, *episode), (AttentionCause::Stopped, 0));
    assert_eq!(detail, "interrupted");
    assert_eq!(result(run.outcome), json!(null));
    assert_eq!(
        finished_reason(&harness),
        "operator marked a stopped agent as failed: interrupted"
    );
    assert_eq!(harness.gateway.stats().sessions, 1);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn rerun_starts_a_stopped_agent_again_in_a_new_session() {
    let (script, harness) = harness(vec![stopped("slow")]);
    let gateway = harness.gateway.clone();
    let (run, _) = harness
        .run_parked_then(&script, Duration::ZERO, |_| {
            gateway.set_rules(vec![rule("slow", "second try", 10, None)]);
            Some(Resolution::Rerun)
        })
        .await;
    assert_eq!(result(run.outcome), json!("second try"));
    assert_eq!(harness.gateway.stats().sessions, 2);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn abort_ends_the_run() {
    let (script, harness) = harness(vec![stopped("slow")]);
    let (run, _) = harness
        .run_parked(&script, Some(Resolution::Abort), Duration::ZERO)
        .await;
    assert!(
        matches!(run.outcome, RunOutcome::Aborted { .. }),
        "{:?}",
        run.outcome
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn use_reply_takes_a_reply_that_already_settled_on_the_same_session() {
    let (script, harness) = harness(vec![stopped("slow")]);
    let gateway = harness.gateway.clone();
    let (run, _) = harness
        .run_parked_then(&script, Duration::from_secs(5), |_| {
            // Someone typed "continue" into the agent's session and its turn finished.
            let session = first_session(&gateway);
            gateway
                .continue_session(&session, rule("other", "picked up again", 10, None))
                .unwrap();
            Some(Resolution::UseReply)
        })
        .await;
    assert_eq!(result(run.outcome), json!("picked up again"));
    let stats = harness.gateway.stats();
    assert_eq!((stats.sessions, stats.admissions), (1, 2));
    let records = harness.records();
    // The adopted reply settles the stopped turn again under the other client's admission.
    let settled: Vec<_> = records
        .iter()
        .filter_map(|line| match &line.record {
            Record::TurnSettled {
                turn, admission_id, ..
            } => Some((*turn, admission_id.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        settled,
        vec![(0, "admission-1".into()), (0, "admission-2".into())]
    );
    assert!(records.iter().any(|line| matches!(
        &line.record,
        Record::CallFinished {
            turns: 1,
            outcome: CallOutcome::Completed { .. },
            ..
        }
    )));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn use_reply_waits_for_a_reply_that_comes_later() {
    let (script, harness) = harness(vec![stopped("slow")]);
    let gateway = harness.gateway.clone();
    let (run, _) = harness
        .run_parked_then(&script, Duration::ZERO, |_| {
            let gateway = gateway.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                let session = first_session(&gateway);
                gateway.continue_session(&session, rule("other", "later", 10, None));
            });
            Some(Resolution::UseReply)
        })
        .await;
    assert_eq!(result(run.outcome), json!("later"));
}

/// The gate the runner uses while it stops the run.
#[derive(Clone)]
struct StoppingGate;
impl AdmissionGate for StoppingGate {
    fn acquire(
        &self,
        _: &AgentRequest,
        _: &ResolvedLaunch,
        _: GateReason,
    ) -> BoxFuture<'static, GateDecision> {
        Box::pin(async { GateDecision::Proceed(GatePermit::noop()) })
    }
    fn budget(&self) -> BudgetSnapshot {
        BudgetSnapshot::default()
    }
    fn run_stopping(&self) -> bool {
        true
    }
}

fn settled_source(harness: &Harness) -> Option<String> {
    harness
        .records()
        .iter()
        .find_map(|line| match &line.record {
            Record::TurnSettled { result_meta, .. } => {
                result_meta["stopped_by"].as_str().map(str::to_owned)
            }
            _ => None,
        })
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_stop_while_the_run_stops_fails_the_call_and_is_journaled_as_the_runs() {
    let (script, harness) = harness(vec![stopped("slow")]);
    let run = harness.run_with(&script, StoppingGate).await;
    assert_eq!(result(run.outcome), json!(null));
    assert!(
        !run.notices
            .iter()
            .any(|n| matches!(n, JournalNotice::NeedsOperator { .. }))
    );
    assert_eq!(settled_source(&harness).as_deref(), Some("run_stop"));
}

/// Start the call, leave the workflow view's note while its turn runs, then let it settle.
async fn stopped_from_the_view(restart: bool) -> (String, Harness) {
    let (script, harness) = harness(vec![ScriptRule {
        hold: Some("hold".into()),
        ..stopped("slow")
    }]);
    harness
        .crash_when(&script, |events| {
            events
                .iter()
                .any(|e| matches!(e, EngineEvent::AgentStarted { .. }))
                && harness.gateway.stats().admissions >= 1
        })
        .await;
    let key = harness
        .records()
        .iter()
        .find_map(|line| match &line.record {
            Record::CallIntent { key, .. } => Some(key.clone()),
            _ => None,
        })
        .unwrap();
    harness.dir.note_agent_stop(&key, restart, 1).unwrap();
    harness.gateway.release("hold");
    (script, harness)
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_stop_from_the_view_parks_and_names_the_view() {
    let (script, harness) = stopped_from_the_view(false).await;
    let (run, notice) = harness
        .run_parked(&script, Some(Resolution::Fail), Duration::ZERO)
        .await;
    assert!(matches!(
        notice,
        JournalNotice::NeedsOperator {
            cause: AttentionCause::Stopped,
            ..
        }
    ));
    assert_eq!(result(run.outcome), json!(null));
    assert_eq!(settled_source(&harness).as_deref(), Some("workflow_view"));
    // The note is consumed once its turn settled.
    assert!(harness.dir.agent_stop(&key_of(&harness)).is_none());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_stop_the_view_sent_to_restart_an_agent_fails_it_without_parking() {
    let (script, harness) = stopped_from_the_view(true).await;
    let run = harness.run(&script).await;
    assert_eq!(result(run.outcome), json!(null));
    assert!(
        !run.notices
            .iter()
            .any(|n| matches!(n, JournalNotice::NeedsOperator { .. }))
    );
    assert_eq!(
        settled_source(&harness).as_deref(),
        Some("workflow_view_restart")
    );
}

fn key_of(harness: &Harness) -> String {
    harness
        .records()
        .iter()
        .find_map(|line| match &line.record {
            Record::CallIntent { key, .. } => Some(key.clone()),
            _ => None,
        })
        .unwrap()
}
