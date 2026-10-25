//! Rewind state machine. Issue 233. Checkpoint rewind and diff.
//!
//! Every test starts from `RewindState::default()` and reaches the screen it tests through real
//! inputs. Each reply carries the ticket of the `Call` effect just emitted, because the machine
//! ignores a reply whose ticket is not the one it awaits. User-visible texts are typed by hand.

use serde_json::{Value, json};
use talaria_core::rewind::{
    Call, CallFailure, Checkpoint, CheckpointList, DOUBLE_ESC_WINDOW_MS, DiffDoc, DiffSource,
    DoubleEsc, LineStyle, MutateOutcome, Reply, RestoreOutcome, RewindEffect, RewindInput,
    RewindKey, RewindScreen, RewindState, SessionRead, ViewLine, update,
};
use talaria_core::session::NoticeKind;

const T1: &str = "A reply is running. Stop it with Esc, then rewind.";
const T3: &str = "There is nothing to rewind: this conversation has no prompts yet.";
const T9: &str = "Talaria lost Hermes's reply, but the rewind went through.";
const T10: &str = "Talaria lost Hermes's reply. The rewind did not happen; nothing was changed.";
const T11: &str = "Talaria lost Hermes's reply, so the restore may or may not have happened. Run /diff to see the files now.";
const T11W: &str = "Talaria lost Hermes's reply, so the restore may or may not have happened, and Hermes may also have removed the last exchange. Run /diff to see the files and /rewind to see the conversation.";
const T13: &str = "The session changed while you were choosing (a new turn, or another window). Nothing was rewound. Open /rewind again.";
const T15: &str = "Hermes will not rewind while a reply is running or prompts are queued. Stop the reply or clear the queue, then try again.";
const T21: &str = "Hermes restores the whole folder only between turns. Stop the reply first, or restore one file with /rollback <n> <file>.";
const T23: &str = "Hermes will not restore the whole folder while prompts are queued: it would restore the files but could not rewind the conversation. Clear the queue first.";
const T27: &str = "Nothing to retry: this conversation has no prompts yet.";
const T28: &str = "/retry resends text only, and the last prompt has attachments. Use /rewind and send it again instead.";
const T34: &str = "── Talaria reloaded the conversation. Lines above that are not in it any more were removed by an earlier rewind. ──";
const T36: &str = "The conversation changed since this screen opened. Check the numbers above, then press Enter again.";
const C1: &str = "Checkpoints are off for this session, so Hermes keeps no file snapshots to compare or restore. They are turned on with checkpoints.enabled: true in the profile's config.yaml.";
const C2: &str = "No checkpoints yet. Hermes takes one just before the agent writes or patches a file, or runs a destructive command.";
const C11: &str = "Comparing files…";
const F1: &str = "Not connected to Hermes; nothing was sent.";
const F2: &str = "Hermes's reply could not be read; nothing was changed.";
const F3: &str = "Hermes did not answer in time; nothing was changed. Try again.";
const F4: &str = "Talaria lost Hermes's reply and could not check the result. Open /rewind to see the conversation.";
const R_NOT_FOUND: &str = "This session no longer exists on the gateway.";

fn t4(k: usize, preview: &str, n: u64) -> String {
    format!(
        "── Rewound to before prompt #{k} (\"{preview}\"). Hermes removed {n} messages. The lines above from that prompt on are no longer part of the conversation. Files were not changed. ──"
    )
}

fn t4f(short: &str, message: &str, k: usize, preview: &str, n: u64) -> String {
    format!(
        "── Restored files to checkpoint {short} ({message}) and rewound to before prompt #{k} (\"{preview}\"). Hermes removed {n} messages. The lines above from that prompt on are no longer part of the conversation. ──"
    )
}

fn t22(reason: &str) -> String {
    format!(
        "Hermes reported {reason} after the restore started. It may have restored the files without rewinding the conversation. Run /diff to check the files and /rewind to check the conversation."
    )
}

fn t29(n: u64) -> String {
    format!(
        "── This session was rewound in another window: Hermes removed {n} messages. Lines above from that point on are no longer part of the conversation. ──"
    )
}

fn t31(k: usize, preview: &str) -> String {
    format!(
        "── The conversation changed: prompt #{k} (\"{preview}\") and everything after it are no longer part of it. ──"
    )
}

fn t32(j: usize, preview: &str) -> String {
    format!(
        "Hermes's restore also removed prompt #{j} (\"{preview}\"), which is older than the prompt you chose, because Hermes picks the newest turn by its own rule. Run /rewind to see the conversation."
    )
}

fn f5(k: usize) -> String {
    format!(
        "Talaria lost Hermes's reply. Prompt #{k} is no longer in the conversation, but Talaria cannot tell whether this rewind removed it. Open /rewind to see the conversation."
    )
}

// Inputs and effects.

fn open(running: bool) -> RewindInput {
    RewindInput::Open {
        turn_running: running,
    }
}

fn step(state: &mut RewindState, input: RewindInput) -> Vec<RewindEffect> {
    update(state, input, 0)
}

fn key(state: &mut RewindState, key: RewindKey) -> Vec<RewindEffect> {
    step(state, RewindInput::Key(key))
}

fn frame(operation: &str, revision: i64, rewound_count: Option<u64>) -> RewindInput {
    RewindInput::SessionUpdated {
        operation: operation.into(),
        revision: Some(revision),
        rewound_count,
    }
}

/// The one `Call` effect in `effects`, with its ticket.
fn call(effects: &[RewindEffect]) -> (u64, Call) {
    let found: Vec<(u64, Call)> = effects
        .iter()
        .filter_map(|effect| match effect {
            RewindEffect::Call { ticket, call } => Some((*ticket, call.clone())),
            _ => None,
        })
        .collect();
    match <[(u64, Call); 1]>::try_from(found) {
        Ok([one]) => one,
        Err(found) => panic!("expected exactly one Call in {effects:?}, found {found:?}"),
    }
}

/// Assert the one `Call` in `effects` is `want`; return its ticket.
fn expect_call(effects: &[RewindEffect], want: Call) -> u64 {
    let (ticket, got) = call(effects);
    assert_eq!(got, want, "wrong call in {effects:?}");
    ticket
}

/// Assert `effects` is exactly one `Call`, equal to `want`; return its ticket.
fn expect_only_call(effects: &[RewindEffect], want: Call) -> u64 {
    assert_eq!(effects.len(), 1, "expected only a call, got {effects:?}");
    expect_call(effects, want)
}

fn reply(ticket: u64, reply: Reply) -> RewindInput {
    RewindInput::Reply { ticket, reply }
}

/// Send `reply` with the ticket of the call in `effects`.
fn answer(state: &mut RewindState, effects: &[RewindEffect], reply: Reply) -> Vec<RewindEffect> {
    let (ticket, _) = call(effects);
    step(state, self::reply(ticket, reply))
}

fn fail(
    state: &mut RewindState,
    effects: &[RewindEffect],
    failure: CallFailure,
) -> Vec<RewindEffect> {
    answer(state, effects, Reply::Failed(failure))
}

