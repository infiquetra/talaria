//! Rewind state machine scenarios, each driven from the default state through real inputs.
//! Every reply takes its ticket from the call effect that asked for it. Issue 233.

use serde_json::{Value, json};
use talaria_core::rewind::{
    Call, CallFailure, CheckpointList, DiffDoc, DiffSource, MutateOutcome, Reply, RestoreOutcome,
    RewindEffect, RewindInput, RewindKey, RewindScreen, RewindState, SessionRead, update,
};
use talaria_core::session::NoticeKind;

const NOW_MS: u64 = 1_792_674_200_000;

fn user(id: i64, text: &str, ts: f64) -> Value {
    json!({"role": "user", "row_id": id, "content": text, "timestamp": ts})
}

fn assistant(id: i64, ts: f64) -> Value {
    json!({"role": "assistant", "row_id": id, "content": "ok", "timestamp": ts,
        "tool_calls": [{"id": "1", "type": "function",
            "function": {"name": "patch", "arguments": "{}"}}]})
}

/// Three prompts (rows 1, 3, 5), each followed by an assistant row that patched a file.
fn convo() -> Vec<Value> {
    vec![
        user(1, "first", 1_792_670_000.0),
        assistant(2, 1_792_670_010.0),
        user(3, "second", 1_792_670_100.0),
        assistant(4, 1_792_670_110.0),
        user(5, "third", 1_792_674_100.0),
        assistant(6, 1_792_674_110.0),
    ]
}

fn first(count: usize) -> Vec<Value> {
    convo()[..count].to_vec()
}

fn session(rev: i64, generation: i64, running: bool, pending: usize, rows: Vec<Value>) -> Reply {
    Reply::SessionRead(SessionRead {
        raw: json!({"revision": rev, "execution_generation": generation, "messages": rows}),
        revision: rev,
        execution_generation: generation,
        running,
        pending,
        messages: rows,
    })
}

fn idle(rev: i64, generation: i64, rows: Vec<Value>) -> Reply {
    session(rev, generation, false, 0, rows)
}

/// Checkpoint 1 (14:02 +01:00, 13:02:11 UTC) lies inside the newest prompt's window.
/// Checkpoint 2 (13:00 +01:00, 12:00 UTC) lies inside the second prompt's window.
fn checkpoints() -> CheckpointList {
    CheckpointList::from_result(
        true,
        vec![
            (
                "bbbbbbbb11111111".into(),
                "2026-10-22T14:02:11+01:00".into(),
                "before patch".into(),
            ),
            (
                "aaaaaaaa22222222".into(),
                "2026-10-22T13:00:00+01:00".into(),
                "before write_file".into(),
            ),
        ],
    )
}

fn restored(history_removed: Option<i64>) -> Reply {
    Reply::Restored(RestoreOutcome {
        success: true,
        restored_to: Some("aaaaaaaa".into()),
        reason: Some("before write_file".into()),
        file: None,
        restored_files: Some(vec!["notes.txt".into()]),
        skipped_user_edits: None,
        skipped_oversize: None,
        failed_deletes: None,
        history_removed,
        error: None,
    })
}

fn mutated(revision: i64, count: u64, content: Option<Value>) -> Reply {
    Reply::Mutated(MutateOutcome {
        revision: Some(revision),
        rewound_count: count,
        target_content: content,
    })
}

fn refused(reason: &str) -> Reply {
    Reply::Failed(CallFailure::Refused {
        reason: reason.into(),
        field: None,
        detail: None,
        revision: Some(11),
        execution_generation: Some(6),
    })
}

fn lost() -> Reply {
    Reply::Failed(CallFailure::Unknown)
}

/// A state plus the newest ticket it issued, so replies can answer the call they belong to.
struct Harness {
    state: RewindState,
    /// Ticket of the newest call effect.
    ticket: Option<u64>,
}

impl Harness {
    fn new() -> Self {
        Self {
            state: RewindState::default(),
            ticket: None,
        }
    }

    fn send(&mut self, input: RewindInput) -> Vec<RewindEffect> {
        let effects = update(&mut self.state, input, NOW_MS);
        for effect in &effects {
            if let RewindEffect::Call { ticket, .. } = effect {
                self.ticket = Some(*ticket);
            }
        }
        effects
    }

    fn key(&mut self, key: RewindKey) -> Vec<RewindEffect> {
        self.send(RewindInput::Key(key))
    }

    fn open(&mut self) -> Vec<RewindEffect> {
        self.send(RewindInput::Open {
            turn_running: false,
        })
    }

    fn undo(&mut self, n: usize) -> Vec<RewindEffect> {
        self.send(RewindInput::Undo {
            n,
            turn_running: false,
        })
    }

    fn retry(&mut self) -> Vec<RewindEffect> {
        self.send(RewindInput::Retry {
            turn_running: false,
        })
    }

    fn typed_restore(&mut self, target: &str, file: Option<&str>, all: bool) -> Vec<RewindEffect> {
        self.send(RewindInput::TypedRestore {
            target: target.into(),
            file_path: file.map(String::from),
            all,
            turn_running: false,
        })
    }

    /// Answer the call of the last input with its own ticket.
    fn reply(&mut self, reply: Reply) -> Vec<RewindEffect> {
        let ticket = self.ticket();
        self.send(RewindInput::Reply { ticket, reply })
    }

    fn ticket(&self) -> u64 {
        self.ticket.expect("an input asked for a call")
    }

    fn screen(&self) -> RewindScreen {
        self.state.screen()
    }

    /// Open, read the given rows, list the given checkpoints: the Turns screen.
    fn at_turns(rows: Vec<Value>, list: CheckpointList) -> Self {
        let mut h = Self::new();
        h.open();
        h.reply(idle(5, 2, rows));
        h.reply(Reply::Checkpoints(list));
        assert_eq!(h.screen(), RewindScreen::Turns);
        h
    }

    /// Turns with the third prompt chosen and the conversation-only confirm showing.
    fn at_confirm_conversation() -> Self {
        let mut h = Self::at_turns(convo(), checkpoints());
        h.key(RewindKey::Enter);
        h.key(RewindKey::C);
        assert_eq!(h.screen(), RewindScreen::Confirm);
        h
    }

