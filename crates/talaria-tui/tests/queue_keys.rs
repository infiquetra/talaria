//! Issue 232. Queue and steer. Selection keys and the composer.
//!
//! Up on an empty draft selects the newest queued row and does not recall
//! history. Enter on a blank draft does nothing. `e` is refused while typing.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use talaria_core::queue::{
    AdmissionId, Controls, EditState, IdSource, QueueIntent, QueueView, QueuedRow, RowStatus,
    SubmissionId, update_queue,
};
use talaria_core::session::{
    Cell, CellId, PromptCell, PromptKey, PromptKind, PromptState, SessionId, SessionState,
};
use talaria_tui::composer::{Composer, ComposerContext};
use talaria_tui::keys::{
    Action, Key, KeyChord, KeyCode as TalariaKey, KeyContext, KeyKind, KeyMap, Mods,
};
use talaria_tui::prompts::{KeyOutcome, PromptStack};
use talaria_tui::queue::handle_selection_key;

struct Ids(u32);

impl IdSource for Ids {
    fn submission_id(&mut self) -> SubmissionId {
        self.0 += 1;
        SubmissionId(format!("sub-{}", self.0))
    }
}

fn row(id: &str, text: &str, seq: i64) -> QueuedRow {
    QueuedRow {
        admission_id: AdmissionId(id.to_string()),
        input_id: SubmissionId(format!("in-{id}")),
        text: text.to_string(),
        status: RowStatus::Queued,
        outcome: None,
        controls: Controls {
            steer: true,
            replace: true,
        },
        steering: false,
        sequence: seq,
        execution_generation: Some(1),
    }
}