fn refused(reason: &str) -> CallFailure {
    CallFailure::Refused {
        reason: reason.into(),
        field: None,
        detail: None,
        revision: None,
        execution_generation: None,
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

fn texts(effects: &[RewindEffect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            RewindEffect::Notice(_, text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Assert `effects` is exactly one notice with `text`.
fn assert_one_notice(effects: &[RewindEffect], text: &str) {
    match effects {
        [RewindEffect::Notice(_, got)] => assert_eq!(got, text),
        other => panic!("expected one notice {text:?}, got {other:?}"),
    }
}

/// The notice lines of the one `Rebuild` in `effects`.
fn rebuild_notice(effects: &[RewindEffect]) -> Vec<String> {
    let found: Vec<&Vec<String>> = effects
        .iter()
        .filter_map(|effect| match effect {
            RewindEffect::Rebuild { notice, .. } => Some(notice),
            _ => None,
        })
        .collect();
    match found.as_slice() {
        [notice] => (*notice).clone(),
        _ => panic!("expected exactly one Rebuild in {effects:?}"),
    }
}

/// The `ReturnPrompt` and `Submit` effects, as `(text, attachments)` and `text`. A path never
/// gives both.
fn prompt_effects(effects: &[RewindEffect]) -> (Vec<(String, bool)>, Vec<String>) {
    let returns: Vec<(String, bool)> = effects
        .iter()
        .filter_map(|effect| match effect {
            RewindEffect::ReturnPrompt { text, attachments } => Some((text.clone(), *attachments)),
            _ => None,
        })
        .collect();
    let submits: Vec<String> = effects
        .iter()
        .filter_map(|effect| match effect {
            RewindEffect::Submit(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        returns.is_empty() || submits.is_empty(),
        "a path returns the prompt or submits it, never both: {effects:?}"
    );
    (returns, submits)
}

fn body(state: &RewindState) -> Vec<ViewLine> {
    state.view(0).body
}

fn body_has(state: &RewindState, needle: &str) -> bool {
    body(state).iter().any(|line| line.text.contains(needle))
}

// Messages.

fn user(id: i64, text: &str) -> Value {
    json!({"role": "user", "row_id": id, "content": text, "timestamp": 1_000.0 + id as f64})
}

fn user_with_image(id: i64, text: &str) -> Value {
    json!({
        "role": "user",
        "row_id": id,
        "timestamp": 1_000.0 + id as f64,
        "content": [
            {"type": "input_text", "text": text},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
        ]
    })
}

fn assistant(id: i64) -> Value {
    json!({"role": "assistant", "row_id": id, "content": "done", "timestamp": 1_000.0 + id as f64})
}

fn assistant_edit(id: i64, tool: &str) -> Value {
    json!({
        "role": "assistant",
        "row_id": id,
        "content": "done",
        "timestamp": 1_000.0 + id as f64,
        "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": tool, "arguments": "{}"}}]
    })
}

/// Two prompts that each edited a file: row 1 and row 4. The edits are stored at 1002 and 1005.
fn edit_session() -> Vec<Value> {
    vec![
        user(1, "older"),
        assistant_edit(2, "write_file"),
        user(4, "chosen"),
        assistant_edit(5, "patch"),
    ]
}

/// Three prompts that each edited a file: rows 1, 4 and 7.
fn triple_session() -> Vec<Value> {
    vec![
        user(1, "first"),
        assistant_edit(2, "write_file"),
        user(4, "second"),
        assistant_edit(5, "patch"),
        user(7, "third"),
        assistant_edit(8, "patch"),
    ]
}

/// Prompt row 3 is the one a conversation rewind targets in the lost-reply tests.
fn verify_session() -> Vec<Value> {
    vec![user(1, "older"), assistant(2), user(3, "prompt")]
}

fn without_chosen() -> Vec<Value> {
    vec![user(1, "older"), assistant_edit(2, "write_file")]
}

// Session reads.

fn raw_snapshot(revision: i64, generation: i64, messages: &[Value]) -> Value {
    json!({"revision": revision, "execution_generation": generation, "messages": messages})
}

fn busy_read(
    revision: i64,
    generation: i64,
    running: bool,
    pending: usize,
    messages: Vec<Value>,
) -> Reply {
    Reply::SessionRead(SessionRead {
        raw: raw_snapshot(revision, generation, &messages),
        revision,
        execution_generation: generation,
        running,
        pending,
        messages,
    })
}

/// A read of an idle session with nothing queued.
fn read(revision: i64, generation: i64, messages: Vec<Value>) -> Reply {
    busy_read(revision, generation, false, 0, messages)
}

// Checkpoints. Times line up with `edit_session`: 1002 and 1005 are the edits.

fn full_hash(fill: char) -> String {
    fill.to_string().repeat(40)
}

fn checkpoint(index: usize, fill: char, message: &str, time: i64) -> Checkpoint {
    Checkpoint {
        index,
        hash: full_hash(fill),
        short: fill.to_string().repeat(8),
        message: message.into(),
        raw_time: "2026-10-22T14:02:11+01:00".into(),
        time: Some(time),
    }
}

fn newest_and_oldest() -> Vec<Checkpoint> {
    vec![
        checkpoint(1, 'b', "before patch", 1005),
        checkpoint(2, 'a', "before write_file", 1002),
    ]
}

fn three_checkpoints() -> Vec<Checkpoint> {
    vec![
        checkpoint(1, 'c', "before patch", 1008),
        checkpoint(2, 'b', "before patch", 1005),
        checkpoint(3, 'a', "before write_file", 1002),
    ]
}

fn rows(list: Vec<Checkpoint>) -> Reply {
    Reply::Checkpoints(CheckpointList::Rows(list))
}

fn off() -> Reply {
    Reply::Checkpoints(CheckpointList::Off)
}

fn restored(short: &str, reason: &str, history_removed: i64) -> Reply {
    Reply::Restored(RestoreOutcome {
        success: true,
        restored_to: Some(short.into()),
        reason: Some(reason.into()),
        file: None,
        restored_files: None,
        skipped_user_edits: None,
        skipped_oversize: None,
        failed_deletes: None,
        history_removed: Some(history_removed),
        error: None,
    })
}

fn mutated(revision: i64, rewound_count: u64, content: Option<&str>) -> Reply {
    Reply::Mutated(MutateOutcome {
        revision: Some(revision),
        rewound_count,
        target_content: content.map(|text| json!(text)),
    })
}

fn diff_doc() -> DiffDoc {
    DiffDoc::Text {
        title: "Checkpoint bbbbbbbb compared with the files now".into(),
        subtitle: None,
        stat: "notes.txt | 2 +-".into(),
        diff: "--- a/notes.txt\n+++ b/notes.txt\n@@ -1 +1 @@\n-alpha\n+beta\n".into(),
        stat_only: false,
        cut_note: None,
    }
}

// Drivers. Each starts from the state it is given and presses real keys and sends real replies.

/// Open, read the session (`loaded`), list checkpoints (`list`): the Turns screen.
fn to_turns(state: &mut RewindState, loaded: Reply, list: Reply) {
    let effects = step(state, open(false));
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(state, &effects, loaded);
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = answer(state, &effects, list);
    assert!(
        effects.is_empty(),
        "the list opens Turns without effects: {effects:?}"
    );
    assert_eq!(state.screen(), RewindScreen::Turns);
}

/// Turns, then `older` presses of Down, then Enter: the Actions screen.
fn to_actions(state: &mut RewindState, loaded: Reply, list: Reply, older: usize) {
    to_turns(state, loaded, list);
    for _ in 0..older {
        assert!(key(state, RewindKey::Down).is_empty());
    }
    assert!(key(state, RewindKey::Enter).is_empty());
    assert_eq!(state.screen(), RewindScreen::Actions);
}

/// Actions, then `choice` (`C` rewinds the conversation, `F` also restores files): Confirm.
fn to_confirm(
    state: &mut RewindState,
    choice: RewindKey,
    loaded: Reply,
    list: Reply,
    older: usize,
) {
    to_actions(state, loaded, list, older);
    assert!(key(state, choice).is_empty());
    assert_eq!(state.screen(), RewindScreen::Confirm);
}

/// Press Enter on a confirm screen, expect the confirm read, and answer it with `reply`.
fn confirm_read(state: &mut RewindState, reply: Reply) -> Vec<RewindEffect> {
    let effects = key(state, RewindKey::Enter);
    expect_only_call(&effects, Call::ReadSession);
    answer(state, &effects, reply)
}

/// Answer the `Mutate` in `mutate_effects`, then the rebuild read; return the rebuild read's effects.
fn finish_rewind(
    state: &mut RewindState,
    mutate_effects: &[RewindEffect],
    outcome: Reply,
    rebuild_read: Reply,
) -> Vec<RewindEffect> {
    let effects = answer(state, mutate_effects, outcome);
    expect_only_call(&effects, Call::ReadSession);
    answer(state, &effects, rebuild_read)
}

/// `Undo` or `Retry` up to the `Mutate` call: the effects that hold it.
fn typed_rewind_to_mutate(
    state: &mut RewindState,
    input: RewindInput,
    session_read: Reply,
    mutate: Call,
) -> Vec<RewindEffect> {
    let effects = step(state, input);
    expect_only_call(&effects, Call::ReadSession);
    assert!(
        !state.is_open(),
        "a typed command's working step is not an overlay"
    );
    let effects = answer(state, &effects, session_read);
    expect_only_call(&effects, mutate);
    effects
}

fn undo(n: usize) -> RewindInput {
    RewindInput::Undo {
        n,
        turn_running: false,
    }
}

fn retry() -> RewindInput {
    RewindInput::Retry {
        turn_running: false,
    }
}

fn typed_restore(target: &str, file_path: Option<&str>, all: bool, running: bool) -> RewindInput {
    RewindInput::TypedRestore {
        target: target.into(),
        file_path: file_path.map(Into::into),
        all,
        turn_running: running,
    }
}

/// Whole-folder restore from the Files confirm screen, waiting on the `Restore` call.
fn at_restore_call() -> (RewindState, Vec<RewindEffect>) {
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::F,
        read(1, 1, edit_session()),
        rows(newest_and_oldest()),
        0,
    );
    let effects = confirm_read(&mut state, read(2, 2, edit_session()));
    expect_only_call(
        &effects,
        Call::Restore {
            hash: full_hash('b'),
            file_path: None,
            all: false,
        },
    );
    (state, effects)
}

/// One-file restore typed with `/rollback 1 src/a.rs`, waiting on the `Restore` call.
fn at_one_file_restore() -> (RewindState, Vec<RewindEffect>) {
    let mut state = RewindState::default();
    let effects = step(
        &mut state,
        typed_restore("1", Some("src/a.rs"), false, false),
    );
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    expect_only_call(
        &effects,
        Call::Restore {
            hash: "1".into(),
            file_path: Some("src/a.rs".into()),
            all: false,
        },
    );
    (state, effects)
}

/// Conversation rewind of row 3 from the overlay, waiting on the `Mutate` call (fences 7 and 4).
fn at_mutate_call() -> (RewindState, Vec<RewindEffect>) {
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::C,
        read(1, 1, verify_session()),
        off(),
        0,
    );
    let effects = confirm_read(&mut state, read(7, 4, verify_session()));
    expect_only_call(
        &effects,
        Call::Mutate {
            row_id: 3,
            expected_revision: 7,
            expected_generation: 4,
        },
    );
    (state, effects)
}

/// Typed whole-folder restore `/rollback 1`, answered `Restored { success, history_removed }`.
/// Returns the effects of the after-restore read.
fn typed_whole_restore_final(history_removed: i64) -> Vec<RewindEffect> {
    let mut state = RewindState::default();
    let effects = step(&mut state, typed_restore("1", None, false, false));
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(5, 3, edit_session()));
    expect_only_call(
        &effects,
        Call::Restore {
            hash: "1".into(),
            file_path: None,
            all: false,
        },
    );
    let effects = answer(
        &mut state,
        &effects,
        restored("bbbbbbbb", "before patch", history_removed),
    );
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(6, 4, without_chosen()));
    assert_eq!(state.screen(), RewindScreen::Closed);
    effects
}

