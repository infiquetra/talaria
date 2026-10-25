//! The status line drawn through the inline terminal: it is the bottom row of the screen.

use std::io;
use std::sync::{Arc, Mutex};

use talaria_tui::live::{LiveSection, LiveView, PlainTextSection};
use talaria_tui::status::{Connection, StatusInputs, StatusPalette, StatusSection, plan};
use talaria_tui::terminal::{InlineTerminal, ProbeReply, RestoreLink, TerminalCaps};

/// A writer that keeps every byte written, shared with the test.
#[derive(Clone, Default)]
struct ByteSink(Arc<Mutex<Vec<u8>>>);

impl ByteSink {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.lock().expect("sink lock"))
    }
}

impl io::Write for ByteSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("sink lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn status_row_is_the_bottom_row_of_the_inline_terminal() {
    let (cols, rows) = (80u16, 24u16);
    let sink = ByteSink::default();
    // The cursor is known to be on the last screen row, so the two row live region is
    // pushed up to rows 22 and 23.
    let probe = ProbeReply {
        caps: TerminalCaps::default(),
        cursor: Some((0, rows - 1)),
        ..Default::default()
    };
    let mut terminal = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        2,
        probe,
        RestoreLink::detached(),
    )
    .expect("inline terminal");

    let inputs = StatusInputs {
        connection: Connection::Connected,
        model: Some("alpha-model".to_string()),
        needs_you: 2,
        ..StatusInputs::default()
    };
    let palette = StatusPalette::default();
    let status = StatusSection::new(&inputs, None, &palette);
    let tail = PlainTextSection::new("streaming tail line");
    let sections: Vec<&dyn LiveSection> = vec![&tail, &status];
    let view = LiveView { sections };
    terminal.draw_live(&view).expect("draw live");

    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(&sink.take());
    let screen: Vec<String> = parser
        .screen()
        .rows(0, cols)
        .map(|row| row.trim_end().to_string())
        .collect();
    assert_eq!(screen.len(), usize::from(rows));

    let expected = plan(&inputs, None, cols).text();
    let expected = expected.trim_end();
    assert!(
        expected.contains("alpha-model"),
        "the status text should show the model: {expected:?}"
    );
    assert_eq!(
        screen[usize::from(rows) - 1],
        expected,
        "the status text is the last screen row"
    );
    assert_eq!(
        screen[usize::from(rows) - 2],
        "streaming tail line",
        "the other live rows sit directly above the status row"
    );
    for (index, row) in screen.iter().enumerate().take(usize::from(rows) - 2) {
        assert!(
            row.is_empty(),
            "row {index} above the live region is blank: {row:?}"
        );
    }
}
