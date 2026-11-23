#![cfg(feature = "fake-gateway")]
//! An agent that ran out of context is started again at most three times, however many times
//! the run is killed and resumed.
use crate::support;
use futures::future::BoxFuture;
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use support::harness::*;
use talaria_workflow::journal::{
    AdmissionState, CallOutcome, CreateSession, GatewayPort, NoGate, PortError, Receipt, Record,
    SessionBound, SubmitTurn, fake_gateway::FakeGateway,
};

/// A gateway on which every turn of every attempt settles with a context overflow.
#[derive(Clone)]
struct Overflowing {
    fake: FakeGateway,
    attempts: Arc<Mutex<BTreeMap<String, u32>>>,
    /// While set, the turn of the second restart never settles.
    hang_second_restart: Arc<AtomicBool>,
}
impl Overflowing {
    fn decorate(&self, mut r: Receipt) -> Receipt {
        let attempt = self.attempts.lock().unwrap().get(&r.session_id).copied();
        if self.hang_second_restart.load(Ordering::SeqCst) && attempt == Some(2) {
            r.status = AdmissionState::Started;
            r.outcome = None;
            r.result = None;
            r.usage = None;
        } else if r.status == AdmissionState::Terminal {
            r.outcome = Some("failed".into());
            r.result = Some(
                json!({"failed":true,"failure_reason":"context_overflow","final_response":""}),
            );
        }
        r
    }
}
impl GatewayPort for Overflowing {
    fn create_session(
        &self,
        r: CreateSession,
    ) -> BoxFuture<'static, Result<SessionBound, PortError>> {
        let this = self.clone();
        Box::pin(async move {
            // Attempt zero has a bare request id; later attempts end in `:a<number>`.
            let attempt = r
                .request_id
                .rsplit(':')
                .next()
                .filter(|tail| tail.len() <= 4)
                .and_then(|tail| tail.strip_prefix('a')?.parse().ok())
                .unwrap_or(0);
            let bound = this.fake.create_session(r).await?;
            this.attempts
                .lock()
                .unwrap()
                .insert(bound.session_id.clone(), attempt);
            Ok(bound)
        })
    }
    fn attach(&self, p: String, s: String) -> BoxFuture<'static, Result<(), PortError>> {
        self.fake.attach(p, s)
    }
    fn submit(&self, r: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>> {
        self.fake.submit(r)
    }
    fn receipt(
        &self,
        p: String,
        s: String,
        a: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let this = self.clone();
        Box::pin(async move { Ok(this.decorate(this.fake.receipt(p, s, a).await?)) })
    }
    fn wait_settled(
        &self,
        p: String,
        s: String,
        a: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        let this = self.clone();
        Box::pin(async move {
            let attempt = this.attempts.lock().unwrap().get(&s).copied();
            if this.hang_second_restart.load(Ordering::SeqCst) && attempt == Some(2) {
                std::future::pending::<()>().await;
            }
            Ok(this.decorate(this.fake.wait_settled(p, s, a).await?))
        })
    }
    fn resolve_unknown(
        &self,
        p: String,
        s: String,
        a: String,
        g: i64,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        self.fake.resolve_unknown(p, s, a, g)
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_restart_count_survives_a_kill_and_a_resume_never_restarts_a_fourth_time() {
    let script = source("return await agent('review',{label:'review'});");
    let harness = Harness::new(&script);
    let hang = Arc::new(AtomicBool::new(true));
    let port = Overflowing {
        fake: harness.gateway.clone(),
        attempts: Default::default(),
        hang_second_restart: hang.clone(),
    };
    // The first execution is killed while the second restart (the third attempt) is running.
    {
        let mut run = Box::pin(harness.run_port(&script, port.clone(), NoGate));
        loop {
            tokio::select! {
                _ = &mut run => panic!("the run ended before the kill"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    if harness.records().iter().any(|l| matches!(l.record, Record::TurnAdmitted { attempt: 2, .. })) {
                        break;
                    }
                }
            }
        }
    }
    // Let the dropped run's background tasks release the journal.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(harness.gateway.stats().sessions, 3);
    // The resume reads the two restarts from the journal: one restart is left, then it stops.
    hang.store(false, Ordering::SeqCst);
    let second = harness.run_port(&script, port.clone(), NoGate).await;
    assert_eq!(result(second.outcome), json!(null));
    assert_eq!(harness.gateway.stats().sessions, 4);
    let retries = |h: &Harness| {
        h.records()
            .iter()
            .filter(|l| matches!(l.record, Record::CallRetryFinished { .. }))
            .count()
    };
    assert_eq!(retries(&harness), 3);
    let reason = harness
        .records()
        .iter()
        .rev()
        .find_map(|l| match &l.record {
            Record::CallFinished {
                outcome: CallOutcome::Failed { reason },
                ..
            } => Some(reason.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        reason.contains("ran out of context 4 times; its 3 restarts are used up"),
        "{reason}"
    );
    // Resuming again runs the failed call once more, as it always does, but the three restarts
    // are spent: it is not started a fourth time.
    let third = harness.run_port(&script, port.clone(), NoGate).await;
    assert_eq!(result(third.outcome), json!(null));
    assert_eq!(harness.gateway.stats().sessions, 5);
    assert_eq!(retries(&harness), 3);
    // The count after a resume is the number of overflows, not "4 times" again.
    let reason = harness
        .records()
        .iter()
        .rev()
        .find_map(|l| match &l.record {
            Record::CallFinished {
                outcome: CallOutcome::Failed { reason },
                ..
            } => Some(reason.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        reason.contains("ran out of context 5 times; its 3 restarts are used up"),
        "{reason}"
    );
    // The attempt a resume ran again was told about the earlier work in its folder.
    let rerun = harness
        .records()
        .iter()
        .find_map(|l| match &l.record {
            Record::TurnIntent {
                attempt: 4,
                turn: 0,
                text,
                ..
            } => text.clone(),
            _ => None,
        })
        .unwrap();
    assert!(
        rerun.starts_with("Note: an earlier attempt at this task ran out of context"),
        "{rerun}"
    );
}
