//! Each reasoning block starts with one quiet label. An answer does not.

use std::time::{Duration, Instant};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use talaria_core::layout::ReasoningDisplay;
use talaria_core::session::{AssistantCell, Cell, CellId, ReasoningCell};
use talaria_tui::history::CellLines;
use talaria_tui::theme::cells::{
    ReasoningClock, ThemedCells, reasoning_collapses, reasoning_summary_lines,
};
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
        latest_step: false,
        no_reply: false,
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
    trimmed == "thinking"
        || trimmed == "\u{2234} thinking"
        || trimmed == "\u{2234} thinking (latest step)"
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
    let themed = ThemedCells {
        theme: &theme,
        transcript_key: None,
    };
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
fn latest_step_reasoning_label_names_the_step() {
    let theme = theme();
    let themed = ThemedCells {
        theme: &theme,
        transcript_key: None,
    };
    let text = "Checked the last result before answering.";
    let late = Cell::Reasoning(ReasoningCell {
        id: CellId::new(1),
        text: text.to_string(),
        sealed: true,
        latest_step: true,
        no_reply: false,
    });
    let lines = themed.lines(&late, WIDTH);

    assert_eq!(lines[0].to_string(), "\u{2234} thinking (latest step)");
    assert_label(&lines[0], theme.reasoning);
    assert_eq!(
        lines.len(),
        2,
        "the label adds one line before the reasoning"
    );
    assert_eq!(lines[1].to_string(), text);

    let plain = themed.lines(&reasoning(2, text), WIDTH);
    assert_eq!(plain[0].to_string(), "\u{2234} thinking");
}

