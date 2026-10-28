//! Request files and the pause clock. The writer stub is the first call in the byte test.

#[path = "support/lines.rs"]
mod lines;
#[path = "support/replay_run.rs"]
mod replay;

use talaria_workflow::progress::live::{ProgressInput, ProgressState, update};
use talaria_workflow::progress::{ControlOutcome, PendingRequest, RunControl};

const RESTART_KEY: &str = "11111111111111111111111111111111";

#[test]
fn requests_are_create_new_and_idempotent() {
    let (kept, dir) = replay::empty_run();
    let _kept = kept;
    let first = RunControl::request_pause(&dir, 1_000).expect("pause");
    assert_eq!(first, ControlOutcome::Published);
    let pause = std::fs::read(dir.path().join("control").join("pause.json")).expect("pause bytes");
    assert_eq!(pause, br#"{"v":1,"kind":"pause","at_ms":1000}"#);

    let duplicate = RunControl::request_pause(&dir, 2_000).expect("duplicate");
    assert_eq!(duplicate, ControlOutcome::AlreadyRequested);
    let winner = std::fs::read(dir.path().join("control").join("pause.json")).expect("winner");
    assert_eq!(winner, br#"{"v":1,"kind":"pause","at_ms":1000}"#);

    let stop = RunControl::request_stop(&dir, 3_000).expect("stop");
    assert_eq!(stop, ControlOutcome::Published);
    let stop_bytes =
        std::fs::read(dir.path().join("control").join("stop.json")).expect("stop bytes");
    assert_eq!(stop_bytes, br#"{"v":1,"kind":"stop","at_ms":3000}"#);

    let restart = RunControl::request_restart(&dir, RESTART_KEY, 4_000).expect("restart");
    assert_eq!(restart, ControlOutcome::Published);
    let restart_path = dir
        .path()
        .join("control")
        .join(format!("restart-{RESTART_KEY}.json"));
    let restart_bytes = std::fs::read(&restart_path).expect("restart bytes");
    let expected = format!(r#"{{"v":1,"kind":"restart","key":"{RESTART_KEY}","at_ms":4000}}"#);
    assert_eq!(restart_bytes, expected.as_bytes());

    let resumed = RunControl::request_resume(&dir).expect("resume");
    assert_eq!(resumed, ControlOutcome::Removed);
    assert!(!dir.path().join("control").join("pause.json").exists());

    let raced_dir = dir.clone();
    let handle = std::thread::spawn(move || RunControl::request_pause(&raced_dir, 5_000));
    let local = RunControl::request_pause(&dir, 6_000).expect("local race");
    let remote = handle.join().expect("thread").expect("remote race");
    assert!(local == ControlOutcome::Published || remote == ControlOutcome::Published);
    assert!(
        local == ControlOutcome::AlreadyRequested || remote == ControlOutcome::AlreadyRequested
    );
    let after = std::fs::read(dir.path().join("control").join("pause.json")).expect("one winner");
    assert!(
        after == br#"{"v":1,"kind":"pause","at_ms":5000}"#
            || after == br#"{"v":1,"kind":"pause","at_ms":6000}"#
    );
}

#[test]
fn pause_resume_and_unanswered_paths() {
    let mut state = ProgressState::blank(&lines::manifest());
    state.pending_controls.pause = Some(PendingRequest {
        v: 1,
        kind: "pause".to_string(),
        key: None,
        at_ms: 1_000,
    });
    state.pending_controls.restarts.push(PendingRequest {
        v: 1,
        kind: "restart".to_string(),
        key: Some(RESTART_KEY.to_string()),
        at_ms: 1_000,
    });
    state.now_ms = 11_000;
    let _effects = update(&mut state, ProgressInput::Tick);
    assert!(
        state
            .footer
            .as_deref()
            .is_some_and(|footer| footer.contains("no runner answered"))
    );
    assert_ne!(state.run_suffix.as_deref(), Some("paused"));
    assert!(
        state
            .footer
            .as_deref()
            .is_none_or(|footer| !footer.contains(RESTART_KEY))
    );
    assert!(
        state
            .pending_controls
            .warnings
            .iter()
            .all(|warning| !warning.contains(RESTART_KEY))
    );

    state.now_ms = 1_500;
    state.footer = None;
    let _effects = update(&mut state, ProgressInput::Tick);
    assert_ne!(state.run_suffix.as_deref(), Some("paused"));

    state.now_ms = 2_000;
    let _effects = update(&mut state, ProgressInput::Tick);
    assert_ne!(
        state.run_suffix.as_deref(),
        Some("paused"),
        "an empty journal is not a pause"
    );
}
