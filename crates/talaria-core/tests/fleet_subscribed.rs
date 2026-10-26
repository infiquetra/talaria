//! Attached-session views.

#[path = "support/events.rs"]
mod events;
#[path = "support/snapshot_json.rs"]
mod snapshot_json;

use talaria_core::fleet::{NeedSlot, Outcome, SubscribedTurn, SubscribedView};
use talaria_core::session::{self, SessionState};

use events::{approval_request, ev};
use snapshot_json::{base, snapshot};

#[test]
fn view_from_session_state() {
    let mut state = SessionState::from_snapshot(snapshot(base("local-session-1")));
    let _ = session::update(
        &mut state,
        ev(
            1,
            "approval.request",
            approval_request("a1b2", 3, &["once", "deny", "session", "always"]),
        ),
    );
    let open = SubscribedView::from_state(&state, &[]);
    assert_eq!(open.needs.len(), 1);
    assert!(matches!(open.needs[0].slot, NeedSlot::Prompt(ref id) if id == "a1b2"));
    assert_eq!(open.needs[0].execution_generation, Some(3));
    let _ = session::update(
        &mut state,
        ev(
            2,
            "message.complete",
            serde_json::json!({"status": "error"}),
        ),
    );
    let finished = SubscribedView::from_state(&state, &[]);
    assert_eq!(finished.turn, SubscribedTurn::Finished(Outcome::Failed));
}
