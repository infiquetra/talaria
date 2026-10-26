//! Foreground needs reach the queue through the owner's view.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::time::{Duration, Instant};

use mock_gateway::StandaloneGateway;
use talaria_app::fleet::{FleetConfig, FleetService};
use talaria_core::attention::NeedSummary;
use talaria_core::fleet::{NeedSlot, RowKey, SubscribedNeed, SubscribedTurn, SubscribedView};

fn view(session_id: &str) -> (RowKey, SubscribedView) {
    let key = RowKey {
        profile: "fixture".into(),
        session_id: session_id.into(),
    };
    let view = SubscribedView {
        turn: SubscribedTurn::Running {
            started_at: Some(1_792_690_000.0),
        },
        needs: vec![SubscribedNeed {
            slot: NeedSlot::Prompt("a1b2".into()),
            summary: NeedSummary::Approval {
                command: "rm -rf ./talaria-fleet-probe".into(),
                description: "recursive delete".into(),
            },
            choices: vec![
                "once".into(),
                "deny".into(),
                "session".into(),
                "always".into(),
            ],
            execution_generation: Some(3),
            timeout_s: None,
        }],
        activity: None,
        cost_usd: None,
        model: Some("minimax-m3.1-flash".into()),
        title: Some(String::new()),
    };
    (key, view)
}

#[tokio::test]
async fn foreground_prompt_reaches_queue() {
    let home = tempfile::tempdir().expect("a temporary home");
    let _gateway = StandaloneGateway::start(home.path());
    let handle = FleetService::start(
        home.path().to_path_buf(),
        "fixture".into(),
        FleetConfig::scaled_for_tests(),
        |_| {},
    );
    let (key, open) = view("local-session-1");
    let started = Instant::now();
    handle.observe_subscribed(key.clone(), open);
    let budget = Duration::from_millis(50);
    while started.elapsed() < budget {
        let snapshot = handle.snapshot();
        if snapshot.queue.iter().any(|item| item.foreground) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let snapshot = handle.snapshot();
    assert!(
        snapshot
            .queue
            .iter()
            .any(|item| { item.foreground && item.id.key.session_id == "local-session-1" })
            && started.elapsed() <= budget,
        "the foreground need is flagged within 50 ms"
    );
    assert!(
        snapshot
            .rows
            .iter()
            .any(|row| row.key == key && row.foreground),
        "the row is flagged foreground"
    );
    let (other, _) = view("local-other");
    handle.set_foreground(Some(other.clone()));
    handle.observe_unsubscribed(key);
    let moved = handle.snapshot();
    assert!(
        moved
            .rows
            .iter()
            .any(|row| row.key == other && row.foreground),
        "a switch moves the foreground flag"
    );
    assert!(
        moved
            .rows
            .iter()
            .all(|row| row.key.session_id == "local-other" || !row.foreground),
        "the previous row is no longer foreground"
    );
}
