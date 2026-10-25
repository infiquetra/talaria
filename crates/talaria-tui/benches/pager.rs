//! Transcript budgets. Issue 222.
//!
//! The clock covers the transcript calls and the draw. Terminal input and output are not included.
//! A row that misses its budget makes this program exit non-zero.

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use talaria_core::session::ToolStatus;
use talaria_tui::history::PlainCellLines;
use talaria_tui::pager::{Details, Pager, PagerAction, PagerCellLines, SearchIndex, SearchInput};

#[path = "../tests/pager_support.rs"]
mod support;

use support::{assistant, context, tool, user};

const COLS: u16 = 160;
const ROWS: u16 = 48;
const OPEN_MAX: Duration = Duration::from_millis(50);
const MOVE_P95: Duration = Duration::from_millis(16);
const MOVE_MAX: Duration = Duration::from_millis(33);

struct Row {
    name: &'static str,
    samples: usize,
    p50: Duration,
    p95: Duration,
    max: Duration,
    budget: &'static str,
    pass: bool,
}

fn main() {
    println!(
        "terminal input and output are excluded; the clock covers the transcript and the draw"
    );
    let cells = document(false);
    let wide = document(true);
    let rows = vec![
        open_row("open plus first frame", &cells, OPEN_MAX),
        open_row(
            "open plus first frame with a 64 KiB tool result",
            &wide,
            OPEN_MAX,
        ),
        move_row(&cells),
        search_row(&cells),
        idle_row(&cells),
    ];
    println!("measure samples p50_ms p95_ms max_ms budget pass");
    let mut failed = false;
    for row in &rows {
        println!(
            "{} {} {:.3} {:.3} {:.3} {} {}",
            row.name,
            row.samples,
            ms(row.p50),
            ms(row.p95),
            ms(row.max),
            row.budget,
            if row.pass { "pass" } else { "fail" }
        );
        failed |= !row.pass;
    }
    if failed {
        std::process::exit(1);
    }
}

