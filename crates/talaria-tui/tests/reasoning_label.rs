//! Each reasoning block starts with one quiet label. An answer does not.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use talaria_core::session::{AssistantCell, Cell, CellId, ReasoningCell};
use talaria_tui::history::CellLines;
use talaria_tui::theme::cells::ThemedCells;
use talaria_tui::theme::{
    ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource, TerminalColors,
    ThemeInputs, resolve,
};

const WIDTH: u16 = 80;

fn theme() -> talaria_tui::theme::Theme {
    let mut theme = resolve(&ThemeInputs {
        skin: None,
        skipped: &[],
        follow_hermes_skin: false,
        polarity: (Polarity::Dark, PolaritySource::Config),
        depth: (ColorDepth::TrueColor, DepthSource::Config),
        motion: (Motion::Full, MotionSource::Default),
        terminal: TerminalColors::default(),
    });
    theme.reasoning = Style::default().fg(Color::Rgb(0xc4, 0xa3, 0x5a));
    theme
}

fn reasoning(id: u64, text: &str) -> Cell {
    Cell::Reasoning(ReasoningCell {
        id: CellId::new(id),
        text: text.to_string(),
        sealed: true,
    })
}

fn answer(id: u64, text: &str) -> Cell {
    Cell::Assistant(AssistantCell {
        id: CellId::new(id),
        text: text.to_string(),
        sealed: true,
        interim: false,
        transformed: false,
    })
}

fn is_label(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed == "thinking" || trimmed == "\u{2234} thinking"
}

fn painted(line: &Line<'_>) -> Vec<Style> {
    line.spans
        .iter()
        .filter(|span| !span.content.is_empty())
        .map(|span| line.style.patch(span.style))
        .collect()
}

fn assert_label(line: &Line<'_>, reasoning: Style) {
    let text = line.to_string();
    assert!(
        is_label(&text),
        "reasoning starts with one dim label line, saw {text:?}"
    );
    let styles = painted(line);
    assert!(!styles.is_empty(), "the label line is drawn");
    for style in styles {
        assert!(
            style.add_modifier.contains(Modifier::DIM),
            "the label line is dim"
        );
        assert_eq!(
            style.fg, reasoning.fg,
            "the label line uses the reasoning colour"
        );
    }
}

#[test]
fn reply_labels_reasoning_once_and_not_the_answer() {
    let theme = theme();
    let themed = ThemedCells { theme: &theme };
    let reasoning_text = "Weighed two routes and kept the shorter one.";
    let answer_text = "The shorter route is the one to take.";
    let reasoning_lines = themed.lines(&reasoning(1, reasoning_text), WIDTH);
    let answer_lines = themed.lines(&answer(2, answer_text), WIDTH);

    assert_label(
        reasoning_lines.first().expect("reasoning has a line"),
        theme.reasoning,
    );
    assert_eq!(
        reasoning_lines
            .iter()
            .filter(|line| is_label(&line.to_string()))
            .count(),
        1,
        "one label line per reasoning block"
    );
    assert_eq!(
        reasoning_lines.len(),
        2,
        "the label adds one line before the reasoning"
    );
    assert_eq!(reasoning_lines[1].to_string(), reasoning_text);

    assert!(
        answer_lines.iter().all(|line| !is_label(&line.to_string())),
        "an answer has no label line"
    );
    assert_eq!(answer_lines.len(), 1, "an answer gains no line");
    assert_eq!(answer_lines[0].to_string(), answer_text);
}

#[test]
fn two_reasoning_blocks_each_get_one_label() {
    let theme = theme();
    let themed = ThemedCells { theme: &theme };
    let first = "First pass kept the river crossing.";
    let second = "Second pass dropped the bridge.";
    let reply: Vec<Line<'static>> = [reasoning(1, first), reasoning(2, second)]
        .into_iter()
        .flat_map(|cell| themed.lines(&cell, WIDTH))
        .collect();
    let labels: Vec<usize> = reply
        .iter()
        .enumerate()
        .filter(|(_, line)| is_label(&line.to_string()))
        .map(|(index, _)| index)
        .collect();

    assert_eq!(labels.len(), 2, "two reasoning blocks get one label each");
    for index in labels {
        assert_label(&reply[index], theme.reasoning);
    }
    assert_eq!(reply.len(), 4, "each label adds one line");
    assert_eq!(reply[1].to_string(), first);
    assert_eq!(reply[3].to_string(), second);
}
