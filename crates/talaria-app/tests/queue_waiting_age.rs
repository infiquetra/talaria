//! Waiting age through the application clock, reconnect refresh and queue strip.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use hermes_client::Inbound;
use hermes_client::channel::{Generation, InboundEvent};
use hermes_client::liveness::LinkEvent;
use hermes_contract::events::EventFrame;
use hermes_contract::schemas::CanonicalSnapshot;
use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};
use serde_json::{Value, json};
use talaria_app::app::{AppEffect, AppMessage, AppState, update};
use talaria_app::sessions::OpenedSession;
use talaria_tui::queue::render_preview;

const FIRST_SEEN: u64 = 10_000;
const MINUTE: u64 = 60_000;

fn pending(id: &str, text: &str, status: &str, steering: bool) -> Value {
    json!({
        "admission_id": id,
        "authority_epoch": 1,
        "execution_generation": 1,
        "input_id": format!("input-{id}"),
        "outcome": null,
        "ref": {"profile_id": "fixture", "session_id": "local-session-1"},
        "sequence": 1,
        "status": status,
        "text": text,
        "controls": ["steer", "replace"],
        "steering": steering
    })
}

fn opened(rows: Vec<Value>) -> OpenedSession {
    let mut value = mock_gateway::recorded_resume_snapshot();
    value["pending"] = json!(rows);
    value["prompts"] = json!([]);
    value["running"] = json!(true);
    value["authority_epoch"] = json!(1);
    value["execution_generation"] = json!(1);
    let snapshot: CanonicalSnapshot = serde_json::from_value(value).unwrap();
    OpenedSession::from_snapshot(snapshot)
}

fn app(rows: Vec<Value>) -> AppState {
    let mut app = AppState::new("0123456789abcdef");
    app.now_ms = FIRST_SEEN;
    update(
        &mut app,
        AppMessage::Opened {
            opened: opened(rows),
            notice: None,
        },
    );
    app
}

fn fanout(app: &mut AppState, rows: Vec<Value>) {
    let opened = app.opened.as_ref().unwrap();
    let raw = json!({
        "type": "session.info",
        "session_id": opened.session_id,
        "replay_epoch": opened.cursor.replay_epoch,
        "seq": opened.cursor.last_sequence + 1,
        "payload": {"pending": rows, "running": true, "authority_epoch": 1, "execution_generation": 1}
    });
    let frame = EventFrame::from_params(&raw).unwrap();
    update(
        app,
        AppMessage::Link(LinkEvent::Inbound {
            item: Inbound::Event(InboundEvent {
                generation: Generation(1),
                frame,
                raw,
            }),
            replayed: false,
        }),
    );
}

fn clock(app: &mut AppState, now_ms: u64) -> Vec<AppEffect> {
    update(app, AppMessage::Tick { now_ms })
}

