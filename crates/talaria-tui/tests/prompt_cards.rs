//! Approval, clarification and masked prompt cards: what is drawn and what each key does.
//!
//! Time is driven explicitly: a card is synced at one instant and keys arrive at a later one, so
//! the key guard is tested without sleeping. Expected screens are hand-written text files under
//! `snapshots/prompt_cards/`, one per case and size.

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use serde_json::json;
use talaria_core::session::{CellId, PromptCell, PromptKey, PromptKind, PromptState};
use talaria_tui::keys::{Key, KeyCode, KeyKind, Mods};
use talaria_tui::live::LiveSection;
use talaria_tui::prompts::{
    KeyOutcome, MaskedValue, MaskedView, PromptIntent, PromptStack, display_choices,
    multi_select_answer,
};

const MAIN: &str = "sess-main";
const MASKED_ID: &str = "srq-0123456789ab";
const MARKER: &str = "TALARIA-MASK-7f3a";
const AFTER_GUARD: Duration = Duration::from_millis(300);
const INSIDE_GUARD: Duration = Duration::from_millis(100);

macro_rules! snapshot {
    ($name:literal) => {
        include_str!(concat!("snapshots/prompt_cards/", $name, ".txt"))
    };
}

// ---------------------------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------------------------

fn press(code: KeyCode, mods: Mods) -> Key {
    Key {
        code,
        mods,
        kind: KeyKind::Press,
    }
}

fn ch(c: char) -> Key {
    press(KeyCode::Char(c), Mods::default())
}

fn named(code: KeyCode) -> Key {
    press(code, Mods::default())
}

fn ctrl_c() -> Key {
    press(
        KeyCode::Char('c'),
        Mods {
            ctrl: true,
            ..Mods::default()
        },
    )
}

// ---------------------------------------------------------------------------------------------
// Cells and views
// ---------------------------------------------------------------------------------------------

fn prompt_key(session: &str, prompt: &str, generation: i64) -> PromptKey {
    PromptKey {
        session_id: session.into(),
        prompt_id: prompt.to_owned(),
        execution_generation: generation,
    }
}

fn approval_in(
    session: &str,
    prompt: &str,
    command: &str,
    description: Option<&str>,
    choices: &[&str],
) -> PromptCell {
    PromptCell {
        id: CellId::new(1),
        key: prompt_key(session, prompt, 3),
        kind: PromptKind::Approval,
        command: Some(command.to_owned()),
        description: description.map(str::to_owned),
        question: None,
        choices: choices.iter().map(|c| (*c).to_owned()).collect(),
        multi_select: false,
        edit: None,
        timeout_s: None,
        state: PromptState::Open,
        sealed: false,
    }
}

fn approval(command: &str, choices: &[&str]) -> PromptCell {
    approval_in(MAIN, "p-1", command, Some("recursive delete"), choices)
}

fn standard_approval() -> PromptCell {
    approval(
        "rm -rf .pytest_cache",
        &["once", "deny", "session", "always"],
    )
}

fn clarify(question: &str, choices: &[&str], multi_select: bool) -> PromptCell {
    PromptCell {
        id: CellId::new(2),
        key: prompt_key(MAIN, "c-1", 2),
        kind: PromptKind::Clarify,
        command: None,
        description: None,
        question: Some(question.to_owned()),
        choices: choices.iter().map(|c| (*c).to_owned()).collect(),
        multi_select,
        edit: None,
        timeout_s: None,
        state: PromptState::Open,
        sealed: false,
    }
}

fn masked_view(method: &str, fields: &[(&str, &str)]) -> MaskedView {
    MaskedView {
        request_id: MASKED_ID.to_owned(),
        session_id: Some(MAIN.to_owned()),
        method: method.to_owned(),
        fields: fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
    }
}

fn sudo_view() -> MaskedView {
    masked_view("sudo", &[("command", "sudo apt-get install jq")])
}

// ---------------------------------------------------------------------------------------------
// Outcomes
// ---------------------------------------------------------------------------------------------

struct Outcome {
    intent: Option<PromptIntent>,
    interrupt_requested: bool,
    notice: Option<String>,
}

fn consumed(outcome: KeyOutcome) -> Outcome {
    match outcome {
        KeyOutcome::Consumed {
            intent,
            interrupt_requested,
            notice,
        } => Outcome {
            intent,
            interrupt_requested,
            notice,
        },
        KeyOutcome::NotHandled => panic!("the card stack did not take the key"),
    }
}

fn intent_kind(intent: &Option<PromptIntent>) -> &'static str {
    match intent {
        None => "no intent",
        Some(PromptIntent::Approve { .. }) => "an approval intent",
        Some(PromptIntent::Clarify { .. }) => "a clarification intent",
        Some(PromptIntent::Masked { .. }) => "a masked intent",
    }
}

impl Outcome {
    /// The key was consumed and asked for nothing.
    fn nothing(self) {
        assert!(
            self.intent.is_none(),
            "expected no intent, got {}",
            intent_kind(&self.intent)
        );
        assert!(!self.interrupt_requested, "expected no interrupt request");
    }

    fn approve(self) -> (PromptKey, String, Option<String>) {
        assert!(!self.interrupt_requested, "an answer is not an interrupt");
        match self.intent {
            Some(PromptIntent::Approve { key, choice, note }) => (key, choice, note),
            other => panic!("expected an approval intent, got {}", intent_kind(&other)),
        }
    }

    fn clarify(self) -> (PromptKey, String) {
        assert!(!self.interrupt_requested, "an answer is not an interrupt");
        match self.intent {
            Some(PromptIntent::Clarify { key, answer }) => (key, answer),
            other => panic!(
                "expected a clarification intent, got {}",
                intent_kind(&other)
            ),
        }
    }

    fn masked(self) -> (String, MaskedValue) {
        assert!(!self.interrupt_requested, "an answer is not an interrupt");
        match self.intent {
            Some(PromptIntent::Masked { request_id, value }) => (request_id, value),
            other => panic!("expected a masked intent, got {}", intent_kind(&other)),
        }
    }

