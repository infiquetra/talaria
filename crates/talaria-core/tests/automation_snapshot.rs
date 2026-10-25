//! Decoding Hermes's standing-state snapshots. Behavior contract: issue 236.

use serde_json::json;
use talaria_core::automation::{AutomationEvent, AutomationState, ControlView, WaitView, update};

fn full_snapshot() -> serde_json::Value {
    json!({
        "goal": {
            "title": "Write the release notes",
            "status": "active",
            "turns_used": 3,
            "max_turns": 20,
            "contract": {
                "outcome": "notes shipped",
                "verification": "",
                "constraints": "",
                "boundaries": "",
                "stop_when": ""
            },
            "subgoals": ["draft", "review"],
            "gates": [
                {"command": "make check", "timeout_seconds": 300, "max_retries": 2, "attempts": 1}
            ],
            "created_at": 1789999999.5,
            "wait_barrier": {"type": "until", "until_at": 1790000000, "reason": "coffee"}
        },
        "loop": {
            "prompt": "check the build",
            "status": "active",
            "mode": "self_paced",
            "interval_seconds": 2700,
            "current_delay": 0,
            "times": 0,
            "until": "",
            "max_ticks": 0,
            "ticks_fired": 2,
            "created_at": 1789990000,
            "last_fired_at": 1789992700,
            "next_due_at": 1789995400,
            "awaiting_response": false,
            "deferred_by_goal": false
        },
        "heartbeat": {
            "prompt": "ping the team",
            "status": "paused",
            "interval_seconds": 3600,
            "created_at": 1789990000,
            "last_fired_at": 0,
            "fire_count": 4
        },
        "revision": "abc123",
        "updated_at": -1
    })
}

#[test]
fn decodes_contract_shapes() {
    let view = ControlView::decode(&full_snapshot()).expect("the full snapshot decodes");

    let goal = view.goal.expect("a goal");
    assert_eq!(goal.title, "Write the release notes");
    assert_eq!(goal.status, "active");
    assert_eq!(goal.turns_used, 3);
    assert_eq!(goal.max_turns, 20);
    assert_eq!(goal.subgoals, 2);
    assert_eq!(goal.gates, 1);
    assert_eq!(goal.paused_reason, None);
    assert_eq!(
        goal.wait,
        Some(WaitView::Until {
            until_ms: 1790000000000,
            reason: "coffee".to_owned(),
        })
    );

    let loop_ = view.loop_.expect("a loop");
    assert_eq!(loop_.prompt, "check the build");
    assert_eq!(loop_.status, "active");
    assert!(loop_.self_paced);
    assert_eq!(loop_.interval_seconds, 2700.0);
    assert_eq!(loop_.times, 0);
    assert_eq!(loop_.ticks_fired, 2);
    assert_eq!(loop_.next_due_ms, 1789995400000);
    assert_eq!(loop_.created_ms, 1789990000000);
    assert!(!loop_.awaiting_response);
    assert!(!loop_.deferred_by_goal);
    assert_eq!(loop_.paused_reason, None);

    let heartbeat = view.heartbeat.expect("a heartbeat");
    assert_eq!(heartbeat.prompt, "ping the team");
    assert_eq!(heartbeat.status, "paused");
    assert_eq!(heartbeat.interval_seconds, 3600);
    assert_eq!(heartbeat.fire_count, 4);
    assert_eq!(
        heartbeat.next_due_ms, 1789993600000,
        "never fired: due from created_at"
    );

    assert_eq!(view.revision, "abc123");

    // All null with an empty revision: no parts.
    let view = ControlView::decode(&json!({
        "goal": null, "loop": null, "heartbeat": null, "revision": "", "updated_at": 0
    }))
    .expect("all null decodes");
    assert_eq!(view.goal, None);
    assert_eq!(view.loop_, None);
    assert_eq!(view.heartbeat, None);
    assert_eq!(view.revision, "");

    // An unknown extra key decodes and is ignored.
    let mut extra = full_snapshot();
    extra["loop"]["future_field"] = json!(1);
    ControlView::decode(&extra).expect("an extra key decodes");

    // Barriers on a session or a process; an integer target prints decimal.
    let mut session = full_snapshot();
    session["goal"]["wait_barrier"] = json!({"type": "session", "target": "s1", "reason": "r"});
    let view = ControlView::decode(&session).expect("a session barrier decodes");
    assert_eq!(
        view.goal.expect("a goal").wait,
        Some(WaitView::Session {
            target: "s1".to_owned(),
            reason: "r".to_owned(),
        })
    );
    let mut pid = full_snapshot();
    pid["goal"]["wait_barrier"] = json!({"type": "pid", "target": 4242});
    let view = ControlView::decode(&pid).expect("a pid barrier decodes");
    assert_eq!(
        view.goal.expect("a goal").wait,
        Some(WaitView::Pid {
            target: "4242".to_owned(),
            reason: String::new(),
        })
    );
}

#[test]
fn rejects_bad_shapes_without_partial_apply() {
    let mut missing_revision = full_snapshot();
    missing_revision
        .as_object_mut()
        .expect("an object")
        .remove("revision");
    let mut status_number = full_snapshot();
    status_number["goal"]["status"] = json!(7);
    let mut negative_due = full_snapshot();
    negative_due["loop"]["next_due_at"] = json!(-5);
    let mut missing_ticks = full_snapshot();
    missing_ticks["loop"]
        .as_object_mut()
        .expect("an object")
        .remove("ticks_fired");
    let bad_shapes = [
        ("missing revision", missing_revision),
        ("goal status number", status_number),
        ("negative next_due_at", negative_due),
        ("missing ticks_fired", missing_ticks),
    ];
    for (name, bad) in &bad_shapes {
        assert!(ControlView::decode(bad).is_err(), "{name} is rejected");
    }

    // Each bad shape as an update: the held view survives, nothing redraws.
    let mut state = AutomationState::default();
    update(
        &mut state,
        AutomationEvent::Attached {
            session_id: "s1".to_owned(),
        },
    );
    let applied = update(
        &mut state,
        AutomationEvent::ControlUpdated {
            session_id: "s1".to_owned(),
            raw: full_snapshot(),
        },
    );
    assert!(
        applied.contains(&talaria_core::automation::AutomationEffect::Redraw),
        "the held view applies"
    );
    for (name, bad) in &bad_shapes {
        let effects = update(
            &mut state,
            AutomationEvent::ControlUpdated {
                session_id: "s1".to_owned(),
                raw: bad.clone(),
            },
        );
        assert!(
            effects.is_empty(),
            "{name}: nothing applies, nothing redraws"
        );
    }
}
