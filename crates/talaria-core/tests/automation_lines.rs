//! Row texts, countdowns and wake-up times. Behavior contract: issue 236.

use serde_json::{Value, json};
use talaria_core::automation::rows as automation_rows;
use talaria_core::automation::{
    AutomationEvent, AutomationState, FastMode, ModeKey, ModeValue, Tone, countdown,
    format_interval, next_tick, update,
};

const NOW: u64 = 1790000000000;
const SESSION: &str = "s1";

fn goal(status: &str) -> Value {
    json!({
        "title": "Write the release notes",
        "status": status,
        "turns_used": 3,
        "max_turns": 20,
        "contract": {"outcome": "", "verification": "", "constraints": "", "boundaries": "", "stop_when": ""},
        "subgoals": ["draft", "review"],
        "gates": [{"command": "make check", "timeout_seconds": 300, "max_retries": 2, "attempts": 1}],
    })
}

fn loop_() -> Value {
    json!({
        "prompt": "check the build",
        "status": "active",
        "mode": "interval",
        "interval_seconds": 2700,
        "current_delay": 0,
        "times": 0,
        "until": "",
        "max_ticks": 0,
        "ticks_fired": 0,
        "created_at": 1790000000,
        "last_fired_at": 0,
        "next_due_at": 1790002700,
        "awaiting_response": false,
        "deferred_by_goal": false
    })
}

fn heartbeat() -> Value {
    json!({
        "prompt": "ping the team",
        "status": "active",
        "interval_seconds": 3600,
        "created_at": 1789997120,
        "last_fired_at": 0,
        "fire_count": 4
    })
}

fn snapshot(goal: Value, loop_: Value, heartbeat: Value) -> Value {
    json!({
        "goal": goal, "loop": loop_, "heartbeat": heartbeat,
        "revision": "r1", "updated_at": 1790000000
    })
}

fn null_snapshot() -> Value {
    json!({"goal": null, "loop": null, "heartbeat": null, "revision": "", "updated_at": 0})
}

/// A state holding `raw` as its control view.
fn state_with_view(raw: Value) -> AutomationState {
    let mut state = AutomationState::default();
    update(
        &mut state,
        AutomationEvent::Attached {
            session_id: SESSION.to_owned(),
        },
    );
    update(
        &mut state,
        AutomationEvent::ControlUpdated {
            session_id: SESSION.to_owned(),
            raw,
        },
    );
    state
}

/// Feed one mode read through the attach's pending token.
fn read_mode(state: &mut AutomationState, key: ModeKey, token: u64, value: Option<&str>) {
    update(
        state,
        AutomationEvent::ModeRead {
            session_id: SESSION.to_owned(),
            key,
            token,
            result: Ok(value.map(str::to_owned)),
        },
    );
}

/// A state with the given mode values, reached through attach reads.
fn state_with_modes(yolo: Option<&str>, fast: Option<&str>) -> AutomationState {
    let mut state = AutomationState::default();
    let effects = update(
        &mut state,
        AutomationEvent::Attached {
            session_id: SESSION.to_owned(),
        },
    );
    let mut tokens = [0u64; 2];
    for effect in &effects {
        if let talaria_core::automation::AutomationEffect::ReadMode { key, token, .. } = effect {
            match key {
                ModeKey::Yolo => tokens[0] = *token,
                ModeKey::Fast => tokens[1] = *token,
            }
        }
    }
    update(
        &mut state,
        AutomationEvent::ControlUpdated {
            session_id: SESSION.to_owned(),
            raw: null_snapshot(),
        },
    );
    if let Some(value) = yolo {
        read_mode(&mut state, ModeKey::Yolo, tokens[0], Some(value));
    }
    if let Some(value) = fast {
        read_mode(&mut state, ModeKey::Fast, tokens[1], Some(value));
    }
    state
}

#[test]
fn countdown_table() {
    for (remaining, text) in [
        (0, "0s"),
        (1, "1s"),
        (59_000, "59s"),
        (59_001, "1m"),
        (60_000, "1m"),
        (60_001, "2m"),
        (2_700_000, "45m"),
        (3_600_000, "1h00m"),
        (3_600_001, "1h01m"),
        (3_900_000, "1h05m"),
        (-5, "0s"),
    ] {
        assert_eq!(countdown(remaining), text, "{remaining} ms");
    }
}

#[test]
fn format_interval_matches_hermes() {
    for (seconds, text) in [
        (0.0, "0s"),
        (30.0, "30s"),
        (90.0, "1m30s"),
        (2700.0, "45m"),
        (3600.0, "1h"),
        (5400.0, "1h30m"),
        (3661.0, "1h1m1s"),
        (29.6, "30s"),
    ] {
        assert_eq!(format_interval(seconds), text, "{seconds} s");
    }
}