    /// An interrupt request carrying the given kind of answer, returned for further checks.
    fn interrupting(self) -> Option<PromptIntent> {
        assert!(self.interrupt_requested, "expected an interrupt request");
        self.intent
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------------------------

/// The section drawn alone in an otherwise empty region of `width` by `rows`, one string per row
/// with trailing spaces removed and trailing empty rows dropped.
fn draw(stack: &PromptStack, width: u16, rows: u16) -> Vec<String> {
    let section = stack.section().expect("a card should be drawn");
    let height = section.height(width).min(rows);
    let mut buf = Buffer::empty(Rect::new(0, 0, width, rows));
    section.render(Rect::new(0, 0, width, height), &mut buf);
    let mut out: Vec<String> = (0..rows)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect();
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

fn assert_snapshot(stack: &PromptStack, width: u16, rows: u16, expected: &str, name: &str) {
    let got = draw(stack, width, rows);
    let want: Vec<&str> = expected.lines().collect();
    assert_eq!(
        got, want,
        "the {width} column screen differs from snapshot {name}"
    );
}

fn has_row(rows: &[String], wanted: &str) -> bool {
    rows.iter().any(|row| row == wanted)
}

fn has_row_containing(rows: &[String], wanted: &str) -> bool {
    rows.iter().any(|row| row.contains(wanted))
}

// ---------------------------------------------------------------------------------------------
// A stack that has had time to take focus
// ---------------------------------------------------------------------------------------------

struct Rig {
    stack: PromptStack,
    at: Instant,
}

impl Rig {
    fn with(prompts: &[&PromptCell], masked: &[MaskedView]) -> Self {
        let start = Instant::now();
        let mut stack = PromptStack::new();
        stack.sync(Some(MAIN), prompts, masked, true, start);
        Self {
            stack,
            at: start + AFTER_GUARD,
        }
    }

    fn approval(cell: &PromptCell) -> Self {
        Self::with(&[cell], &[])
    }

    fn masked(view: MaskedView) -> Self {
        Self::with(&[], &[view])
    }

    fn press(&mut self, key: Key) -> Outcome {
        consumed(self.stack.handle_key(key, self.at))
    }

    fn paste(&mut self, text: &str) -> Outcome {
        consumed(self.stack.handle_paste(text, self.at))
    }

    /// Type each character; none of them may ask for anything.
    fn type_text(&mut self, text: &str) {
        for (index, c) in text.chars().enumerate() {
            let outcome = self.press(ch(c));
            assert!(
                outcome.intent.is_none() && !outcome.interrupt_requested,
                "typed character number {index} asked for something"
            );
        }
    }

    fn rows(&self) -> Vec<String> {
        draw(&self.stack, 80, 24)
    }
}

// ---------------------------------------------------------------------------------------------
// 1. Approval choices
// ---------------------------------------------------------------------------------------------

fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn approval_choices_follow_fixed_order_filtered_by_offer() {
    assert_eq!(
        display_choices(&owned(&["once", "deny", "session", "always"])),
        ["once", "session", "always", "deny"],
        "the full offer is shown with deny last"
    );
    assert_eq!(
        display_choices(&owned(&["once", "deny"])),
        ["once", "deny"],
        "a short offer is shown as offered, in the fixed order"
    );
    assert_eq!(
        display_choices(&owned(&["deny", "once"])),
        ["once", "deny"],
        "the payload order never decides the display order"
    );
    assert_eq!(
        display_choices(&owned(&["once", "deny", "session", "always", "extra"])),
        ["once", "session", "always", "deny", "extra"],
        "an unknown offered string comes last"
    );
    assert_eq!(
        display_choices(&owned(&["zeta", "once", "alpha", "deny", "session"])),
        ["once", "session", "deny", "zeta", "alpha"],
        "unknown strings keep their payload order after the known ones"
    );
    assert_eq!(
        display_choices(&owned(&["always", "session"])),
        ["session", "always"],
        "only the offered strings are shown"
    );
    assert!(display_choices(&[]).is_empty());
}

// ---------------------------------------------------------------------------------------------
// 2. Approval snapshots
// ---------------------------------------------------------------------------------------------

#[test]
fn approval_card_snapshot_80_and_160_cols() {
    let now = Instant::now();

    // Full card: command, a description that wraps at 80 columns, the timeout line, all four
    // choices with the highlight on the first.
    let mut full = approval(
        "rm -rf .pytest_cache",
        &["once", "deny", "session", "always"],
    );
    full.description = Some(
        "Recursively removes the pytest cache directory and everything below it; \
         this cannot be undone."
            .to_owned(),
    );
    full.timeout_s = Some(300.0);
    let mut stack = PromptStack::new();
    stack.sync(Some(MAIN), &[&full], &[], true, now);
    assert_snapshot(
        &stack,
        80,
        24,
        snapshot!("approval_full_80"),
        "approval_full_80",
    );
    assert_snapshot(
        &stack,
        160,
        48,
        snapshot!("approval_full_160"),
        "approval_full_160",
    );

    // Ten row cap with the hidden row count, the edit line, a timeout rounded down, no
    // description, and a session that is not the foreground one.
    let command = (1..=12)
        .map(|n| format!("echo step-{n:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut capped = approval_in("f00dfeed-0001", "p-2", &command, None, &["once", "deny"]);
    capped.edit = Some(json!({ "command": "echo replaced" }));
    capped.timeout_s = Some(45.9);
    let mut stack = PromptStack::new();
    stack.sync(Some(MAIN), &[&capped], &[], true, now);
    assert_snapshot(
        &stack,
        80,
        24,
        snapshot!("approval_capped_80"),
        "approval_capped_80",
    );
    assert_snapshot(
        &stack,
        160,
        48,
        snapshot!("approval_capped_160"),
        "approval_capped_160",
    );

    // A hostile command: control and bidirectional characters are drawn as visible stand-ins.
    let hostile = approval_in(
        MAIN,
        "p-3",
        "echo \u{1b}[2J\u{202e} done",
        None,
        &["once", "deny"],
    );
    let mut stack = PromptStack::new();
    stack.sync(Some(MAIN), &[&hostile], &[], true, now);
    assert_snapshot(
        &stack,
        80,
        24,
        snapshot!("approval_hostile_80"),
        "approval_hostile_80",
    );
    assert_snapshot(
        &stack,
        160,
        48,
        snapshot!("approval_hostile_160"),
        "approval_hostile_160",
    );
}

// ---------------------------------------------------------------------------------------------
// 3. Approval keys
// ---------------------------------------------------------------------------------------------

#[test]
fn approval_keys_matrix() {
    let cell = standard_approval();

    // The highlight starts on the first shown choice.
    let mut rig = Rig::approval(&cell);
    let rows = rig.rows();
    assert!(
        has_row(&rows, "› 1. once"),
        "highlight starts on once: {rows:?}"
    );
    assert!(has_row(&rows, "  2. session"));
    assert!(has_row(&rows, "  4. deny"));
    let (key, choice, note) = rig.press(named(KeyCode::Enter)).approve();
    assert_eq!(key, cell.key);
    assert_eq!(choice, "once");
    assert_eq!(note, None);

    // A digit sends the raw string shown next to it, not a payload position.
    for (digit, expected) in [
        ('1', "once"),
        ('2', "session"),
        ('3', "always"),
        ('4', "deny"),
    ] {
        let mut rig = Rig::approval(&cell);
        let (key, choice, note) = rig.press(ch(digit)).approve();
        assert_eq!(key, cell.key, "digit {digit}");
        assert_eq!(choice, expected, "digit {digit}");
        assert_eq!(note, None, "digit {digit}");
    }

    // Digits out of range and a non-digit do nothing, and are consumed rather than passed on.
    for digit in ['0', '5', '9'] {
        let mut rig = Rig::approval(&cell);
        rig.press(ch(digit)).nothing();
        let (_, choice, _) = rig.press(named(KeyCode::Enter)).approve();
        assert_eq!(choice, "once", "digit {digit} must not move the highlight");
    }

    // Down moves the highlight and stops at the last choice.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Down)).nothing();
    assert!(has_row(&rig.rows(), "› 2. session"));
    for _ in 0..10 {
        rig.press(named(KeyCode::Down)).nothing();
    }
    let rows = rig.rows();
    assert!(
        has_row(&rows, "› 4. deny"),
        "Down stops at the end: {rows:?}"
    );
    let (_, choice, _) = rig.press(named(KeyCode::Enter)).approve();
    assert_eq!(choice, "deny");

    // Up stops at the first choice.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Down)).nothing();
    rig.press(named(KeyCode::Down)).nothing();
    for _ in 0..10 {
        rig.press(named(KeyCode::Up)).nothing();
    }
    assert!(has_row(&rig.rows(), "› 1. once"));
    let (_, choice, _) = rig.press(named(KeyCode::Enter)).approve();
    assert_eq!(choice, "once");

    // Esc sends deny.
    let mut rig = Rig::approval(&cell);
    let (key, choice, note) = rig.press(named(KeyCode::Esc)).approve();
    assert_eq!(key, cell.key);
    assert_eq!(choice, "deny");
    assert_eq!(note, None);

    // Tab opens the note field. Digits are text there and a digit never sends.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Tab)).nothing();
    rig.type_text("go 2 ");
    assert!(
        has_row(&rig.rows(), "note: go 2"),
        "the note is drawn: {:?}",
        rig.rows()
    );
    // Esc goes back to the choices and keeps the note.
    rig.press(named(KeyCode::Esc)).nothing();
    // A digit on the choices sends that choice with the note exactly as typed.
    let (_, choice, note) = rig.press(ch('2')).approve();
    assert_eq!(choice, "session");
    assert_eq!(note.as_deref(), Some("go 2 "), "the note is not trimmed");

    // In the note field Enter sends the highlighted choice with the note.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Tab)).nothing();
    rig.type_text("x");
    let (_, choice, note) = rig.press(named(KeyCode::Enter)).approve();
    assert_eq!(choice, "once");
    assert_eq!(note.as_deref(), Some("x"));

    // Tab leaves the note field too, keeping the note.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Tab)).nothing();
    rig.type_text("x");
    rig.press(named(KeyCode::Tab)).nothing();
    let (_, choice, note) = rig.press(named(KeyCode::Enter)).approve();
    assert_eq!(choice, "once");
    assert_eq!(note.as_deref(), Some("x"));

    // A digit typed in the note field is text.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Tab)).nothing();
    rig.type_text("1");
    let (_, choice, note) = rig.press(named(KeyCode::Enter)).approve();
    assert_eq!(choice, "once");
    assert_eq!(note.as_deref(), Some("1"));

    // Backspace edits the note.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Tab)).nothing();
    rig.type_text("ab");
    rig.press(named(KeyCode::Backspace)).nothing();
    let (_, _, note) = rig.press(named(KeyCode::Enter)).approve();
    assert_eq!(note.as_deref(), Some("a"));

    // A blank note counts as none.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Tab)).nothing();
    rig.type_text("  ");
    rig.press(named(KeyCode::Esc)).nothing();
    let (_, choice, note) = rig.press(ch('1')).approve();
    assert_eq!(choice, "once");
    assert_eq!(note, None, "a whitespace note is no note");

    // Esc on the choices sends deny and the note travels with it.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Tab)).nothing();
    rig.type_text("do this instead");
    rig.press(named(KeyCode::Esc)).nothing();
    let (_, choice, note) = rig.press(named(KeyCode::Esc)).approve();
    assert_eq!(choice, "deny");
    assert_eq!(note.as_deref(), Some("do this instead"));

    // Ctrl+C asks for an interrupt and no approval, and drops the note.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Tab)).nothing();
    rig.type_text("dropped");
    rig.press(named(KeyCode::Esc)).nothing();
    assert!(
        rig.press(ctrl_c()).interrupting().is_none(),
        "an approval card sends no answer on Ctrl+C"
    );

    // Ctrl+C inside the note field is the same.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Tab)).nothing();
    rig.type_text("dropped");
    assert!(rig.press(ctrl_c()).interrupting().is_none());

    // The cards of the session then show stopping and ignore keys until stop_cleared.
    let mut rig = Rig::approval(&cell);
    assert!(rig.press(ctrl_c()).interrupting().is_none());
    assert!(
        has_row(&rig.rows(), "stopping…"),
        "stopping hint: {:?}",
        rig.rows()
    );
    rig.at += Duration::from_millis(100);
    rig.press(ch('1')).nothing();
    rig.press(named(KeyCode::Enter)).nothing();
    rig.press(named(KeyCode::Esc)).nothing();
    rig.stack.stop_cleared();
    rig.at += Duration::from_secs(10);
    assert!(!has_row(&rig.rows(), "stopping…"));
    let (_, choice, _) = rig.press(ch('1')).approve();
    assert_eq!(choice, "once");

    // A choice with a control character is drawn with a stand-in and sent raw.
    let odd = approval("ls", &["once", "deny", "ev\u{1b}il"]);
    let mut rig = Rig::approval(&odd);
    let rows = rig.rows();
    assert!(has_row(&rows, "  3. ev␛il"), "stand-in drawn: {rows:?}");
    let (_, choice, _) = rig.press(ch('3')).approve();
    assert_eq!(choice, "ev\u{1b}il", "the raw string is sent");
}

#[test]
fn approval_keys_are_ignored_inside_the_guard_except_ctrl_c() {
    let cell = standard_approval();
    let start = Instant::now();

    let mut stack = PromptStack::new();
    stack.sync(Some(MAIN), &[&cell], &[], true, start);
    consumed(stack.handle_key(ch('1'), start + INSIDE_GUARD)).nothing();
    consumed(stack.handle_key(named(KeyCode::Enter), start + INSIDE_GUARD)).nothing();
    consumed(stack.handle_key(named(KeyCode::Esc), start + INSIDE_GUARD)).nothing();
    let (_, choice, _) = consumed(stack.handle_key(ch('1'), start + AFTER_GUARD)).approve();
    assert_eq!(
        choice, "once",
        "the same key works once the guard has passed"
    );

    let mut stack = PromptStack::new();
    stack.sync(Some(MAIN), &[&cell], &[], true, start);
    assert!(
        consumed(stack.handle_key(ctrl_c(), start + INSIDE_GUARD))
            .interrupting()
            .is_none(),
        "Ctrl+C is never held back by the guard"
    );
}

#[test]
fn approval_without_deny_offered_closes_with_a_notice_and_sends_nothing() {
    let cell = approval("ls", &["once", "session"]);
    let mut rig = Rig::approval(&cell);
    let outcome = rig.press(named(KeyCode::Esc));
    assert!(outcome.intent.is_none(), "deny was not offered");
    assert!(!outcome.interrupt_requested);
    assert_eq!(
        outcome.notice.as_deref(),
        Some("closed without an answer: Hermes did not offer deny")
    );

    // The stack hides that key from then on.
    assert!(rig.stack.section().is_none(), "the card is hidden");
    assert!(matches!(
        rig.stack.handle_key(ch('1'), rig.at),
        KeyOutcome::NotHandled
    ));

    // Once the key has left the open prompts the stack forgets it, so it can show again.
    rig.stack.sync(Some(MAIN), &[], &[], true, rig.at);
    rig.stack.sync(Some(MAIN), &[&cell], &[], true, rig.at);
    assert!(rig.stack.section().is_some());
}

#[test]
fn approval_answering_shows_the_sent_choice_and_ignores_keys() {
    let mut cell = standard_approval();
    cell.state = PromptState::Answering("once".to_owned());
    let mut rig = Rig::approval(&cell);
    assert!(
        has_row(&rig.rows(), "sending once…"),
        "answering hint: {:?}",
        rig.rows()
    );
    rig.press(ch('1')).nothing();
    rig.press(ch('4')).nothing();
    rig.press(named(KeyCode::Enter)).nothing();
    rig.press(named(KeyCode::Esc)).nothing();
    // Ctrl+C asks for the interrupt and sends no deny: the answer in flight stands.
    assert!(rig.press(ctrl_c()).interrupting().is_none());
}

// ---------------------------------------------------------------------------------------------
// 4. Clarification, single select and free text
// ---------------------------------------------------------------------------------------------

#[test]
fn clarify_single_other_and_free_text() {
    let cell = clarify("Which environment?", &["staging", "prod"], false);

    // A digit sends the label; Enter sends the first one.
    let mut rig = Rig::with(&[&cell], &[]);
    let (key, answer) = rig.press(ch('2')).clarify();
    assert_eq!(key, cell.key);
    assert_eq!(answer, "prod");
    let mut rig = Rig::with(&[&cell], &[]);
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, "staging");

    // Digit 0 and a digit past the last row do nothing.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch('0')).nothing();
    rig.press(ch('4')).nothing();
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, "staging");

    // Up and Down stop at the ends, the Other row included.
    let mut rig = Rig::with(&[&cell], &[]);
    for _ in 0..5 {
        rig.press(named(KeyCode::Down)).nothing();
    }
    let rows = rig.rows();
    assert!(
        has_row(&rows, "› 3. Other (type an answer)"),
        "Down stops on the Other row: {rows:?}"
    );
    for _ in 0..5 {
        rig.press(named(KeyCode::Up)).nothing();
    }
    assert!(has_row(&rig.rows(), "› 1. staging"));

    // The Other row opens the text field, by Enter or by its digit; Enter then sends the text.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(named(KeyCode::Down)).nothing();
    rig.press(named(KeyCode::Down)).nothing();
    rig.press(named(KeyCode::Enter)).nothing();
    rig.type_text("qa");
    let rows = rig.rows();
    assert!(has_row(&rows, "answer: qa"), "text field drawn: {rows:?}");
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, "qa");

    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch('3')).nothing();
    rig.type_text("42 now");
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, "42 now", "digits are text in the field");

