//! A streaming reply's closed Markdown blocks reach scrollback while it streams, the live tail
//! draws only the rest, and the rows that end in scrollback are the rows of one whole rendering.

use crate::events;
use crate::support;

use ratatui::text::Line;
use serde_json::json;
use support::{Alac, ByteSink, probe};
use talaria_core::session::{
    AssistantCell, Cell, CellId, ReasoningCell, SessionId, SessionState, ToolCell, ToolStatus,
    update,
};
use talaria_tui::history::{CellLines, CommitLedger, PlainCellLines};
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::markdown::section::MarkdownTailSection;
use talaria_tui::markdown::{MarkdownOptions, closed_blocks_len, render_markdown};
use talaria_tui::reply_commit::{CUT_OFF_NOTICE, REVISED_NOTICE, ReplyCommits, after_committed};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

/// Replies and reasoning as Markdown, every other cell plain.
struct MarkdownCells;

impl CellLines for MarkdownCells {
    fn lines(&self, cell: &Cell, width: u16) -> Vec<Line<'static>> {
        let options = MarkdownOptions {
            width,
            ..MarkdownOptions::default()
        };
        match cell {
            Cell::Assistant(reply) => render_markdown(&reply.text, &options),
            Cell::Reasoning(reasoning) => {
                let mut lines = vec![Line::from("thinking")];
                lines.extend(render_markdown(&reasoning.text, &options));
                lines
            }
            _ => PlainCellLines.lines(cell, width),
        }
    }
}

fn reply(id: u64, text: &str, sealed: bool) -> Cell {
    Cell::Assistant(AssistantCell {
        id: CellId::new(id),
        text: text.to_owned(),
        sealed,
        interim: false,
        transformed: false,
    })
}

