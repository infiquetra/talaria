//! The checklist through the application: toggle, commits, resync, and the pager.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use hermes_client::liveness::LinkEvent;
use hermes_client::replay::{GapReason, ReplayGap};
use hermes_contract::events::EventFrame;
use serde_json::{Value, json};
use talaria_app::app::{AppEffect, AppMessage, AppState, InputCode, InputKey, UserAction, update};
use talaria_app::sessions::OpenedSession;
use talaria_core::checklist::{Checklist, CommitKind, TodoItem, TodoStatus};
use talaria_core::session::{Event, SessionId};
use talaria_tui::checklist::{ChecklistSection, ChecklistStyle, committed_block_lines};

const LAUNCH: &str = "0123456789abcdef";
const SESSION: &str = "local-session-1";

fn todos() -> Value {
    json!([
        {"id": "1", "content": "Read the failing test", "status": "completed"},
        {"id": "2", "content": "Fix average()", "status": "in_progress"},
        {"id": "3", "content": "Run the test suite", "status": "pending"},
    ])
}

fn todo_state(revision: i64) -> Value {
    json!({"todos": todos(), "revision": revision})
}

fn snapshot(running: bool, todo: Option<Value>) -> Value {
    let mut snap = mock_gateway::recorded_resume_snapshot();
    snap["running"] = json!(running);
    match todo {
        Some(state) => snap["todo_state"] = state,
        None => {
            snap.as_object_mut()
                .expect("a snapshot object")
                .remove("todo_state");
        }
    }
    snap
}

fn opened(snap: Value) -> OpenedSession {
    let snapshot: hermes_contract::schemas::CanonicalSnapshot =
        serde_json::from_value(snap).expect("a contract-shaped snapshot");
    OpenedSession::from_snapshot(snapshot)
}

fn opened_running_with_todos() -> OpenedSession {
    opened(snapshot(true, Some(todo_state(2))))
}

fn opened_idle_with_todos(revision: i64) -> OpenedSession {
    opened(snapshot(false, Some(todo_state(revision))))
}

fn opened_idle_without_todos() -> OpenedSession {
    opened(snapshot(false, None))
}

/// One reducer event with contract-shaped params. No sequence number: the
/// prompt path feeds the reducer directly, past the replay cursor.
fn session_event(kind: &str, payload: Value) -> Event {
    let params = json!({
        "type": kind,
        "session_id": SESSION,
        "payload": payload,
    });
    let frame = EventFrame::from_params(&params).expect("a contract-shaped event");
    Event::Session {
        session_id: SessionId::new(SESSION),
        frame,
        raw: params,
    }
}

fn todo_updated(revision: i64) -> Event {
    session_event(
        "todo.updated",
        json!({"revision": revision, "todos": todos()}),
    )
}

fn prompt_done(events: Vec<Event>) -> AppMessage {
    AppMessage::PromptDone {
        events,
        notices: Vec::new(),
        masked: None,
    }
}

fn input_key(code: InputCode, ctrl: bool) -> AppMessage {
    AppMessage::Input(InputKey {
        code,
        ctrl,
        alt: false,
        shift: false,
        super_key: false,
        other: false,
    })
}

fn gap() -> AppMessage {
    AppMessage::Link(LinkEvent::Gap(ReplayGap {
        session_id: SESSION.to_string(),
        reason: GapReason::SeqJump,
        latest_seq: None,
        replay_epoch: None,
    }))
}

fn redraws(effects: &[AppEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, AppEffect::Redraw))
        .count()
}

fn checklist_commits(effects: &[AppEffect]) -> Vec<&talaria_core::checklist::ChecklistCommit> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            AppEffect::CommitChecklist(commit) => Some(commit),
            _ => None,
        })
        .collect()
}

fn is_checklist_block(effect: &AppEffect) -> bool {
    matches!(effect, AppEffect::CommitChecklist(_))
}

fn is_cell_commit(effect: &AppEffect) -> bool {
    matches!(effect, AppEffect::CommitCells(_))
}

