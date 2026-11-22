//! The `/theme` and `/skin` selector, drawn by the slash palette's painter, and the sample block
//! that shows the selected look above it.
//!
//! Finished scrollback keeps the colours it was written in, so the sample block is what shows a
//! candidate theme or skin on the transcript's own rows. It is drawn by the caller's cell
//! renderer, the one the transcript uses, so it cannot drift from the real rows.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};
use talaria_core::commands::{ChoiceList, PaletteState};
use talaria_core::session::{
    AssistantCell, Cell, CellId, NoticeCell, NoticeKind, ReasoningCell, ToolCell, ToolStatus,
    UserCell,
};

use super::PaletteSection;
use crate::history::CellLines;
use crate::live::LiveSection;
use crate::theme::surfaces::{ViewStyles, tokens as t};

/// Key hints at the end of the selector's footer.
const CHOICE_KEYS: &str = "\u{2191}\u{2193} preview \u{b7} Enter apply \u{b7} Esc cancel";

/// One frame of the selector, laid out like the slash palette, under its note.
///
/// The note has rows of its own, wrapped, so a long one never pushes the key hints off the
/// footer.
pub struct ChoiceSection {
    palette: PaletteSection,
    note: Option<String>,
    views: Option<ViewStyles>,
}

impl ChoiceSection {
    /// `terminal_rows` is the whole terminal, used only for the height cap.
    #[must_use]
    pub fn new(list: &ChoiceList, terminal_rows: u16) -> Self {
        let filter = if list.filter().is_empty() {
            "type to filter".to_owned()
        } else {
            format!("filter: {}", list.filter())
        };
        let footer = format!("{filter} \u{b7} {CHOICE_KEYS}");
        let rows = list.rows();
        let state = PaletteState {
            open: true,
            selected: list.selected_row(),
            total: list.items().len(),
            footer: Some(footer),
            rows,
            ..PaletteState::closed()
        };
        Self {
            palette: PaletteSection::new(&state, terminal_rows),
            // The note can carry Hermes's own refusal text.
            note: list.note().map(super::render::clean),
            views: None,
        }
    }

    /// Draw with the resolved appearance, as the palette does.
    #[must_use]
    pub fn with_theme(self, views: ViewStyles) -> Self {
        Self {
            palette: self.palette.with_theme(views.clone()),
            views: Some(views),
            ..self
        }
    }

    fn note(&self) -> Option<Paragraph<'_>> {
        let note = self.note.as_deref()?;
        Some(
            Paragraph::new(note)
                .style(Style::new().add_modifier(Modifier::DIM))
                .wrap(Wrap { trim: true }),
        )
    }

    fn note_height(&self, width: u16) -> u16 {
        self.note().map_or(0, |note| {
            u16::try_from(note.line_count(width.max(1))).unwrap_or(u16::MAX)
        })
    }
}

impl LiveSection for ChoiceSection {
    fn height(&self, width: u16) -> u16 {
        let palette = self.palette.height(width);
        if palette == 0 {
            return 0;
        }
        palette.saturating_add(self.note_height(width))
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let palette = self.palette.height(area.width).min(area.height);
        let note = (area.height - palette).min(self.note_height(area.width));
        if let Some(text) = self.note().filter(|_| note > 0) {
            let rows = Rect::new(area.x, area.y, area.width, note);
            for y in rows.top()..rows.bottom() {
                for x in rows.left()..rows.right() {
                    buf[(x, y)].reset();
                }
            }
            text.render(rows, buf);
            if let Some(views) = &self.views {
                views.finish(rows, buf, t::LIST_ROW, false);
            }
        }
        self.palette
            .render(Rect::new(area.x, area.y + note, area.width, palette), buf);
    }
}

/// A few synthetic transcript rows, one of each common kind, drawn by `cells`: a prompt, a
/// thinking line, a tool row, a reply with inline code and a notice, under a dim title.
pub fn sample_lines(cells: &dyn CellLines, width: u16) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        "Sample of the selected look",
        Style::new().add_modifier(Modifier::DIM),
    )];
    for cell in sample_cells() {
        lines.extend(cells.lines(&cell, width));
    }
    lines
}

fn sample_cells() -> [Cell; 5] {
    [
        Cell::User(UserCell {
            id: CellId::new(1),
            text: "Sample prompt: tidy the build script".into(),
        }),
        Cell::Reasoning(ReasoningCell {
            id: CellId::new(2),
            text: "The script repeats one step.".into(),
            sealed: true,
            latest_step: false,
            no_reply: false,
        }),
        Cell::Tool(ToolCell {
            id: CellId::new(3),
            tool_id: "sample".into(),
            name: "read_file".into(),
            context: Some("build.sh".into()),
            args: None,
            labels: Vec::new(),
            status: ToolStatus::Succeeded,
            progress: None,
            summary: None,
            result: None,
            diff: None,
            duration_s: Some(0.2),
        }),
        Cell::Assistant(AssistantCell {
            id: CellId::new(4),
            text: "Removed the repeated step and ran `cargo fmt`.".into(),
            sealed: true,
            interim: false,
            transformed: false,
        }),
        Cell::Notice(NoticeCell {
            id: CellId::new(5),
            kind: NoticeKind::Diagnostic,
            text: "Sample notice from Talaria".into(),
            detail: None,
            body: None,
        }),
    ]
}
