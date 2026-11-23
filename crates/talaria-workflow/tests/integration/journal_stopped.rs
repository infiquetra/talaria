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
        JournalNotice, Record, Resolution, ResolvedLaunch, StopSource, StopTurn,
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

fn admission_of(harness: &Harness) -> String {
    harness
        .records()
        .iter()
        .rev()
        .find_map(|line| match &line.record {
            Record::TurnAdmitted { admission_id, .. } => Some(admission_id.clone()),
            _ => None,
        })
        .unwrap()
}

/// Start the call, stop the runner while its turn is held, and return the harness. The turn
/// settles stopped once released.
async fn crashed_while_running(rules: Vec<ScriptRule>) -> (String, Harness) {
    let (script, harness) = harness(rules);
    harness
        .crash_when(&script, |events| {
            events
                .iter()
                .any(|e| matches!(e, EngineEvent::AgentStarted { .. }))
                && harness.gateway.stats().admissions >= 1
        })
        .await;
    (script, harness)
}

fn held_stop() -> Vec<ScriptRule> {
    vec![ScriptRule {
        hold: Some("hold".into()),
        ..stopped("slow")
    }]
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn use_reply_on_a_resumed_runner_sees_a_reply_typed_while_the_call_waits() {
    let (script, harness) = harness(vec![stopped("slow")]);
    // The first runner parks the stopped call and then goes away.
    harness
        .crash_when(&script, |_| {
            harness
                .records()
                .iter()
                .any(|line| matches!(line.record, Record::CallAttention { .. }))
        })
        .await;
    // The next runner connects afresh: Hermes has shown it nothing on the session yet.
    harness.gateway.new_client();
    let gateway = harness.gateway.clone();
    let (run, _) = tokio::time::timeout(
        Duration::from_secs(24 * 3600),
        harness.run_parked_then(&script, Duration::ZERO, |_| {
            // While the call waits for its answer, someone continues the session and that turn
            // settles at once, before the answer is read.
            let session = first_session(&gateway);
            gateway
                .continue_session(&session, rule("other", "typed while parked", 0, None))
                .unwrap();
            Some(Resolution::UseReply)
        }),
    )
    .await
    .expect("use-reply never saw the reply typed while the call waited");
    assert_eq!(result(run.outcome), json!("typed while parked"));
    // The wait for the reply was announced.
    assert!(
        run.notices
            .iter()
            .any(|n| matches!(n, JournalNotice::WaitingForReply { label, .. } if label == "slow")),
        "{:?}",
        run.notices
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn use_reply_never_takes_a_reply_that_settled_before_the_stop() {
    let script = source(
        "return await agent('slow',{label:'slow',schema:{type:'object',properties:{v:{type:'string'}}}});",
    );
    let mut harness = Harness::new(&script);
    harness.gateway = FakeGateway::new().with_script(vec![
        // The schema correction turn is the one that gets stopped.
        ScriptRule {
            text_contains: Some("Return valid JSON".into()),
            ..stopped("slow")
        },
        rule("slow", "not json", 10, Some("first")),
    ]);
    let gateway = harness.gateway.clone();
    // While turn 0 runs, a person types a message into the session; Hermes queues it and it is
    // answered before the run's correction turn starts.
    tokio::spawn({
        let gateway = gateway.clone();
        async move {
            while gateway.stats().admissions < 1 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let session = first_session(&gateway);
            gateway
                .continue_session(&session, rule("other", r#"{"v":"stale"}"#, 0, None))
                .unwrap();
            gateway.release("first");
        }
    });
    let (run, notice) = harness
        .run_parked_then(&script, Duration::ZERO, |_| {
            let gateway = gateway.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let session = first_session(&gateway);
                gateway.continue_session(&session, rule("other", r#"{"v":"fresh"}"#, 10, None));
            });
            Some(Resolution::UseReply)
        })
        .await;
    assert!(matches!(
        notice,
        JournalNotice::NeedsOperator {
            cause: AttentionCause::Stopped,
            ..
        }
    ));
    assert_eq!(result(run.outcome), json!({"v":"fresh"}));
    // The attention record kept the reply that came before the stop, so it was passed over.
    assert!(harness.records().iter().any(|line| matches!(
        &line.record,
        Record::CallAttention { seen_admissions, .. } if seen_admissions == &["admission-2".to_owned()]
    )));
}

/// The runner noted its stop and sent it, but the turn settled only after it went away.
async fn stop_noted_before_a_crash(source: StopSource) -> (String, Harness) {
    let (script, harness) = crashed_while_running(held_stop()).await;
    let turn = StopTurn {
        attempt: 0,
        turn: 0,
        admission_id: Some(admission_of(&harness)),
    };
    harness
        .dir
        .note_stop(&key_of(&harness), source, Some(&turn), 1)
        .unwrap();
    (script, harness)
}

fn finishes(harness: &Harness) -> Vec<(u32, &'static str, bool)> {
    harness
        .records()
        .iter()
        .filter_map(|line| match &line.record {
            Record::CallFinished {
                attempt, outcome, ..
            } => Some((*attempt, outcome.kind(), false)),
            Record::CallRetryFinished {
                attempt, outcome, ..
            } => Some((*attempt, outcome.kind(), true)),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_budget_stop_noted_before_a_crash_is_still_the_budgets_in_the_next_execution() {
    let (script, harness) = stop_noted_before_a_crash(StopSource::Budget).await;
    harness.gateway.release("hold");
    // The next execution is not stopping the run, yet the stop stays the runner's.
    let run = harness.run(&script).await;
    // The script sees the budget's error, as in the execution that sent the stop.
    match &run.outcome {
        RunOutcome::Failed { error } => assert_eq!(error.name, "BudgetExhaustedError"),
        other => panic!("expected the budget's error: {other:?}"),
    }
    assert!(
        !run.notices
            .iter()
            .any(|n| matches!(n, JournalNotice::NeedsOperator { .. }))
    );
    assert_eq!(settled_source(&harness).as_deref(), Some("budget"));
    assert!(harness.dir.agent_stop(&key_of(&harness)).is_none());
    // It ends the call stopped, so the next resume with a larger budget runs it again.
    assert_eq!(finishes(&harness), vec![(0, "stopped", false)]);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_turn_the_run_stop_ended_is_run_again_by_the_next_execution() {
    let (script, harness) = stop_noted_before_a_crash(StopSource::RunStop).await;
    harness.gateway.release("hold");
    harness
        .gateway
        .set_rules(vec![rule("slow", "second try", 10, None)]);
    // Resuming a stopped run runs the interrupted agent again; it neither fails nor parks.
    let run = harness.run(&script).await;
    assert_eq!(result(run.outcome.clone()), json!("second try"));
    assert!(
        !run.notices
            .iter()
            .any(|n| matches!(n, JournalNotice::NeedsOperator { .. }))
    );
    assert_eq!(settled_source(&harness).as_deref(), Some("run_stop"));
    assert!(harness.dir.agent_stop(&key_of(&harness)).is_none());
    assert_eq!(
        finishes(&harness),
        vec![(0, "failed", true), (1, "completed", false)]
    );
    assert_eq!(harness.gateway.stats().sessions, 2);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_stop_note_about_another_turn_is_ignored_and_the_call_parks() {
    let (script, harness) = crashed_while_running(held_stop()).await;
    // A restart note left over from an earlier attempt's turn: it does not name this one.
    let stale = StopTurn {
        attempt: 0,
        turn: 0,
        admission_id: Some("an-earlier-admission".into()),
    };
    harness
        .dir
        .note_stop(
            &key_of(&harness),
            StopSource::WorkflowViewRestart,
            Some(&stale),
            1,
        )
        .unwrap();
    harness.gateway.release("hold");
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
    assert_eq!(settled_source(&harness), None);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_new_execution_removes_stop_notes_whose_turn_is_not_running() {
    let (_script, harness) = crashed_while_running(held_stop()).await;
    let key = key_of(&harness);
    // A note naming the running turn is kept.
    harness.dir.note_agent_stop(&key, false, 1).unwrap();
    harness.dir.prune_agent_stops().unwrap();
    assert_eq!(harness.dir.agent_stop(&key), Some(StopSource::WorkflowView));
    // One naming an attempt that is not running is removed.
    let other = StopTurn {
        attempt: 3,
        turn: 0,
        admission_id: None,
    };
    harness
        .dir
        .note_stop(&key, StopSource::WorkflowView, Some(&other), 1)
        .unwrap();
    harness.dir.prune_agent_stops().unwrap();
    assert!(harness.dir.agent_stop(&key).is_none());
    // So is one written before notes named their turn.
    let old = harness
        .dir
        .path()
        .join("control")
        .join(talaria_workflow::journal::agent_stop_name(&key));
    std::fs::write(
        &old,
        format!(r#"{{"v":1,"kind":"agent_restart","key":"{key}","at_ms":1}}"#),
    )
    .unwrap();
    assert!(
        harness
            .dir
            .agent_stop_for(&key, 0, 0, "admission-1")
            .is_none()
    );
    harness.dir.prune_agent_stops().unwrap();
    assert!(!old.exists());
}