#[test]
fn ctrl_t_toggles_and_never_reaches_the_composer() {
    // The draft and the focused card live on the screen side: the app proves
    // it leaves them alone by returning exactly one redraw and nothing else.
    let mut state = AppState::new(LAUNCH);
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened: opened_running_with_todos(),
            notice: None,
        },
    );
    assert!(state.checklist.live().is_some(), "the panel is live");
    let before = state.checklist.expanded();
    let effects = update(&mut state, AppMessage::User(UserAction::ToggleTasks));
    assert_eq!(state.checklist.expanded(), !before, "the form flips");
    assert_eq!(redraws(&effects), 1, "one redraw");
    assert!(
        effects
            .iter()
            .all(|effect| matches!(effect, AppEffect::Redraw)),
        "nothing else, so draft abc and any focused card are untouched"
    );
    // An approval card focused, then a question card: the same single redraw.
    // Focus itself is screen state; the app half is identical either way.
    for _ in 0..2 {
        let effects = update(&mut state, AppMessage::User(UserAction::ToggleTasks));
        assert_eq!(redraws(&effects), 1, "one redraw with a card focused");
        assert!(
            effects
                .iter()
                .all(|effect| matches!(effect, AppEffect::Redraw)),
            "the card keeps focus"
        );
    }
    // No panel: no redraw.
    let mut idle = AppState::new(LAUNCH);
    let effects = update(&mut idle, AppMessage::User(UserAction::ToggleTasks));
    assert_eq!(redraws(&effects), 0, "no panel, no redraw");
    // The pager owns the keys while open: Control-O opens it, and Control-T
    // sent as an input key reaches the pager, changing nothing in the
    // checklist while the live panel stays up underneath.
    let opened_effects = update(&mut state, input_key(InputCode::Char('o'), true));
    assert!(state.pager_open, "Control-O opens the transcript");
    assert!(
        opened_effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Transcript(_))),
        "opening reaches the transcript screen"
    );
    let expanded = state.checklist.expanded();
    let effects = update(&mut state, input_key(InputCode::Char('t'), true));
    assert!(state.pager_open, "the transcript is still open");
    assert_eq!(
        state.checklist.expanded(),
        expanded,
        "Control-T changes nothing in the checklist"
    );
    assert!(
        state.checklist.live().is_some(),
        "the live panel is untouched"
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Transcript(_))),
        "the key reaches the pager, never the checklist"
    );
}

#[test]
fn block_is_committed_after_the_turns_last_cell() {
    let mut state = AppState::new(LAUNCH);
    let mut snap = snapshot(true, Some(todo_state(2)));
    snap["prompts"] = json!([]);
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened: opened(snap),
            notice: None,
        },
    );
    let history = state
        .opened
        .as_ref()
        .expect("an open session")
        .state
        .cells()
        .len();
    let _ = update(
        &mut state,
        prompt_done(vec![session_event(
            "message.delta",
            json!({"text": "The average is fixed."}),
        )]),
    );
    let grown = state
        .opened
        .as_ref()
        .expect("an open session")
        .state
        .cells()
        .len();
    assert!(grown > history, "the turn's last cell arrives");
    let effects = update(
        &mut state,
        prompt_done(vec![session_event(
            "message.complete",
            json!({"status": "complete"}),
        )]),
    );
    let block = effects
        .iter()
        .position(is_checklist_block)
        .expect("the block is committed after the turn");
    let cell = effects
        .iter()
        .rposition(is_cell_commit)
        .expect("the turn's last cell is committed");
    assert!(cell < block, "the block follows the turn's last cell");
    let commits = checklist_commits(&effects);
    assert_eq!(commits.len(), 1, "exactly one block");
    assert_eq!(commits[0].kind, CommitKind::TurnEnded);
    // With colour off the block's spans carry no colour. The plain style is
    // the colour-off half; the screen picks it exactly when the Markdown
    // renderer is told to render without colour.
    for line in committed_block_lines(commits[0], 80, &ChecklistStyle::plain()) {
        for span in &line.spans {
            assert!(span.style.fg.is_none(), "no foreground colour");
            assert!(span.style.bg.is_none(), "no background colour");
        }
    }
}

#[test]
fn resumed_idle_session_commits_block_after_history() {
    let mut state = AppState::new(LAUNCH);
    let effects = update(
        &mut state,
        AppMessage::Opened {
            opened: opened_idle_with_todos(2),
            notice: None,
        },
    );
    let history = effects
        .iter()
        .position(is_cell_commit)
        .expect("history is committed");
    let commits = checklist_commits(&effects);
    assert_eq!(commits.len(), 1, "exactly one Resumed block");
    assert_eq!(commits[0].kind, CommitKind::Resumed);
    let block = effects
        .iter()
        .position(is_checklist_block)
        .expect("a block");
    assert!(history < block, "the history commit comes first");
    // A following identical reducer update adds no block.
    let effects = update(&mut state, prompt_done(vec![todo_updated(2)]));
    assert!(
        checklist_commits(&effects).is_empty(),
        "an identical update adds no block"
    );
}

