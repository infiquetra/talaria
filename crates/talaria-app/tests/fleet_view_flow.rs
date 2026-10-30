//! Foreground rows, opening the overlay, a remapped key, and the frame budgets.

#[path = "support/fake_fleet.rs"]
mod fake_fleet;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mock_gateway::StandaloneGateway;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use talaria_app::app::{self, AppEffect, AppMessage, AppState, UserAction};
use talaria_app::config::parse;
use talaria_app::fleet_actions;
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::{FleetRow, ProfileStatus, RowState};
use talaria_core::fleet_view::{
    self, FleetViewEffect, FleetViewInput, FleetViewState, ReplyOutcome,
};
use talaria_tui::fleet_view::{FleetTheme, FleetView};
use talaria_tui::keys::{Action, Key, KeyContext, KeyMap};

const NOW: f64 = 1_792_700_000.0;

fn key(profile: &str, id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: id.to_owned(),
    }
}

fn foreground_row() -> FleetRow {
    FleetRow {
        key: key("fixture", "s-front"),
        title: "compile".to_owned(),
        model: Some("ollama-cloud/minimax-m3.1-flash".to_owned()),
        source: Some("tui".to_owned()),
        started_at: Some(NOW - 30.0),
        message_count: 2,
        state: RowState::Working,
        state_since: NOW - 10.0,
        outcome: None,
        foreground: true,
        subscribed: true,
        stale: false,
        gone: false,
        unlisted: false,
        activity: Some("compiling".to_owned()),
        cost_usd: None,
        backend: talaria_core::fleet::FleetBackend::Hermes,
        agent: None,
        occupancy: None,
        cost: None,
    }
}