/// Checkpoints overlay: `r`, Enter, answered `Restored { success, history_removed }`. Returns the
/// effects of the after-restore read.
fn overlay_restore_final(history_removed: i64) -> Vec<RewindEffect> {
    let mut state = RewindState::default();
    let effects = step(&mut state, RewindInput::OpenCheckpoints);
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    assert!(effects.is_empty());
    assert_eq!(state.screen(), RewindScreen::Checkpoints);
    assert!(key(&mut state, RewindKey::R).is_empty());
    assert_eq!(state.screen(), RewindScreen::CheckpointConfirm);
    let effects = confirm_read(&mut state, read(5, 3, edit_session()));
    expect_only_call(
        &effects,
        Call::Restore {
            hash: full_hash('b'),
            file_path: None,
            all: false,
        },
    );
    let effects = answer(
        &mut state,
        &effects,
        restored("bbbbbbbb", "before patch", history_removed),
    );
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(6, 4, without_chosen()));
    assert_eq!(state.screen(), RewindScreen::Closed);
    effects
}

/// Conversation rewind from the overlay whose rebuild read fails: the state then holds a pending
/// reload. Returns the state and the effects of the failed rebuild read.
fn failed_rebuild() -> (RewindState, Vec<RewindEffect>) {
    let mut state = RewindState::default();
    let messages = vec![user(5, "prompt")];
    to_confirm(
        &mut state,
        RewindKey::C,
        read(1, 1, messages.clone()),
        off(),
        0,
    );
    let effects = confirm_read(&mut state, read(7, 4, messages));
    expect_only_call(
        &effects,
        Call::Mutate {
            row_id: 5,
            expected_revision: 7,
            expected_generation: 4,
        },
    );
    let effects = answer(&mut state, &effects, mutated(8, 2, Some("prompt")));
    expect_only_call(&effects, Call::ReadSession);
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    assert_eq!(state.screen(), RewindScreen::Closed);
    (state, effects)
}

fn state_with_reload_pending() -> RewindState {
    failed_rebuild().0
}

// Tests 19 to 36.

#[test]
fn open_refused_while_turn_runs() {
    let mut state = RewindState::default();
    let effects = step(&mut state, open(true));
    assert_eq!(
        effects,
        vec![RewindEffect::Notice(NoticeKind::Warning, T1.into())]
    );
    assert_eq!(state.screen(), RewindScreen::Closed);
    assert!(!state.is_open());
}

#[test]
fn open_reads_session_then_checkpoints() {
    // Open reads the session, then lists checkpoints, then selects the newest prompt.
    let mut state = RewindState::default();
    let effects = step(&mut state, open(false));
    expect_only_call(&effects, Call::ReadSession);
    assert_eq!(state.screen(), RewindScreen::Loading);
    assert!(state.is_open());
    assert!(body_has(&state, "Loading the conversation…"));
    let messages = vec![
        user(1, "first"),
        assistant(2),
        user(3, "second"),
        assistant(4),
        user(5, "third"),
    ];
    let effects = answer(&mut state, &effects, read(1, 1, messages));
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = answer(
        &mut state,
        &effects,
        Reply::Checkpoints(CheckpointList::Empty),
    );
    assert!(effects.is_empty());
    assert_eq!(state.screen(), RewindScreen::Turns);
    let view = state.view(0);
    assert_eq!(view.selected, 0);
    let labels: Vec<&str> = view.rows.iter().map(|row| row.label.trim()).collect();
    assert_eq!(labels, ["#3", "#2", "#1"]);
    assert!(view.rows[0].mark);

    // A failed list still opens Turns, with the checkpoints marked unknown.
    let mut unknown = RewindState::default();
    let effects = step(&mut unknown, open(false));
    let effects = answer(&mut unknown, &effects, read(1, 1, vec![user(1, "hello")]));
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = fail(&mut unknown, &effects, CallFailure::Unknown);
    assert!(effects.is_empty());
    assert_eq!(unknown.screen(), RewindScreen::Turns);
    assert_eq!(unknown.view(0).rows.len(), 1);
    assert!(
        unknown
            .view(0)
            .footer
            .contains("checkpoints unknown: Hermes did not answer in time")
    );

    // No eligible prompt: T3 and closed.
    let mut empty = RewindState::default();
    let effects = step(&mut empty, open(false));
    let effects = answer(&mut empty, &effects, read(1, 1, vec![assistant(2)]));
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = answer(&mut empty, &effects, off());
    assert_eq!(
        effects,
        vec![RewindEffect::Notice(NoticeKind::System, T3.into())]
    );
    assert_eq!(empty.screen(), RewindScreen::Closed);

    // With a reload pending, the read first rebuilds with T34.
    let mut pending = state_with_reload_pending();
    let effects = step(&mut pending, open(false));
    expect_only_call(&effects, Call::ReadSession);
    let messages = vec![user(1, "hello")];
    let effects = answer(&mut pending, &effects, read(9, 6, messages.clone()));
    assert_eq!(effects.len(), 2, "{effects:?}");
    assert_eq!(
        effects[0],
        RewindEffect::Rebuild {
            snapshot: raw_snapshot(9, 6, &messages),
            notice: vec![T34.into()],
        }
    );
    expect_call(&effects[1..], Call::ListCheckpoints);
}

#[test]
fn selection_moves_clamp_and_page() {
    let mut state = RewindState::default();
    let messages: Vec<Value> = (1..=30).map(|id| user(id, "prompt")).collect();
    to_turns(&mut state, read(1, 1, messages), off());
    assert!(step(&mut state, RewindInput::Viewport(10)).is_empty());
    assert_eq!(state.view(0).selected, 0);
    assert!(key(&mut state, RewindKey::Up).is_empty());
    assert_eq!(state.view(0).selected, 0);
    assert!(key(&mut state, RewindKey::End).is_empty());
    let end = state.view(0).selected;
    assert_eq!(end, 29);
    assert!(key(&mut state, RewindKey::Down).is_empty());
    assert_eq!(state.view(0).selected, end);
    assert!(key(&mut state, RewindKey::Home).is_empty());
    assert_eq!(state.view(0).selected, 0);
    assert!(key(&mut state, RewindKey::PageDown).is_empty());
    assert_eq!(state.view(0).selected, 10);
    assert!(key(&mut state, RewindKey::PageDown).is_empty());
    assert_eq!(state.view(0).selected, 20);
    assert!(key(&mut state, RewindKey::PageDown).is_empty());
    assert_eq!(state.view(0).selected, 29);
    assert!(key(&mut state, RewindKey::PageUp).is_empty());
    assert_eq!(state.view(0).selected, 19);
}

/// Open Actions for `messages` (the `older`-th prompt from the newest) and return its body.
fn actions_for(messages: Vec<Value>, list: Reply, older: usize) -> (RewindState, Vec<ViewLine>) {
    let mut state = RewindState::default();
    to_actions(&mut state, read(1, 1, messages), list, older);
    let lines = body(&state);
    (state, lines)
}

/// `c` is always offered.
fn assert_rewind_offered(lines: &[ViewLine]) {
    let line = lines
        .iter()
        .find(|line| line.text.trim_start().starts_with("c "))
        .expect("the c line");
    assert!(
        line.text
            .contains("Rewind the conversation to before this prompt (files stay as they are)")
    );
}

/// `f` is drawn dim with `reason`, `d` is omitted, and neither key does anything.
fn assert_no_match(state: &mut RewindState, lines: &[ViewLine], reason: &str) {
    assert_rewind_offered(lines);
    let line = lines
        .iter()
        .find(|line| line.text.contains(reason))
        .unwrap_or_else(|| panic!("no line with {reason:?} in {lines:?}"));
    assert_eq!(line.style, LineStyle::Dim);
    assert!(
        !lines
            .iter()
            .any(|line| line.text.trim_start().starts_with("d ")),
        "d must be omitted: {lines:?}"
    );
    assert!(key(state, RewindKey::F).is_empty());
    assert_eq!(state.screen(), RewindScreen::Actions);
    assert!(key(state, RewindKey::D).is_empty());
    assert_eq!(state.screen(), RewindScreen::Actions);
}

#[test]
fn actions_depend_on_checkpoints() {
    let (mut state, lines) = actions_for(edit_session(), off(), 0);
    assert_no_match(&mut state, &lines, "Restore files: checkpoints are off");

    let empty = Reply::Checkpoints(CheckpointList::Empty);
    let (mut state, lines) = actions_for(edit_session(), empty, 0);
    assert_no_match(
        &mut state,
        &lines,
        "Restore files: no file changes recorded since this prompt",
    );

    // A list that failed to load.
    let (mut state, lines) = actions_for(edit_session(), Reply::Failed(CallFailure::Unknown), 0);
    assert_no_match(
        &mut state,
        &lines,
        "Restore files: checkpoints could not be listed",
    );

    // Rows, and one matches the newest prompt: f and d are both offered.
    let (_state, lines) = actions_for(edit_session(), rows(newest_and_oldest()), 0);
    assert_rewind_offered(&lines);
    let f = lines
        .iter()
        .find(|line| line.text.trim_start().starts_with("f "))
        .expect("the f line");
    assert!(
        f.text
            .contains("Also restore files to checkpoint bbbbbbbb · 14:02 · before patch")
    );
    assert_ne!(f.style, LineStyle::Dim);
    let d = lines
        .iter()
        .find(|line| line.text.trim_start().starts_with("d "))
        .expect("the d line");
    assert!(d.text.contains("Show file changes since this prompt"));
    assert!(
        !lines
            .iter()
            .any(|line| line.text.contains("Restore files:"))
    );

    // Rows, but every checkpoint is older than the newest prompt.
    let old = rows(vec![checkpoint(1, 'a', "before write_file", 990)]);
    let (mut state, lines) = actions_for(edit_session(), old, 0);
    assert_no_match(
        &mut state,
        &lines,
        "Restore files: no file changes recorded since this prompt",
    );

    // The older prompt edited files, but no checkpoint falls inside its own turn.
    let late = rows(vec![checkpoint(1, 'b', "before patch", 1005)]);
    let (mut state, lines) = actions_for(edit_session(), late, 1);
    assert_no_match(
        &mut state,
        &lines,
        "Restore files: Hermes recorded no checkpoint before this prompt's edits",
    );

    // A prompt with no stored time cannot be matched.
    let untimed = vec![json!({"role": "user", "row_id": 1, "content": "hello"})];
    let (mut state, lines) = actions_for(untimed, rows(newest_and_oldest()), 0);
    assert_no_match(
        &mut state,
        &lines,
        "Restore files: Hermes stored no time for this prompt",
    );
}

