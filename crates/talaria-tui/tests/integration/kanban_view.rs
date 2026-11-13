//! Board overlay drawing at a fixed size.

use std::io::Write;
use std::sync::{Arc, Mutex};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{TerminalOptions, Viewport};
use talaria_core::kanban::{BoardCard, FrameCard, FrameColumn, KanbanFrame};
use talaria_tui::kanban_view::{KanbanOverlay, draw_kanban};

#[test]
fn board_renders_columns_and_cards() {
    let parser = render(&frame());
    let text = parser.screen().contents();
    assert!(text.contains("kanban: mock"), "view missing");
    assert!(text.contains("2/2"), "view missing");
    assert!(text.contains("live:12"), "view missing");
    assert!(text.contains("> fix the parser"), "view missing");
    assert!(text.contains("! waiting input"), "view missing");
    let mut overlay = KanbanOverlay::open();
    assert!(overlay.is_open(), "view missing");
    overlay.close();
    assert!(overlay.showing_inline(), "view missing");
    assert!(!overlay.is_open(), "view missing");
}

fn render(frame: &KanbanFrame) -> vt100::Parser {
    let sink = ByteSink::default();
    let backend = CrosstermBackend::new(sink.clone());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 120, 36)),
        },
    )
    .expect("terminal");
    terminal
        .draw(|ui| draw_kanban(frame, ui.area(), ui.buffer_mut()))
        .expect("draw");
    let mut parser = vt100::Parser::new(36, 120, 0);
    parser.process(&sink.take());
    parser
}

fn frame() -> KanbanFrame {
    KanbanFrame {
        board: "mock".into(),
        shown: 2,
        total: 2,
        live: "12".into(),
        columns: vec![
            FrameColumn {
                label: "Ready".into(),
                cards: vec![FrameCard {
                    id: "t_ready".into(),
                    title: "fix the parser".into(),
                    assignee: "fixture".into(),
                    status: "ready".into(),
                    age: "1d".into(),
                    selected: true,
                }],
            },
            FrameColumn {
                label: "Blocked".into(),
                cards: vec![FrameCard {
                    id: "t_blocked".into(),
                    title: "waiting input".into(),
                    assignee: "fixture".into(),
                    status: "blocked".into(),
                    age: "1d".into(),
                    selected: false,
                }],
            },
        ],
        status_line: "live:12".into(),
        detail: Some(BoardCard {
            id: "t_ready".into(),
            title: "fix the parser".into(),
            assignee: "fixture".into(),
            status: "ready".into(),
            created_at_ms: 1_792_993_000,
            started_at_ms: None,
            parent_titles: vec![("sort the inbox".into(), "triage".into())],
            child_titles: Vec::new(),
            body: "Repair the parser and leave a summary.".into(),
            latest_summary: None,
            comments: Vec::new(),
        }),
    }
}

#[derive(Clone, Default)]
struct ByteSink(Arc<Mutex<Vec<u8>>>);

impl ByteSink {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.lock().expect("sink lock"))
    }
}

impl Write for ByteSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("sink lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
