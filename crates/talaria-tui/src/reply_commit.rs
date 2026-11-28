//! A streaming reply's finished Markdown blocks, committed to scrollback before the reply ends.
//!
//! Inline history is the terminal's own scrollback, which cannot be redrawn, and a cell reaches
//! it only once it is final. A reply that streams taller than the live region would otherwise
//! scroll under it: the region shows only its last rows, and the rest cannot be read until the
//! reply ends. So while an open reply is the first cell of the live tail (everything before it
//! is already in scrollback), each top-level block that later text cannot change is committed
//! as it closes, and the live tail draws only the rest. When the reply becomes final only the
//! rest is committed.
//!
//! This changes what the inline terminal commits and when, never the session's cells: the
//! transcript, search, copy and every other reader of cells see the reply whole.

use ratatui::text::Line;
use talaria_core::session::{AssistantCell, Cell, CellId, NoticeCell, NoticeKind};

use crate::history::CellLines;
use crate::markdown::closed_blocks_len;

/// Drawn before a reply's final text when that text no longer starts with the part already in
/// scrollback, because Hermes replaced or transformed it.
pub const REVISED_NOTICE: &str = "The reply above was revised; its final text follows.";

/// Drawn after a partly committed reply whose rest will not come, because the session's state
/// was replaced while it streamed: a reload, a reconnect, a rewind or a switch.
pub const CUT_OFF_NOTICE: &str =
    "The reply above stops here: the session was reloaded before it finished.";

/// The part of one reply already in scrollback.
#[derive(Clone, Debug)]
struct Partial {
    id: CellId,
    /// The reply's source text that is committed, a prefix of its text.
    committed: String,
}

/// Remembers how much of the streaming reply is in scrollback.
#[derive(Clone, Debug, Default)]
pub struct ReplyCommits {
    partial: Option<Partial>,
}

