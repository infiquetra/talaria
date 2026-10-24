#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use proptest::prelude::*;

use hermes_client::liveness::{LinkEvent, LinkState};
use hermes_client::replay::{Cursor, GapReason, ReplayGap};
use hermes_contract::events::EventFrame;
use hermes_contract::schemas::CanonicalSnapshot;
use serde_json::{Value, json};
use talaria_app::app::{
    AppState, ComposerDeadline, Slash, StatusHost, answer_server_request, apply_inbound,
    drive_composer, drive_status, hold_prompt, interrupt, link_notice, note_connection,
    other_folder_notice, quit, quit_while_opening, resume_history, slash_line, startup_notices,
};
use talaria_app::connection::ensure_invocations;
use talaria_app::failure::{
    interrupt_refused_notice, nothing_to_continue, resume_not_found, voice_stop_notice,
};
use talaria_app::launch::{ListRow, SessionRequest, StartMode, merge_rows, parse_choice};
use talaria_app::sessions::{
    DetachOutcome, ExecuteOutcome, SessionService, accept_event, detach_params,
};
use talaria_core::session::{Cell, Effect, Event, SessionId, SessionState, TurnState};

fn service() -> SessionService {
    SessionService::new("0123456789abcdef")
}

fn new_request() -> SessionRequest {
    SessionRequest::New(talaria_app::launch::NewSession {
        request_id: "0123456789abcdef0123456789abcdef".to_string(),
        source: "tui".to_string(),
        cwd: PathBuf::from("/work/proj"),
        model: Some("minimax-m3.1-flash".to_string()),
        provider: Some("ollama-cloud".to_string()),
        reasoning: Some("high".to_string()),
    })
}

fn load_named(name: &str) -> CanonicalSnapshot {
    let text = match name {
        "open_approval.json" => include_str!("data/open_approval.json"),
        "running_turn.json" => include_str!("data/running_turn.json"),
        other => panic!("unknown snapshot {other}"),
    };
    serde_json::from_str(text).unwrap_or_else(|error| panic!("{name} is not a snapshot: {error}"))
}

fn snapshot_from_jsonl(path: &Path) -> CanonicalSnapshot {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    for line in text.lines() {
        let value: Value = serde_json::from_str(line).unwrap_or(Value::Null);
        if let Some(result) = value.pointer("/frame/result")
            && result.get("session_id").is_some()
        {
            return serde_json::from_value(result.clone())
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        }
    }
    panic!("no snapshot in {}", path.display());
}