fn text_of(lines: &[Line<'_>]) -> Vec<String> {
    lines.iter().map(ToString::to_string).collect()
}

const PARAGRAPHS: &str = "First paragraph of the reply.\n\nSecond paragraph, which \
wraps across more than one row when the screen is narrow.\n\nThird paragraph.\n";

const TABLE: &str = "Here is the plan:\n\n| Step | What happens | Owner |\n|---|---|---|\n\
| 1 | Read the request and list every file the change touches | planner |\n\
| 2 | Write the failing test first | builder |\n| 3 | Make it pass | builder |\n\n\
After the table.\n";

const LIST: &str = "Before the list.\n\n- one\n- two with **bold** text\n  that continues\n\
- three\n\n3. third\n4. fourth\n\nAfter the lists.\n";

const FENCE: &str = "Code:\n\n```rust\nfn main() {\n\n    // # not a heading\n    \
println!(\"hi\");\n}\n```\n\nAfter the code.\n";

const HEADINGS: &str = "# Title\n\nIntro text.\n\n## Part two\n\n> A quote\n> over two lines\n\n\
---\n\nClosing line.";

const SHAPES: [(&str, &str); 5] = [
    ("paragraphs", PARAGRAPHS),
    ("table", TABLE),
    ("list", LIST),
    ("code fence", FENCE),
    ("headings", HEADINGS),
];

#[test]
fn closed_blocks_are_counted_only_once_later_text_cannot_change_them() {
    assert_eq!(closed_blocks_len("One"), 0);
    assert_eq!(
        closed_blocks_len("One\n"),
        0,
        "the last block is still open"
    );
    assert_eq!(
        closed_blocks_len("One\n\nTw"),
        0,
        "a partial line is no evidence yet: it could still continue a list item"
    );
    assert_eq!(closed_blocks_len("One\n\nTwo\n"), "One\n\n".len());
    assert_eq!(
        closed_blocks_len("One\n\nTwo\n\nThree\n"),
        "One\n\nTwo\n\n".len()
    );

    let table = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    assert_eq!(
        closed_blocks_len(table),
        0,
        "a table is open until a block follows it"
    );
    assert_eq!(
        closed_blocks_len(&format!("{table}| 3 | 4 |\n")),
        0,
        "a new row belongs to the table"
    );
    assert_eq!(
        closed_blocks_len(&format!("{table}\nAfter\n")),
        format!("{table}\n").len()
    );

    let list = "- one\n- two\n";
    assert_eq!(closed_blocks_len(list), 0);
    assert_eq!(closed_blocks_len(&format!("{list}\n  more of two\n")), 0);
    assert_eq!(
        closed_blocks_len(&format!("{list}\nNext\n")),
        format!("{list}\n").len()
    );

    let open_fence = "```rust\nfn x() {}\n\n# inside\n";
    assert_eq!(
        closed_blocks_len(open_fence),
        0,
        "an open fence is never split"
    );
    let fence = format!("{open_fence}```\n");
    assert_eq!(
        closed_blocks_len(&fence),
        fence.len(),
        "a closed fence commits"
    );
    assert_eq!(
        closed_blocks_len(&format!("{fence}Aft")),
        fence.len(),
        "a partial line after a closed fence waits"
    );

    // Byte lengths count the raw text, whatever sanitizing does to it.
    let raw = "Caf\u{e9} \u{7}bell\r\n\r\nNext\r\n";
    assert_eq!(closed_blocks_len(raw), "Caf\u{e9} \u{7}bell\r\n\r\n".len());
}

/// Stream `text` in pieces of `step` bytes through `commits`, as a live frame would after each
/// delta, then commit the final cell. Returns every committed row, in order.
fn stream_and_finish(text: &str, step: usize, width: u16) -> (Vec<Line<'static>>, usize) {
    let mut commits = ReplyCommits::new();
    let mut committed = Vec::new();
    let mut early = 0;
    let mut end = 0;
    while end < text.len() {
        end = (end + step).min(text.len());
        while !text.is_char_boundary(end) {
            end += 1;
        }
        let lines = commits.before_frame(
            &[reply(7, &text[..end], false)],
            0,
            true,
            &MarkdownCells,
            width,
        );
        early += lines.len();
        committed.extend(lines);
    }
    committed.extend(commits.final_lines(&[reply(7, text, true)], &MarkdownCells, width));
    assert!(!commits.is_partial(), "the final commit closes the reply");
    (committed, early)
}

#[test]
fn a_split_commit_gives_the_rows_of_one_whole_commit() {
    for (name, text) in SHAPES {
        for width in [80, 24] {
            let whole = MarkdownCells.lines(&reply(7, text, true), width);
            for step in [1, 5, 13, 64] {
                let (committed, early) = stream_and_finish(text, step, width);
                assert_eq!(
                    text_of(&committed),
                    text_of(&whole),
                    "{name} at width {width}, streamed {step} bytes at a time"
                );
                assert_eq!(committed, whole, "{name}: the styles differ");
                assert!(
                    early > 0,
                    "{name} at width {width}: nothing reached scrollback before the reply ended"
                );
            }
        }
    }
}

#[test]
fn a_table_or_fence_is_committed_whole_when_it_closes() {
    let mut commits = ReplyCommits::new();
    let head = "Here is the plan:\n\n| Step | What |\n|---|---|\n| 1 | read |\n";
    let lines = commits.before_frame(&[reply(3, head, false)], 0, true, &MarkdownCells, 80);
    assert_eq!(text_of(&lines), vec!["Here is the plan:"]);
    let more = format!("{head}| 2 | write |\n");
    assert!(
        commits
            .before_frame(&[reply(3, &more, false)], 0, true, &MarkdownCells, 80)
            .is_empty(),
        "a table is never split"
    );
    let closed = format!("{more}\nDone\n");
    let lines = commits.before_frame(&[reply(3, &closed, false)], 0, true, &MarkdownCells, 80);
    let table = MarkdownCells.lines(
        &reply(
            3,
            "| Step | What |\n|---|---|\n| 1 | read |\n| 2 | write |\n",
            true,
        ),
        80,
    );
    let mut wanted = vec![String::new()];
    wanted.extend(text_of(&table));
    assert_eq!(
        text_of(&lines),
        wanted,
        "the table commits whole, after one blank row"
    );
    // The live tail draws only the open block, after the same blank row.
    let rest = commits
        .live_rest(&reply(3, &closed, false))
        .expect("the reply is partly committed");
    assert_eq!(
        text_of(&after_committed(MarkdownCells.lines(&rest, 80))),
        vec!["", "Done"]
    );
}

#[test]
fn nothing_is_committed_unless_the_reply_heads_the_live_tail() {
    let running = Cell::Tool(ToolCell {
        id: CellId::new(1),
        tool_id: "call-1".into(),
        name: "terminal".into(),
        context: None,
        args: None,
        labels: Vec::new(),
        status: ToolStatus::Running,
        progress: None,
        summary: None,
        result: None,
        diff: None,
        duration_s: None,
    });
    let text = "One\n\nTwo\n\nThree";
    let mut commits = ReplyCommits::new();
    assert!(
        commits
            .before_frame(
                &[running, reply(2, text, false)],
                0,
                true,
                &MarkdownCells,
                80
            )
            .is_empty(),
        "scrollback is append-only, so a reply behind a running tool waits"
    );
    assert!(
        commits
            .before_frame(&[reply(2, text, false)], 0, false, &MarkdownCells, 80)
            .is_empty(),
        "nothing is committed while committing is not allowed"
    );
    assert!(!commits.is_partial());
    // An idle frame with nothing new commits nothing, so it writes nothing.
    assert!(
        !commits
            .before_frame(&[reply(2, text, false)], 0, true, &MarkdownCells, 80)
            .is_empty()
    );
    assert!(
        commits
            .before_frame(&[reply(2, text, false)], 0, true, &MarkdownCells, 80)
            .is_empty()
    );
}

#[test]
fn a_revised_final_text_follows_a_notice_instead_of_repeating_silently() {
    let streamed = "Draft one.\n\nDraft two.\n\nDraft three.\n";
    for (label, transformed, interim) in [("transformed", true, false), ("interim", false, true)] {
        let mut commits = ReplyCommits::new();
        let early = commits.before_frame(&[reply(4, streamed, false)], 0, true, &MarkdownCells, 80);
        assert_eq!(text_of(&early), vec!["Draft one.", "", "Draft two."]);
        let revised = Cell::Assistant(AssistantCell {
            id: CellId::new(4),
            text: "Final answer.\n\nWith a second paragraph.".into(),
            sealed: true,
            interim,
            transformed,
        });
        let lines = commits.final_lines(std::slice::from_ref(&revised), &MarkdownCells, 80);
        let mut wanted = vec![format!("\u{b7} {REVISED_NOTICE}")];
        wanted.extend(text_of(&MarkdownCells.lines(&revised, 80)));
        assert_eq!(text_of(&lines), wanted, "{label}");
        assert!(!commits.is_partial());
    }
}

#[test]
fn end_of_turn_reasoning_follows_a_reply_already_partly_in_scrollback() {
    let reasoning = Cell::Reasoning(ReasoningCell {
        id: CellId::new(9),
        text: "why".into(),
        sealed: true,
        latest_step: true,
    });
    let text = "Part one.\n\nPart two.\n";
    // Nothing committed early: the reasoning keeps its place in front of the reply.
    let mut untouched = ReplyCommits::new();
    let lines = untouched.final_lines(
        &[reasoning.clone(), reply(5, text, true)],
        &MarkdownCells,
        80,
    );
    assert_eq!(
        text_of(&lines),
        vec!["thinking", "why", "Part one.", "", "Part two."]
    );
    // Part of the reply is above already, so its rest comes first and the reasoning after it.
    let mut commits = ReplyCommits::new();
    let early = commits.before_frame(&[reply(5, text, false)], 0, true, &MarkdownCells, 80);
    assert_eq!(text_of(&early), vec!["Part one."]);
    let lines = commits.final_lines(&[reasoning, reply(5, text, true)], &MarkdownCells, 80);
    assert_eq!(text_of(&lines), vec!["", "Part two.", "thinking", "why"]);
}

#[test]
fn a_reply_that_cannot_finish_is_closed_with_a_notice() {
    let text = "Part one.\n\nPart two.\n";
    let notice = vec![format!("\u{b7} {CUT_OFF_NOTICE}")];

    let mut replaced = ReplyCommits::new();
    assert!(
        !replaced
            .before_frame(&[reply(5, text, false)], 0, true, &MarkdownCells, 80)
            .is_empty()
    );
    assert_eq!(text_of(&replaced.replaced(&MarkdownCells, 80)), notice);
    assert!(
        replaced.replaced(&MarkdownCells, 80).is_empty(),
        "said once"
    );

    // Another session's cell that happens to carry the same id is not the reply's rest.
    let mut collided = ReplyCommits::new();
    assert!(
        !collided
            .before_frame(&[reply(5, text, false)], 0, true, &MarkdownCells, 80)
            .is_empty()
    );
    let other = reply(5, "Something else entirely.", true);
    let mut wanted = notice.clone();
    wanted.push("Something else entirely.".into());
    assert_eq!(
        text_of(&collided.final_lines(&[other], &MarkdownCells, 80)),
        wanted
    );

    // A frame whose live tail no longer starts with the reply closes it too.
    let mut gone = ReplyCommits::new();
    assert!(
        !gone
            .before_frame(&[reply(5, text, false)], 0, true, &MarkdownCells, 80)
            .is_empty()
    );
    assert_eq!(
        text_of(&gone.before_frame(&[reply(6, "New", false)], 0, true, &MarkdownCells, 80)),
        notice
    );
}

#[test]
fn a_final_reply_waiting_for_its_commit_is_not_cut_off() {
    let text = "Part one.\n\nPart two.\n";
    let mut commits = ReplyCommits::new();
    assert!(
        !commits
            .before_frame(&[reply(5, text, false)], 0, true, &MarkdownCells, 80)
            .is_empty()
    );
    // Sealed and past the live tail, with a new reply open after it.
    let cells = [reply(5, text, true), reply(6, "Next\n", false)];
    assert!(
        commits
            .before_frame(&cells, 1, true, &MarkdownCells, 80)
            .is_empty(),
        "no notice and no commit of the next reply while the first awaits its commit"
    );
    assert!(commits.is_partial());
    assert_eq!(
        text_of(&commits.final_lines(&cells[..1], &MarkdownCells, 80)),
        vec!["", "Part two."]
    );
}

#[test]
fn rows_committed_before_a_width_change_stay_and_the_rest_uses_the_new_width() {
    let text = "A first paragraph that is long enough to wrap at a narrow width.\n\n\
                A second paragraph that is also long enough to wrap.\n";
    let mut commits = ReplyCommits::new();
    let early = commits.before_frame(&[reply(5, text, false)], 0, true, &MarkdownCells, 80);
    assert_eq!(
        text_of(&early),
        vec!["A first paragraph that is long enough to wrap at a narrow width."]
    );
    let rest = commits.final_lines(&[reply(5, text, true)], &MarkdownCells, 20);
    let mut wanted = vec![String::new()];
    wanted.extend(text_of(&MarkdownCells.lines(
        &reply(
            5,
            "A second paragraph that is also long enough to wrap.",
            true,
        ),
        20,
    )));
    assert_eq!(text_of(&rest), wanted);
}

const COLS: u16 = 80;
const ROWS: u16 = 24;
const MAX_TAIL_ROWS: usize = 15;

/// The live tail as the inline screen draws it: a partly committed reply shows only its rest,
/// and at most `MAX_TAIL_ROWS` rows show.
fn tail_lines(state: &SessionState, commits: &ReplyCommits) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = state
        .live_tail()
        .iter()
        .flat_map(|cell| match commits.live_rest(cell) {
            Some(rest) => after_committed(MarkdownCells.lines(&rest, COLS)),
            None => MarkdownCells.lines(cell, COLS),
        })
        .collect();
    if lines.len() > MAX_TAIL_ROWS {
        lines.drain(..lines.len() - MAX_TAIL_ROWS);
    }
    lines
}