    /// Turns with a prompt chosen (0 is the newest) and the files confirm showing.
    fn at_confirm_files(down: usize) -> Self {
        let mut h = Self::at_turns(convo(), checkpoints());
        for _ in 0..down {
            h.key(RewindKey::Down);
        }
        h.key(RewindKey::Enter);
        h.key(RewindKey::F);
        assert_eq!(h.screen(), RewindScreen::Confirm);
        h
    }

    /// The files action confirmed and the confirm read answered: the restore is in flight.
    fn restoring(down: usize) -> Self {
        let mut h = Self::at_confirm_files(down);
        h.key(RewindKey::Enter);
        h.reply(idle(7, 4, convo()));
        h
    }
}

fn calls(effects: &[RewindEffect]) -> Vec<&Call> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            RewindEffect::Call { call, .. } => Some(call),
            _ => None,
        })
        .collect()
}

/// Every notice and every rebuild notice line, joined.
fn texts(effects: &[RewindEffect]) -> String {
    effects
        .iter()
        .flat_map(|effect| match effect {
            RewindEffect::Notice(_, text) => vec![text.clone()],
            RewindEffect::Rebuild { notice, .. } => notice.clone(),
            _ => Vec::new(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rebuilds(effects: &[RewindEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, RewindEffect::Rebuild { .. }))
        .count()
}

fn only_notice_kinds(effects: &[RewindEffect], kind: NoticeKind) -> bool {
    !effects.is_empty()
        && effects
            .iter()
            .all(|effect| matches!(effect, RewindEffect::Notice(found, _) if *found == kind))
}

fn body(h: &Harness) -> Vec<String> {
    h.state
        .view(NOW_MS)
        .body
        .iter()
        .map(|line| line.text.clone())
        .collect()
}

#[test]
fn open_refused_while_turn_runs() {
    let mut h = Harness::new();
    let effects = h.send(RewindInput::Open { turn_running: true });
    assert_eq!(
        effects,
        vec![RewindEffect::Notice(
            NoticeKind::Warning,
            "A reply is running. Stop it with Esc, then rewind.".into()
        )]
    );
    assert_eq!(h.screen(), RewindScreen::Closed);
    assert!(!h.state.is_open());
}

#[test]
fn open_reads_session_then_checkpoints() {
    let mut h = Harness::new();
    let effects = h.open();
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    assert_eq!(h.screen(), RewindScreen::Loading);
    assert!(h.state.is_open());
    assert_eq!(body(&h), vec!["Loading the conversation…"]);
    let effects = h.reply(idle(5, 2, convo()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::ListCheckpoints]
    ));
    let effects = h.reply(Reply::Checkpoints(checkpoints()));
    assert!(effects.is_empty());
    assert_eq!(h.screen(), RewindScreen::Turns);
    let view = h.state.view(NOW_MS);
    assert_eq!(view.selected, 0);
    let labels: Vec<&str> = view.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, ["#3", "#2", "#1"]);
    assert_eq!(view.rows[0].tail, "files");
    assert_eq!(view.rows[2].tail, "·");
}

#[test]
fn open_with_failed_list_shows_unavailable_footer() {
    let mut h = Harness::new();
    h.open();
    h.reply(idle(5, 2, convo()));
    let effects = h.reply(lost());
    assert!(effects.is_empty());
    assert_eq!(h.screen(), RewindScreen::Turns);
    assert!(h.state.view(NOW_MS).footer.contains("checkpoints unknown"));
    assert!(
        h.state
            .view(NOW_MS)
            .rows
            .iter()
            .all(|row| row.tail.is_empty())
    );
}

#[test]
fn open_with_no_prompts_says_nothing_to_rewind() {
    let mut h = Harness::new();
    h.open();
    // The table lists checkpoints first, then closes with T3 on the list reply.
    let effects = h.reply(idle(5, 2, vec![assistant(2, 1.0)]));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::ListCheckpoints]
    ));
    let effects = h.reply(Reply::Checkpoints(CheckpointList::Off));
    assert!(texts(&effects).contains("nothing to rewind"));
    assert!(calls(&effects).is_empty());
    assert_eq!(h.screen(), RewindScreen::Closed);
}

#[test]
fn selection_moves_clamp_and_page() {
    let rows: Vec<Value> = (1..=30).map(|id| user(id, "prompt", 1_000.0)).collect();
    let mut h = Harness::at_turns(rows, CheckpointList::Off);
    h.send(RewindInput::Viewport(10));
    assert_eq!(h.state.view(NOW_MS).selected, 0);
    h.key(RewindKey::Up);
    assert_eq!(h.state.view(NOW_MS).selected, 0);
    h.key(RewindKey::PageDown);
    assert_eq!(h.state.view(NOW_MS).selected, 10);
    h.key(RewindKey::End);
    assert_eq!(h.state.view(NOW_MS).selected, 29);
    h.key(RewindKey::Down);
    assert_eq!(h.state.view(NOW_MS).selected, 29);
    assert!(h.state.view(NOW_MS).first_visible + 10 > 29);
    h.key(RewindKey::PageUp);
    assert_eq!(h.state.view(NOW_MS).selected, 19);
    h.key(RewindKey::Home);
    let view = h.state.view(NOW_MS);
    assert_eq!((view.selected, view.first_visible), (0, 0));
}