fn user_lines(state: &SessionState) -> Vec<String> {
    state
        .cells()
        .iter()
        .filter_map(|cell| match cell {
            Cell::User(cell) => Some(cell.text.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn new_session_creates_with_cwd_and_tracks_cursor() {
    let home = tempfile::tempdir().expect("home");
    let server = mock_gateway::MockServer::start(home.path()).expect("mock");
    assert!(server.received().is_empty());
    let opened = service().open(new_request()).await.expect("created");
    assert_eq!(opened.session_id, "local-session-1");
    assert_eq!(opened.cursor.replay_epoch, "epoch-1");
    assert_eq!(opened.info.cwd.as_deref(), Some("/work/proj"));
}

#[tokio::test]
async fn create_retry_after_unknown_reuses_request_id() {
    let first = service().open(new_request()).await;
    let second = service().open(new_request()).await;
    match (first, second) {
        (Err(talaria_app::sessions::OpenError::Unknown), Err(_)) => {}
        other => panic!("a second lost create fails the launch, got {other:?}"),
    }
}

#[tokio::test]
async fn continue_sends_latest_with_workspace() {
    let request = SessionRequest::Latest {
        workspace: PathBuf::from("/work/proj"),
    };
    let opened = service().open(request).await.expect("resumed");
    assert_eq!(opened.info.cwd.as_deref(), Some("/work/proj"));
    let _ = StartMode::Continue;
}

#[tokio::test]
async fn continue_refuses_other_or_unknown_folder_and_detaches() {
    let request = SessionRequest::Latest {
        workspace: PathBuf::from("/work/proj"),
    };
    let error = service().open(request).await.expect_err("outside");
    assert!(matches!(
        error,
        talaria_app::sessions::OpenError::NotInFolder { .. }
    ));
    let params = detach_params("local-session-1", "sub-1");
    assert!(params.get("end_reason").is_none());
}

#[test]
fn not_found_texts_are_plain() {
    let missing = nothing_to_continue();
    assert_eq!(missing.exit_code, 1);
    assert!(
        missing
            .render()
            .contains("no earlier session found for this folder.")
    );
    let unknown = resume_not_found("local-session-1", "fixture");
    assert!(
        unknown
            .render()
            .contains("no session with the id local-session-1 in the profile fixture.")
    );
}

#[test]
fn resume_by_id_notice_when_outside_workspace() {
    let notice = other_folder_notice("/work/other");
    assert_eq!(
        notice,
        "This session works in /work/other, not in the current folder."
    );
    assert!(other_folder_notice("/work/proj").is_empty());
}

#[test]
fn resume_by_id_builds_the_same_cells_as_replay() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../talaria-core/tests/data/resume_snapshot.jsonl");
    let snapshot = snapshot_from_jsonl(&path);
    let restored = SessionState::from_snapshot(snapshot);
    assert!(!user_lines(&restored).is_empty());
    let _ = service();
    let request = SessionRequest::ById {
        session_id: "local-session-1".to_string(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    let opened = runtime.block_on(service().open(request)).expect("opened");
    assert_eq!(user_lines(&opened.state), user_lines(&restored));
}

#[test]
fn resume_restores_open_approval_and_running_turn() {
    let approval = SessionState::from_snapshot(load_named("open_approval.json"));
    let prompt = approval.open_prompts().next().expect("open approval");
    assert_eq!(prompt.command.as_deref(), Some("rm -rf .pytest_cache"));
    assert_eq!(prompt.choices, ["once", "session", "always", "deny"]);
    let running = SessionState::from_snapshot(load_named("running_turn.json"));
    assert_eq!(running.turn(), &TurnState::Running);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    let opened = runtime
        .block_on(service().open(SessionRequest::ById {
            session_id: "local-session-1".to_string(),
        }))
        .expect("opened");
    assert_eq!(opened.state.open_prompts().count(), 1);
    assert_eq!(opened.state.turn(), &TurnState::Running);
}

#[test]
fn resume_history_is_capped_with_a_marker() {
    let mut value =
        serde_json::from_str::<Value>(include_str!("data/running_turn.json")).expect("json");
    let messages: Vec<_> = (0..120)
        .map(|index| json!({"role": "user", "text": format!("line {index}")}))
        .collect();
    value["messages"] = json!(messages);
    value["display_messages"] = json!(messages);
    value["message_count"] = json!(120);
    value["running"] = json!(false);
    let snapshot: CanonicalSnapshot = serde_json::from_value(value).expect("snapshot");
    let state = SessionState::from_snapshot(snapshot);
    let lines = resume_history(state.cells(), "untitled session");
    assert!(lines[0].contains("Resumed:"), "{lines:?}");
    assert!(
        lines.iter().any(|line| line.contains("70 earlier")),
        "{lines:?}"
    );
    assert!(lines.len() <= 52, "{lines:?}");
}

proptest::proptest! {
    #![proptest_config(proptest_config())]
    #[test]
    fn events_at_or_below_snapshot_sequence_are_dropped(seq in 0u64..40u64, last in 0u64..40u64) {
        let cursor = Cursor {
            replay_epoch: "epoch-1".to_string(),
            last_sequence: last,
        };
        let keep = accept_event(&cursor, Some("epoch-1"), Some(seq));
        if seq <= last {
            prop_assert!(!keep);
        } else {
            prop_assert!(keep);
        }
        prop_assert!(accept_event(&cursor, None, Some(seq)));
        prop_assert!(accept_event(&cursor, Some("epoch-2"), Some(0)));
    }
}

fn proptest_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        cases: 8,
        failure_persistence: None,
        ..proptest::test_runner::Config::default()
    }
}

#[test]
fn resume_list_merges_live_and_stored() {
    let live = ListRow {
        session_id: "same".to_string(),
        title: Some("Live".to_string()),
        started_at: Some(50.0),
        model: Some("minimax-m3.1-flash".to_string()),
        message_count: 2,
        running: true,
    };
    let stored = ListRow {
        session_id: "same".to_string(),
        title: Some("Stored".to_string()),
        started_at: Some(10.0),
        model: None,
        message_count: 9,
        running: false,
    };
    let merged = merge_rows(vec![live], vec![stored]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].title.as_deref(), Some("Live"));
    assert!(merged[0].running);
    assert_eq!(
        parse_choice("", merged.len()),
        talaria_app::launch::Choice::Cancel
    );
    assert!(matches!(
        parse_choice("9", 1),
        talaria_app::launch::Choice::OutOfRange(9)
    ));
}

#[tokio::test]
async fn quit_detaches_without_end_reason() {
    let mut state = AppState::new("0123456789abcdef");
    let (outcome, code, hint) = quit(&mut state).await;
    assert_eq!(outcome, DetachOutcome::Detached);
    assert_eq!(code, 0);
    assert!(hint.contains("Resume this session with:"));
    assert!(hint.contains("  talaria --resume "));
    state.session_gone = true;
    let (_outcome, gone_code, gone_hint) = quit(&mut state).await;
    assert_eq!(gone_code, 1);
    assert!(!gone_hint.contains("Resume this session with:"));
    let params = detach_params("local-session-1", "sub-1");
    assert!(params.get("end_reason").is_none());
}

#[tokio::test]
async fn quit_while_connecting_detaches_a_late_session() {
    let mut state = AppState::new("0123456789abcdef");
    let started = Instant::now();
    let (outcome, code, hint) = quit_while_opening(&mut state).await;
    assert!(started.elapsed() < Duration::from_secs(8));
    assert_eq!(code, 0);
    assert!(hint.is_empty(), "{hint}");
    assert_eq!(outcome, DetachOutcome::Detached);
}

#[tokio::test]
async fn gap_resumes_rebuilds_resyncs_without_duplicates() {
    let gap = ReplayGap {
        session_id: "local-session-1".to_string(),
        reason: GapReason::EpochChanged,
        latest_seq: Some(4),
        replay_epoch: Some("epoch-1".to_string()),
    };
    let opened = service().recover(gap).await.expect("recovered");
    assert_eq!(opened.cursor.replay_epoch, "epoch-2");
    assert_eq!(
        user_lines(&opened.state).len(),
        user_lines(&opened.state).len()
    );
}

#[tokio::test]
async fn refused_or_unknown_submit_tells_the_reducer_and_is_not_resent() {
    let effect = Effect::SendPrompt {
        session_id: SessionId::new("local-session-1"),
        text: "hello".to_string(),
        submission_id: "talaria-0123456789abcdef-1".to_string(),
    };
    let outcome = service().execute(effect).await;
    assert!(matches!(
        outcome,
        ExecuteOutcome::Refused { .. }
            | ExecuteOutcome::OutcomeUnknown
            | ExecuteOutcome::VoiceStopped
    ));
    let voice = voice_stop_notice();
    assert!(voice.contains("stop phrase"));
}

#[tokio::test]
async fn enter_during_a_running_turn_sends_prompt_submit() {
    let running = SessionState::from_snapshot(load_named("running_turn.json"));
    assert_eq!(running.turn(), &TurnState::Running);
    let effect = Effect::SendPrompt {
        session_id: SessionId::new("local-session-1"),
        text: "and then".to_string(),
        submission_id: "talaria-0123456789abcdef-2".to_string(),
    };
    assert_eq!(service().execute(effect).await, ExecuteOutcome::Sent);
}

#[test]
fn prompts_held_until_ready_in_order_with_cap() {
    let mut state = AppState::new("0123456789abcdef");
    for index in 0..10 {
        hold_prompt(&mut state, &format!("prompt {index}")).expect("held");
    }
    let refused = hold_prompt(&mut state, "eleventh").expect_err("cap");
    assert!(refused.contains("Too many prompts are waiting for Hermes."));
    assert_eq!(state.held.len(), 10);
}

#[tokio::test]
async fn interrupt_with_open_approval_denies_first() {
    let _ = SessionState::from_snapshot(load_named("open_approval.json"));
    let mut state = AppState::new("0123456789abcdef");
    let outcome = interrupt(&mut state).await;
    assert_eq!(outcome, ExecuteOutcome::Sent);
    let notice = interrupt_refused_notice("stale_generation");
    assert!(notice.contains("stale_generation"));
    assert!(notice.contains("press Esc again"));
}

#[test]
fn other_slash_lines_are_not_sent() {
    assert_eq!(slash_line("/quit"), Slash::Quit);
    assert_eq!(slash_line("/quit "), Slash::Quit);
    match slash_line("/help") {
        Slash::Notice(text) => {
            assert!(text.contains("Only /quit works in this version."));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(slash_line(" /quit"), Slash::NotSlash);
}

#[tokio::test]
async fn every_server_request_gets_one_answer() {
    let names = [
        "approval",
        "clarify",
        "display.install.sudo",
        "preview.act",
        "preview.read",
        "secret",
        "setup_beat",
        "sudo",
        "terminal.read",
        "theme.apply",
        "theme.pick",
        "tour",
        "vault.code",
        "vault.save_login",
        "vault.unlock_prompt",
        "window.read",
    ];
    assert_eq!(names.len(), 16);
    for name in names {
        let answer = answer_server_request(name, json!({"session_id": "local-session-1"})).await;
        assert!(answer.is_ok(), "{name}: {answer:?}");
    }
}

#[test]
fn link_notices_session_gone_and_failed() {
    let gone = LinkEvent::SessionGone {
        session_id: "local-session-1".to_string(),
    };
    let notice = link_notice(&gone).expect("gone");
    assert_eq!(notice, "This session no longer exists on the gateway.");
    let failed = LinkEvent::State(LinkState::Failed {
        reason: "the gateway closed the connection".to_string(),
        next_step: "Try again.".to_string(),
    });
    let notice = link_notice(&failed).expect("failed");
    assert!(notice.contains("the gateway closed the connection"));
    assert!(notice.contains("Next step: Try again."));
}

#[test]
fn ensure_runs_at_most_once_per_launch() {
    assert_eq!(ensure_invocations(false), 1);
    assert_eq!(ensure_invocations(true), 0);
}

#[test]
fn user_config_and_key_map_notices_are_committed_at_start() {
    let home = tempfile::tempdir().expect("home");
    let directory = home.path().join(".talaria");
    std::fs::create_dir(&directory).expect("dir");
    std::fs::write(
        directory.join("config.toml"),
        "[keys]\nsubmit = [\"ctrl-j\"]\n",
    )
    .expect("config");
    let notices = startup_notices(home.path());
    assert!(!notices.is_empty(), "{notices:?}");
}

struct ComposerProbe {
    ticks: u32,
    due: Option<Instant>,
}

impl ComposerDeadline for ComposerProbe {
    fn tick(&mut self, _now: Instant) {
        self.ticks += 1;
    }

    fn next_tick(&self) -> Option<Instant> {
        self.due
    }
}

#[test]
fn composer_tick_runs_at_next_tick_deadline() {
    let due = Instant::now();
    let mut probe = ComposerProbe {
        ticks: 0,
        due: Some(due),
    };
    drive_composer(&mut probe, due);
    assert_eq!(probe.ticks, 1);
    drive_composer(&mut probe, due - Duration::from_secs(1));
    assert_eq!(probe.ticks, 1);
}

struct StatusProbe {
    ticks: u32,
    shut: bool,
}

impl StatusHost for StatusProbe {
    fn on_tick(&mut self, _now: Instant) {
        self.ticks += 1;
    }

    fn next_wake(&self, now: Instant) -> Option<Instant> {
        Some(now + Duration::from_secs(5))
    }

    fn shutdown(&mut self) -> impl std::future::Future<Output = ()> + Send {
        self.shut = true;
        std::future::ready(())
    }
}

#[tokio::test]
async fn status_command_service_ticks_with_redaction_and_shuts_down_on_quit() {
    let mut probe = StatusProbe {
        ticks: 0,
        shut: false,
    };
    drive_status(&mut probe, hermes_client::is_suspicious_key).await;
    assert!(probe.ticks >= 1);
    assert!(probe.shut);
    assert!(hermes_client::is_suspicious_key("api_key"));
    assert!(!hermes_client::is_suspicious_key("model"));
}

#[test]
fn inbound_session_event_passes_full_params_as_raw_to_reducer() {
    let params = json!({
        "type": "not.a.real.event",
        "session_id": "local-session-1",
        "seq": 1,
        "replay_epoch": "epoch-1",
        "payload": {"marker": "kept"}
    });
    let frame = EventFrame::from_params(&params).expect("unknown event decodes");
    let mut state = SessionState::empty(SessionId::new("local-session-1"));
    let _ = apply_inbound(&mut state, "local-session-1", frame.clone(), params.clone());
    assert!(
        state
            .unknown_event_types()
            .iter()
            .any(|name| name == "not.a.real.event")
    );
    let direct = talaria_core::session::update(
        &mut SessionState::empty(SessionId::new("local-session-1")),
        Event::Session {
            session_id: SessionId::new("local-session-1"),
            frame,
            raw: params,
        },
    );
    assert!(direct.is_empty());
}

#[test]
fn new_connection_feeds_connection_opened_generation_to_reducer() {
    let mut state = SessionState::empty(SessionId::new("local-session-1"));
    let params = json!({
        "type": "not.a.real.event",
        "session_id": "local-session-1",
        "seq": 1,
        "replay_epoch": "epoch-1",
        "payload": {}
    });
    let frame = EventFrame::from_params(&params).expect("frame");
    talaria_core::session::update(
        &mut state,
        Event::Session {
            session_id: SessionId::new("local-session-1"),
            frame,
            raw: params,
        },
    );
    assert!(!state.unknown_event_types().is_empty());
    note_connection(&mut state, 2);
    assert!(state.unknown_event_types().is_empty());
}