#[test]
fn confirm_rewind_emits_one_fenced_mutation() {
    let mut state = RewindState::default();
    let messages = vec![user(5, "prompt")];
    to_confirm(
        &mut state,
        RewindKey::C,
        read(6, 3, messages.clone()),
        off(),
        0,
    );

    // Enter sends only a fresh read; the overlay stays open while it works.
    let effects = key(&mut state, RewindKey::Enter);
    let read_ticket = expect_only_call(&effects, Call::ReadSession);
    assert_eq!(state.screen(), RewindScreen::Working);
    assert!(state.is_open());

    // The new read's fences, not the loaded read's, go into the one mutation.
    let effects = answer(&mut state, &effects, read(7, 4, messages.clone()));
    let mutate_ticket = expect_only_call(
        &effects,
        Call::Mutate {
            row_id: 5,
            expected_revision: 7,
            expected_generation: 4,
        },
    );
    assert_ne!(read_ticket, mutate_ticket, "every call gets a new ticket");

    let effects = answer(&mut state, &effects, mutated(8, 2, Some("prompt")));
    let _ = expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(8, 5, vec![]));
    assert_eq!(
        effects,
        vec![
            RewindEffect::Rebuild {
                snapshot: raw_snapshot(8, 5, &[]),
                notice: vec![t4(1, "prompt", 2)],
            },
            RewindEffect::ReturnPrompt {
                text: "prompt".into(),
                attachments: false,
            },
        ]
    );
    assert_eq!(state.screen(), RewindScreen::Closed);
}

#[test]
fn running_or_queued_snapshot_blocks_confirm() {
    let target = vec![user(5, "target"), assistant(6), assistant(7)];

    // The loaded read shows a running turn: Enter sends nothing.
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::C,
        busy_read(2, 1, true, 0, target.clone()),
        off(),
        0,
    );
    assert!(key(&mut state, RewindKey::Enter).is_empty());
    assert_eq!(state.screen(), RewindScreen::Confirm);

    // The loaded read shows two queued prompts: Enter sends nothing and the block line says so.
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::C,
        busy_read(2, 1, false, 2, target.clone()),
        off(),
        0,
    );
    assert!(key(&mut state, RewindKey::Enter).is_empty());
    assert_eq!(state.screen(), RewindScreen::Confirm);
    let lines = body(&state);
    let block = lines
        .iter()
        .find(|line| line.text.contains("2 queued"))
        .expect("the block line");
    assert_eq!(
        block.text.trim(),
        "Hermes will not rewind while a reply runs or prompts are queued (2 queued)."
    );
    assert_eq!(block.style, LineStyle::Warning);
    let view = state.view(0);
    assert!(
        !lines.iter().any(|line| line.text.contains("enter confirm"))
            && !view.footer.contains("enter confirm"),
        "a blocked confirm offers esc back only"
    );

    // A clean loaded read, but the confirm read shows a running turn: T15 and no mutation.
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::C,
        read(2, 1, target.clone()),
        off(),
        0,
    );
    let effects = confirm_read(&mut state, busy_read(3, 1, true, 0, target.clone()));
    assert_one_notice(&effects, T15);
    assert_eq!(state.screen(), RewindScreen::Closed);

    // The same for queued prompts in the confirm read.
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::C,
        read(2, 1, target.clone()),
        off(),
        0,
    );
    let effects = confirm_read(&mut state, busy_read(3, 1, false, 1, target.clone()));
    assert_one_notice(&effects, T15);
    assert_eq!(state.screen(), RewindScreen::Closed);

    // The confirm read is clean, but the target now has 5 rows from it on where it had 3.
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::C,
        read(2, 1, target.clone()),
        off(),
        0,
    );
    assert!(body_has(
        &state,
        "Remove prompt #1 and everything after it (3 messages) from the conversation?"
    ));
    let mut longer = target;
    longer.extend([assistant(8), assistant(9)]);
    let effects = confirm_read(&mut state, read(3, 2, longer.clone()));
    assert!(
        effects.is_empty(),
        "nothing is sent when the count changed: {effects:?}"
    );
    assert_eq!(state.screen(), RewindScreen::Confirm);
    assert!(body_has(
        &state,
        "Remove prompt #1 and everything after it (5 messages) from the conversation?"
    ));
    let warning = body(&state)
        .into_iter()
        .find(|line| line.text.contains(T36))
        .expect("the changed-conversation warning");
    assert_eq!(warning.style, LineStyle::Warning);

    // Enter again reads again; the count now matches, so exactly one mutation goes out.
    let effects = confirm_read(&mut state, read(4, 3, longer));
    expect_only_call(
        &effects,
        Call::Mutate {
            row_id: 5,
            expected_revision: 4,
            expected_generation: 3,
        },
    );
}

#[test]
fn restore_then_reread_newest_turn() {
    // The chosen prompt is the newest. Hermes's restore removes it, so only a rebuild follows.
    let (mut state, effects) = at_restore_call();
    let effects = answer(
        &mut state,
        &effects,
        restored("bbbbbbbb", "before patch", 2),
    );
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(3, 3, without_chosen()));
    assert!(calls(&effects).is_empty(), "no mutation: {effects:?}");
    assert_eq!(effects.len(), 2, "{effects:?}");
    let notice = rebuild_notice(&effects);
    assert_eq!(notice[0], t4f("bbbbbbbb", "before patch", 2, "chosen", 2));
    assert_eq!(
        notice[1],
        "Restored files to checkpoint bbbbbbbb (before patch)."
    );
    assert!(
        notice
            .iter()
            .any(|line| line.starts_with("Hermes also removed the last exchange ("))
    );
    assert_eq!(
        effects[1],
        RewindEffect::ReturnPrompt {
            text: "chosen".into(),
            attachments: false,
        }
    );
    assert_eq!(state.screen(), RewindScreen::Closed);

    // Hermes removed a newer compaction carrier instead, so the chosen prompt is still there:
    // the machine rewinds to it with the new read's fences.
    let carrier = user(7, "[CONTEXT SUMMARY]: earlier work");
    let mut with_carrier = edit_session();
    with_carrier.push(carrier);
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::F,
        read(1, 1, with_carrier.clone()),
        rows(newest_and_oldest()),
        0,
    );
    let effects = confirm_read(&mut state, read(2, 2, with_carrier));
    expect_only_call(
        &effects,
        Call::Restore {
            hash: full_hash('b'),
            file_path: None,
            all: false,
        },
    );
    let effects = answer(
        &mut state,
        &effects,
        restored("bbbbbbbb", "before patch", 2),
    );
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(3, 3, edit_session()));
    expect_only_call(
        &effects,
        Call::Mutate {
            row_id: 4,
            expected_revision: 3,
            expected_generation: 3,
        },
    );
    let effects = finish_rewind(
        &mut state,
        &effects,
        mutated(4, 3, Some("chosen")),
        read(4, 4, without_chosen()),
    );
    let notice = rebuild_notice(&effects);
    assert_eq!(notice[0], t4f("bbbbbbbb", "before patch", 2, "chosen", 3));
    assert!(
        !notice
            .iter()
            .any(|line| line.starts_with("Hermes's restore also removed"))
    );
    assert_eq!(
        prompt_effects(&effects),
        (vec![("chosen".to_string(), false)], vec![])
    );
}

#[test]
fn restore_and_rewind_older_turn_rereads_then_rewinds() {
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::F,
        read(1, 1, edit_session()),
        rows(newest_and_oldest()),
        1,
    );
    let effects = confirm_read(&mut state, read(2, 2, edit_session()));
    expect_only_call(
        &effects,
        Call::Restore {
            hash: full_hash('a'),
            file_path: None,
            all: false,
        },
    );
    let effects = answer(
        &mut state,
        &effects,
        restored("aaaaaaaa", "before write_file", 2),
    );
    expect_only_call(&effects, Call::ReadSession);
    // Hermes removed the newest exchange; the chosen older prompt is still active.
    let effects = answer(&mut state, &effects, read(9, 3, without_chosen()));
    expect_only_call(
        &effects,
        Call::Mutate {
            row_id: 1,
            expected_revision: 9,
            expected_generation: 3,
        },
    );
    let effects = finish_rewind(
        &mut state,
        &effects,
        mutated(10, 2, Some("older")),
        read(10, 4, vec![]),
    );
    assert_eq!(effects.len(), 2, "{effects:?}");
    assert_eq!(
        rebuild_notice(&effects)[0],
        t4f("aaaaaaaa", "before write_file", 1, "older", 2)
    );
    assert_eq!(
        prompt_effects(&effects),
        (vec![("older".to_string(), false)], vec![])
    );
    assert_eq!(state.screen(), RewindScreen::Closed);
}