#[test]
fn actions_depend_on_checkpoints() {
    let line_starting = |h: &Harness, prefix: &str| body(h).iter().any(|l| l.starts_with(prefix));
    let reason_of = |h: &Harness| -> String {
        body(h)
            .into_iter()
            .find(|l| l.starts_with("f  Restore files:"))
            .unwrap_or_default()
    };

    let mut h = Harness::at_turns(convo(), checkpoints());
    h.key(RewindKey::Enter);
    assert_eq!(h.screen(), RewindScreen::Actions);
    assert!(line_starting(&h, "c  "));
    assert!(line_starting(
        &h,
        "f  Also restore files to checkpoint bbbbbbbb"
    ));
    assert!(line_starting(&h, "d  "));

    for (list, reason) in [
        (CheckpointList::Off, "checkpoints are off"),
        (CheckpointList::Empty, "no file changes recorded"),
        (
            CheckpointList::Unavailable("x".into()),
            "checkpoints could not be listed",
        ),
    ] {
        let mut h = Harness::at_turns(convo(), list);
        h.key(RewindKey::Enter);
        assert!(reason_of(&h).contains(reason), "{reason}");
        assert!(!line_starting(&h, "d  "));
        assert!(h.key(RewindKey::F).is_empty());
        assert_eq!(h.screen(), RewindScreen::Actions);
        assert!(h.key(RewindKey::D).is_empty());
        assert_eq!(h.screen(), RewindScreen::Actions);
    }

    // The oldest prompt edited files, but the first checkpoint came after its turn ended.
    let mut h = Harness::at_turns(convo(), checkpoints());
    h.key(RewindKey::End);
    h.key(RewindKey::Enter);
    assert!(reason_of(&h).contains("recorded no checkpoint before this prompt's edits"));
    assert!(!line_starting(&h, "d  "));
}

#[test]
fn confirm_rewind_emits_one_fenced_mutation() {
    let mut h = Harness::at_confirm_conversation();
    let lines = body(&h);
    assert!(lines[0].starts_with("Remove prompt #3 and everything after it (2 messages)"));
    assert!(lines.contains(&"Files stay as they are.".to_string()));
    assert!(lines.contains(&"enter confirm · esc back".to_string()));
    let effects = h.key(RewindKey::Enter);
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    assert_eq!(h.screen(), RewindScreen::Working);
    assert!(h.state.is_open());
    assert_eq!(body(&h), vec!["Working…"]);
    let effects = h.reply(idle(7, 4, convo()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Mutate {
            row_id: 5,
            expected_revision: 7,
            expected_generation: 4
        }]
    ));
    let effects = h.reply(mutated(8, 2, Some(json!("third"))));
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(8, 5, first(4)));
    assert_eq!(effects.len(), 2);
    assert!(
        texts(&effects)
            .contains("Rewound to before prompt #3 (\"third\"). Hermes removed 2 messages.")
    );
    assert!(
        matches!(&effects[0], RewindEffect::Rebuild { snapshot, .. } if snapshot["revision"] == 8)
    );
    assert_eq!(
        effects[1],
        RewindEffect::ReturnPrompt {
            text: "third".into(),
            attachments: false
        }
    );
    assert!(!h.state.is_open());
}

#[test]
fn running_or_queued_snapshot_blocks_confirm() {
    // The loaded read shows two queued prompts: Enter sends nothing and the block line says so.
    let mut h = Harness::new();
    h.open();
    h.reply(session(5, 2, false, 2, convo()));
    h.reply(Reply::Checkpoints(CheckpointList::Off));
    h.key(RewindKey::Enter);
    h.key(RewindKey::C);
    assert!(h.key(RewindKey::Enter).is_empty());
    assert_eq!(h.screen(), RewindScreen::Confirm);
    let lines = body(&h);
    assert!(lines.iter().any(|l| l.contains("(2 queued)")));
    assert!(!lines.iter().any(|l| l.contains("enter confirm")));

    // The loaded read is clean but the confirm read shows a running reply: T15, no mutate.
    let mut h = Harness::at_confirm_conversation();
    h.key(RewindKey::Enter);
    let effects = h.reply(session(7, 4, true, 0, convo()));
    assert!(texts(&effects).contains("Hermes will not rewind while a reply is running"));
    assert!(calls(&effects).is_empty());
    assert_eq!(h.screen(), RewindScreen::Closed);

    // The confirm read shows queued prompts.
    let mut h = Harness::at_confirm_conversation();
    h.key(RewindKey::Enter);
    let effects = h.reply(session(7, 4, false, 1, convo()));
    assert!(texts(&effects).contains("will not rewind"));
    assert!(calls(&effects).is_empty());

    // The prompt is gone from the confirm read.
    let mut h = Harness::at_confirm_conversation();
    h.key(RewindKey::Enter);
    let effects = h.reply(idle(7, 4, first(4)));
    assert!(texts(&effects).contains("That prompt is no longer in the conversation"));
    assert!(calls(&effects).is_empty());

    // The target's message count changed: nothing sent, Confirm again with the new count.
    let mut h = Harness::at_confirm_conversation();
    h.key(RewindKey::Enter);
    let mut longer = convo();
    longer.push(assistant(7, 1_792_674_120.0));
    let effects = h.reply(idle(7, 4, longer.clone()));
    assert!(effects.is_empty());
    assert_eq!(h.screen(), RewindScreen::Confirm);
    let lines = body(&h);
    assert!(lines[0].contains("(3 messages)"));
    assert!(lines.iter().any(|l| l.contains("Check the numbers above")));
    h.key(RewindKey::Enter);
    let effects = h.reply(idle(7, 4, longer));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Mutate { row_id: 5, .. }]
    ));
}

#[test]
fn restore_then_reread_newest_turn() {
    // Chosen prompt is the newest: one restore, a re-read, and no mutate when its row is gone.
    let mut h = Harness::at_confirm_files(0);
    let effects = h.key(RewindKey::Enter);
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(7, 4, convo()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Restore { hash, file_path: None, all: false }] if hash == "bbbbbbbb11111111"
    ));
    let effects = h.reply(restored(Some(2)));
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(9, 5, first(4)));
    assert!(calls(&effects).is_empty());
    let text = texts(&effects);
    assert!(text.contains("Restored files to checkpoint"));
    assert!(text.contains("rewound to before prompt #3"));
    assert!(text.contains("Hermes removed 2 messages"));
    assert!(text.contains("Restored 1 file."));
    assert!(matches!(
        effects.last(),
        Some(RewindEffect::ReturnPrompt { text, .. }) if text == "third"
    ));
    assert_eq!(rebuilds(&effects), 1);

    // Hermes removed a newer carrier row only: the chosen row is still active, so one mutate.
    let mut h = Harness::restoring(0);
    let effects = h.reply(restored(Some(1)));
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(9, 5, convo()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Mutate {
            row_id: 5,
            expected_revision: 9,
            expected_generation: 5
        }]
    ));
}