#[test]
fn two_reasoning_blocks_each_get_one_label() {
    let theme = theme();
    let themed = ThemedCells {
        theme: &theme,
        transcript_key: None,
    };
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

fn block(lines: usize, sealed: bool, latest_step: bool, no_reply: bool) -> ReasoningCell {
    ReasoningCell {
        id: CellId::new(7),
        text: (1..=lines)
            .map(|line| format!("step {line} of the plan"))
            .collect::<Vec<_>>()
            .join("\n"),
        sealed,
        latest_step,
        no_reply,
    }
}

fn summary(cell: &ReasoningCell, took: Option<Duration>, key: Option<&str>) -> Vec<String> {
    reasoning_summary_lines(cell, &theme(), took, key, WIDTH)
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Every painted span of the line is dim and in the reasoning colour, as the heading is.
fn assert_quiet(line: &Line<'_>, reasoning: Style) {
    let styles = painted(line);
    assert!(!styles.is_empty(), "the line is drawn");
    for style in styles {
        assert!(
            style.add_modifier.contains(Modifier::DIM),
            "the line is dim"
        );
        assert_eq!(style.fg, reasoning.fg, "the line uses the reasoning colour");
    }
}

#[test]
fn collapsed_reasoning_is_one_quiet_line_naming_the_count_and_key() {
    let theme = theme();
    let cell = block(34, true, false, false);
    let lines = reasoning_summary_lines(&cell, &theme, None, Some("Ctrl+O"), WIDTH);
    assert_eq!(
        lines.iter().map(ToString::to_string).collect::<Vec<_>>(),
        vec!["\u{2234} thinking (+34 lines, Ctrl+O to view)"]
    );
    assert_quiet(&lines[0], theme.reasoning);
}

#[test]
fn collapsed_reasoning_without_a_key_names_the_view() {
    assert_eq!(
        summary(&block(34, true, false, false), None, None),
        vec!["\u{2234} thinking (+34 lines in transcript view)"]
    );
}

#[test]
fn collapsed_latest_step_reasoning_says_so() {
    assert_eq!(
        summary(&block(34, true, true, false), None, Some("Ctrl+O")),
        vec!["\u{2234} thinking, latest step (+34 lines, Ctrl+O to view)"]
    );
}

#[test]
fn one_line_of_reasoning_counts_in_the_singular() {
    assert_eq!(
        summary(&block(1, true, false, false), None, Some("Ctrl+O")),
        vec!["\u{2234} thinking (+1 line, Ctrl+O to view)"]
    );
}

#[test]
fn timed_reasoning_leads_with_how_long_it_took() {
    let cell = block(34, true, false, false);
    assert_eq!(
        summary(&cell, Some(Duration::from_millis(12_400)), Some("Ctrl+O")),
        vec!["\u{2234} thought for 12s (+34 lines, Ctrl+O to view)"]
    );
    assert_eq!(
        summary(&cell, Some(Duration::from_secs(75)), Some("Ctrl+O")),
        vec!["\u{2234} thought for 1m 15s (+34 lines, Ctrl+O to view)"]
    );
    assert_eq!(
        summary(&cell, Some(Duration::from_millis(400)), Some("Ctrl+O")),
        vec!["\u{2234} thinking (+34 lines, Ctrl+O to view)"],
        "under a second is not worth a duration"
    );
}

#[test]
fn streaming_reasoning_is_one_line_however_long_it_grows() {
    assert_eq!(
        summary(&block(4, false, false, false), None, Some("Ctrl+O")),
        vec!["\u{2234} thinking\u{2026} (+4 lines, Ctrl+O to view)"]
    );
    assert_eq!(
        summary(&block(200, false, false, false), None, Some("Ctrl+O")),
        vec!["\u{2234} thinking\u{2026} (+200 lines, Ctrl+O to view)"]
    );
}

#[test]
fn collapsed_line_fits_a_narrow_terminal() {
    let lines = reasoning_summary_lines(
        &block(34, true, true, false),
        &theme(),
        None,
        Some("Ctrl+O"),
        20,
    );
    assert!(!lines.is_empty());
    for line in &lines {
        assert!(
            line.width() <= 20,
            "{:?} is wider than 20",
            line.to_string()
        );
    }
}

#[test]
fn only_answered_reasoning_collapses_and_only_when_collapsed() {
    let answered = block(3, true, false, false);
    let only_output = block(3, true, false, true);
    assert!(reasoning_collapses(&answered, ReasoningDisplay::Collapsed));
    assert!(!reasoning_collapses(
        &only_output,
        ReasoningDisplay::Collapsed
    ));
    assert!(!reasoning_collapses(&answered, ReasoningDisplay::Full));
    assert!(!reasoning_collapses(&only_output, ReasoningDisplay::Full));
    assert!(reasoning_collapses(
        &block(3, false, false, false),
        ReasoningDisplay::Collapsed
    ));
}

#[test]
fn the_reasoning_clock_times_only_blocks_it_saw_stream() {
    let start = Instant::now();
    let open = block(2, false, false, false);
    let sealed = ReasoningCell {
        sealed: true,
        ..open.clone()
    };
    let mut clock = ReasoningClock::new();
    clock.observe(&[Cell::Reasoning(open.clone())], start);
    clock.observe(
        &[Cell::Reasoning(sealed.clone())],
        start + Duration::from_secs(12),
    );
    assert_eq!(
        clock.took(sealed.id, start + Duration::from_secs(40)),
        Some(Duration::from_secs(12)),
        "the time stops when the block is sealed"
    );

    let mut unseen = ReasoningClock::new();
    unseen.observe(&[Cell::Reasoning(sealed.clone())], start);
    assert_eq!(
        unseen.took(sealed.id, start + Duration::from_secs(5)),
        None,
        "a block that arrived whole has no duration"
    );

    let mut sealing = ReasoningClock::new();
    sealing.observe(&[Cell::Reasoning(open)], start);
    assert_eq!(
        sealing.took(sealed.id, start + Duration::from_secs(3)),
        Some(Duration::from_secs(3)),
        "a block sealed since the last frame runs until now"
    );
}