    // Typed text is sent as typed, not trimmed; Backspace edits it.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch('3')).nothing();
    rig.type_text(" qaz ");
    rig.press(named(KeyCode::Backspace)).nothing();
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, " qaz");

    // Enter on a blank text does nothing in single select.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch('3')).nothing();
    rig.press(named(KeyCode::Enter)).nothing();
    rig.type_text("  ");
    rig.press(named(KeyCode::Enter)).nothing();

    // Esc in the field goes back to the choices without sending, keeping the text.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch('3')).nothing();
    rig.type_text("qa");
    rig.press(named(KeyCode::Esc)).nothing();
    rig.press(ch('3')).nothing();
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, "qa", "the text typed before Esc is kept");

    // Esc on the choices sends the empty answer.
    let mut rig = Rig::with(&[&cell], &[]);
    let (key, answer) = rig.press(named(KeyCode::Esc)).clarify();
    assert_eq!(key, cell.key);
    assert_eq!(answer, "");

    // Ctrl+C sends the empty answer and asks for the interrupt.
    let mut rig = Rig::with(&[&cell], &[]);
    match rig.press(ctrl_c()).interrupting() {
        Some(PromptIntent::Clarify { key, answer }) => {
            assert_eq!(key, cell.key);
            assert_eq!(answer, "");
        }
        other => panic!("expected the empty answer, got {}", intent_kind(&other)),
    }

    // Without choices the card is a text field: Enter sends, blank does nothing, Esc sends "".
    let free = clarify("Anything else?", &[], false);
    let mut rig = Rig::with(&[&free], &[]);
    let rows = rig.rows();
    assert_eq!(rows.first().map(String::as_str), Some("Question"));
    assert!(has_row(&rows, "Anything else?"));
    assert!(has_row(&rows, "answer:"), "an empty text field: {rows:?}");
    assert!(
        has_row(&rows, "Enter send · Esc skip · Ctrl+C skip and stop"),
        "text only hint: {rows:?}"
    );
    rig.press(named(KeyCode::Enter)).nothing();
    rig.type_text("42");
    let (key, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(key, free.key);
    assert_eq!(answer, "42");
    let mut rig = Rig::with(&[&free], &[]);
    rig.type_text("draft");
    let (_, answer) = rig.press(named(KeyCode::Esc)).clarify();
    assert_eq!(answer, "", "Esc skips the question");

    // multi_select without choices is still a plain text field, answered with the text.
    let multi_free = clarify("Any notes?", &[], true);
    let mut rig = Rig::with(&[&multi_free], &[]);
    assert!(has_row(&rig.rows(), "answer:"));
    rig.type_text("ok");
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, "ok", "no JSON array without choices");

    // Digit 9 addresses row 9; rows from 10 are reached with the arrow keys only.
    let labels: Vec<String> = (1..=11).map(|n| format!("c{n}")).collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let long = clarify("Pick one", &label_refs, false);
    let mut rig = Rig::with(&[&long], &[]);
    let (_, answer) = rig.press(ch('9')).clarify();
    assert_eq!(answer, "c9");
    let mut rig = Rig::with(&[&long], &[]);
    rig.press(ch('0')).nothing();
    for _ in 0..9 {
        rig.press(named(KeyCode::Down)).nothing();
    }
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, "c10");
}