#[test]
fn restore_and_rewind_older_turn_rereads_then_rewinds() {
    let mut h = Harness::restoring(1);
    let effects = h.reply(restored(Some(2)));
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(9, 5, first(4)));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Mutate {
            row_id: 3,
            expected_revision: 9,
            expected_generation: 5
        }]
    ));
    let effects = h.reply(mutated(10, 2, None));
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(10, 6, first(2)));
    let text = texts(&effects);
    assert!(text.contains("and rewound to before prompt #2"));
    assert!(text.contains("Hermes removed 2 messages"));
    assert!(matches!(
        effects.last(),
        Some(RewindEffect::ReturnPrompt { text, .. }) if text == "second"
    ));
}

#[test]
fn partial_failure_says_what_happened() {
    let mut h = Harness::restoring(1);
    h.reply(restored(Some(2)));
    let effects = h.reply(idle(9, 5, first(4)));
    assert!(matches!(calls(&effects).as_slice(), [Call::Mutate { .. }]));
    let effects = h.reply(refused("stale_generation"));
    let text = texts(&effects);
    assert!(text.contains("Restored files to checkpoint aaaaaaaa"));
    assert!(text.contains("rewinding further to prompt #2 failed"));
    assert!(text.contains("The session changed while you were choosing"));
    assert!(!h.state.is_open());

    // The after-restore read also lacks an older prompt: T32 names it.
    let mut h = Harness::restoring(0);
    h.reply(restored(Some(2)));
    let msgs = vec![convo()[0].clone(), convo()[1].clone(), convo()[4].clone()];
    let effects = h.reply(idle(9, 5, msgs));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Mutate { row_id: 5, .. }]
    ));
    let effects = h.reply(refused("stale_generation"));
    let text = texts(&effects);
    assert!(text.contains("rewinding further to prompt #3"));
    assert!(text.contains("Hermes's restore also removed prompt #2"));

    // The restore removed an older prompt and the chosen one: rebuild with T32.
    let mut h = Harness::restoring(0);
    h.reply(restored(Some(4)));
    let effects = h.reply(idle(9, 5, first(2)));
    assert!(texts(&effects).contains("Hermes's restore also removed prompt #2"));
    assert_eq!(rebuilds(&effects), 1);
}

#[test]
fn ambiguous_restore_refusal_text() {
    let mut h = Harness::restoring(0);
    let effects = h.reply(refused("session_busy"));
    let text = texts(&effects);
    assert!(text.contains("after the restore started"));
    assert!(!text.contains("Stop the reply first"));
    assert!(calls(&effects).is_empty());

    let mut h = Harness::restoring(0);
    let effects = h.reply(refused("not_found"));
    assert!(texts(&effects).contains("This session no longer exists"));

    // One file: a refusal never gives T21 or T22.
    let mut h = Harness::new();
    h.typed_restore("2", Some("a.txt"), false);
    h.reply(Reply::Checkpoints(checkpoints()));
    let effects = h.reply(refused("session_busy"));
    let text = texts(&effects);
    assert!(text.contains("Hermes refused to restore files (session_busy)."));
    assert!(!text.contains("after the restore started"));
    assert!(!text.contains("Stop the reply first"));
}

#[test]
fn restore_success_false_says_hermes_error() {
    let mut h = Harness::restoring(0);
    let effects = h.reply(Reply::Restored(RestoreOutcome {
        success: false,
        restored_to: None,
        reason: None,
        file: None,
        restored_files: None,
        skipped_user_edits: None,
        skipped_oversize: None,
        failed_deletes: None,
        history_removed: None,
        error: Some("Checkpoint not found".into()),
    }));
    assert_eq!(
        texts(&effects),
        "Hermes did not restore anything: Checkpoint not found"
    );
    assert!(calls(&effects).is_empty());
}

#[test]
fn unknown_outcomes_are_verified_or_reported() {
    let confirmed_rewind = || {
        let mut h = Harness::at_confirm_conversation();
        h.key(RewindKey::Enter);
        h.reply(idle(7, 4, convo()));
        h
    };

    // Unknown, then the row is gone and the generation rose: it went through.
    let mut h = confirmed_rewind();
    let effects = h.reply(lost());
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(8, 5, first(4)));
    let text = texts(&effects);
    assert!(text.contains("but the rewind went through"));
    assert!(text.contains("prompt #3 (\"third\") and everything after it"));
    assert_eq!(rebuilds(&effects), 1);
    assert!(matches!(
        effects.last(),
        Some(RewindEffect::ReturnPrompt { .. })
    ));

    // Unreadable, then the row is still active: it did not happen.
    let mut h = confirmed_rewind();
    let effects = h.reply(Reply::Failed(CallFailure::Unreadable));
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(7, 4, convo()));
    assert!(texts(&effects).contains("The rewind did not happen; nothing was changed."));
    assert_eq!(rebuilds(&effects), 0);

    // Row gone, generation unchanged: cannot tell.
    let mut h = confirmed_rewind();
    h.reply(lost());
    let effects = h.reply(idle(7, 4, first(4)));
    assert!(texts(&effects).contains("cannot tell whether this rewind removed it"));
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, RewindEffect::ReturnPrompt { .. }))
    );

    // The verify read itself fails.
    let mut h = confirmed_rewind();
    h.reply(lost());
    let effects = h.reply(lost());
    assert!(texts(&effects).contains("could not check the result"));

    // A one-file restore with a lost reply: T11, nothing else sent.
    let mut h = Harness::new();
    h.typed_restore("2", Some("a.txt"), false);
    let effects = h.reply(Reply::Checkpoints(checkpoints()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Restore { file_path: Some(path), all: false, .. }] if path == "a.txt"
    ));
    let effects = h.reply(lost());
    assert!(texts(&effects).contains("Run /diff to see the files now."));
    assert!(calls(&effects).is_empty());

    // A whole-folder restore with a lost reply: a read, then T11w (no rebuild if unchanged).
    let mut h = Harness::restoring(0);
    let effects = h.reply(lost());
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(7, 4, convo()));
    assert!(texts(&effects).contains("Hermes may also have removed the last exchange"));
    assert_eq!(rebuilds(&effects), 0);

    // The revision rose: rebuild with T11w and T31 naming the oldest gone prompt.
    let mut h = Harness::restoring(0);
    h.reply(lost());
    let effects = h.reply(idle(9, 5, first(2)));
    let text = texts(&effects);
    assert!(text.contains("Hermes may also have removed the last exchange"));
    assert!(text.contains("prompt #2 (\"second\")"));
    assert_eq!(rebuilds(&effects), 1);
}