fn strip(app: &AppState, width: u16, height: u16) -> Vec<String> {
    let session = app.view();
    let queue = session.state.unwrap().queue.view(session.turn_running());
    let lines = render_preview(&queue, width, false);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| frame.render_widget(Paragraph::new(lines), frame.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

#[test]
fn live_waiting_age_uses_elapsed_minutes_at_both_screen_sizes() {
    for (width, height) in [(80, 24), (120, 40)] {
        for (minutes, suffix) in [(27, "waiting 27m"), (62, "waiting 1h02m")] {
            let mut app = app(Vec::new());
            fanout(
                &mut app,
                vec![pending("waiting", "queued work", "queued", false)],
            );
            clock(&mut app, FIRST_SEEN + minutes * MINUTE);
            let lines = strip(&app, width, height);
            assert!(lines[0].contains(suffix), "{width}x{height}: {lines:?}");
            assert!(
                !lines[0].contains(&format!("{suffix}+")),
                "live arrival has a measured age"
            );
        }
    }
}

#[test]
fn snapshot_waiting_age_is_a_minimum_at_both_screen_sizes() {
    for (width, height) in [(80, 24), (120, 40)] {
        let mut app = app(vec![pending("waiting", "queued work", "queued", false)]);
        clock(&mut app, FIRST_SEEN + 27 * MINUTE);
        let lines = strip(&app, width, height);
        assert!(
            lines[0].contains("waiting 27m+"),
            "{width}x{height}: {lines:?}"
        );
    }
}

#[test]
fn waiting_age_is_hidden_until_one_whole_minute() {
    let mut app = app(Vec::new());
    fanout(
        &mut app,
        vec![pending("waiting", "queued work", "queued", false)],
    );
    clock(&mut app, FIRST_SEEN + MINUTE - 1);
    assert!(!strip(&app, 80, 24)[0].contains("waiting "));
    clock(&mut app, FIRST_SEEN + MINUTE);
    assert!(strip(&app, 80, 24)[0].contains("waiting 1m"));
}

#[test]
fn waiting_age_stays_whole_when_long_unicode_text_is_cut() {
    for (width, height) in [(80, 24), (120, 40)] {
        let mut app = app(Vec::new());
        fanout(
            &mut app,
            vec![pending("waiting", &"界".repeat(150), "queued", false)],
        );
        clock(&mut app, FIRST_SEEN + 62 * MINUTE);
        let lines = strip(&app, width, height);
        assert!(lines[0].contains('…'), "text gives way: {lines:?}");
        assert!(
            lines[0].contains("waiting 1h02m"),
            "age stays whole: {lines:?}"
        );
        assert!(
            lines[1..].iter().all(String::is_empty),
            "no wrapping: {lines:?}"
        );
    }
}

#[test]
fn steering_started_and_lost_rows_never_show_waiting_age() {
    let mut app = app(Vec::new());
    fanout(
        &mut app,
        vec![
            pending("steer", "steering work", "started", true),
            pending("started", "started work", "started", false),
            pending("lost", "lost work", "unknown", false),
        ],
    );
    clock(&mut app, FIRST_SEEN + 27 * MINUTE);
    let lines = strip(&app, 80, 24);
    assert!(lines.iter().any(|line| line.contains("steering into")));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("lost in a gateway restart"))
    );
    assert!(lines.iter().all(|line| !line.contains("waiting ")));
    assert!(lines.iter().all(|line| !line.contains("started work")));
}

#[test]
fn reconnect_keeps_known_first_seen_time_and_marks_new_rows_as_minimum() {
    let mut app = app(Vec::new());
    let known = pending("known", "known work", "queued", false);
    fanout(&mut app, vec![known.clone()]);
    clock(&mut app, FIRST_SEEN + 20 * MINUTE);
    let fresh = opened(vec![known, pending("new", "new work", "queued", false)]);
    update(&mut app, AppMessage::Recovered(Ok(fresh)));
    clock(&mut app, FIRST_SEEN + 27 * MINUTE);
    let lines = strip(&app, 120, 40);
    let known = lines
        .iter()
        .find(|line| line.contains("known work"))
        .unwrap();
    let new = lines.iter().find(|line| line.contains("new work")).unwrap();
    assert!(
        known.contains("waiting 27m"),
        "known count survives: {known}"
    );
    assert!(
        !known.contains("waiting 27m+"),
        "known origin survives: {known}"
    );
    assert!(
        new.contains("waiting 7m+"),
        "new snapshot row is a minimum: {new}"
    );
}

#[test]
fn a_row_that_leaves_the_queue_loses_its_first_seen_time() {
    let mut app = app(Vec::new());
    let row = pending("same-id", "queued work", "queued", false);
    fanout(&mut app, vec![row.clone()]);
    clock(&mut app, FIRST_SEEN + 27 * MINUTE);
    fanout(&mut app, Vec::new());
    clock(&mut app, FIRST_SEEN + 30 * MINUTE);
    fanout(&mut app, vec![row]);
    clock(&mut app, FIRST_SEEN + 31 * MINUTE);
    let lines = strip(&app, 80, 24);
    assert!(
        lines[0].contains("waiting 1m"),
        "returning row starts a fresh count: {lines:?}"
    );
    assert!(!lines[0].contains("waiting 31m"));
}

#[test]
fn waiting_age_minute_boundary_redraws_without_an_inbound_event() {
    let mut app = app(Vec::new());
    fanout(
        &mut app,
        vec![pending("waiting", "queued work", "queued", false)],
    );
    clock(&mut app, FIRST_SEEN + MINUTE - 1);
    let effects = clock(&mut app, FIRST_SEEN + MINUTE);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Redraw)),
        "minute boundary redraws waiting age"
    );
    assert!(strip(&app, 80, 24)[0].contains("waiting 1m"));
}
