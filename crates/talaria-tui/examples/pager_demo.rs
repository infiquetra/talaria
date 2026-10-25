//! Opens the transcript on a small session. Issue 222.
//!
//! Run it with `cargo run -p talaria-tui --example pager_demo`. It draws one frame of the
//! transcript into a buffer and prints it, so the layout can be seen without a gateway.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::session::{AssistantCell, Cell, CellId, UserCell};
use talaria_tui::history::PlainCellLines;
use talaria_tui::pager::{ConnectionBanner, Pager, PagerContext, SearchIndex};

const COLS: u16 = 80;
const ROWS: u16 = 12;

fn main() {
    let cells = vec![
        Cell::User(UserCell {
            id: CellId::new(1),
            text: "Show me the transcript.".to_string(),
        }),
        Cell::Assistant(AssistantCell {
            id: CellId::new(2),
            text: "The transcript holds every cell of the session.".to_string(),
            sealed: true,
            interim: false,
            transformed: false,
        }),
    ];
    let ctx = PagerContext {
        cells: &cells,
        source_epoch: 1,
        title: "demo",
        running: false,
        needs_answer: false,
        connection: ConnectionBanner::Connected,
        history_truncated: false,
    };
    let mut index = SearchIndex::new();
    let mut pager = Pager::open(&ctx, COLS, ROWS, &mut index);
    let mut buf = Buffer::empty(Rect::new(0, 0, COLS, ROWS));
    pager.draw(buf.area, &mut buf, &ctx, &PlainCellLines);
    for y in 0..buf.area.height {
        let row: String = (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>();
        println!("{row}");
    }
}