#[test]
fn esc_backs_out_one_level() {
    let mut h = Harness::at_turns(convo(), checkpoints());
    h.key(RewindKey::Enter);
    let effects = h.key(RewindKey::D);
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::CheckpointDiff { hash }] if hash == "bbbbbbbb11111111"
    ));
    assert_eq!(h.screen(), RewindScreen::Diff);
    assert_eq!(body(&h), vec!["Comparing files…"]);
    h.reply(Reply::DiffLoaded(DiffDoc::Empty("none".into())));
    assert!(h.state.view(NOW_MS).diff.is_some());
    h.key(RewindKey::Esc);
    assert_eq!(h.screen(), RewindScreen::Actions);
    h.key(RewindKey::C);
    assert_eq!(h.screen(), RewindScreen::Confirm);
    h.key(RewindKey::Esc);
    assert_eq!(h.screen(), RewindScreen::Actions);
    h.key(RewindKey::Esc);
    assert_eq!(h.screen(), RewindScreen::Turns);
    h.key(RewindKey::Esc);
    assert_eq!(h.screen(), RewindScreen::Closed);

    // Esc in Working does nothing.
    let mut h = Harness::at_confirm_conversation();
    h.key(RewindKey::Enter);
    assert!(h.key(RewindKey::Esc).is_empty());
    assert_eq!(h.screen(), RewindScreen::Working);
    assert!(h.key(RewindKey::Close).is_empty());
    assert_eq!(h.screen(), RewindScreen::Working);

    // Esc while loading closes, and the late reply is dropped.
    let mut h = Harness::new();
    h.open();
    let stale = h.ticket();
    h.key(RewindKey::Esc);
    assert_eq!(h.screen(), RewindScreen::Closed);
    let effects = h.send(RewindInput::Reply {
        ticket: stale,
        reply: idle(1, 1, convo()),
    });
    assert!(effects.is_empty());
    assert_eq!(h.screen(), RewindScreen::Closed);

    // A reply with the wrong ticket changes nothing.
    let mut h = Harness::new();
    h.open();
    let wrong = h.ticket() + 1;
    let effects = h.send(RewindInput::Reply {
        ticket: wrong,
        reply: idle(1, 1, convo()),
    });
    assert!(effects.is_empty());
    assert_eq!(h.screen(), RewindScreen::Loading);

    // Ctrl+C closes from every screen but Working.
    let mut h = Harness::at_turns(convo(), checkpoints());
    h.key(RewindKey::Enter);
    h.key(RewindKey::Close);
    assert_eq!(h.screen(), RewindScreen::Closed);

    // A slow diff: Esc goes back and the late diff reply is dropped.
    let mut h = Harness::at_turns(convo(), checkpoints());
    h.key(RewindKey::Enter);
    h.key(RewindKey::D);
    let late = h.ticket();
    h.key(RewindKey::Esc);
    let effects = h.send(RewindInput::Reply {
        ticket: late,
        reply: Reply::DiffLoaded(DiffDoc::Off),
    });
    assert!(effects.is_empty());
    assert_eq!(h.screen(), RewindScreen::Actions);
    assert!(h.state.view(NOW_MS).diff.is_none());
}

#[test]
fn read_failures_close_with_text() {
    let failures = [
        (CallFailure::Unknown, "did not answer in time"),
        (CallFailure::Unreadable, "reply could not be read"),
        (CallFailure::NotSent("x".into()), "Not connected to Hermes"),
    ];
    for (failure, expected) in failures {
        let starts: [fn(&mut Harness) -> Vec<RewindEffect>; 3] = [
            |h| h.open(),
            |h| h.send(RewindInput::OpenCheckpoints),
            |h| {
                h.send(RewindInput::OpenDiff(DiffSource::Session {
                    stat_only: false,
                }))
            },
        ];
        for start in starts {
            let mut h = Harness::new();
            start(&mut h);
            let effects = h.reply(Reply::Failed(failure.clone()));
            assert!(texts(&effects).contains(expected), "{expected}");
            assert!(only_notice_kinds(&effects, NoticeKind::Warning));
            assert_eq!(h.screen(), RewindScreen::Closed);
        }
        // A checkpoint diff opened from the Actions screen.
        let mut h = Harness::at_turns(convo(), checkpoints());
        h.key(RewindKey::Enter);
        h.key(RewindKey::D);
        let effects = h.reply(Reply::Failed(failure.clone()));
        assert!(texts(&effects).contains(expected));
        assert_eq!(h.screen(), RewindScreen::Closed);
    }

    // A refusal reads through the refusal table.
    let mut h = Harness::new();
    h.open();
    let effects = h.reply(refused("permission_denied"));
    assert!(texts(&effects).contains("may not read the session (permission_denied)"));
    let mut h = Harness::new();
    h.send(RewindInput::OpenDiff(DiffSource::Checkpoint("99".into())));
    h.reply(Reply::Checkpoints(checkpoints()));
    let effects = h.reply(Reply::Failed(CallFailure::Refused {
        reason: "invalid_checkpoint".into(),
        field: None,
        detail: Some("99".into()),
        revision: None,
        execution_generation: None,
    }));
    assert!(texts(&effects).contains("There is no checkpoint 99."));
}

