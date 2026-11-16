#[path = "support/snapshot_json.rs"]
mod snapshot_json;

use serde_json::json;
use talaria_core::fleet::{NeedSlot, NeedSummary, SubscribedView};
use talaria_core::session::SessionState;

#[test]
fn read_only_recovered_card_is_not_a_need() {
    let mut value = snapshot_json::base("sess-1");
    value["open_requests"] = json!([
        {"id": "req-1", "method": "sudo", "params": {}},
        {"id": "req-2", "method": "vault.save_login", "params": {}},
    ]);
    let state = SessionState::from_snapshot(snapshot_json::snapshot(value));
    let view = SubscribedView::from_state(&state, &[]);
    assert!(
        view.needs
            .iter()
            .any(|need| need.slot == NeedSlot::Prompt("req-1".to_owned())
                && need.summary == NeedSummary::Sudo),
        "expected req-1 sudo need in {:?}",
        view.needs
    );
    assert!(
        !view
            .needs
            .iter()
            .any(|need| need.slot == NeedSlot::Prompt("req-2".to_owned())),
        "read-only req-2 must not appear in {:?}",
        view.needs
    );
}