// ---------------------------------------------------------------------------------------------
// 5. Clarification, multi select
// ---------------------------------------------------------------------------------------------

#[test]
fn clarify_multi_select_sends_json_array() {
    let cell = clarify("Which environments?", &["staging", "prod"], true);

    // Digits toggle; the array lists ticked labels in choice order, not tick order.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch('2')).nothing();
    rig.press(ch('1')).nothing();
    let rows = rig.rows();
    assert!(
        has_row_containing(&rows, "1. [x] staging"),
        "ticked: {rows:?}"
    );
    assert!(has_row_containing(&rows, "2. [x] prod"), "ticked: {rows:?}");
    let (key, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(key, cell.key);
    assert_eq!(answer, r#"["staging","prod"]"#);

    // The Other row's digit opens the text field; its text goes last.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch('2')).nothing();
    rig.press(ch('1')).nothing();
    rig.press(ch('3')).nothing();
    rig.type_text("qa");
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, r#"["staging","prod","qa"]"#);

    // Enter alone sends the empty string.
    let mut rig = Rig::with(&[&cell], &[]);
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, "");

    // Space toggles the highlighted row.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch(' ')).nothing();
    rig.press(named(KeyCode::Down)).nothing();
    rig.press(ch(' ')).nothing();
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, r#"["staging","prod"]"#);

    // Toggling twice unticks.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch(' ')).nothing();
    rig.press(ch(' ')).nothing();
    rig.press(ch('1')).nothing();
    rig.press(ch('1')).nothing();
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, "");

    // Other text alone is a one element array.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch('3')).nothing();
    rig.type_text("qa");
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, r#"["qa"]"#);

    // Esc in the field goes back keeping the text; Enter on the choices then sends everything.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch('1')).nothing();
    rig.press(ch('3')).nothing();
    rig.type_text("qa");
    rig.press(named(KeyCode::Esc)).nothing();
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, r#"["staging","qa"]"#);

    // Space on the Other row opens the text field, and Enter there sends the whole answer.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(named(KeyCode::Down)).nothing();
    rig.press(named(KeyCode::Down)).nothing();
    rig.press(ch(' ')).nothing();
    rig.type_text("x");
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, r#"["x"]"#);

    // Enter on the Other row sends; it does not open the field.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(named(KeyCode::Down)).nothing();
    rig.press(named(KeyCode::Down)).nothing();
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer, "");

    // Esc on the choices skips, ticks or not.
    let mut rig = Rig::with(&[&cell], &[]);
    rig.press(ch('1')).nothing();
    let (_, answer) = rig.press(named(KeyCode::Esc)).clarify();
    assert_eq!(answer, "");

    // The pure helper agrees with the card.
    let choices = owned(&["staging", "prod"]);
    assert_eq!(
        multi_select_answer(&choices, &[0, 1], None),
        r#"["staging","prod"]"#
    );
    assert_eq!(
        multi_select_answer(&choices, &[0, 1], Some("qa")),
        r#"["staging","prod","qa"]"#
    );
    assert_eq!(multi_select_answer(&choices, &[1], None), r#"["prod"]"#);
    assert_eq!(multi_select_answer(&choices, &[], Some("qa")), r#"["qa"]"#);
    assert_eq!(multi_select_answer(&choices, &[], Some("   ")), "");
    assert_eq!(multi_select_answer(&choices, &[], None), "");
    let quoted = owned(&["say \"hi\"", "back\\slash"]);
    assert_eq!(
        multi_select_answer(&quoted, &[0, 1], None),
        r#"["say \"hi\"","back\\slash"]"#,
        "labels are escaped as JSON strings"
    );
}

// ---------------------------------------------------------------------------------------------
// 6. Clarification length
// ---------------------------------------------------------------------------------------------

/// Every row of the card, however tall it is.
fn draw_everything(stack: &PromptStack, width: u16) -> Vec<String> {
    draw(stack, width, 2000)
}

#[test]
fn clarify_length_counts_code_points() {
    let free = clarify("Tell me everything", &[], false);

    // 16,384 CJK characters are 49,152 bytes and are still sent.
    let at_limit = "字".repeat(16_384);
    let mut rig = Rig::with(&[&free], &[]);
    rig.paste(&at_limit).nothing();
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer.chars().count(), 16_384);
    assert_eq!(answer.len(), 49_152);

    // One more is refused with a line, and nothing is sent.
    let over = "字".repeat(16_385);
    let mut rig = Rig::with(&[&free], &[]);
    rig.paste(&over).nothing();
    let refused = rig.press(named(KeyCode::Enter));
    assert!(
        refused.intent.is_none(),
        "an answer over the limit is not sent"
    );
    assert!(!refused.interrupt_requested);
    let rows = draw_everything(&rig.stack, 80);
    assert!(
        has_row_containing(&rows, "answer too long ("),
        "the too-long line is drawn"
    );
    assert!(has_row_containing(&rows, "of 16,384 characters)"));

    // Removing one character makes it sendable again.
    rig.press(named(KeyCode::Backspace)).nothing();
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer.chars().count(), 16_384);

    // A multi select counts the JSON text that would be sent, not the labels. Two labels of 8,190
    // characters add up to 16,380, but the array text is 16,387.
    let long_a = "語".repeat(8_190);
    let long_b = "語".repeat(8_190);
    let multi = clarify("Pick", &[long_a.as_str(), long_b.as_str()], true);
    let mut rig = Rig::with(&[&multi], &[]);
    rig.press(ch('1')).nothing();
    rig.press(ch('2')).nothing();
    let refused = rig.press(named(KeyCode::Enter));
    assert!(
        refused.intent.is_none(),
        "the array text is over the limit even though the labels are not"
    );
    assert!(!refused.interrupt_requested);
    assert!(has_row_containing(
        &draw_everything(&rig.stack, 80),
        "answer too long ("
    ));

    // Labels that make the array text exactly 16,384 code points are sent.
    let exact_a = "語".repeat(8_189);
    let exact_b = "語".repeat(8_188);
    let multi = clarify("Pick", &[exact_a.as_str(), exact_b.as_str()], true);
    let mut rig = Rig::with(&[&multi], &[]);
    rig.press(ch('1')).nothing();
    rig.press(ch('2')).nothing();
    let (_, answer) = rig.press(named(KeyCode::Enter)).clarify();
    assert_eq!(answer.chars().count(), 16_384);
    assert!(answer.starts_with("[\"語"));
}