/// A rewind whose rebuild read failed: the flag for a reload is set.
fn failed_rebuild() -> Harness {
    let mut h = Harness::new();
    h.undo(1);
    h.reply(idle(7, 4, convo()));
    h.reply(mutated(8, 2, None));
    let effects = h.reply(lost());
    let text = texts(&effects);
    assert!(text.contains("could not reload it"), "{text}");
    assert!(matches!(
        effects.last(),
        Some(RewindEffect::ReturnPrompt { text, .. }) if text == "third"
    ));
    assert_eq!(rebuilds(&effects), 0);
    h
}

#[test]
fn rebuild_read_failure_sets_reload_pending() {
    // Open: the next read rebuilds with the reload marker first, then asks for the list.
    let mut h = failed_rebuild();
    h.open();
    let effects = h.reply(idle(8, 5, first(4)));
    assert!(matches!(&effects[0], RewindEffect::Rebuild { notice, .. }
        if notice.len() == 1 && notice[0].contains("reloaded the conversation")));
    assert!(matches!(
        &effects[1],
        RewindEffect::Call {
            call: Call::ListCheckpoints,
            ..
        }
    ));
    // The flag is clear afterwards.
    h.key(RewindKey::Esc);
    h.open();
    let effects = h.reply(idle(8, 5, first(4)));
    assert_eq!(rebuilds(&effects), 0);

    // Undo: the confirm read rebuilds first, then mutates.
    let mut h = failed_rebuild();
    h.undo(1);
    let effects = h.reply(idle(8, 5, first(4)));
    assert!(matches!(&effects[0], RewindEffect::Rebuild { notice, .. }
        if notice[0].contains("reloaded the conversation")));
    assert!(matches!(
        &effects[1],
        RewindEffect::Call {
            call: Call::Mutate { .. },
            ..
        }
    ));

    // Retry.
    let mut h = failed_rebuild();
    h.retry();
    let effects = h.reply(idle(8, 5, first(4)));
    assert!(matches!(&effects[0], RewindEffect::Rebuild { .. }));
    assert!(matches!(
        &effects[1],
        RewindEffect::Call {
            call: Call::Mutate { .. },
            ..
        }
    ));

    // A typed whole-folder restore.
    let mut h = failed_rebuild();
    h.typed_restore("1", None, false);
    h.reply(Reply::Checkpoints(checkpoints()));
    let effects = h.reply(idle(8, 5, first(4)));
    assert!(matches!(&effects[0], RewindEffect::Rebuild { .. }));
    assert!(matches!(
        &effects[1],
        RewindEffect::Call {
            call: Call::Restore { .. },
            ..
        }
    ));

    // A rebuild that serves the flag adds no second marker.
    let mut h = failed_rebuild();
    h.undo(1);
    h.reply(idle(8, 5, first(4)));
    h.reply(mutated(9, 2, None));
    let effects = h.reply(idle(9, 6, first(2)));
    assert_eq!(rebuilds(&effects), 1);
    assert!(!texts(&effects).contains("reloaded the conversation"));
}

#[test]
fn undo_and_retry_targets() {
    // Undo 2 targets the second newest.
    let mut h = Harness::new();
    let effects = h.undo(2);
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    assert!(!h.state.is_open());
    let effects = h.reply(idle(7, 4, convo()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Mutate {
            row_id: 3,
            expected_revision: 7,
            expected_generation: 4
        }]
    ));
    h.reply(mutated(8, 4, None));
    let effects = h.reply(idle(8, 5, first(2)));
    assert!(texts(&effects).contains("Rewound to before prompt #2 (\"second\")"));
    assert!(matches!(
        effects.last(),
        Some(RewindEffect::ReturnPrompt { text, .. }) if text == "second"
    ));

    // Undo 5 with three prompts.
    let mut h = Harness::new();
    h.undo(5);
    let effects = h.reply(idle(7, 4, convo()));
    assert_eq!(texts(&effects), "There are only 3 prompts to undo.");
    assert!(calls(&effects).is_empty());
    let mut h = Harness::new();
    h.undo(1);
    let effects = h.reply(idle(7, 4, vec![]));
    assert_eq!(texts(&effects), "There is no prompt to undo.");

    // Retry with an image prompt.
    let image = json!({"role": "user", "row_id": 9, "timestamp": 1.0,
        "content": [{"type": "text", "text": "x"}, {"type": "image_url", "image_url": {"url": "u"}}]});
    let mut h = Harness::new();
    h.retry();
    let effects = h.reply(idle(7, 4, vec![image]));
    assert!(texts(&effects).contains("/retry resends text only"));
    assert!(calls(&effects).is_empty());

    // Retry with no prompt, and with an empty prompt.
    let mut h = Harness::new();
    h.retry();
    let effects = h.reply(idle(7, 4, vec![]));
    assert!(texts(&effects).contains("Nothing to retry"));
    let mut h = Harness::new();
    h.retry();
    let effects = h.reply(idle(7, 4, vec![user(9, "", 1.0)]));
    assert!(texts(&effects).contains("Nothing to retry"));
    assert!(calls(&effects).is_empty());

    // Retry succeeds: rebuild then submit, preferring the result's content.
    let mut h = Harness::new();
    h.retry();
    let effects = h.reply(idle(7, 4, convo()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Mutate { row_id: 5, .. }]
    ));
    h.reply(mutated(
        8,
        2,
        Some(json!([{"type": "text", "text": "live ask"}])),
    ));
    let effects = h.reply(idle(8, 5, first(4)));
    assert_eq!(effects.len(), 2);
    assert!(matches!(&effects[0], RewindEffect::Rebuild { .. }));
    assert_eq!(effects[1], RewindEffect::Submit("live ask".into()));

    // Retry falls back to the snapshot's text when the content is not lossless.
    let mut h = Harness::new();
    h.retry();
    h.reply(idle(7, 4, convo()));
    h.reply(mutated(8, 2, None));
    let effects = h.reply(idle(8, 5, first(4)));
    assert_eq!(effects[1], RewindEffect::Submit("third".into()));
}