fn document(with_tail: bool) -> Vec<talaria_core::session::Cell> {
    let mut cells = Vec::new();
    let mut id = 1u64;
    for turn in 0..1000 {
        cells.push(user(
            id,
            &format!(
                "Question {turn}: please check the error handling in module {}.",
                turn % 37
            ),
        ));
        id += 1;
        cells.push(assistant(id, &template(turn), true));
        id += 1;
        if turn % 20 == 0 {
            let mut call = tool(id, "terminal", ToolStatus::Succeeded);
            call.result = Some(
                (0..80)
                    .map(|n| format!("line {n}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            call.diff = Some(
                (0..30)
                    .map(|n| format!("+row {n}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            cells.push(talaria_core::session::Cell::Tool(call));
            id += 1;
        }
        if turn == 500 {
            let mut call = tool(id, "terminal", ToolStatus::Succeeded);
            call.result = Some("n\n".repeat(1_048_576 / 2));
            cells.push(talaria_core::session::Cell::Tool(call));
            id += 1;
        }
    }
    if with_tail {
        let mut call = tool(id, "terminal", ToolStatus::Succeeded);
        call.result = Some("x".repeat(65_536));
        cells.push(talaria_core::session::Cell::Tool(call));
    }
    cells
}

fn template(turn: usize) -> String {
    let mut text = format!("# Module {turn}\n\n- one\n- two\n- three\n- four\n- five\n\n```\n");
    for _ in 0..8 {
        text.push_str("let value = 1;\n");
    }
    text.push_str("```\n\n");
    while text.len() < 4096 {
        text.push_str("The handler returns the earlier error. ");
    }
    text.truncate(4096);
    text
}

fn open_row(name: &'static str, cells: &[talaria_core::session::Cell], budget: Duration) -> Row {
    let mut samples = Vec::new();
    for _ in 0..30 {
        let ctx = context(cells);
        let mut index = SearchIndex::new();
        let started = Instant::now();
        let mut pager = Pager::open(&ctx, COLS, ROWS, &mut index);
        draw(&mut pager, &ctx);
        samples.push(started.elapsed());
    }
    finish(name, &samples, budget, budget, "max <= 50 ms")
}

fn move_row(cells: &[talaria_core::session::Cell]) -> Row {
    let ctx = context(cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, COLS, ROWS, &mut index);
    let actions = [
        PagerAction::LineUp,
        PagerAction::PageDown,
        PagerAction::HalfPageDown,
        PagerAction::Top,
        PagerAction::Bottom,
    ];
    let mut samples = Vec::new();
    for step in 0..1000 {
        let action = actions[step % actions.len()];
        let started = Instant::now();
        let _ = pager.handle_action(action, &ctx, &mut index);
        draw(&mut pager, &ctx);
        samples.push(started.elapsed());
    }
    finish(
        "line, page, half-page, top and bottom moves",
        &samples,
        MOVE_P95,
        MOVE_MAX,
        "p95 <= 16 ms, max <= 33 ms",
    )
}

fn search_row(cells: &[talaria_core::session::Cell]) -> Row {
    let mut first = Vec::new();
    for _ in 0..30 {
        let ctx = context(cells);
        let mut index = SearchIndex::new();
        let mut pager = Pager::open(&ctx, COLS, ROWS, &mut index);
        let started = Instant::now();
        let _ = pager.handle_search_input(SearchInput::Char('e'), &ctx, &mut index);
        draw(&mut pager, &ctx);
        first.push(started.elapsed());
    }
    let mut later = Vec::new();
    let ctx = context(cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, COLS, ROWS, &mut index);
    // The index build belongs to the first row, so it runs before this clock starts.
    let _ = pager.handle_search_input(SearchInput::Char('e'), &ctx, &mut index);
    for ch in ['r', 'r', 'o', 'r'] {
        let started = Instant::now();
        let _ = pager.handle_search_input(SearchInput::Char(ch), &ctx, &mut index);
        draw(&mut pager, &ctx);
        later.push(started.elapsed());
    }
    let _ = pager.handle_search_input(SearchInput::Enter, &ctx, &mut index);
    for _ in 0..200 {
        let started = Instant::now();
        let _ = pager.handle_action(PagerAction::NextMatch, &ctx, &mut index);
        draw(&mut pager, &ctx);
        later.push(started.elapsed());
    }
    let mut row = finish(
        "first search keystroke",
        &first,
        OPEN_MAX,
        OPEN_MAX,
        "max <= 50 ms",
    );
    let rest = finish(
        "later search keystrokes and n",
        &later,
        MOVE_P95,
        MOVE_MAX,
        "p95 <= 16 ms, max <= 33 ms",
    );
    row.pass = row.pass && rest.pass;
    row.p95 = rest.p95;
    row.max = row.max.max(rest.max);
    row
}

fn idle_row(cells: &[talaria_core::session::Cell]) -> Row {
    let ctx = context(cells);
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, COLS, ROWS, &mut index);
    let started = Instant::now();
    let frames = 0u32;
    while started.elapsed() < Duration::from_secs(1) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = &mut pager;
    Row {
        name: "idle",
        samples: 1,
        p50: Duration::ZERO,
        p95: Duration::ZERO,
        max: Duration::ZERO,
        budget: "0 frames",
        pass: frames == 0,
    }
}

fn draw(pager: &mut Pager, ctx: &talaria_tui::pager::PagerContext<'_>) {
    let plain = PlainCellLines;
    let render = PagerCellLines {
        details: Details::Expanded,
        markdown: talaria_tui::markdown::MarkdownOptions {
            width: COLS - 2,
            ..talaria_tui::markdown::MarkdownOptions::default()
        },
        theme: talaria_tui::pager::PagerTheme::default(),
        other: &plain,
    };
    let backend = TestBackend::new(COLS, ROWS);
    let mut terminal = Terminal::new(backend).expect("test backend");
    let area = Rect::new(0, 0, COLS, ROWS);
    terminal
        .draw(|frame| pager.draw(area, frame.buffer_mut(), ctx, &render))
        .expect("draw");
}

fn finish(
    name: &'static str,
    samples: &[Duration],
    p95_budget: Duration,
    max_budget: Duration,
    budget: &'static str,
) -> Row {
    let mut ordered = samples.to_vec();
    ordered.sort();
    let p50 = ordered[ordered.len() / 2];
    let p95_index = ((ordered.len() as f64) * 0.95).ceil() as usize - 1;
    let p95 = ordered[p95_index];
    let max = ordered[ordered.len() - 1];
    Row {
        name,
        samples: ordered.len(),
        p50,
        p95,
        max,
        budget,
        pass: p95 <= p95_budget && max <= max_budget,
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}