impl ReplyCommits {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lines to commit before a live frame is drawn. `cells` are the session's cells and
    /// `live_from` is where its live tail starts. With `allowed` false (scrollback is not what
    /// the screen shows, or earlier final cells are not committed yet), nothing new is
    /// committed, but a partly committed reply that is gone is still closed with a notice. A
    /// frame with nothing newly closed returns nothing.
    pub fn before_frame(
        &mut self,
        cells: &[Cell],
        live_from: usize,
        allowed: bool,
        render: &dyn CellLines,
        width: u16,
    ) -> Vec<Line<'static>> {
        let head = match cells.get(live_from) {
            Some(Cell::Assistant(reply)) if !reply.sealed => Some(reply),
            _ => None,
        };
        let mut lines = Vec::new();
        if let Some(partial) = &self.partial {
            // The reply leaves only through `final_lines`. If the cells no longer hold it, the
            // state was replaced, and the rest of it will never come.
            let alive = cells.iter().rev().any(|cell| {
                matches!(cell, Cell::Assistant(reply) if reply.id == partial.id
                    && (reply.text.starts_with(partial.committed.as_str())
                        || reply.transformed
                        || reply.interim))
            });
            if !alive {
                self.partial = None;
                lines.extend(notice_lines(CUT_OFF_NOTICE, render, width));
            }
        }
        let Some(reply) = head.filter(|_| allowed) else {
            return lines;
        };
        let done = match &self.partial {
            Some(partial) if partial.id == reply.id => partial.committed.len(),
            // Another reply heads the tail while the partial one waits to be committed.
            Some(_) => return lines,
            None => 0,
        };
        let Some(rest) = reply.text.get(done..) else {
            return lines;
        };
        let more = closed_blocks_len(rest);
        if more == 0 {
            return lines;
        }
        let piece = &rest[..more];
        let piece_cell = with_text(reply, piece);
        let rendered = if self.partial.is_some() {
            render.continuation_lines(&piece_cell, width)
        } else {
            render.lines(&piece_cell, width)
        };
        if rendered.is_empty() {
            // Nothing to show yet, such as a lone link definition: it waits for later blocks.
            return lines;
        }
        match &mut self.partial {
            Some(partial) => {
                lines.extend(after_committed(rendered));
                partial.committed.push_str(piece);
            }
            None => {
                lines.extend(rendered);
                self.partial = Some(Partial {
                    id: reply.id,
                    committed: piece.to_owned(),
                });
            }
        }
        lines
    }

    /// The session's state was replaced. A partly committed reply cannot finish, so it is
    /// closed with a notice.
    pub fn replaced(&mut self, render: &dyn CellLines, width: u16) -> Vec<Line<'static>> {
        match self.partial.take() {
            Some(_) => notice_lines(CUT_OFF_NOTICE, render, width),
            None => Vec::new(),
        }
    }

    /// The cell the live tail draws in place of `cell`: what is not committed yet of a partly
    /// committed reply. `None` for any other cell.
    #[must_use]
    pub fn live_rest(&self, cell: &Cell) -> Option<Cell> {
        let partial = self.partial.as_ref()?;
        match cell {
            Cell::Assistant(reply) if reply.id == partial.id => reply
                .text
                .strip_prefix(partial.committed.as_str())
                .map(|rest| with_text(reply, rest)),
            _ => None,
        }
    }

    /// Lines that commit newly final cells, with only the rest of a partly committed reply.
    ///
    /// Cells in front of that reply in `cells` arrived after its first part was committed, such
    /// as the end-of-turn reasoning, which goes before an open reply. Scrollback cannot take
    /// them in front of it any more, so they follow its rest.
    pub fn final_lines(
        &mut self,
        cells: &[Cell],
        render: &dyn CellLines,
        width: u16,
    ) -> Vec<Line<'static>> {
        let whole = |cells: &[Cell]| -> Vec<Line<'static>> {
            cells
                .iter()
                .flat_map(|cell| render.lines(cell, width))
                .collect()
        };
        let Some(partial) = self.partial.take() else {
            return whole(cells);
        };
        let found = cells.iter().enumerate().find_map(|(at, cell)| match cell {
            Cell::Assistant(reply) if reply.id == partial.id => Some((at, reply)),
            _ => None,
        });
        let Some((at, reply)) = found else {
            // Cells committed at once while the reply still streams.
            self.partial = Some(partial);
            return whole(cells);
        };
        let mut lines = if let Some(rest) = reply.text.strip_prefix(partial.committed.as_str()) {
            after_committed(render.continuation_lines(&with_text(reply, rest), width))
        } else if reply.transformed || reply.interim {
            let mut lines = notice_lines(REVISED_NOTICE, render, width);
            lines.extend(render.lines(&cells[at], width));
            lines
        } else {
            // Only those two replace a reply's text, so this is another session's cell that
            // carries the same id: the partly committed reply ended without its rest.
            let mut lines = notice_lines(CUT_OFF_NOTICE, render, width);
            lines.extend(whole(cells));
            return lines;
        };
        lines.extend(whole(&cells[..at]));
        lines.extend(whole(&cells[at + 1..]));
        lines
    }

    /// Whether part of a reply is in scrollback now.
    #[must_use]
    pub fn is_partial(&self) -> bool {
        self.partial.is_some()
    }
}

/// The rows that follow a committed part: a blank row, then `lines`, as one rendering of the
/// whole text separates two blocks. Nothing when `lines` is empty.
#[must_use]
pub fn after_committed(mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    if !lines.is_empty() {
        lines.insert(0, Line::default());
    }
    lines
}

fn with_text(reply: &AssistantCell, text: &str) -> Cell {
    Cell::Assistant(AssistantCell {
        text: text.to_owned(),
        ..reply.clone()
    })
}

fn notice_lines(text: &str, render: &dyn CellLines, width: u16) -> Vec<Line<'static>> {
    render.lines(
        &Cell::Notice(NoticeCell {
            id: CellId::new(0),
            kind: NoticeKind::System,
            text: text.to_owned(),
            detail: None,
            body: None,
        }),
        width,
    )
}