#[test]
fn rewound_prompt_is_returned_once() {
    let returns = |effects: &[RewindEffect]| {
        effects
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    RewindEffect::ReturnPrompt { .. } | RewindEffect::Submit(_)
                )
            })
            .count()
    };

    // Overlay c.
    let mut h = Harness::at_confirm_conversation();
    h.key(RewindKey::Enter);
    h.reply(idle(7, 4, convo()));
    h.reply(mutated(8, 2, None));
    let effects = h.reply(idle(8, 5, first(4)));
    assert_eq!(returns(&effects), 1);

    // Overlay f, row gone at the end.
    let mut h = Harness::restoring(0);
    h.reply(restored(Some(2)));
    let effects = h.reply(idle(9, 5, first(4)));
    assert_eq!(returns(&effects), 1);

    // A prompt with an image: text parts return and the attachments flag is set.
    let image = json!({"role": "user", "row_id": 9, "timestamp": 1.0,
        "content": [{"type": "text", "text": "look"}, {"type": "image_url", "image_url": {"url": "u"}}]});
    let mut h = Harness::new();
    h.undo(1);
    h.reply(idle(7, 4, vec![image]));
    h.reply(mutated(8, 1, None));
    let effects = h.reply(idle(8, 5, vec![]));
    assert_eq!(
        effects.last(),
        Some(&RewindEffect::ReturnPrompt {
            text: "look".into(),
            attachments: true
        })
    );

    // Lost reply, row gone: ReturnPrompt (a retry returns it too, never submits it).
    let mut h = Harness::new();
    h.retry();
    h.reply(idle(7, 4, convo()));
    h.reply(lost());
    let effects = h.reply(idle(8, 5, first(4)));
    assert_eq!(returns(&effects), 1);
    assert!(matches!(
        effects.last(),
        Some(RewindEffect::ReturnPrompt { .. })
    ));

    // Retry submits and never returns.
    let mut h = Harness::new();
    h.retry();
    h.reply(idle(7, 4, convo()));
    h.reply(mutated(8, 2, None));
    let effects = h.reply(idle(8, 5, first(4)));
    assert_eq!(returns(&effects), 1);
    assert!(matches!(effects.last(), Some(RewindEffect::Submit(_))));

    // Checkpoints r, typed whole-folder and one-file restores return nothing.
    let mut h = Harness::new();
    h.send(RewindInput::OpenCheckpoints);
    h.reply(Reply::Checkpoints(checkpoints()));
    h.key(RewindKey::R);
    h.key(RewindKey::Enter);
    h.reply(idle(7, 4, convo()));
    h.reply(restored(Some(2)));
    let effects = h.reply(idle(9, 5, first(4)));
    assert_eq!(returns(&effects), 0);
    let mut h = Harness::new();
    h.typed_restore("1", None, false);
    h.reply(Reply::Checkpoints(checkpoints()));
    h.reply(idle(7, 4, convo()));
    h.reply(restored(Some(2)));
    let effects = h.reply(idle(9, 5, first(4)));
    assert_eq!(returns(&effects), 0);
    let mut h = Harness::new();
    h.typed_restore("1", Some("a.txt"), false);
    h.reply(Reply::Checkpoints(checkpoints()));
    let effects = h.reply(restored(None));
    assert_eq!(returns(&effects), 0);
}

#[test]
fn session_updated_from_elsewhere_rebuilds_once() {
    let frame = |operation: &str, revision: i64| RewindInput::SessionUpdated {
        operation: operation.into(),
        revision: Some(revision),
        rewound_count: Some(3),
    };

    // Our own rewind records revision 8; its frame is then not above it.
    let mut h = Harness::new();
    h.undo(1);
    h.reply(idle(7, 4, convo()));
    h.reply(mutated(8, 2, None));
    h.reply(idle(8, 5, first(4)));
    assert!(h.send(frame("rewind", 8)).is_empty());

    // A frame from elsewhere with revision 9 reads, then rebuilds with the other-window marker.
    let effects = h.send(frame("rewind", 9));
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    assert!(!h.state.is_open());
    let effects = h.reply(idle(9, 6, first(2)));
    assert_eq!(rebuilds(&effects), 1);
    assert!(texts(&effects).contains("rewound in another window: Hermes removed 3 messages"));
    // The same revision again: nothing. Another operation: nothing.
    assert!(h.send(frame("rewind", 9)).is_empty());
    assert!(h.send(frame("rename", 12)).is_empty());
    assert!(h.send(frame("workspace", 13)).is_empty());

    // While Working the frame is ignored.
    let mut h = Harness::at_confirm_conversation();
    h.key(RewindKey::Enter);
    assert!(h.send(frame("rewind", 20)).is_empty());
    h.reply(idle(21, 4, convo()));
    h.reply(mutated(22, 2, None));
    // After the restore reply and before the read, also ignored; after a read at 22 a frame 22 is stale.
    assert!(h.send(frame("rewind", 22)).is_empty());
    h.reply(idle(22, 5, first(4)));
    assert!(h.send(frame("rewind", 22)).is_empty());

    // An open Turns overlay closes first, then rebuilds.
    let mut h = Harness::at_turns(convo(), checkpoints());
    assert!(h.state.is_open());
    let effects = h.send(frame("rewind", 9));
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    assert!(!h.state.is_open());
    h.reply(idle(9, 6, first(2)));
    assert_eq!(h.screen(), RewindScreen::Closed);

    // A failed read gives the reload notice.
    let mut h = Harness::new();
    h.send(frame("rewind", 9));
    let effects = h.reply(lost());
    assert!(texts(&effects).contains("could not reload it"));
}