#[test]
fn partial_failure_says_what_happened() {
    // The restore worked and the follow-up rewind is refused: one notice that says both.
    let (mut state, effects) = at_restore_call();
    let effects = answer(
        &mut state,
        &effects,
        restored("bbbbbbbb", "before patch", 2),
    );
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(3, 3, edit_session()));
    expect_only_call(
        &effects,
        Call::Mutate {
            row_id: 4,
            expected_revision: 3,
            expected_generation: 3,
        },
    );
    let effects = fail(
        &mut state,
        &effects,
        CallFailure::Refused {
            reason: "stale_generation".into(),
            field: None,
            detail: None,
            revision: Some(11),
            execution_generation: Some(6),
        },
    );
    assert_eq!(state.screen(), RewindScreen::Closed);
    assert_eq!(effects.len(), 1, "{effects:?}");
    let text = texts(&effects)[0];
    assert!(text.starts_with("Restored files to checkpoint bbbbbbbb, and Hermes removed the last exchange, but rewinding further to prompt #2 failed: "));
    assert!(text.contains(T13));
    assert!(text.ends_with("Run /rewind to try again."));

    // Hermes's restore also removed an older prompt than the one chosen: T32 names it.
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::F,
        read(1, 1, triple_session()),
        rows(three_checkpoints()),
        0,
    );
    let effects = confirm_read(&mut state, read(2, 2, triple_session()));
    expect_only_call(
        &effects,
        Call::Restore {
            hash: full_hash('c'),
            file_path: None,
            all: false,
        },
    );
    let effects = answer(
        &mut state,
        &effects,
        restored("cccccccc", "before patch", 2),
    );
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(3, 3, without_chosen()));
    assert!(
        calls(&effects).is_empty(),
        "the chosen row is gone, so no mutation"
    );
    let notice = rebuild_notice(&effects);
    assert_eq!(notice[0], t4f("cccccccc", "before patch", 3, "third", 2));
    assert_eq!(
        notice.last().map(String::as_str),
        Some(t32(2, "second").as_str())
    );
}

#[test]
fn ambiguous_restore_refusal_text() {
    // A whole-folder restore refused with session_busy after a clean read: the files may have moved.
    let (mut state, effects) = at_restore_call();
    let effects = fail(&mut state, &effects, refused("session_busy"));
    assert_one_notice(&effects, &t22("session_busy"));
    assert_eq!(state.screen(), RewindScreen::Closed);
    assert!(!texts(&effects)[0].contains("Stop the reply first"));

    // The reasons that mean nothing was done are plain refusals.
    let (mut state, effects) = at_restore_call();
    let effects = fail(&mut state, &effects, refused("not_found"));
    assert_one_notice(&effects, R_NOT_FOUND);
    let (mut state, effects) = at_restore_call();
    let effects = fail(&mut state, &effects, refused("permission_denied"));
    assert_one_notice(
        &effects,
        "Hermes refused: this connection may not restore files (permission_denied).",
    );

    // A one-file restore never touches the conversation, so session_busy is a plain refusal.
    let (mut state, effects) = at_one_file_restore();
    let effects = fail(&mut state, &effects, refused("session_busy"));
    assert_one_notice(&effects, "Hermes refused to restore files (session_busy).");
    assert_eq!(state.screen(), RewindScreen::Closed);
}

#[test]
fn unknown_outcomes_are_verified_or_reported() {
    // A lost mutate reply: read again. The row is gone and the generation rose: it went through.
    let (mut state, effects) = at_mutate_call();
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    expect_only_call(&effects, Call::ReadSession);
    let gone = vec![user(1, "older"), assistant(2)];
    let effects = answer(&mut state, &effects, read(8, 5, gone.clone()));
    assert_eq!(
        effects[0],
        RewindEffect::Rebuild {
            snapshot: raw_snapshot(8, 5, &gone),
            notice: vec![T9.into(), t31(2, "prompt")],
        }
    );
    assert_eq!(
        prompt_effects(&effects),
        (vec![("prompt".to_string(), false)], vec![])
    );
    assert_eq!(effects.len(), 2, "{effects:?}");
    assert_eq!(state.screen(), RewindScreen::Closed);

    // A reply that could not be decoded is verified the same way.
    let (mut state, effects) = at_mutate_call();
    let effects = fail(&mut state, &effects, CallFailure::Unreadable);
    expect_only_call(&effects, Call::ReadSession);

    // The row is still there: the rewind did not happen.
    let (mut state, effects) = at_mutate_call();
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    let effects = answer(&mut state, &effects, read(7, 4, verify_session()));
    assert_one_notice(&effects, T10);
    assert_eq!(state.screen(), RewindScreen::Closed);

    // The row is gone but the generation did not rise: Talaria cannot tell who removed it.
    let (mut state, effects) = at_mutate_call();
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    let effects = answer(&mut state, &effects, read(8, 4, gone.clone()));
    assert_eq!(
        effects,
        vec![RewindEffect::Rebuild {
            snapshot: raw_snapshot(8, 4, &gone),
            notice: vec![f5(2)],
        }]
    );

    // The verify read itself fails.
    let (mut state, effects) = at_mutate_call();
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    assert_one_notice(&effects, F4);
    assert_eq!(state.screen(), RewindScreen::Closed);

    // A one-file restore with a lost reply: T11 and nothing is sent.
    let (mut state, effects) = at_one_file_restore();
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    assert_one_notice(&effects, T11);
    let (mut state, effects) = at_one_file_restore();
    let effects = fail(&mut state, &effects, CallFailure::Unreadable);
    assert_one_notice(&effects, T11);

    // A whole-folder restore with a lost reply: read, then T11w. The revision rose, so rebuild too.
    let (mut state, effects) = at_restore_call();
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    expect_only_call(&effects, Call::ReadSession);
    let after = without_chosen();
    let effects = answer(&mut state, &effects, read(3, 2, after.clone()));
    assert_eq!(
        effects[0],
        RewindEffect::Rebuild {
            snapshot: raw_snapshot(3, 2, &after),
            notice: vec![T11W.into(), t31(2, "chosen")],
        }
    );
    assert_eq!(state.screen(), RewindScreen::Closed);

    let (mut state, effects) = at_restore_call();
    let effects = fail(&mut state, &effects, CallFailure::Unreadable);
    expect_only_call(&effects, Call::ReadSession);

    // The revision did not rise: only T11w, as a warning.
    let (mut state, effects) = at_restore_call();
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    let effects = answer(&mut state, &effects, read(2, 2, edit_session()));
    assert_eq!(
        effects,
        vec![RewindEffect::Notice(NoticeKind::Warning, T11W.into())]
    );

    // The read after a lost restore reply fails too.
    let (mut state, effects) = at_restore_call();
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    assert_one_notice(&effects, T11W);
}

#[test]
fn esc_backs_out_one_level() {
    // Diff, Actions, Turns, Closed. The diff is still loading when Esc goes back.
    let mut state = RewindState::default();
    to_actions(
        &mut state,
        read(1, 1, edit_session()),
        rows(newest_and_oldest()),
        0,
    );
    let diff_effects = key(&mut state, RewindKey::D);
    expect_only_call(
        &diff_effects,
        Call::CheckpointDiff {
            hash: full_hash('b'),
        },
    );
    assert_eq!(state.screen(), RewindScreen::Diff);
    assert!(body_has(&state, C11));
    assert!(key(&mut state, RewindKey::Esc).is_empty());
    assert_eq!(state.screen(), RewindScreen::Actions);
    // The late reply carries a stale ticket and changes nothing.
    let late = answer(&mut state, &diff_effects, Reply::DiffLoaded(diff_doc()));
    assert!(late.is_empty());
    assert_eq!(state.screen(), RewindScreen::Actions);
    // A diff that loads in time.
    let diff_effects = key(&mut state, RewindKey::D);
    expect_only_call(
        &diff_effects,
        Call::CheckpointDiff {
            hash: full_hash('b'),
        },
    );
    assert!(answer(&mut state, &diff_effects, Reply::DiffLoaded(diff_doc())).is_empty());
    assert_eq!(state.screen(), RewindScreen::Diff);
    assert!(!body_has(&state, C11));
    for expected in [
        RewindScreen::Actions,
        RewindScreen::Turns,
        RewindScreen::Closed,
    ] {
        assert!(key(&mut state, RewindKey::Esc).is_empty());
        assert_eq!(state.screen(), expected);
    }

    // Confirm goes back to Actions.
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::C,
        read(1, 1, edit_session()),
        off(),
        0,
    );
    assert!(key(&mut state, RewindKey::Esc).is_empty());
    assert_eq!(state.screen(), RewindScreen::Actions);

    // The Checkpoints overlay: its confirm and its diff go back to the list, the list closes.
    let mut state = RewindState::default();
    let effects = step(&mut state, RewindInput::OpenCheckpoints);
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    assert!(effects.is_empty());
    assert_eq!(state.screen(), RewindScreen::Checkpoints);
    assert!(key(&mut state, RewindKey::R).is_empty());
    assert_eq!(state.screen(), RewindScreen::CheckpointConfirm);
    assert!(key(&mut state, RewindKey::Esc).is_empty());
    assert_eq!(state.screen(), RewindScreen::Checkpoints);
    let diff_effects = key(&mut state, RewindKey::Enter);
    expect_only_call(
        &diff_effects,
        Call::CheckpointDiff {
            hash: full_hash('b'),
        },
    );
    assert_eq!(state.screen(), RewindScreen::Diff);
    assert!(key(&mut state, RewindKey::Esc).is_empty());
    assert_eq!(state.screen(), RewindScreen::Checkpoints);
    assert!(key(&mut state, RewindKey::Esc).is_empty());
    assert_eq!(state.screen(), RewindScreen::Closed);

    // A diff opened by a command closes on Esc.
    let mut state = RewindState::default();
    let effects = step(
        &mut state,
        RewindInput::OpenDiff(DiffSource::Session { stat_only: false }),
    );
    expect_only_call(&effects, Call::SessionDiff);
    assert!(answer(&mut state, &effects, Reply::DiffLoaded(diff_doc())).is_empty());
    assert_eq!(state.screen(), RewindScreen::Diff);
    assert!(key(&mut state, RewindKey::Esc).is_empty());
    assert_eq!(state.screen(), RewindScreen::Closed);

    // Esc in Loading closes. The read that was in flight then carries a stale ticket.
    let mut state = RewindState::default();
    let first = step(&mut state, open(false));
    let stale_ticket = call(&first).0;
    assert_eq!(state.screen(), RewindScreen::Loading);
    assert!(key(&mut state, RewindKey::Esc).is_empty());
    assert_eq!(state.screen(), RewindScreen::Closed);
    let late = answer(&mut state, &first, read(1, 1, edit_session()));
    assert!(late.is_empty());
    assert_eq!(state.screen(), RewindScreen::Closed);
    // Open again: the first read's ticket is still stale and the second read's works.
    let second = step(&mut state, open(false));
    let second_ticket = expect_only_call(&second, Call::ReadSession);
    assert_ne!(stale_ticket, second_ticket);
    let late = step(&mut state, reply(stale_ticket, read(1, 1, edit_session())));
    assert!(late.is_empty());
    assert_eq!(state.screen(), RewindScreen::Loading);
    let next = answer(&mut state, &second, read(1, 1, edit_session()));
    expect_only_call(&next, Call::ListCheckpoints);

    assert_ctrl_c_closes_every_screen_but_working();

    // Esc and Ctrl+C do nothing while Working.
    let mut state = RewindState::default();
    to_confirm(
        &mut state,
        RewindKey::C,
        read(1, 1, edit_session()),
        off(),
        0,
    );
    let effects = key(&mut state, RewindKey::Enter);
    expect_only_call(&effects, Call::ReadSession);
    assert_eq!(state.screen(), RewindScreen::Working);
    for pressed in [
        RewindKey::Esc,
        RewindKey::Close,
        RewindKey::Up,
        RewindKey::Enter,
    ] {
        assert!(key(&mut state, pressed).is_empty());
        assert_eq!(state.screen(), RewindScreen::Working);
    }
}