/// A reply far taller than the screen: a table, a code fence and many unique paragraphs.
fn tall_reply() -> String {
    let mut text = String::from("| Step | What happens |\n|---|---|\n");
    for row in 1..=9 {
        text.push_str(&format!(
            "| {row} | Table row {row:02} describes a step that is long enough to wrap |\n"
        ));
    }
    text.push_str("\n```text\nfenced line 01\nfenced line 02\n```\n");
    for n in 1..=24 {
        text.push_str(&format!("\nParagraph {n:02} of the reply.\n"));
    }
    text
}

#[test]
fn a_reply_taller_than_the_screen_scrolls_up_and_ends_in_scrollback_once() {
    let sink = ByteSink::default();
    let mut alac = Alac::new(COLS, ROWS);
    let mut term = InlineTerminal::new(
        sink.clone(),
        (COLS, ROWS),
        2,
        probe(false, false, Some((0, ROWS - 1))),
        RestoreLink::detached(),
    )
    .expect("inline terminal");
    let mut state = SessionState::empty(SessionId::new(events::SESSION.to_owned()));
    let mut ledger = CommitLedger::new();
    let mut commits = ReplyCommits::new();
    let text = tall_reply();
    let whole = text_of(&MarkdownCells.lines(&reply(1, &text, true), COLS));

    let frame = |state: &SessionState,
                 ledger: &mut CommitLedger,
                 commits: &mut ReplyCommits,
                 term: &mut InlineTerminal<ByteSink>,
                 alac: &mut Alac| {
        let ready = ledger.take_ready(state.cells());
        term.commit_lines(commits.final_lines(ready, &MarkdownCells, COLS))
            .expect("commit final cells");
        let early = commits.before_frame(
            state.cells(),
            state.final_prefix_len(),
            true,
            &MarkdownCells,
            COLS,
        );
        term.commit_lines(early).expect("commit closed blocks");
        let tail = MarkdownTailSection::new(tail_lines(state, commits));
        let sections: Vec<&dyn LiveSection> = vec![&tail];
        term.draw_live(&LiveView { sections }).expect("draw live");
        alac.absorb(&sink);
    };

    let mut seq = 1;
    let mut send = |state: &mut SessionState, kind: &str, payload: serde_json::Value| {
        let _ = update(state, events::ev(seq, kind, payload));
        seq += 1;
    };
    send(&mut state, "message.start", json!({}));
    let middle = text.find("Paragraph 12").expect("a middle paragraph");
    for piece in text.as_bytes()[..middle].chunks(40) {
        send(
            &mut state,
            "message.delta",
            json!({"text": std::str::from_utf8(piece).expect("ascii")}),
        );
        frame(&state, &mut ledger, &mut commits, &mut term, &mut alac);
    }

    // Mid-stream: the table and the earlier paragraphs are already above the live region.
    // Rows above the live region's top are committed history, on screen or scrolled off.
    let rows = alac.history_and_screen();
    let top = rows.len() - usize::from(ROWS) + usize::from(term.viewport().y);
    let (above, live) = rows.split_at(top);
    for wanted in [
        "Table row 01",
        "fenced line 01",
        "Paragraph 01 of the reply.",
    ] {
        assert!(
            above.iter().any(|row| row.contains(wanted)),
            "{wanted:?} is not above the live region while the reply streams.\n\
             above: {above:#?}\nlive: {live:#?}"
        );
        assert!(
            live.iter().all(|row| !row.contains(wanted)),
            "{wanted:?} is still drawn in the live region: {live:#?}"
        );
    }
    assert!(
        above.iter().all(|row| !row.contains("Paragraph 11")),
        "the open block was committed before it closed: {above:#?}"
    );

    for piece in text.as_bytes()[middle..].chunks(40) {
        send(
            &mut state,
            "message.delta",
            json!({"text": std::str::from_utf8(piece).expect("ascii")}),
        );
        frame(&state, &mut ledger, &mut commits, &mut term, &mut alac);
    }
    send(
        &mut state,
        "message.complete",
        json!({"status": "complete", "text": text}),
    );
    frame(&state, &mut ledger, &mut commits, &mut term, &mut alac);

    // Every row of one whole rendering is in scrollback, in order, exactly once.
    let rows = alac.history_and_screen();
    let start = rows
        .iter()
        .position(|row| row == &whole[0])
        .expect("the reply's first row");
    assert_eq!(
        &rows[start..start + whole.len()],
        &whole[..],
        "the committed rows differ from one whole rendering"
    );
    for wanted in whole
        .iter()
        .filter(|row| row.contains("Paragraph") || row.contains("row"))
    {
        assert_eq!(
            rows.iter().filter(|row| *row == wanted).count(),
            1,
            "{wanted:?} is not in scrollback exactly once"
        );
    }
}
