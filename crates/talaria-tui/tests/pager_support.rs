//! Cells, replay and screen rows for the transcript tests. Issue 222.
#![allow(dead_code)]

#[path = "../../talaria-core/tests/support/drive.rs"]
pub mod drive;
#[path = "../../talaria-core/tests/support/frames.rs"]
pub mod frames;

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use serde_json::Value;
use talaria_core::session::{
    AssistantCell, Cell, CellId, Event, NoticeCell, NoticeKind, PromptCell, PromptKey, PromptKind,
    PromptState, ReasoningCell, SessionId, SessionState, ToolCell, ToolStatus, UserCell,
};
use talaria_tui::history::PlainCellLines;
use talaria_tui::markdown::MarkdownOptions;
use talaria_tui::pager::{ConnectionBanner, Details, PagerCellLines, PagerContext, PagerTheme};

pub const FIXTURES: &[&str] = &[
    "approval.jsonl",
    "clarification.jsonl",
    "interrupted.jsonl",
    "plain-answer.jsonl",
    "queued-prompt.jsonl",
    "subagent-batch.jsonl",
    "tool-diff.jsonl",
];

pub fn user(id: u64, text: &str) -> Cell {
    Cell::User(UserCell {
        id: CellId::new(id),
        text: text.to_string(),
    })
}

pub fn assistant(id: u64, text: &str, sealed: bool) -> Cell {
    Cell::Assistant(AssistantCell {
        id: CellId::new(id),
        text: text.to_string(),
        sealed,
        interim: false,
        transformed: false,
    })
}

pub fn interim(id: u64, text: &str) -> Cell {
    Cell::Assistant(AssistantCell {
        id: CellId::new(id),
        text: text.to_string(),
        sealed: false,
        interim: true,
        transformed: false,
    })
}

pub fn reasoning(id: u64, text: &str) -> Cell {
    Cell::Reasoning(ReasoningCell {
        id: CellId::new(id),
        text: text.to_string(),
        sealed: true,
    })
}

pub fn notice(id: u64, text: &str) -> Cell {
    Cell::Notice(NoticeCell {
        id: CellId::new(id),
        kind: NoticeKind::Error,
        text: text.to_string(),
        detail: None,
    })
}

pub fn tool(id: u64, name: &str, status: ToolStatus) -> ToolCell {
    ToolCell {
        id: CellId::new(id),
        tool_id: format!("tool-{id}"),
        name: name.to_string(),
        context: None,
        args: None,
        labels: Vec::new(),
        status,
        progress: None,
        summary: None,
        result: None,
        diff: None,
        duration_s: None,
    }
}

pub fn prompt(id: u64, kind: PromptKind, state: PromptState) -> PromptCell {
    PromptCell {
        id: CellId::new(id),
        key: PromptKey {
            session_id: SessionId::new("sess-pager"),
            prompt_id: format!("prompt-{id}"),
            execution_generation: 1,
        },
        kind,
        command: None,
        description: None,
        question: None,
        choices: Vec::new(),
        multi_select: false,
        edit: None,
        timeout_s: None,
        state,
        sealed: false,
    }
}

pub fn context(cells: &[Cell]) -> PagerContext<'_> {
    PagerContext {
        cells,
        source_epoch: 1,
        title: "notes",
        running: false,
        needs_answer: false,
        connection: ConnectionBanner::Connected,
        history_truncated: false,
    }
}

pub fn lines_for(details: Details, width: u16, other: &PlainCellLines) -> PagerCellLines<'_> {
    PagerCellLines {
        details,
        markdown: MarkdownOptions {
            width: width.saturating_sub(2),
            ..MarkdownOptions::default()
        },
        theme: PagerTheme::default(),
        other,
    }
}

pub fn row_text(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

pub fn replay_fixture(file: &str) -> SessionState {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames")
        .join(file);
    let log = frames::read_path(&path).unwrap_or_else(|error| panic!("{file}: {error}"));
    let session = session_id(&log);
    if let Some(snapshot) = log.initial_snapshot.clone() {
        drive::reduce_snapshot(snapshot, log.events).0
    } else {
        drive::reduce(&session, log.events).0
    }
}

fn session_id(log: &frames::FrameLog) -> String {
    for event in &log.events {
        if let Event::Session { session_id, .. } = event
            && !session_id.as_str().is_empty()
        {
            return session_id.as_str().to_string();
        }
    }
    for outbound in &log.outbound {
        if let Some(id) = outbound.params.get("session_id").and_then(Value::as_str)
            && !id.is_empty()
        {
            return id.to_string();
        }
    }
    "local-session-1".to_string()
}