/// Every screen but Working closes on Ctrl+C.
fn assert_ctrl_c_closes_every_screen_but_working() {
    type Build = fn() -> RewindState;
    let builders: [(&str, Build); 7] = [
        ("loading", || {
            let mut state = RewindState::default();
            let _ = step(&mut state, open(false));
            state
        }),
        ("turns", || {
            let mut state = RewindState::default();
            to_turns(
                &mut state,
                read(1, 1, edit_session()),
                rows(newest_and_oldest()),
            );
            state
        }),
        ("actions", || {
            let mut state = RewindState::default();
            to_actions(
                &mut state,
                read(1, 1, edit_session()),
                rows(newest_and_oldest()),
                0,
            );
            state
        }),
        ("confirm", || {
            let mut state = RewindState::default();
            to_confirm(
                &mut state,
                RewindKey::C,
                read(1, 1, edit_session()),
                off(),
                0,
            );
            state
        }),
        ("checkpoints", || {
            let mut state = RewindState::default();
            let effects = step(&mut state, RewindInput::OpenCheckpoints);
            let _ = answer(&mut state, &effects, rows(newest_and_oldest()));
            state
        }),
        ("checkpoint confirm", || {
            let mut state = RewindState::default();
            let effects = step(&mut state, RewindInput::OpenCheckpoints);
            let _ = answer(&mut state, &effects, rows(newest_and_oldest()));
            let _ = key(&mut state, RewindKey::R);
            state
        }),
        ("diff", || {
            let mut state = RewindState::default();
            let _ = step(
                &mut state,
                RewindInput::OpenDiff(DiffSource::Session { stat_only: true }),
            );
            state
        }),
    ];
    for (name, build) in builders {
        let mut state = build();
        assert_ne!(state.screen(), RewindScreen::Closed, "{name} did not open");
        assert!(key(&mut state, RewindKey::Close).is_empty(), "{name}");
        assert_eq!(state.screen(), RewindScreen::Closed, "{name}");
    }
}

type Start = fn() -> (RewindState, Vec<RewindEffect>);

#[test]
fn read_failures_close_with_text() {
    let starts: [(&str, Start, Option<&str>); 5] = [
        (
            "read the session",
            || {
                let mut state = RewindState::default();
                let effects = step(&mut state, open(false));
                expect_only_call(&effects, Call::ReadSession);
                (state, effects)
            },
            Some("read the session"),
        ),
        (
            "list checkpoints",
            || {
                let mut state = RewindState::default();
                let effects = step(&mut state, RewindInput::OpenCheckpoints);
                expect_only_call(&effects, Call::ListCheckpoints);
                (state, effects)
            },
            Some("list checkpoints"),
        ),
        (
            "session diff",
            || {
                let mut state = RewindState::default();
                let effects = step(
                    &mut state,
                    RewindInput::OpenDiff(DiffSource::Session { stat_only: false }),
                );
                expect_only_call(&effects, Call::SessionDiff);
                (state, effects)
            },
            None,
        ),
        (
            "checkpoint diff",
            || {
                let mut state = RewindState::default();
                let effects = step(
                    &mut state,
                    RewindInput::OpenDiff(DiffSource::Checkpoint("1".into())),
                );
                expect_only_call(&effects, Call::ListCheckpoints);
                let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
                expect_only_call(&effects, Call::CheckpointDiff { hash: "1".into() });
                (state, effects)
            },
            Some("compare files"),
        ),
        (
            "confirm read",
            || {
                let mut state = RewindState::default();
                to_confirm(
                    &mut state,
                    RewindKey::C,
                    read(1, 1, edit_session()),
                    off(),
                    0,
                );
                let effects = key(&mut state, RewindKey::Enter);
                expect_only_call(&effects, Call::ReadSession);
                (state, effects)
            },
            Some("read the session"),
        ),
    ];
    for (name, start, action) in starts {
        let mut failures = vec![
            (CallFailure::Unknown, F3.to_string()),
            (CallFailure::Unreadable, F2.to_string()),
            (CallFailure::NotSent(F1.into()), F1.to_string()),
            (refused("not_found"), R_NOT_FOUND.to_string()),
        ];
        if let Some(action) = action {
            failures.push((
                refused("permission_denied"),
                format!("Hermes refused: this connection may not {action} (permission_denied)."),
            ));
        }
        for (failure, text) in failures {
            let (mut state, effects) = start();
            let effects = fail(&mut state, &effects, failure.clone());
            assert_eq!(texts(&effects), [text.as_str()], "{name}: {failure:?}");
            assert_eq!(effects.len(), 1, "{name}: {failure:?}");
            assert_eq!(state.screen(), RewindScreen::Closed, "{name}: {failure:?}");
        }
    }

    // The rebuild read failing after a committed rewind: T33, and the prompt still returns.
    let (state, effects) = failed_rebuild();
    assert_eq!(effects.len(), 2, "{effects:?}");
    match &effects[0] {
        RewindEffect::Notice(NoticeKind::Warning, text) => {
            assert!(
                text.starts_with(
                    "Hermes changed the conversation, but Talaria could not reload it ("
                )
            );
            assert!(text.ends_with(
                "). The lines above may still show removed turns as current. Open /rewind to reload it."
            ));
        }
        other => panic!("expected the T33 warning, got {other:?}"),
    }
    assert_eq!(
        effects[1],
        RewindEffect::ReturnPrompt {
            text: "prompt".into(),
            attachments: false,
        }
    );
    assert_eq!(state.screen(), RewindScreen::Closed);

    assert_reload_pending_rebuilds_first_with_t34();
}

/// After a failed rebuild the next read of each origin rebuilds first with T34.
fn assert_reload_pending_rebuilds_first_with_t34() {
    let messages = vec![user(3, "older"), assistant(4), user(5, "newest")];

    // /undo: the first read rebuilds with T34, then the mutation goes out.
    let mut state = state_with_reload_pending();
    let effects = step(&mut state, undo(1));
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(9, 6, messages.clone()));
    assert_eq!(effects.len(), 2, "{effects:?}");
    assert_eq!(
        effects[0],
        RewindEffect::Rebuild {
            snapshot: raw_snapshot(9, 6, &messages),
            notice: vec![T34.into()],
        }
    );
    let mutate = Call::Mutate {
        row_id: 5,
        expected_revision: 9,
        expected_generation: 6,
    };
    expect_call(&effects[1..], mutate);
    // The rewind then finishes with its own marker only: the flag was cleared by the first read.
    let effects = finish_rewind(
        &mut state,
        &effects[1..],
        mutated(10, 2, Some("newest")),
        read(10, 7, vec![]),
    );
    assert_eq!(rebuild_notice(&effects), vec![t4(2, "newest", 2)]);
    // A second /rewind gives no T34.
    let effects = step(&mut state, open(false));
    let effects = answer(&mut state, &effects, read(10, 7, messages.clone()));
    expect_only_call(&effects, Call::ListCheckpoints);

    // /retry.
    let mut state = state_with_reload_pending();
    let effects = step(&mut state, retry());
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(9, 6, messages.clone()));
    assert_eq!(effects.len(), 2, "{effects:?}");
    assert_eq!(
        effects[0],
        RewindEffect::Rebuild {
            snapshot: raw_snapshot(9, 6, &messages),
            notice: vec![T34.into()],
        }
    );
    expect_call(
        &effects[1..],
        Call::Mutate {
            row_id: 5,
            expected_revision: 9,
            expected_generation: 6,
        },
    );

    // A typed whole-folder restore: T34 comes before the restore.
    let mut state = state_with_reload_pending();
    let effects = step(&mut state, typed_restore("1", None, false, false));
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(9, 6, messages.clone()));
    assert_eq!(effects.len(), 2, "{effects:?}");
    assert_eq!(
        effects[0],
        RewindEffect::Rebuild {
            snapshot: raw_snapshot(9, 6, &messages),
            notice: vec![T34.into()],
        }
    );
    expect_call(
        &effects[1..],
        Call::Restore {
            hash: "1".into(),
            file_path: None,
            all: false,
        },
    );

    // Screens that read no session leave the flag set; the next /rewind still rebuilds.
    let mut state = state_with_reload_pending();
    let effects = step(&mut state, RewindInput::OpenCheckpoints);
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    assert!(effects.is_empty());
    assert!(key(&mut state, RewindKey::Esc).is_empty());
    let effects = step(&mut state, typed_restore("1", Some("a.rs"), false, false));
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    expect_only_call(
        &effects,
        Call::Restore {
            hash: "1".into(),
            file_path: Some("a.rs".into()),
            all: false,
        },
    );
    let effects = answer(
        &mut state,
        &effects,
        restored("bbbbbbbb", "before patch", 0),
    );
    assert_eq!(effects.len(), 1, "{effects:?}");
    let effects = step(&mut state, open(false));
    let effects = answer(&mut state, &effects, read(9, 6, messages.clone()));
    assert_eq!(effects.len(), 2, "{effects:?}");
    assert_eq!(rebuild_notice(&effects), vec![T34.to_string()]);

    // A rebuild already on its way serves: an external rewind clears the flag without T34.
    let mut state = state_with_reload_pending();
    let effects = step(&mut state, frame("rewind", 20, Some(2)));
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(20, 8, messages.clone()));
    assert_eq!(rebuild_notice(&effects), vec![t29(2)]);
    let effects = step(&mut state, open(false));
    let effects = answer(&mut state, &effects, read(20, 8, messages));
    expect_only_call(&effects, Call::ListCheckpoints);
}

