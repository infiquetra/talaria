//! Red-run tests 35 to 37c (#231): picker keys, registration and frames.

#[path = "support/directory_drive.rs"]
mod drive;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::collections::BTreeMap;

use drive::{committed_text, locked, settle_short};
use talaria_app::launch::StartMode;
use talaria_core::commands::local::LocalCommandTable;
use talaria_core::directory::state::PickerInput;
use talaria_tui::keys::{Action, Key, KeyCode, KeyContext, KeyKind, KeyMap, Mods};

fn key(code: KeyCode) -> Key {
    Key {
        code,
        mods: Mods::default(),
        kind: KeyKind::Press,
    }
}

#[tokio::test]
async fn overlay_owns_its_keys() {
    // The map: j is down in the list and JSON in the dialog.
    let map = KeyMap::defaults();
    let j = key(KeyCode::Char('j'));
    assert_eq!(
        map.resolve_in(&j, KeyContext::SessionPicker),
        Some(Action::PickerDown)
    );
    assert_eq!(
        map.resolve_in(&j, KeyContext::SessionPickerExport),
        Some(Action::PickerFormatJson)
    );
    assert_eq!(map.resolve_in(&j, KeyContext::Composer), None);

    // Esc cannot be rebound in the picker.
    let mut entries = BTreeMap::new();
    entries.insert("picker_close".to_string(), vec!["esc".to_string()]);
    let (rebound, notices) = KeyMap::from_entries(&entries);
    assert_eq!(notices.len(), 1);
    assert!(notices[0].contains("unreadable"), "the notice: {notices:?}");
    assert_eq!(rebound, KeyMap::defaults());

    // session_picker_open is bindable in the composer. Ctrl+O belongs to the transcript, so
    // the example uses a free chord.
    let mut entries = BTreeMap::new();
    entries.insert("session_picker_open".to_string(), vec!["alt+o".to_string()]);
    let (bound, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty());
    let open = Key {
        code: KeyCode::Char('o'),
        mods: Mods {
            alt: true,
            ..Mods::default()
        },
        kind: KeyKind::Press,
    };
    assert_eq!(
        bound.resolve(&open, KeyContext::Composer),
        Some(Action::SessionPickerOpen)
    );

    // While the overlay is open, Esc closes it without interrupting.
    let rig = drive::Rig::start();
    rig.gateway.set_list_rows_for(
        "fixture",
        "live",
        vec![mock_gateway::list_row(
            "s-one",
            "Only work",
            Some(1_792_700_000.0),
            None,
            4,
            false,
        )],
    );
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/resume");
            settle_short().await;
            driver.picker(PickerInput::Escape);
            settle_short().await;
            driver.submit("hello");
            settle_short().await;
            driver.quit();
            driver.finished().await;
        })
        .await;
    assert!(
        !rig.gateway.requests("session.list").is_empty(),
        "the picker listed"
    );
    assert!(
        rig.gateway.requests("session.interrupt").is_empty(),
        "no interrupt was sent"
    );
    assert_eq!(
        rig.gateway.requests("prompt.submit").len(),
        1,
        "the composer is live after the close"
    );
    let lines = committed_text(&log);
    assert!(
        !lines.iter().any(|line| line.contains("Interrupted")),
        "no interrupt notice: {lines:?}"
    );
}

#[test]
fn local_commands_are_registered() {
    let table = LocalCommandTable::builtin();
    for (name, summary) in [
        (
            "resume",
            "Open the session picker, or switch to one session",
        ),
        ("rename", "Rename the current session"),
        (
            "branch",
            "Branch the current session and switch into the child",
        ),
        ("export", "Export a session to a file"),
    ] {
        let entry = table.find(name).expect("the command is registered");
        assert!(entry.takes_argument, "{name} takes an argument");
        assert_eq!(entry.summary, summary);
    }
}

#[tokio::test]
async fn first_frame_precedes_any_call() {
    let rig = drive::Rig::start();
    rig.gateway.set_list_rows_for(
        "fixture",
        "live",
        vec![mock_gateway::list_row(
            "s-one",
            "Only work",
            Some(1_792_700_000.0),
            None,
            4,
            false,
        )],
    );
    let stamp: std::sync::Arc<std::sync::Mutex<(usize, std::time::Instant)>> =
        std::sync::Arc::new(std::sync::Mutex::new((0, std::time::Instant::now())));
    let inside = stamp.clone();
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            let draws_before = driver.draws();
            driver.submit("/resume");
            *inside.lock().expect("the stamp is open") = (draws_before, std::time::Instant::now());
            drive::eventually("a draw after /resume", || driver.draws() > draws_before).await;
            driver.quit();
            driver.finished().await;
        })
        .await;

    let lists = rig.gateway.requests("session.list");
    assert!(!lists.is_empty(), "the picker listed");
    let (draws_before, submitted_at) = *stamp.lock().expect("the stamp is open");
    let draws = locked(&log).draw_at.clone();
    let first_after = draws[draws_before];
    assert!(
        first_after - submitted_at <= std::time::Duration::from_millis(50),
        "the first frame lands within 50 ms"
    );
    assert!(
        first_after < lists[0].at,
        "the first frame precedes any side call"
    );
}

#[tokio::test]
async fn idle_overlay_draws_no_frames() {
    let rig = drive::Rig::start();
    rig.gateway.set_list_rows_for(
        "fixture",
        "live",
        vec![mock_gateway::list_row(
            "s-one",
            "Only work",
            Some(1_792_700_000.0),
            None,
            4,
            false,
        )],
    );
    let gateway = &rig.gateway;
    let (_outcome, log) = rig
        .drive(StartMode::New, |driver| async move {
            driver.opened().await;
            driver.submit("/resume");
            drive::eventually("the lists to arrive", || {
                !gateway.requests("session.list").is_empty()
            })
            .await;
            // One more beat for the last list's frame to land.
            settle_short().await;
            let draws_before = driver.draws();
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            assert_eq!(driver.draws(), draws_before, "no frame while idle");
            driver.quit();
            driver.finished().await;
        })
        .await;
    let _ = committed_text(&log);
}