#[test]
fn typed_commands_check_turn_and_list_first() {
    // Undo and Retry while a turn runs.
    for input in [
        RewindInput::Undo {
            n: 1,
            turn_running: true,
        },
        RewindInput::Retry { turn_running: true },
    ] {
        let mut h = Harness::new();
        let effects = h.send(input);
        assert_eq!(
            texts(&effects),
            "A reply is running. Stop it with Esc, then rewind."
        );
        assert!(calls(&effects).is_empty());
    }

    // A whole-folder restore while a turn runs.
    let mut h = Harness::new();
    let effects = h.send(RewindInput::TypedRestore {
        target: "1".into(),
        file_path: None,
        all: false,
        turn_running: true,
    });
    assert!(texts(&effects).contains("only between turns"));
    assert!(calls(&effects).is_empty());

    // Off gives C1 and sends nothing more.
    let mut h = Harness::new();
    let effects = h.typed_restore("1", None, false);
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::ListCheckpoints]
    ));
    let effects = h.reply(Reply::Checkpoints(CheckpointList::Off));
    assert!(texts(&effects).contains("Checkpoints are off for this session"));
    assert!(calls(&effects).is_empty());

    // A whole-folder restore: queued prompts give T23, a running reply gives T21.
    let mut h = Harness::new();
    h.typed_restore("1", None, false);
    let effects = h.reply(Reply::Checkpoints(checkpoints()));
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(session(7, 4, false, 1, convo()));
    assert!(texts(&effects).contains("while prompts are queued"));
    let mut h = Harness::new();
    h.typed_restore("1", None, false);
    h.reply(Reply::Checkpoints(checkpoints()));
    let effects = h.reply(session(7, 4, true, 0, convo()));
    assert!(texts(&effects).contains("only between turns"));

    // Idle: Restore with the typed target, exactly as typed.
    let mut h = Harness::new();
    h.typed_restore("2", None, true);
    h.reply(Reply::Checkpoints(checkpoints()));
    let effects = h.reply(idle(7, 4, convo()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Restore { hash, file_path: None, all: true }] if hash == "2"
    ));
    assert!(!h.state.is_open());

    // One file: no session read, and a running turn does not stop it.
    let mut h = Harness::new();
    let effects = h.send(RewindInput::TypedRestore {
        target: "2".into(),
        file_path: Some("src/a  b.rs".into()),
        all: false,
        turn_running: true,
    });
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::ListCheckpoints]
    ));
    let effects = h.reply(Reply::Checkpoints(checkpoints()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Restore { hash, file_path: Some(path), all: false }]
            if hash == "2" && path == "src/a  b.rs"
    ));
    let effects = h.reply(restored(None));
    assert!(only_notice_kinds(&effects, NoticeKind::System));
    assert_eq!(h.screen(), RewindScreen::Closed);

    // OpenDiff of a checkpoint lists first; Off gives C1.
    let mut h = Harness::new();
    let effects = h.send(RewindInput::OpenDiff(DiffSource::Checkpoint("2".into())));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::ListCheckpoints]
    ));
    let effects = h.reply(Reply::Checkpoints(CheckpointList::Off));
    assert!(texts(&effects).contains("Checkpoints are off"));
    let mut h = Harness::new();
    h.send(RewindInput::OpenDiff(DiffSource::Checkpoint("2".into())));
    let effects = h.reply(Reply::Checkpoints(checkpoints()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::CheckpointDiff { hash }] if hash == "2"
    ));

    // The session diff carries the --stat choice onto the loaded document.
    let mut h = Harness::new();
    let effects = h.send(RewindInput::OpenDiff(DiffSource::Session {
        stat_only: true,
    }));
    assert!(matches!(calls(&effects).as_slice(), [Call::SessionDiff]));
    h.reply(Reply::DiffLoaded(DiffDoc::Text {
        title: "t".into(),
        subtitle: None,
        stat: "s".into(),
        diff: "d".into(),
        stat_only: false,
        cut_note: None,
    }));
    assert!(matches!(
        h.state.view(NOW_MS).diff,
        Some(DiffDoc::Text {
            stat_only: true,
            ..
        })
    ));
    h.key(RewindKey::Esc);
    assert_eq!(h.screen(), RewindScreen::Closed);
}

#[test]
fn checkpoints_overlay_restore_without_a_chosen_prompt() {
    let to_confirm = || {
        let mut h = Harness::new();
        h.send(RewindInput::OpenCheckpoints);
        h.reply(Reply::Checkpoints(checkpoints()));
        assert_eq!(h.screen(), RewindScreen::Checkpoints);
        assert_eq!(h.state.view(NOW_MS).rows.len(), 2);
        h.key(RewindKey::Down);
        h.key(RewindKey::R);
        assert_eq!(h.screen(), RewindScreen::CheckpointConfirm);
        h
    };

    let mut h = to_confirm();
    let effects = h.key(RewindKey::Enter);
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(7, 4, convo()));
    assert!(matches!(
        calls(&effects).as_slice(),
        [Call::Restore { hash, file_path: None, all: false }] if hash == "aaaaaaaa22222222"
    ));
    let effects = h.reply(restored(Some(2)));
    assert!(matches!(calls(&effects).as_slice(), [Call::ReadSession]));
    let effects = h.reply(idle(9, 5, first(4)));
    assert_eq!(effects.len(), 1);
    assert!(matches!(&effects[0], RewindEffect::Rebuild { notice, .. }
        if notice.iter().all(|l| !l.contains("rewound to before prompt"))
            && notice.iter().any(|l| l.contains("Restored files to checkpoint"))));

    // With nothing removed from the conversation: notices only.
    let mut h = to_confirm();
    h.key(RewindKey::Enter);
    h.reply(idle(7, 4, convo()));
    h.reply(restored(Some(0)));
    let effects = h.reply(idle(7, 4, convo()));
    assert!(only_notice_kinds(&effects, NoticeKind::System));

    // Esc steps back to the list.
    let mut h = to_confirm();
    h.key(RewindKey::Esc);
    assert_eq!(h.screen(), RewindScreen::Checkpoints);
    h.key(RewindKey::Esc);
    assert_eq!(h.screen(), RewindScreen::Closed);

    // Off and Empty close with their texts.
    let mut h = Harness::new();
    h.send(RewindInput::OpenCheckpoints);
    let effects = h.reply(Reply::Checkpoints(CheckpointList::Empty));
    assert!(texts(&effects).contains("No checkpoints yet."));
    let mut h = Harness::new();
    h.send(RewindInput::OpenCheckpoints);
    let effects = h.reply(Reply::Checkpoints(CheckpointList::Off));
    assert!(texts(&effects).contains("Checkpoints are off for this session"));
}