#[test]
fn undo_and_retry_targets() {
    // /undo 2 targets the second newest prompt.
    let three = vec![
        user(1, "first"),
        assistant(2),
        user(3, "second"),
        assistant(4),
        user(5, "third"),
    ];
    let mut state = RewindState::default();
    let mutate = Call::Mutate {
        row_id: 3,
        expected_revision: 5,
        expected_generation: 2,
    };
    let effects = typed_rewind_to_mutate(&mut state, undo(2), read(5, 2, three.clone()), mutate);
    let effects = finish_rewind(
        &mut state,
        &effects,
        mutated(6, 3, Some("second")),
        read(6, 3, vec![user(1, "first"), assistant(2)]),
    );
    assert_eq!(rebuild_notice(&effects), vec![t4(2, "second", 3)]);
    assert_eq!(
        prompt_effects(&effects),
        (vec![("second".to_string(), false)], vec![])
    );

    // Only three prompts, so /undo 5 says so and sends nothing.
    let mut state = RewindState::default();
    let effects = step(&mut state, undo(5));
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(&mut state, &effects, read(5, 2, three));
    assert_one_notice(&effects, "There are only 3 prompts to undo.");
    assert_eq!(state.screen(), RewindScreen::Closed);
    let mut state = RewindState::default();
    let effects = step(&mut state, undo(1));
    let effects = answer(&mut state, &effects, read(5, 2, vec![assistant(2)]));
    assert_one_notice(&effects, "There is no prompt to undo.");

    // /retry with an image prompt cannot resend it: T28 and no mutation.
    let mut state = RewindState::default();
    let effects = step(&mut state, retry());
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(
        &mut state,
        &effects,
        read(5, 2, vec![user_with_image(3, "look at this")]),
    );
    assert_one_notice(&effects, T28);
    assert_eq!(state.screen(), RewindScreen::Closed);

    // Nothing to retry: no prompts, or an empty prompt.
    for messages in [vec![assistant(2)], vec![user(1, "")]] {
        let mut state = RewindState::default();
        let effects = step(&mut state, retry());
        let effects = answer(&mut state, &effects, read(5, 2, messages));
        assert_one_notice(&effects, T27);
    }

    // /retry prefers the rewind result's target content, and submits after the rebuild.
    let session = vec![user(1, "retry me"), assistant(2)];
    let mutate = Call::Mutate {
        row_id: 1,
        expected_revision: 5,
        expected_generation: 2,
    };
    let mut state = RewindState::default();
    let effects = typed_rewind_to_mutate(
        &mut state,
        retry(),
        read(5, 2, session.clone()),
        mutate.clone(),
    );
    let effects = finish_rewind(
        &mut state,
        &effects,
        mutated(6, 2, Some("live ask")),
        read(6, 3, vec![]),
    );
    assert_eq!(
        effects,
        vec![
            RewindEffect::Rebuild {
                snapshot: raw_snapshot(6, 3, &[]),
                notice: vec![t4(1, "retry me", 2)],
            },
            RewindEffect::Submit("live ask".into()),
        ]
    );

    // Without target content, or with content that is not lossless text, the snapshot's text is used.
    let lossy = Reply::Mutated(MutateOutcome {
        revision: Some(6),
        rewound_count: 2,
        target_content: Some(json!([{"type": "input_text", "text": "x", "extra": 1}])),
    });
    let absent = Reply::Mutated(MutateOutcome {
        revision: Some(6),
        rewound_count: 2,
        target_content: None,
    });
    for outcome in [absent, lossy] {
        let mut state = RewindState::default();
        let effects = typed_rewind_to_mutate(
            &mut state,
            retry(),
            read(5, 2, session.clone()),
            mutate.clone(),
        );
        let effects = finish_rewind(&mut state, &effects, outcome, read(6, 3, vec![]));
        assert_eq!(
            prompt_effects(&effects),
            (vec![], vec!["retry me".to_string()])
        );
    }
}

#[test]
fn rewound_prompt_is_returned_once() {
    let none: (Vec<(String, bool)>, Vec<String>) = (vec![], vec![]);

    // Overlay c returns the prompt text.
    let (mut state, effects) = at_mutate_call();
    let end = finish_rewind(
        &mut state,
        &effects,
        mutated(8, 2, Some("prompt")),
        read(8, 5, vec![]),
    );
    assert_eq!(
        prompt_effects(&end),
        (vec![("prompt".to_string(), false)], vec![])
    );

    // A prompt with an image returns its text parts and says attachments were lost.
    let mut state = RewindState::default();
    let session = vec![user_with_image(1, "look at this")];
    to_confirm(
        &mut state,
        RewindKey::C,
        read(1, 1, session.clone()),
        off(),
        0,
    );
    let effects = confirm_read(&mut state, read(2, 2, session));
    let end = finish_rewind(
        &mut state,
        &effects,
        mutated(3, 1, None),
        read(3, 3, vec![]),
    );
    assert_eq!(
        prompt_effects(&end),
        (vec![("look at this".to_string(), true)], vec![])
    );

    // /undo returns the target's text.
    let mut state = RewindState::default();
    let mutate = Call::Mutate {
        row_id: 3,
        expected_revision: 7,
        expected_generation: 4,
    };
    let effects = typed_rewind_to_mutate(&mut state, undo(1), read(7, 4, verify_session()), mutate);
    let end = finish_rewind(
        &mut state,
        &effects,
        mutated(8, 1, Some("prompt")),
        read(8, 5, vec![]),
    );
    assert_eq!(
        prompt_effects(&end),
        (vec![("prompt".to_string(), false)], vec![])
    );

    // Overlay f returns it when Hermes's restore removed the chosen row ...
    let (mut state, effects) = at_restore_call();
    let effects = answer(
        &mut state,
        &effects,
        restored("bbbbbbbb", "before patch", 2),
    );
    let end = answer(&mut state, &effects, read(3, 3, without_chosen()));
    assert_eq!(
        prompt_effects(&end),
        (vec![("chosen".to_string(), false)], vec![])
    );

    // ... and when the follow-up mutation removed it.
    let (mut state, effects) = at_restore_call();
    let effects = answer(
        &mut state,
        &effects,
        restored("bbbbbbbb", "before patch", 2),
    );
    let effects = answer(&mut state, &effects, read(3, 3, edit_session()));
    let end = finish_rewind(
        &mut state,
        &effects,
        mutated(4, 2, Some("chosen")),
        read(4, 4, without_chosen()),
    );
    assert_eq!(
        prompt_effects(&end),
        (vec![("chosen".to_string(), false)], vec![])
    );

    // A lost rewind reply whose row is gone returns it.
    let (mut state, effects) = at_mutate_call();
    let effects = fail(&mut state, &effects, CallFailure::Unknown);
    let end = answer(
        &mut state,
        &effects,
        read(8, 5, vec![user(1, "older"), assistant(2)]),
    );
    assert_eq!(
        prompt_effects(&end),
        (vec![("prompt".to_string(), false)], vec![])
    );

    // /retry submits and returns nothing.
    let mut state = RewindState::default();
    let mutate = Call::Mutate {
        row_id: 3,
        expected_revision: 7,
        expected_generation: 4,
    };
    let effects = typed_rewind_to_mutate(&mut state, retry(), read(7, 4, verify_session()), mutate);
    let end = finish_rewind(
        &mut state,
        &effects,
        mutated(8, 1, Some("prompt")),
        read(8, 5, vec![]),
    );
    assert_eq!(prompt_effects(&end), (vec![], vec!["prompt".to_string()]));

    // Restores with no chosen prompt, from the Checkpoints overlay or typed, return nothing.
    for history_removed in [2, 0] {
        assert_eq!(
            prompt_effects(&typed_whole_restore_final(history_removed)),
            none
        );
        assert_eq!(
            prompt_effects(&overlay_restore_final(history_removed)),
            none
        );
    }

    // So does a one-file restore.
    let (mut state, effects) = at_one_file_restore();
    let end = answer(
        &mut state,
        &effects,
        restored("bbbbbbbb", "before patch", 0),
    );
    assert_eq!(prompt_effects(&end), none);
}

