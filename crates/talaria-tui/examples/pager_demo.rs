//! Opens the transcript on an empty session. Issue 222. The screen is not drawn yet.

use talaria_tui::pager::{ConnectionBanner, Pager, PagerContext, SearchIndex};

fn main() {
    let cells = Vec::new();
    let ctx = PagerContext {
        cells: &cells,
        source_epoch: 1,
        title: "",
        running: false,
        needs_answer: false,
        connection: ConnectionBanner::Connected,
        history_truncated: false,
    };
    let mut index = SearchIndex::new();
    let _pager = Pager::open(&ctx, 80, 24, &mut index);
}