#[test]
fn goal_rows() {
    let state = state_with_view(snapshot(goal("active"), Value::Null, Value::Null));
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tone, Tone::Accent);
    assert_eq!(rows[0].label, "⊙ goal");
    assert_eq!(rows[0].detail, "3/20 turns · 2 subgoals · 1 gate");
    assert_eq!(rows[0].tail, "Write the release notes");

    let mut singular = goal("active");
    singular["subgoals"] = json!(["draft"]);
    singular["gates"] = json!([]);
    let state = state_with_view(snapshot(singular, Value::Null, Value::Null));
    assert_eq!(
        automation_rows(&state, NOW)[0].detail,
        "3/20 turns · 1 subgoal"
    );

    let mut paused = goal("paused");
    paused["paused_reason"] = json!("turn budget exhausted (20/20)");
    let state = state_with_view(snapshot(paused, Value::Null, Value::Null));
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows[0].tone, Tone::Warning);
    assert_eq!(rows[0].label, "⏸ goal paused");
    assert_eq!(
        rows[0].detail,
        "turn budget exhausted (20/20) · 3/20 turns · 2 subgoals · 1 gate"
    );

    let mut parked = goal("active");
    parked["wait_barrier"] = json!({"type": "until", "until_at": 1790000720, "reason": "coffee"});
    let state = state_with_view(snapshot(parked, Value::Null, Value::Null));
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows[0].tone, Tone::Warning);
    assert_eq!(rows[0].label, "⏳ goal parked");
    assert_eq!(
        rows[0].detail,
        "12m left · coffee · 3/20 turns · 2 subgoals · 1 gate"
    );

    let mut past = goal("active");
    past["wait_barrier"] = json!({"type": "until", "until_at": 1789999000});
    let state = state_with_view(snapshot(past, Value::Null, Value::Null));
    assert_eq!(automation_rows(&state, NOW)[0].label, "⏳ goal parked");
    assert!(
        automation_rows(&state, NOW)[0]
            .detail
            .starts_with("due now"),
        "a past barrier reads due now"
    );

    let mut pid = goal("active");
    pid["wait_barrier"] = json!({"type": "pid", "target": 4242});
    let state = state_with_view(snapshot(pid, Value::Null, Value::Null));
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows[0].label, "⏳ goal parked");
    assert!(
        rows[0].detail.starts_with("on pid 4242"),
        "a pid barrier names its target"
    );

    let state = state_with_view(snapshot(goal("done"), Value::Null, Value::Null));
    assert!(
        automation_rows(&state, NOW).is_empty(),
        "a done goal shows no row"
    );
}

#[test]
fn loop_rows() {
    let state = state_with_view(snapshot(Value::Null, loop_(), Value::Null));
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tone, Tone::Accent);
    assert_eq!(rows[0].label, "↻ loop");
    assert_eq!(rows[0].detail, "every 45m · 0 runs · next in 45m");
    assert_eq!(rows[0].tail, "check the build");

    let mut created = loop_();
    created["next_due_at"] = json!(1790000000);
    let state = state_with_view(snapshot(Value::Null, created, Value::Null));
    assert_eq!(
        automation_rows(&state, NOW)[0].detail,
        "every 45m · 0 runs · due, waiting for idle"
    );

    let mut capped = loop_();
    capped["times"] = json!(5);
    capped["ticks_fired"] = json!(2);
    let state = state_with_view(snapshot(Value::Null, capped, Value::Null));
    assert!(
        automation_rows(&state, NOW)[0].detail.contains("2/5 runs"),
        "a capped loop counts"
    );

    let mut one = loop_();
    one["ticks_fired"] = json!(1);
    let state = state_with_view(snapshot(Value::Null, one, Value::Null));
    assert!(
        automation_rows(&state, NOW)[0].detail.contains("1 run ·"),
        "one run reads singular"
    );

    let mut running = loop_();
    running["awaiting_response"] = json!(true);
    running["deferred_by_goal"] = json!(true);
    let state = state_with_view(snapshot(Value::Null, running, Value::Null));
    assert!(
        automation_rows(&state, NOW)[0]
            .detail
            .ends_with("running now"),
        "running now wins over deferred"
    );

    let mut deferred = loop_();
    deferred["deferred_by_goal"] = json!(true);
    let state = state_with_view(snapshot(Value::Null, deferred, Value::Null));
    assert!(
        automation_rows(&state, NOW)[0]
            .detail
            .ends_with("deferred to the goal"),
        "a held loop says so"
    );

    let mut paused = loop_();
    paused["status"] = json!("paused");
    paused["paused_reason"] = json!("user-paused");
    paused["ticks_fired"] = json!(2);
    let state = state_with_view(snapshot(Value::Null, paused, Value::Null));
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows[0].tone, Tone::Warning);
    assert_eq!(rows[0].label, "⏸ loop paused");
    assert_eq!(rows[0].detail, "user-paused · 2 runs");

    let mut paced = loop_();
    paced["mode"] = json!("self_paced");
    let state = state_with_view(snapshot(Value::Null, paced, Value::Null));
    assert!(
        automation_rows(&state, NOW)[0]
            .detail
            .starts_with("self-paced ·"),
        "a self-paced loop names its cadence"
    );
}