// ---------------------------------------------------------------------------------------------
// 7. Masked input
// ---------------------------------------------------------------------------------------------

fn value_row(rows: &[String]) -> &str {
    rows.iter()
        .find(|row| row.starts_with("value:"))
        .map(String::as_str)
        .expect("the card has a value row")
}

#[test]
fn masked_input_draws_bullets_only() {
    // A typed marker shows as bullets only.
    let mut rig = Rig::masked(sudo_view());
    rig.type_text(MARKER);
    let rows = rig.rows();
    let expected = format!("value: {}", "•".repeat(MARKER.chars().count()));
    assert_eq!(value_row(&rows), expected);
    for row in &rows {
        assert!(!row.contains("TALARIA"), "the marker leaked onto a row");
        assert!(!row.contains("7f3a"), "the marker leaked onto a row");
    }
    let (request_id, value) = rig.press(named(KeyCode::Enter)).masked();
    assert_eq!(request_id, MASKED_ID);
    assert_eq!(value.len_chars(), MARKER.chars().count());
    assert_eq!(value.into_inner().as_str(), MARKER);

    // The intent's Debug text names the kind and the length, never the value.
    let mut rig = Rig::masked(sudo_view());
    rig.type_text(MARKER);
    let intent = rig.press(named(KeyCode::Enter)).intent;
    let text = format!("{intent:?}");
    assert!(!text.contains(MARKER), "Debug output leaked the value");
    assert!(!text.contains("7f3a"), "Debug output leaked the value");

    // A 300 character paste shows 32 bullets, and the buffer keeps all of it.
    let mut rig = Rig::masked(sudo_view());
    let long = "p".repeat(300);
    rig.paste(&long).nothing();
    let rows = rig.rows();
    assert_eq!(value_row(&rows), format!("value: {}", "•".repeat(32)));
    let (_, value) = rig.press(named(KeyCode::Enter)).masked();
    assert_eq!(value.len_chars(), 300);
    assert_eq!(value.into_inner().as_str(), long);

    // A paste stops at its first line break and never acts as Enter.
    for pasted in ["abc\ndef", "abc\r\ndef", "abc\rdef"] {
        let mut rig = Rig::masked(sudo_view());
        rig.paste(pasted).nothing();
        assert_eq!(
            value_row(&rig.rows()),
            format!("value: {}", "•".repeat(3)),
            "only the text before the break is kept"
        );
        let (_, value) = rig.press(named(KeyCode::Enter)).masked();
        assert_eq!(value.into_inner().as_str(), "abc");
    }

    // Backspace removes the last character.
    let mut rig = Rig::masked(sudo_view());
    rig.type_text("abc");
    rig.press(named(KeyCode::Backspace)).nothing();
    let (_, value) = rig.press(named(KeyCode::Enter)).masked();
    assert_eq!(value.into_inner().as_str(), "ab");

    // Esc answers with an empty value, even with text typed.
    let mut rig = Rig::masked(sudo_view());
    rig.type_text(MARKER);
    let (request_id, value) = rig.press(named(KeyCode::Esc)).masked();
    assert_eq!(request_id, MASKED_ID);
    assert_eq!(value.len_chars(), 0);

    // Ctrl+C declines too, and asks for the interrupt.
    let mut rig = Rig::masked(sudo_view());
    rig.type_text("abc");
    match rig.press(ctrl_c()).interrupting() {
        Some(PromptIntent::Masked { request_id, value }) => {
            assert_eq!(request_id, MASKED_ID);
            assert_eq!(value.len_chars(), 0);
        }
        other => panic!("expected a declined value, got {}", intent_kind(&other)),
    }

    // The buffer holds 16,384 characters; the rest is dropped with a line until Backspace.
    let mut rig = Rig::masked(sudo_view());
    rig.paste(&"a".repeat(16_400)).nothing();
    let rows = rig.rows();
    assert_eq!(value_row(&rows), format!("value: {}", "•".repeat(32)));
    assert!(
        has_row(&rows, "value too long (16,384 characters at most)"),
        "too-long line: {rows:?}"
    );
    rig.press(named(KeyCode::Backspace)).nothing();
    assert!(!has_row(
        &rig.rows(),
        "value too long (16,384 characters at most)"
    ));
    let (_, value) = rig.press(named(KeyCode::Enter)).masked();
    assert_eq!(value.len_chars(), 16_383);
}