#[test]
fn session_updated_from_elsewhere_rebuilds_once() {
    // Our own rewind: its session.updated frame is ignored whenever it arrives.
    let mut state = RewindState::default();
    let mutate = Call::Mutate {
        row_id: 1,
        expected_revision: 7,
        expected_generation: 4,
    };
    let session = vec![user(1, "first"), assistant(2)];
    let effects = typed_rewind_to_mutate(&mut state, undo(1), read(7, 4, session), mutate);
    assert_eq!(state.screen(), RewindScreen::Working);
    assert!(step(&mut state, frame("rewind", 8, Some(2))).is_empty());
    let effects = answer(&mut state, &effects, mutated(8, 2, Some("first")));
    expect_only_call(&effects, Call::ReadSession);
    assert!(step(&mut state, frame("rewind", 8, Some(2))).is_empty());
    let end = answer(&mut state, &effects, read(8, 5, vec![]));
    assert_eq!(rebuild_notice(&end), vec![t4(1, "first", 2)]);
    assert_eq!(state.screen(), RewindScreen::Closed);
    assert!(step(&mut state, frame("rewind", 8, Some(2))).is_empty());

    // Another window rewinds: one read, one rebuild with T29.
    let effects = step(&mut state, frame("rewind", 9, Some(2)));
    expect_only_call(&effects, Call::ReadSession);
    let fresh = vec![user(1, "fresh")];
    let effects = answer(&mut state, &effects, read(9, 6, fresh.clone()));
    assert_eq!(
        effects,
        vec![RewindEffect::Rebuild {
            snapshot: raw_snapshot(9, 6, &fresh),
            notice: vec![t29(2)],
        }]
    );
    assert_eq!(state.screen(), RewindScreen::Closed);
    // The same revision again, an older one, and other operations do nothing.
    assert!(step(&mut state, frame("rewind", 9, Some(2))).is_empty());
    assert!(step(&mut state, frame("rewind", 8, Some(2))).is_empty());
    assert!(step(&mut state, frame("rename", 12, None)).is_empty());
    assert!(step(&mut state, frame("workspace", 13, None)).is_empty());

    // With no revision recorded yet, the first rewind frame from elsewhere rebuilds.
    let mut fresh_state = RewindState::default();
    let effects = step(&mut fresh_state, frame("rewind", 9, Some(2)));
    expect_only_call(&effects, Call::ReadSession);

    // An open overlay closes first, then the rebuild happens.
    let mut open_state = RewindState::default();
    to_turns(&mut open_state, read(1, 1, edit_session()), off());
    let effects = step(&mut open_state, frame("rewind", 3, Some(4)));
    expect_only_call(&effects, Call::ReadSession);
    assert_ne!(open_state.screen(), RewindScreen::Turns);
    let effects = answer(&mut open_state, &effects, read(3, 2, vec![]));
    assert_eq!(rebuild_notice(&effects), vec![t29(4)]);
    assert_eq!(open_state.screen(), RewindScreen::Closed);

    // A whole-folder restore: the frame between the restore reply and the read is ignored, and
    // after that read recorded its revision a frame with the same revision is too.
    let (mut state, effects) = at_restore_call();
    let effects = answer(
        &mut state,
        &effects,
        restored("bbbbbbbb", "before patch", 2),
    );
    expect_only_call(&effects, Call::ReadSession);
    assert!(step(&mut state, frame("rewind", 99, Some(2))).is_empty());
    assert_eq!(state.screen(), RewindScreen::Working);
    let end = answer(&mut state, &effects, read(5, 3, without_chosen()));
    assert_eq!(end.len(), 2, "{end:?}");
    assert_eq!(state.screen(), RewindScreen::Closed);
    assert!(step(&mut state, frame("rewind", 5, Some(2))).is_empty());
}

#[test]
fn double_esc_window() {
    assert_eq!(DOUBLE_ESC_WINDOW_MS, 600);
    let mut esc = DoubleEsc::default();
    assert!(!esc.press(0, true));
    assert!(esc.press(599, true));
    let mut esc = DoubleEsc::default();
    assert!(!esc.press(0, true));
    assert!(!esc.press(600, true));
    // The press at 600 became the new first press.
    assert!(esc.press(1_100, true));
    let mut esc = DoubleEsc::default();
    assert!(!esc.press(0, true));
    assert!(!esc.press(100, false));
    assert!(!esc.press(200, true));
    let mut esc = DoubleEsc::default();
    assert!(!esc.press(0, true));
    assert!(esc.press(100, true));
    assert!(!esc.press(100, true));
    assert!(esc.press(100, true));
}

#[test]
fn typed_commands_check_turn_and_list_first() {
    // /undo and /retry while a turn runs: T1 and nothing sent.
    let mut state = RewindState::default();
    let effects = step(
        &mut state,
        RewindInput::Undo {
            n: 1,
            turn_running: true,
        },
    );
    assert_one_notice(&effects, T1);
    assert_eq!(state.screen(), RewindScreen::Closed);
    let effects = step(&mut state, RewindInput::Retry { turn_running: true });
    assert_one_notice(&effects, T1);
    assert_eq!(state.screen(), RewindScreen::Closed);

    // A whole-folder restore while a turn runs: T21.
    let effects = step(&mut state, typed_restore("1", None, false, true));
    assert_one_notice(&effects, T21);
    assert_eq!(state.screen(), RewindScreen::Closed);

    // Otherwise the checkpoints are listed first; off gives C1.
    let effects = step(&mut state, typed_restore("1", None, false, false));
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = answer(&mut state, &effects, off());
    assert_one_notice(&effects, C1);
    assert_eq!(state.screen(), RewindScreen::Closed);

    // /rollback with no target: off gives C1, on with none gives C2.
    for (list, text) in [(off(), C1), (Reply::Checkpoints(CheckpointList::Empty), C2)] {
        let mut state = RewindState::default();
        let effects = step(&mut state, RewindInput::OpenCheckpoints);
        expect_only_call(&effects, Call::ListCheckpoints);
        let effects = answer(&mut state, &effects, list);
        assert_one_notice(&effects, text);
        assert_eq!(state.screen(), RewindScreen::Closed);
    }

    // A whole-folder restore then reads the session: queued prompts give T23, a running turn T21.
    let mut state = RewindState::default();
    let effects = step(&mut state, typed_restore("1", None, false, false));
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    expect_only_call(&effects, Call::ReadSession);
    let effects = answer(
        &mut state,
        &effects,
        busy_read(5, 3, false, 1, edit_session()),
    );
    assert_one_notice(&effects, T23);
    assert_eq!(state.screen(), RewindScreen::Closed);
    let mut state = RewindState::default();
    let effects = step(&mut state, typed_restore("1", None, false, false));
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    let effects = answer(
        &mut state,
        &effects,
        busy_read(5, 3, true, 0, edit_session()),
    );
    assert_one_notice(&effects, T21);

    // An idle read sends the restore with the typed target and flags.
    let mut state = RewindState::default();
    let effects = step(&mut state, typed_restore("a1b2c3d4", None, true, false));
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    let effects = answer(&mut state, &effects, read(5, 3, edit_session()));
    expect_only_call(
        &effects,
        Call::Restore {
            hash: "a1b2c3d4".into(),
            file_path: None,
            all: true,
        },
    );

    // With a file there is no read: the restore follows the list at once, even with a turn running.
    let mut state = RewindState::default();
    let effects = step(
        &mut state,
        typed_restore("1", Some("src/a.rs"), false, true),
    );
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    expect_only_call(
        &effects,
        Call::Restore {
            hash: "1".into(),
            file_path: Some("src/a.rs".into()),
            all: false,
        },
    );
    let one_file = Reply::Restored(RestoreOutcome {
        success: true,
        restored_to: Some("abcdef01".into()),
        reason: Some("before patch".into()),
        file: Some("src/a.rs".into()),
        restored_files: None,
        skipped_user_edits: None,
        skipped_oversize: None,
        failed_deletes: None,
        history_removed: None,
        error: None,
    });
    let effects = answer(&mut state, &effects, one_file);
    assert_eq!(
        effects,
        vec![RewindEffect::Notice(
            NoticeKind::System,
            "Restored src/a.rs from checkpoint abcdef01.".into()
        )]
    );
    assert_eq!(state.screen(), RewindScreen::Closed);

    // A typed diff lists first; off gives C1, otherwise the diff of the typed target is requested.
    let mut state = RewindState::default();
    let effects = step(
        &mut state,
        RewindInput::OpenDiff(DiffSource::Checkpoint("2".into())),
    );
    expect_only_call(&effects, Call::ListCheckpoints);
    let effects = answer(&mut state, &effects, off());
    assert_one_notice(&effects, C1);
    assert_eq!(state.screen(), RewindScreen::Closed);
    let mut state = RewindState::default();
    let effects = step(
        &mut state,
        RewindInput::OpenDiff(DiffSource::Checkpoint("2".into())),
    );
    let effects = answer(&mut state, &effects, rows(newest_and_oldest()));
    expect_only_call(&effects, Call::CheckpointDiff { hash: "2".into() });
    assert_eq!(state.screen(), RewindScreen::Diff);

    // A restore with no chosen prompt (typed, then the Checkpoints overlay's r): the rebuild
    // carries the restore summary only. No marker, no T32, no mutation, no returned prompt.
    for effects in [typed_whole_restore_final(2), overlay_restore_final(2)] {
        assert_eq!(effects.len(), 1, "{effects:?}");
        assert!(calls(&effects).is_empty());
        let notice = rebuild_notice(&effects);
        assert_eq!(
            notice[0],
            "Restored files to checkpoint bbbbbbbb (before patch)."
        );
        assert!(
            notice
                .iter()
                .any(|line| line.starts_with("Hermes also removed the last exchange ("))
        );
        assert!(!notice.iter().any(|line| line.starts_with("──")));
        assert!(
            !notice
                .iter()
                .any(|line| line.starts_with("Hermes's restore also removed"))
        );
    }
    // When Hermes removed no rows, only a notice is shown and nothing is rebuilt.
    for effects in [typed_whole_restore_final(0), overlay_restore_final(0)] {
        assert_eq!(
            effects,
            vec![RewindEffect::Notice(
                NoticeKind::System,
                "Restored files to checkpoint bbbbbbbb (before patch).".into()
            )]
        );
    }
}