#[test]
fn heartbeat_and_modes_rows() {
    let state = state_with_view(snapshot(Value::Null, Value::Null, heartbeat()));
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tone, Tone::Accent);
    assert_eq!(rows[0].label, "♥ heartbeat");
    assert_eq!(rows[0].detail, "every 1h · 4 sent · next in 12m");
    assert_eq!(rows[0].tail, "ping the team");

    let mut paused = heartbeat();
    paused["status"] = json!("paused");
    let state = state_with_view(snapshot(Value::Null, Value::Null, paused));
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows[0].label, "⏸ heartbeat paused");
    assert_eq!(rows[0].detail, "4 sent");

    let state = state_with_modes(Some("1"), None);
    assert_eq!(state.modes(), (ModeValue::Known(true), ModeValue::NotRead));
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tone, Tone::Warning);
    assert_eq!(rows[0].label, "⚠ YOLO");
    assert_eq!(
        rows[0].detail,
        "commands run without approval in this session"
    );
    assert!(rows[0].tail.is_empty());

    let state = state_with_modes(Some("0"), Some("fast"));
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tone, Tone::Accent);
    assert_eq!(rows[0].label, "⚡ fast mode");
    assert!(rows[0].detail.is_empty());
    assert!(rows[0].tail.is_empty());

    let state = state_with_modes(Some("1"), Some("fast"));
    assert_eq!(
        state.modes(),
        (ModeValue::Known(true), ModeValue::Known(FastMode::Fast))
    );
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tone, Tone::Warning);
    assert_eq!(rows[0].label, "⚠ YOLO");
    assert_eq!(rows[0].tail, "⚡ fast mode");

    let state = state_with_modes(Some("1"), Some("ultrafast"));
    assert_eq!(automation_rows(&state, NOW)[0].tail, "⚡ ultrafast mode");

    let mut state = state_with_modes(None, None);
    let effects = update(
        &mut state,
        AutomationEvent::Attached {
            session_id: SESSION.to_owned(),
        },
    );
    let token = effects
        .iter()
        .find_map(|effect| match effect {
            talaria_core::automation::AutomationEffect::ReadMode { key, token, .. }
                if *key == ModeKey::Yolo =>
            {
                Some(*token)
            }
            _ => None,
        })
        .expect("a yolo read");
    update(
        &mut state,
        AutomationEvent::ModeRead {
            session_id: SESSION.to_owned(),
            key: ModeKey::Yolo,
            token,
            result: Err(talaria_core::automation::ReadFailure::TimedOut),
        },
    );
    let rows = automation_rows(&state, NOW);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "⚠ YOLO");
    assert_eq!(rows[0].detail, "state unknown: Hermes did not answer");

    let state = state_with_modes(Some("0"), None);
    assert!(
        automation_rows(&state, NOW).is_empty(),
        "fast unknown shows no row"
    );

    let mut untitled = goal("active");
    untitled["title"] = json!("");
    let state = state_with_view(snapshot(untitled, Value::Null, Value::Null));
    let rows = automation_rows(&state, NOW);
    assert!(!rows[0].detail.is_empty());
    assert!(rows[0].tail.is_empty());
}

#[test]
fn next_tick_only_for_visible_countdowns() {
    let state = state_with_view(null_snapshot());
    assert_eq!(next_tick(&state, NOW), None);

    let state = state_with_view(snapshot(Value::Null, loop_(), Value::Null));
    assert_eq!(next_tick(&state, NOW), Some(NOW + 60_000));

    let mut soon = loop_();
    soon["next_due_at"] = json!(1790000061.5);
    let state = state_with_view(snapshot(Value::Null, soon, Value::Null));
    assert_eq!(next_tick(&state, NOW), Some(NOW + 1_500));

    let mut running = loop_();
    running["awaiting_response"] = json!(true);
    let state = state_with_view(snapshot(Value::Null, running, Value::Null));
    assert_eq!(next_tick(&state, NOW), None);

    let mut sooner = heartbeat();
    sooner["created_at"] = json!(1789997150);
    let state = state_with_view(snapshot(Value::Null, loop_(), sooner));
    assert_eq!(
        next_tick(&state, NOW),
        Some(NOW + 30_000),
        "the earlier countdown wins"
    );
}