#[test]
fn masked_keys_inside_the_guard_are_not_typed() {
    let view = sudo_view();
    let start = Instant::now();
    let mut stack = PromptStack::new();
    stack.sync(Some(MAIN), &[], std::slice::from_ref(&view), true, start);
    for c in "abc".chars() {
        consumed(stack.handle_key(ch(c), start + INSIDE_GUARD)).nothing();
    }
    consumed(stack.handle_key(ch('z'), start + AFTER_GUARD)).nothing();
    let rows = draw(&stack, 80, 24);
    assert_eq!(value_row(&rows), "value: •");
}

// ---------------------------------------------------------------------------------------------
// 8. Masked card titles
// ---------------------------------------------------------------------------------------------

#[test]
fn masked_card_titles_per_kind_snapshot() {
    let now = Instant::now();
    let cases = [
        (
            "sudo",
            sudo_view(),
            snapshot!("masked_sudo_80"),
            snapshot!("masked_sudo_160"),
        ),
        (
            "sudo without a command",
            masked_view("sudo", &[]),
            snapshot!("masked_sudo_no_command_80"),
            snapshot!("masked_sudo_no_command_160"),
        ),
        (
            "sudo with an empty command",
            masked_view("sudo", &[("command", "")]),
            snapshot!("masked_sudo_no_command_80"),
            snapshot!("masked_sudo_no_command_160"),
        ),
        (
            "secret",
            masked_view(
                "secret",
                &[
                    ("env_var", "OPENAI_API_KEY"),
                    ("prompt", "Paste the key for the search tool"),
                ],
            ),
            snapshot!("masked_secret_80"),
            snapshot!("masked_secret_160"),
        ),
        (
            "vault code with a site",
            masked_view(
                "vault.code",
                &[
                    ("site", "example.org"),
                    ("hint", "6 digit code from your authenticator"),
                ],
            ),
            snapshot!("masked_vault_code_80"),
            snapshot!("masked_vault_code_160"),
        ),
        (
            "vault code without a site",
            masked_view(
                "vault.code",
                &[("hint", "6 digit code from your authenticator")],
            ),
            snapshot!("masked_vault_code_no_site_80"),
            snapshot!("masked_vault_code_no_site_160"),
        ),
        (
            "vault unlock",
            masked_view(
                "vault.unlock_prompt",
                &[("backend", "bitwarden"), ("display_name", "Team vault")],
            ),
            snapshot!("masked_vault_unlock_80"),
            snapshot!("masked_vault_unlock_160"),
        ),
    ];
    for (name, view, at_80, at_160) in cases {
        let mut stack = PromptStack::new();
        stack.sync(Some(MAIN), &[], std::slice::from_ref(&view), true, now);
        assert_snapshot(&stack, 80, 24, at_80, name);
        assert_snapshot(&stack, 160, 48, at_160, name);
    }
}