#[test]
fn resend_and_resize_do_not_commit_or_redraw_twice() {
    let mut state = AppState::new(LAUNCH);
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened: opened_running_with_todos(),
            notice: None,
        },
    );
    // An identical todo.updated (same revision and items): no redraw, no commit.
    let effects = update(&mut state, prompt_done(vec![todo_updated(2)]));
    assert!(
        checklist_commits(&effects).is_empty(),
        "a resend commits nothing"
    );
    assert_eq!(redraws(&effects), 0, "a resend redraws nothing");
    // A resize recomputes the panel rows from the new width and commits
    // nothing: the resize never reaches the application update, so the panel
    // is re-rendered from the live list at the new width.
    let list = Checklist {
        revision: 2,
        items: vec![
            TodoItem {
                id: "1".to_string(),
                content: "Read the failing test".to_string(),
                status: TodoStatus::Completed,
                parent: None,
            },
            TodoItem {
                id: "2".to_string(),
                content: "Fix average() so the suite passes again".to_string(),
                status: TodoStatus::InProgress,
                parent: None,
            },
            TodoItem {
                id: "3".to_string(),
                content: "Run the test suite".to_string(),
                status: TodoStatus::Pending,
                parent: None,
            },
        ],
        skipped: 0,
    };
    let style = ChecklistStyle::default();
    let section = ChecklistSection::new(&list, true, 8, None, &style);
    let wide: Vec<String> = section
        .lines(80)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();
    let narrow_lines = section.lines(30);
    let narrow: Vec<String> = narrow_lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();
    assert_ne!(wide, narrow, "rows recomputed from the new width");
    assert!(
        narrow_lines.iter().all(|line| line.width() <= 30),
        "every row fits the new width"
    );
}

#[test]
fn turn_end_under_an_overlay_still_commits() {
    let mut state = AppState::new(LAUNCH);
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened: opened_running_with_todos(),
            notice: None,
        },
    );
    let _ = update(&mut state, input_key(InputCode::Char('o'), true));
    assert!(state.pager_open, "the transcript is open");
    let effects = update(
        &mut state,
        prompt_done(vec![session_event(
            "message.complete",
            json!({"status": "complete"}),
        )]),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Transcript(_))),
        "the open transcript hears about the turn end"
    );
    let commits = checklist_commits(&effects);
    assert_eq!(commits.len(), 1, "one commit effect for the block");
    assert_eq!(commits[0].kind, CommitKind::TurnEnded);
}

#[test]
fn gap_resync_does_not_reprint_the_block() {
    // A completed turn commits its block; the gap resync after it, with the
    // same list, commits no second block.
    let mut state = AppState::new(LAUNCH);
    let _ = update(
        &mut state,
        AppMessage::Opened {
            opened: opened_running_with_todos(),
            notice: None,
        },
    );
    let effects = update(
        &mut state,
        prompt_done(vec![session_event(
            "message.complete",
            json!({"status": "complete"}),
        )]),
    );
    assert_eq!(
        checklist_commits(&effects).len(),
        1,
        "one block at turn end"
    );
    let effects = update(&mut state, gap());
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Recover(_))),
        "the gap triggers a resume"
    );
    let effects = update(
        &mut state,
        AppMessage::Recovered(Ok(opened_idle_with_todos(2))),
    );
    assert!(
        checklist_commits(&effects).is_empty(),
        "the resync reprints no block"
    );
    // A turn that began and ended entirely inside the gap: the resume reply
    // shows an idle session with a list at a revision the app never saw.
    // Nothing is committed and no panel appears; the stored list goes live
    // at the next running turn.
    let mut quiet = AppState::new(LAUNCH);
    let _ = update(
        &mut quiet,
        AppMessage::Opened {
            opened: opened_idle_without_todos(),
            notice: None,
        },
    );
    let _ = update(&mut quiet, gap());
    let effects = update(
        &mut quiet,
        AppMessage::Recovered(Ok(opened_idle_with_todos(5))),
    );
    assert!(
        checklist_commits(&effects).is_empty(),
        "a turn wholly inside the gap commits no block"
    );
    assert!(
        quiet.checklist.live().is_none(),
        "no panel for a turn wholly inside the gap"
    );
    let _ = update(
        &mut quiet,
        prompt_done(vec![session_event("message.start", json!({}))]),
    );
    assert!(
        quiet.checklist.live().is_some(),
        "the stored list goes live at the next running turn"
    );
}