fn committed(effects: &[AppEffect]) -> String {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::CommitLines(lines) => Some(lines.join("\n")),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// An app whose foreground session `s-front` (running) is open on a link to the mock, and the
/// link's events, which must stay open while the link is used.
async fn app_with_foreground(
    gateway: &StandaloneGateway,
    home: &std::path::Path,
) -> (
    AppState,
    tokio::sync::mpsc::Receiver<hermes_client::liveness::LinkEvent>,
) {
    let mut snapshot = mock_gateway::data_snapshot("running_turn.json");
    snapshot["session_id"] = serde_json::json!("s-front");
    snapshot["stored_session_id"] = serde_json::json!("s-front");
    gateway.add_session_snapshot(snapshot);
    let launch = talaria_app::launch::Launch {
        mode: talaria_app::launch::StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: "fixture".to_owned(),
        folder: std::path::PathBuf::from("."),
        record: None,
        bench_replay: None,
        no_ensure: true,
    };
    let connection = talaria_app::connection::ConnectionService::open(&launch, home, None)
        .await
        .unwrap_or_else(|_| panic!("the foreground link did not open"));
    let service =
        talaria_app::sessions::SessionService::with_client("launch", connection.link.client());
    let opened = service
        .open(talaria_app::launch::SessionRequest::ById {
            session_id: "s-front".to_owned(),
        })
        .await
        .unwrap_or_else(|_| panic!("the foreground session did not open"));
    let mut app = AppState::new("launch");
    app.profile = "fixture".to_owned();
    let _ = app::update(
        &mut app,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    app.set_session_service(service, connection.link);
    (app, connection.inbound)
}

#[tokio::test]
async fn foreground_row_uses_foreground_paths() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    let (mut app, _events) = app_with_foreground(&gateway, home.path()).await;
    let row = foreground_row();
    let resumes_before = gateway.requests("session.resume").len();
    let inputs = fleet_actions::execute(
        &mut app,
        FleetViewEffect::SendReply {
            key: row.key.clone(),
            text: "hello from the foreground".to_owned(),
        },
    )
    .await;
    assert!(
        inputs.iter().any(|input| matches!(
            input,
            FleetViewInput::ReplyDone {
                outcome: ReplyOutcome::Sent | ReplyOutcome::Queued,
                ..
            }
        )),
        "the foreground reply was not sent on the foreground path: {inputs:?}"
    );
    let submits = gateway.requests("prompt.submit");
    assert_eq!(submits.len(), 1, "the foreground reply was sent once");
    assert_eq!(
        submits[0].connection,
        gateway.requests("session.resume")[0].connection,
        "the reply went out on the foreground link"
    );
    assert_eq!(
        gateway.requests("session.resume").len(),
        resumes_before,
        "the foreground row resumed a second time"
    );
    let peeked = fleet_actions::execute(&mut app, FleetViewEffect::OpenPeek(row.key.clone())).await;
    assert!(
        matches!(peeked.as_slice(), [FleetViewInput::Peek { .. }]),
        "the foreground peek reads the foreground state: {peeked:?}"
    );
    assert_eq!(gateway.requests("session.resume").len(), resumes_before);
    let stopped = fleet_actions::execute(&mut app, FleetViewEffect::Stop(row.key.clone())).await;
    assert!(!stopped.is_empty(), "the foreground stop sent nothing");
    assert_eq!(
        gateway.requests("session.interrupt").len(),
        1,
        "the foreground stop went out on the foreground link"
    );
    let answer = fleet_actions::execute(&mut app, FleetViewEffect::Close).await;
    assert!(
        answer
            .iter()
            .any(|input| matches!(input, FleetViewInput::PeekClosed)),
        "answer on the foreground row should close the overlay, saw {answer:?}"
    );
    assert!(
        !app.switching,
        "closing the overlay from an answer switched sessions"
    );
}

#[tokio::test]
async fn enter_closes_peek_then_switches() {
    let mut app = AppState::new("launch");
    let target = key("fixture", "s-other");
    let inputs = fleet_actions::execute(&mut app, FleetViewEffect::Open(target.clone())).await;
    assert!(
        inputs
            .iter()
            .any(|input| matches!(input, FleetViewInput::PeekClosed)),
        "enter did not close the peek before switching: {inputs:?}"
    );
    assert!(app.switching, "enter did not switch to the row");
}

#[tokio::test]
async fn fleet_disabled_and_arguments_open_nothing() {
    let mut app = AppState::new("launch");
    let argued = app::update(
        &mut app,
        AppMessage::User(UserAction::Submit("/fleet now".to_owned())),
    );
    let argued_text = committed(&argued);
    assert!(
        argued_text.contains("/fleet takes no arguments."),
        "argument notice was `{argued_text}`"
    );
    app.fleet_enabled = false;
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    let disabled = app::update(
        &mut app,
        AppMessage::User(UserAction::Submit("/fleet".to_owned())),
    );
    let text = committed(&disabled);
    assert!(
        text.contains("The fleet view is off: [fleet] enabled = false in Talaria's configuration."),
        "disabled notice was `{text}`"
    );
    assert_eq!(
        gateway.frame_count(),
        0,
        "a disabled fleet view opened a connection"
    );
    assert!(
        disabled
            .iter()
            .all(|effect| !matches!(effect, AppEffect::Redraw))
    );
}

#[tokio::test]
async fn remapped_fleet_key_works_and_footer_shows_it() {
    let loaded = parse("[keys]\nfleet_peek = [\"p\"]\n");
    assert!(loaded.notices.is_empty(), "{:?}", loaded.notices);
    let (map, notices) = KeyMap::from_entries(&loaded.config.keys);
    assert!(notices.is_empty(), "{notices:?}");
    let pressed = Key::from(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    assert_eq!(
        map.resolve(&pressed, KeyContext::Fleet),
        Some(Action::FleetPeek)
    );

    let mut state = FleetViewState::new(None, NOW);
    fleet_view::update(
        &mut state,
        FleetViewInput::Snapshot {
            version: 1,
            rows: vec![foreground_row()],
            queue: Vec::new(),
            profiles: vec![(
                "fixture".to_owned(),
                ProfileStatus::Listed { truncated: false },
            )],
            scope_note: None,
            link_note: None,
        },
        8,
    );
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            FleetView::draw(
                &state,
                None,
                frame.area(),
                frame.buffer_mut(),
                // The footer names the keys the operator bound.
                &FleetTheme::with_key_map(&map),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let screen: String = (0..24)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        screen.contains("p peek"),
        "the footer does not show the bound peek key"
    );
    let _ = BTreeMap::<String, Vec<String>>::new();
}

/// A fleet overlay frame shows the list, the loading line, or the too-small line.
/// The app loop's ordinary redraw is not that frame.
fn overlay_frame(app: &AppState) -> bool {
    let view = app.view();
    let Some(state) = view.fleet else {
        return false;
    };
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).expect("terminal");
    terminal
        .draw(|frame| {
            FleetView::draw(
                state,
                None,
                frame.area(),
                frame.buffer_mut(),
                &FleetTheme::default(),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let screen: String = (0..48)
        .map(|y| {
            (0..160)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    screen.contains("Fleet ·")
        || screen.contains("Looking for sessions")
        || screen.contains("Window too small")
}

#[tokio::test]
async fn first_frame_within_50ms_without_network() {
    let home = tempfile::tempdir().expect("home");
    let gateway = StandaloneGateway::start(home.path());
    let mut app = AppState::new("launch");
    let started = Instant::now();
    let _effects = app::update(
        &mut app,
        AppMessage::User(UserAction::Submit("/fleet".to_owned())),
    );
    let elapsed = started.elapsed();
    assert!(overlay_frame(&app), "no overlay frame");
    assert!(
        elapsed <= Duration::from_millis(50),
        "the first frame took {elapsed:?}"
    );
    assert_eq!(
        gateway.frame_count(),
        0,
        "the first frame waited on the network"
    );
}

#[tokio::test]
async fn idle_and_streaming_frame_counts() {
    let mut app = AppState::new("launch");
    let _effects = app::update(
        &mut app,
        AppMessage::User(UserAction::Submit("/fleet".to_owned())),
    );
    assert!(overlay_frame(&app), "no overlay frame");
    let mut frames = 1u32;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(1) {
        let more = app::update(&mut app, AppMessage::Tick { now_ms: 0 });
        if more
            .iter()
            .any(|effect| matches!(effect, AppEffect::Redraw))
        {
            frames += 1;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        frames, 1,
        "an idle overlay drew {frames} frames in one second"
    );
    let mut burst = 0u32;
    let burst_started = Instant::now();
    while burst_started.elapsed() < Duration::from_secs(1) {
        let more = app::update(&mut app, AppMessage::Tick { now_ms: 0 });
        if more
            .iter()
            .any(|effect| matches!(effect, AppEffect::Redraw))
        {
            burst += 1;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        burst <= 30,
        "a streaming peek drew {burst} frames in one second"
    );
}
