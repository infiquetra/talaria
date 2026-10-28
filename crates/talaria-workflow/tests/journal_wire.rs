#![cfg(feature = "fake-gateway")]
mod support;
use support::harness::*;
use talaria_workflow::journal::NoGate;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn no_unattended_anywhere() {
    let harness = Harness::new(SCRIPT);
    let port = TracePort::new(harness.gateway.clone());
    let trace = port.requests.clone();
    harness.run_port(SCRIPT, port, NoGate).await;
    let requests = trace.lock().unwrap();
    assert!(!requests.is_empty());
    for request in requests.iter() {
        assert!(
            !serde_json::to_string(request)
                .unwrap()
                .contains("\"unattended\"")
        );
    }
    for record in harness.records() {
        assert!(
            !serde_json::to_string(&record)
                .unwrap()
                .contains("\"unattended\"")
        );
    }
}