fn counted(selected: Option<usize>) -> QueueView {
    QueueView {
        rows: vec![
            row("adm-a", "Reply with the word alpha in capital letters.", 1),
            row("adm-b", "Reply with the word bravo in capital letters.", 2),
        ],
        steering: Vec::new(),
        selected,
        paused: false,
        running: false,
        editing: None,
        sending: 0,
    }
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn talaria_key(code: TalariaKey) -> Key {
    Key {
        code,
        mods: Mods::default(),
        kind: KeyKind::Press,
    }
}

fn idle() -> ComposerContext {
    ComposerContext {
        turn_running: false,
    }
}

#[test]
fn up_on_empty_selects_newest() {
    let keys = KeyMap::defaults();
    let view = counted(None);
    let composer = Composer::with_history(keys.clone(), vec!["older draft".to_string()]);
    assert!(composer.text().is_empty());
    assert!(composer.cursor_on_first_row());
    let intent = handle_selection_key(press(KeyCode::Up), &view, &keys);
    assert_eq!(intent, Some(QueueIntent::EnterSelection));
    assert!(
        composer.text().is_empty(),
        "Up on an empty draft selects the newest queued row and does not recall history"
    );
}

#[test]
fn up_with_draft_keeps_history() {
    let keys = KeyMap::defaults();
    let mut composer = Composer::with_history(keys, vec!["older draft".to_string()]);
    composer.handle_key(talaria_key(TalariaKey::Char('g')), idle(), Instant::now());
    assert_eq!(composer.text(), "g");
    composer.handle_key(talaria_key(TalariaKey::Up), idle(), Instant::now());
    assert_eq!(composer.text(), "older draft");
    let _ = counted(None);
}

#[test]
fn up_past_oldest_enters_history() {
    let keys = KeyMap::defaults();
    let view = counted(Some(0));
    let intent = handle_selection_key(press(KeyCode::Up), &view, &keys);
    assert_eq!(intent, None, "Up past the oldest leaves selection");
    let mut composer = Composer::with_history(keys, vec!["older draft".to_string()]);
    composer.handle_key(talaria_key(TalariaKey::Up), idle(), Instant::now());
    assert_eq!(composer.text(), "older draft");
}

#[test]
fn esc_in_selection_never_interrupts() {
    let keys = KeyMap::defaults();
    let intent = handle_selection_key(press(KeyCode::Esc), &counted(Some(1)), &keys);
    assert_eq!(intent, Some(QueueIntent::Leave));
    assert_eq!(
        keys.resolve_in(&talaria_key(TalariaKey::Esc), KeyContext::QueueSelection),
        Some(Action::QueueLeave)
    );
    assert_ne!(
        keys.resolve_in(&talaria_key(TalariaKey::Esc), KeyContext::QueueSelection),
        Some(Action::Interrupt)
    );
}

#[test]
fn other_key_leaves_and_types() {
    let keys = KeyMap::defaults();
    let intent = handle_selection_key(press(KeyCode::Char('x')), &counted(Some(1)), &keys);
    assert_eq!(intent, None);
    let mut composer = Composer::new(keys);
    composer.handle_key(talaria_key(TalariaKey::Char('x')), idle(), Instant::now());
    assert_eq!(composer.text(), "x");
}

#[test]
fn approval_card_blocks_selection() {
    let mut stack = PromptStack::new();
    let cell = PromptCell {
        id: CellId::new(1),
        key: PromptKey {
            session_id: SessionId::new("local-session-1"),
            prompt_id: "prompt-1".to_string(),
            execution_generation: 1,
        },
        kind: PromptKind::Approval,
        command: Some("echo hi".to_string()),
        description: None,
        question: None,
        choices: vec!["allow".to_string(), "deny".to_string()],
        multi_select: false,
        edit: None,
        timeout_s: None,
        state: PromptState::Open,
        sealed: false,
    };
    let now = Instant::now();
    stack.sync(Some("local-session-1"), &[&cell], &[], true, now);
    let later = now + Duration::from_millis(300);
    let outcome = stack.handle_key(talaria_key(TalariaKey::Up), later);
    assert!(
        matches!(outcome, KeyOutcome::Consumed { .. }),
        "an open approval owns Up, so selection does not start"
    );
}

#[test]
fn enter_in_edit_saves_even_empty() {
    let mut state = SessionState::empty(SessionId::new("local-session-1"));
    state.queue.rows = vec![row("adm-a", "alpha", 1)];
    state.queue.edit = Some(EditState {
        admission_id: AdmissionId("adm-a".to_string()),
        original_text: "alpha".to_string(),
        index: 1,
    });
    let mut ids = Ids(0);
    let effects = update_queue(&mut state, QueueIntent::SaveEdit(String::new()), &mut ids);
    assert!(effects.is_empty());
    assert!(state.queue.edit.is_some());
    assert!(
        state.cells().iter().any(|cell| matches!(
            cell,
            Cell::Notice(notice)
                if notice.text
                    == "An empty edit would remove the prompt. Press d on it to remove it instead."
        )),
        "Enter on a blank edit does nothing"
    );
}

#[test]
fn edit_mode_esc_and_arrows() {
    let keys = KeyMap::defaults();
    let mut editing = counted(Some(0));
    editing.editing = Some((1, 2));
    assert_eq!(
        handle_selection_key(press(KeyCode::Esc), &editing, &keys),
        Some(QueueIntent::CancelEdit)
    );
    assert_eq!(
        handle_selection_key(press(KeyCode::Up), &editing, &keys),
        None,
        "an arrow in an edit is not a queue move"
    );
    assert_eq!(
        handle_selection_key(press(KeyCode::Left), &editing, &keys),
        None
    );
}

#[test]
fn queue_actions_in_own_context() {
    let keys = KeyMap::defaults();
    assert!(KeyChord::parse("e", KeyContext::Composer).is_err());
    let typed = talaria_key(TalariaKey::Char('e'));
    assert_eq!(keys.resolve(&typed, KeyContext::Composer), None);
    assert_eq!(
        keys.resolve_in(&typed, KeyContext::QueueSelection),
        Some(Action::QueueEdit)
    );
    assert_eq!(
        keys.resolve_in(
            &talaria_key(TalariaKey::Char('s')),
            KeyContext::QueueSelection
        ),
        Some(Action::QueueSteer)
    );
    assert!(keys.chords(Action::ComposerSteer).is_empty());
}

#[test]
fn keys_configurable_from_config() {
    let dir = tempfile::tempdir().unwrap_or_else(|_| panic!("write the config"));
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "[keys]\nqueue_edit = [\"e\"]\nqueue_steer = [\"s\"]\ncomposer_steer = [\"ctrl+g\"]\n",
    )
    .unwrap_or_else(|_| panic!("write the config"));
    let loaded = talaria_app::config::load_file(&path);
    assert!(
        loaded.notices.is_empty(),
        "config notices stay empty for known queue keys"
    );
    let (map, notices) = KeyMap::from_entries(&loaded.config.keys);
    assert!(notices.is_empty(), "queue keys parse");
    assert_eq!(map.label(Action::QueueEdit).as_deref(), Some("E"));
    assert_eq!(map.label(Action::QueueSteer).as_deref(), Some("S"));
    assert_eq!(map.label(Action::ComposerSteer).as_deref(), Some("Ctrl+G"));
}