// ---------------------------------------------------------------------------------------------
// 9. The stack
// ---------------------------------------------------------------------------------------------

#[test]
fn stack_orders_masked_first_and_focuses_only_empty_draft() {
    let approval_cell = approval("rm -rf .pytest_cache", &["once", "deny"]);
    let question = clarify("Which environment?", &["staging", "prod"], false);
    let view = sudo_view();
    let start = Instant::now();

    // Masked first, counting every card, and keys reach only that card.
    let mut stack = PromptStack::new();
    stack.sync(
        Some(MAIN),
        &[&approval_cell, &question],
        std::slice::from_ref(&view),
        true,
        start,
    );
    let rows = draw(&stack, 80, 24);
    assert_eq!(
        rows.first().map(String::as_str),
        Some("sudo password (1 of 3)")
    );
    consumed(stack.handle_key(ch('1'), start + AFTER_GUARD)).nothing();
    let rows = draw(&stack, 80, 24);
    assert_eq!(
        value_row(&rows),
        "value: •",
        "the key was typed into the masked card, not used as the approval's choice"
    );
    let (request_id, value) =
        consumed(stack.handle_key(named(KeyCode::Enter), start + AFTER_GUARD)).masked();
    assert_eq!(request_id, MASKED_ID);
    assert_eq!(value.into_inner().as_str(), "1");

    // Once the masked card is gone the approval is first, and it takes focus anew with a fresh
    // guard: a key inside the guard is ignored.
    let later = start + Duration::from_secs(5);
    stack.sync(Some(MAIN), &[&approval_cell, &question], &[], true, later);
    let rows = draw(&stack, 80, 24);
    assert_eq!(
        rows.first().map(String::as_str),
        Some("Approval needed (1 of 2)")
    );
    consumed(stack.handle_key(ch('1'), later + INSIDE_GUARD)).nothing();
    let (key, choice, note) = consumed(stack.handle_key(ch('1'), later + AFTER_GUARD)).approve();
    assert_eq!(key, approval_cell.key);
    assert_eq!(choice, "once");
    assert_eq!(note, None);

    // A single card has no counter.
    stack.sync(Some(MAIN), &[&question], &[], true, later);
    assert_eq!(
        draw(&stack, 80, 24).first().map(String::as_str),
        Some("Question")
    );
}

