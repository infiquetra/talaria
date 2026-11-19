//! Oversized prompt cards draw a fixed number of lines and return promptly.

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::session::{CellId, PromptCell, PromptKey, PromptKind, PromptState};
use talaria_tui::live::LiveSection;
use talaria_tui::prompts::{MaskedView, PromptStack};

const MAIN: &str = "sess-main";
const WIDTH: u16 = 40;
const LINE_BUDGET: u16 = 24;
const TIME_BUDGET: Duration = Duration::from_millis(1000);

fn prompt_key(prompt: &str) -> PromptKey {
    PromptKey {
        session_id: MAIN.into(),
        prompt_id: prompt.to_owned(),
        execution_generation: 1,
    }
}

fn approval(command: &str, description: &str) -> PromptCell {
    PromptCell {
        id: CellId::new(1),
        key: prompt_key("approval"),
        kind: PromptKind::Approval,
        command: Some(command.to_owned()),
        description: Some(description.to_owned()),
        question: None,
        choices: vec!["once".to_owned(), "deny".to_owned()],
        multi_select: false,
        edit: None,
        timeout_s: None,
        state: PromptState::Open,
        sealed: false,
    }
}

fn clarify(question: &str, choices: &[&str]) -> PromptCell {
    PromptCell {
        id: CellId::new(2),
        key: prompt_key("clarify"),
        kind: PromptKind::Clarify,
        command: None,
        description: None,
        question: Some(question.to_owned()),
        choices: choices.iter().map(|choice| (*choice).to_owned()).collect(),
        multi_select: false,
        edit: None,
        timeout_s: None,
        state: PromptState::Open,
        sealed: false,
    }
}

fn draw(stack: &PromptStack) -> (u16, Vec<String>, Duration) {
    let started = Instant::now();
    let section = stack.section().expect("a card should be drawn");
    let height = section.height(WIDTH);
    let rows = height.min(LINE_BUDGET);
    let mut buf = Buffer::empty(Rect::new(0, 0, WIDTH, rows.max(1)));
    section.render(Rect::new(0, 0, WIDTH, rows.max(1)), &mut buf);
    let elapsed = started.elapsed();
    let lines = (0..rows)
        .map(|y| {
            (0..WIDTH)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .filter(|line| !line.is_empty())
        .collect();
    (height, lines, elapsed)
}

fn assert_bounded(label: &str, stack: &PromptStack) {
    let (height, lines, elapsed) = draw(stack);
    assert!(height <= LINE_BUDGET, "{label} drew {height} lines");
    assert!(elapsed < TIME_BUDGET, "{label} took {elapsed:?}");
    assert!(
        lines.iter().any(|line| line.contains("more lines")),
        "{label} hid the overflow without a marker: {lines:?}"
    );
}

#[test]
fn prompt_cards_render_oversized_fields_with_a_line_cap() {
    let huge = "A".repeat(8 * 1024 * 1024);
    let now = Instant::now();

    let view = MaskedView {
        request_id: "srq-huge".to_owned(),
        session_id: Some(MAIN.to_owned()),
        method: "sudo".to_owned(),
        fields: vec![("command".to_owned(), huge.clone())],
    };
    let mut masked = PromptStack::new();
    masked.sync(Some(MAIN), &[], &[view], true, now);
    assert_bounded("sudo.command", &masked);

    let cell = approval("true", &huge);
    let mut described = PromptStack::new();
    described.sync(Some(MAIN), &[&cell], &[], true, now);
    assert_bounded("approval.description", &described);

    let command = approval(&huge, "short description");
    let mut commanded = PromptStack::new();
    commanded.sync(Some(MAIN), &[&command], &[], true, now);
    assert_bounded("approval.command", &commanded);

    let question = clarify(&huge, &["yes", "no"]);
    let mut asked = PromptStack::new();
    asked.sync(Some(MAIN), &[&question], &[], true, now);
    assert_bounded("clarify.question", &asked);

    let choice = clarify("Which file?", &[&huge]);
    let mut labeled = PromptStack::new();
    labeled.sync(Some(MAIN), &[&choice], &[], true, now);
    assert_bounded("clarify choice", &labeled);
}
