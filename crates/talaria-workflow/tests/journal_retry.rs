#![cfg(feature = "fake-gateway")]
mod support;
use futures_util::future::BoxFuture;
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use support::harness::*;
use talaria_workflow::{
    engine::RunOutcome,
    journal::{
        CreateSession, GatewayPort, NoGate, PortError, Receipt, SessionBound, SubmitTurn,
        fake_gateway::FakeGateway,
    },
};

type OutageState = (u8, tokio::time::Instant, Vec<(String, bool)>);
#[derive(Clone)]
struct Outages {
    inner: FakeGateway,
    state: Arc<Mutex<OutageState>>,
}
impl Outages {
    fn new(inner: FakeGateway) -> Self {
        Self {
            inner,
            state: Arc::new(Mutex::new((0, tokio::time::Instant::now(), Vec::new()))),
        }
    }
    fn wrap<T: Send + 'static>(
        &self,
        name: &'static str,
        inner: BoxFuture<'static, Result<T, PortError>>,
    ) -> BoxFuture<'static, Result<T, PortError>> {
        let state = self.state.clone();
        Box::pin(async move {
            let refuse = {
                let mut state = state.lock().unwrap();
                if state.0 < 2 && state.1.elapsed() < Duration::from_secs(60) {
                    state.2.push((name.into(), false));
                    true
                } else {
                    if state.0 < 2 {
                        state.0 += 1;
                        state.1 = tokio::time::Instant::now();
                    }
                    false
                }
            };
            if refuse {
                return Err(PortError::Transport("offline".into()));
            }
            let result = inner.await;
            state.lock().unwrap().2.push((name.into(), result.is_ok()));
            result
        })
    }
}
impl GatewayPort for Outages {
    fn create_session(
        &self,
        r: CreateSession,
    ) -> BoxFuture<'static, Result<SessionBound, PortError>> {
        self.wrap("create", self.inner.create_session(r))
    }
    fn attach(&self, p: String, s: String) -> BoxFuture<'static, Result<(), PortError>> {
        self.wrap("attach", self.inner.attach(p, s))
    }
    fn submit(&self, r: SubmitTurn) -> BoxFuture<'static, Result<Receipt, PortError>> {
        self.wrap("submit", self.inner.submit(r))
    }
    fn receipt(
        &self,
        p: String,
        s: String,
        a: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        self.wrap("receipt", self.inner.receipt(p, s, a))
    }
    fn wait_settled(
        &self,
        p: String,
        s: String,
        a: String,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        self.wrap("wait", self.inner.wait_settled(p, s, a))
    }
    fn resolve_unknown(
        &self,
        p: String,
        s: String,
        a: String,
        g: i64,
    ) -> BoxFuture<'static, Result<Receipt, PortError>> {
        self.wrap("resolve", self.inner.resolve_unknown(p, s, a, g))
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn outage_budget_is_not_turn_time() {
    let script = source("return await agent('slow',{label:'slow'});");
    let mut harness = Harness::new(&script);
    harness.gateway = FakeGateway::new().with_script(vec![rule("slow", "ok", 600_000, None)]);
    let start = tokio::time::Instant::now();
    assert_eq!(result(harness.run(&script).await.outcome), json!("ok"));
    assert!(start.elapsed() >= Duration::from_secs(600));
    let harness = Harness::new(&script);
    let outages = Outages::new(harness.gateway.clone());
    let trace = outages.state.clone();
    let start = tokio::time::Instant::now();
    assert_eq!(
        result(harness.run_port(&script, outages, NoGate).await.outcome),
        json!("ok")
    );
    assert!(start.elapsed() >= Duration::from_secs(120));
    let trace = trace.lock().unwrap().2.clone();
    if let Some(failed) = trace.iter().position(|(name, ok)| name == "submit" && !ok) {
        let retry = trace
            .iter()
            .enumerate()
            .skip(failed + 1)
            .find(|(_, entry)| entry.0 == "submit")
            .unwrap()
            .0;
        assert!(
            trace[failed + 1..retry]
                .iter()
                .any(|(name, ok)| name == "attach" && *ok)
        );
    } else {
        panic!("outage did not interrupt submit");
    }
    let harness = Harness::new(&script);
    harness.gateway.fail_next(1000, PortError::Timeout);
    let start = tokio::time::Instant::now();
    match harness.run(&script).await.outcome {
        RunOutcome::Aborted { reason } => assert!(reason.contains("gateway unavailable")),
        other => panic!("expected outage abort: {other:?}"),
    };
    assert!(start.elapsed() >= Duration::from_secs(120));
}