#[test]
fn stack_does_not_take_keys_while_the_draft_has_text() {
    let approval_cell = standard_approval();
    let view = sudo_view();
    let start = Instant::now();

    // A masked card asks for the draft to be cleared; every key goes to the composer.
    let mut stack = PromptStack::new();
    stack.sync(
        Some(MAIN),
        &[&approval_cell],
        std::slice::from_ref(&view),
        false,
        start,
    );
    let rows = draw(&stack, 80, 24);
    assert!(
        has_row(&rows, "Clear your draft to type the value here"),
        "masked hint: {rows:?}"
    );
    let at = start + AFTER_GUARD;
    assert!(matches!(
        stack.handle_key(ch('1'), at),
        KeyOutcome::NotHandled
    ));
    assert!(matches!(
        stack.handle_key(named(KeyCode::Enter), at),
        KeyOutcome::NotHandled
    ));
    assert!(matches!(
        stack.handle_key(ctrl_c(), at),
        KeyOutcome::NotHandled
    ));
    assert!(matches!(
        stack.handle_paste("secret", at),
        KeyOutcome::NotHandled
    ));

    // An approval card says to send or clear the draft.
    let mut stack = PromptStack::new();
    stack.sync(Some(MAIN), &[&approval_cell], &[], false, start);
    let rows = draw(&stack, 80, 24);
    assert!(
        has_row(&rows, "Send or clear your draft to answer"),
        "approval hint: {rows:?}"
    );
    assert!(matches!(
        stack.handle_key(ch('1'), at),
        KeyOutcome::NotHandled
    ));

    // When the draft becomes empty the card takes focus anew, with a fresh guard.
    let emptied = start + Duration::from_secs(5);
    stack.sync(Some(MAIN), &[&approval_cell], &[], true, emptied);
    consumed(stack.handle_key(ch('1'), emptied + INSIDE_GUARD)).nothing();
    let (_, choice, _) = consumed(stack.handle_key(ch('1'), emptied + AFTER_GUARD)).approve();
    assert_eq!(choice, "once");
}

#[test]
fn stack_keeps_typed_state_of_a_card_pushed_back() {
    let approval_cell = standard_approval();
    let view = sudo_view();
    let start = Instant::now();

    let mut stack = PromptStack::new();
    stack.sync(Some(MAIN), &[&approval_cell], &[], true, start);
    let mut at = start + AFTER_GUARD;
    consumed(stack.handle_key(named(KeyCode::Tab), at)).nothing();
    for c in "keep me".chars() {
        consumed(stack.handle_key(ch(c), at)).nothing();
    }
    consumed(stack.handle_key(named(KeyCode::Esc), at)).nothing();

    // A masked card arrives in front and is answered.
    at += Duration::from_secs(1);
    stack.sync(
        Some(MAIN),
        &[&approval_cell],
        std::slice::from_ref(&view),
        true,
        at,
    );
    assert_eq!(
        draw(&stack, 80, 24).first().map(String::as_str),
        Some("sudo password (1 of 2)")
    );
    at += AFTER_GUARD;
    consumed(stack.handle_key(named(KeyCode::Esc), at)).masked();

    // The approval is first again; its note is still there.
    at += Duration::from_secs(1);
    stack.sync(Some(MAIN), &[&approval_cell], &[], true, at);
    let (_, choice, note) = consumed(stack.handle_key(ch('1'), at + AFTER_GUARD)).approve();
    assert_eq!(choice, "once");
    assert_eq!(note.as_deref(), Some("keep me"));
}

#[test]
fn stack_draws_a_foreign_session_and_a_counter_after_the_header() {
    let foreign = approval_in("f00dfeed-0001", "p-9", "ls", None, &["once", "deny"]);
    let other = clarify("Which environment?", &["staging", "prod"], false);
    let mut stack = PromptStack::new();
    stack.sync(Some(MAIN), &[&foreign, &other], &[], true, Instant::now());
    assert_eq!(
        draw(&stack, 80, 24).first().map(String::as_str),
        Some("Approval needed (1 of 2) · session f00dfeed")
    );

    // A foreign session's card takes keys like any first card, and answers with its own key.
    let at = Instant::now() + Duration::from_secs(1);
    let mut stack = PromptStack::new();
    stack.sync(Some(MAIN), &[&foreign], &[], true, at - AFTER_GUARD);
    let (key, choice, _) = consumed(stack.handle_key(ch('2'), at)).approve();
    assert_eq!(key, foreign.key);
    assert_eq!(choice, "deny");
}

/// Shift+Tab is the approvals toggle's key (#237): a focused card takes it and keeps its state, so
/// it never opens or closes the note field.
#[test]
fn shift_tab_leaves_the_note_field_alone() {
    let cell = standard_approval();
    let shift_tab = press(
        KeyCode::Tab,
        Mods {
            shift: true,
            ..Mods::default()
        },
    );

    // On the choices, Shift+Tab opens no note field: a digit still sends its choice.
    let mut rig = Rig::approval(&cell);
    rig.press(shift_tab).nothing();
    let (_, choice, note) = rig.press(ch('1')).approve();
    assert_eq!(choice, "once");
    assert_eq!(note, None);

    // In the note field, Shift+Tab does not leave it: a digit is still text.
    let mut rig = Rig::approval(&cell);
    rig.press(named(KeyCode::Tab)).nothing();
    rig.press(shift_tab).nothing();
    rig.type_text("1");
    assert!(
        has_row(&rig.rows(), "note: 1"),
        "the digit stays in the note: {:?}",
        rig.rows()
    );
}
