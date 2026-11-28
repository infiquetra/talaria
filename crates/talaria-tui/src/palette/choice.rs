//! The inline `/theme` and `/skin` selector and its transcript preview.
//!
//! Finished scrollback keeps the colours it was written in, so the sample block is what shows a
//! candidate theme or skin on the transcript's own rows. It is drawn by the caller's cell
//! renderer, the one the transcript uses, so it cannot drift from the real rows.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use talaria_core::commands::{ChoiceItem, ChoiceList, PaletteRow, RowKind};
use talaria_core::session::{
    AssistantCell, Cell, CellId, NoticeCell, NoticeKind, ReasoningCell, ToolCell, ToolStatus,
    UserCell,
};

use crate::checklist::cut_to_width;
use crate::history::CellLines;
use crate::live::LiveSection;
use crate::theme::surfaces::{ViewStyles, tokens as t};

/// Key hints at the end of the selector's footer.
const CHOICE_KEYS: &str = "\u{2191}\u{2193} preview \u{b7} Enter apply \u{b7} Esc cancel";

/// The appearance list, preview and key hints share one height budget.
pub struct ChoiceSection {
    rows: Vec<PaletteRow>,
    selected_row: usize,
    selected: Option<ChoiceItem>,
    current_in_group: Option<String>,
    filter: String,
    cap: u16,
    note: Option<String>,
    sample: Vec<Line<'static>>,
    views: Option<ViewStyles>,
}

impl ChoiceSection {
    /// Keep at least four live rows for the composer and status on short screens.
    #[must_use]
    pub fn new(list: &ChoiceList, terminal_rows: u16) -> Self {
        let selected = list.selected().cloned();
        let current_in_group = selected
            .as_ref()
            .filter(|item| !item.current)
            .and_then(|item| {
                list.items()
                    .iter()
                    .find(|other| other.current && other.group == item.group)
            })
            .map(|item| item.name.clone());
        Self {
            rows: list.rows(),
            selected_row: list.selected_row(),
            selected,
            current_in_group,
            filter: super::render::clean(list.filter()),
            cap: (terminal_rows / 2)
                .clamp(3, 13)
                .min(crate::live::max_live_height(terminal_rows).saturating_sub(4)),
            note: list.note().map(super::render::clean),
            sample: Vec::new(),
            views: None,
        }
    }

    #[must_use]
    pub fn with_theme(mut self, views: ViewStyles) -> Self {
        self.views = Some(views);
        self
    }

    /// Width available to the transcript renderer. A single-column layout keeps the description.
    pub fn sample_width(width: u16) -> Option<u16> {
        (width >= 56).then(|| width - list_width(width) - 2)
    }

    #[must_use]
    pub fn with_sample(mut self, sample: Vec<Line<'static>>) -> Self {
        self.sample = sample;
        self
    }

    /// Restrict the selector to rows left by the composer and its status lines.
    #[must_use]
    pub fn within_rows(mut self, available: u16) -> Self {
        self.cap = self.cap.min(available);
        self
    }

    fn style(&self, token: crate::theme::surfaces::Token, fallback: Style) -> Style {
        self.views
            .as_ref()
            .map_or(fallback, |views| views.inline(token, fallback))
    }

    fn description(&self, width: u16) -> Vec<String> {
        self.selected
            .as_ref()
            .filter(|item| !item.description.is_empty())
            .map(|item| crate::history::word_wrapped(&item.description, width))
            .unwrap_or_default()
    }

    fn note_lines(&self, width: u16) -> Vec<String> {
        self.note
            .as_deref()
            .map_or_else(Vec::new, |note| crate::history::word_wrapped(note, width))
    }

    fn draw_choices(&self, area: Rect, buf: &mut Buffer) {
        let room = usize::from(area.height);
        if room == 0 {
            return;
        }
        let mut first = (self.selected_row + 1).saturating_sub(room);
        let group = |index: usize| {
            self.rows
                .get(index)
                .filter(|row| row.kind == RowKind::Command)
                .and_then(|row| row.group.as_deref())
        };
        // If the viewport starts partway through a group, keep its label above the choices.
        if first > 0 && room > 1 && group(first).is_some() {
            first += 1;
        }
        let sticky = (first > 0 && room > 1).then(|| group(first)).flatten();
        let heading = self.style(t::LIST_HEADER, Style::new().add_modifier(Modifier::BOLD));
        let mut offset = 0;
        if let Some(group) = sticky {
            put(area, 0, &format!("  {group}"), heading, buf);
            offset += 1;
        }
        for (index, row) in self.rows.iter().enumerate().skip(first).take(room - offset) {
            let (prefix, style) = match row.kind {
                RowKind::Header => ("  ", heading),
                RowKind::Command if index == self.selected_row => (
                    "> ",
                    self.views.as_ref().map_or(heading, |views| {
                        views.inline_selection(t::LIST_SELECTED, heading)
                    }),
                ),
                RowKind::Command => ("  ", self.style(t::LIST_ROW, Style::new())),
                _ => (
                    "  ",
                    self.style(t::LIST_MUTED, Style::new().add_modifier(Modifier::DIM)),
                ),
            };
            put(
                area,
                offset as u16,
                &format!("{prefix}{}", row.name),
                style,
                buf,
            );
            offset += 1;
        }
    }

    fn draw_preview(&self, area: Rect, include_sample: bool, buf: &mut Buffer) {
        let Some(item) = self.selected.as_ref() else {
            return;
        };
        if area.height == 0 {
            return;
        }
        let mark = if item.current { " (current)" } else { "" };
        let title = format!("Preview  {}{mark}", item.name);
        let heading = self.style(t::LIST_HEADER, Style::new().add_modifier(Modifier::BOLD));
        let muted = self.style(t::LIST_MUTED, Style::new().add_modifier(Modifier::DIM));
        put(area, 0, &title, heading, buf);
        let description = self.description(area.width);
        let shown = usize::from(area.height.saturating_sub(1)).min(description.len());
        for (index, line) in description.iter().take(shown).enumerate() {
            let row = 1 + index as u16;
            if index + 1 == shown && shown < description.len() {
                put(area, row, &format!("{line}…"), muted, buf);
            } else {
                put(area, row, line, muted, buf);
            }
        }
        if include_sample {
            let start = shown + 2;
            // Keep the sample heading together with at least one example row.
            if usize::from(area.height).saturating_sub(start) >= 2 {
                for (y, line) in (area.y + start as u16..area.bottom()).zip(&self.sample) {
                    buf.set_line(area.x, y, line, area.width);
                }
            }
        }
    }

    fn footer(&self, width: u16) -> String {
        let text = if self.selected.is_none() {
            "type to filter · Esc cancel"
        } else {
            CHOICE_KEYS
        };
        let with_filter = format!("type to filter · {text}");
        let fits = |s: &str| unicode_width::UnicodeWidthStr::width(s) <= usize::from(width);
        if self.filter.is_empty() && self.selected.is_some() && fits(&with_filter) {
            with_filter
        } else if fits(text) {
            text.to_owned()
        } else if self.selected.is_some() && fits("↑↓ preview · Esc cancel") {
            "↑↓ preview · Esc cancel".into()
        } else {
            "Esc cancel".into()
        }
    }
}

impl LiveSection for ChoiceSection {
    fn height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }
        let list = self.rows.len();
        let preview = usize::from(self.selected.is_some());
        let body = if let Some(sample_width) =
            Self::sample_width(width).filter(|_| self.selected.is_some())
        {
            let sample = self.sample.len() + usize::from(!self.sample.is_empty());
            list.max(preview + self.description(sample_width).len() + sample)
        } else {
            list + (preview + self.description(width).len()).min(4)
        };
        (2 + self.note_lines(width).len() + body).min(usize::from(self.cap)) as u16
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let height = area.height.min(self.height(area.width));
        if height == 0 {
            return;
        }
        let area = Rect::new(area.x, area.y, area.width, height);
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buf[(x, y)].reset();
            }
        }
        if let Some(views) = &self.views {
            views.finish(area, buf, t::LIST_ROW, false);
        }
        let title = u16::from(height >= 3);
        let heading = self.style(t::LIST_HEADER, Style::new().add_modifier(Modifier::BOLD));
        let muted = self.style(t::LIST_MUTED, Style::new().add_modifier(Modifier::DIM));
        let footer = self.footer(area.width);
        if title > 0 {
            let text = if self.filter.is_empty() {
                if area.width >= 30 && !footer.starts_with("type to filter") {
                    "Appearance  ·  type to filter".to_owned()
                } else {
                    "Appearance".to_owned()
                }
            } else {
                format!("Appearance  ·  Filter: {}", self.filter)
            };
            put(area, 0, &text, heading, buf);
            if let Some(current) = &self.current_in_group {
                let start = unicode_width::UnicodeWidthStr::width(text.as_str()) + 2;
                let start = u16::try_from(start).unwrap_or(u16::MAX);
                let remaining = area.width.saturating_sub(start);
                if remaining >= 16 {
                    let current = cut_to_width(
                        &format!("Current: {}", super::render::clean(current)),
                        remaining,
                    );
                    let width = unicode_width::UnicodeWidthStr::width(current.as_str()) as u16;
                    let right = Rect::new(area.right() - width, area.y, width, 1);
                    put(right, 0, &current, muted, buf);
                }
            }
        }
        let note_room = height.saturating_sub(title + 4);
        let mut note_height = 0;
        for (row, line) in (title..title + note_room).zip(self.note_lines(area.width)) {
            put(area, row, &line, muted, buf);
            note_height += 1;
        }
        let y = area.y + title + note_height;
        let body = Rect::new(area.x, y, area.width, area.bottom() - y - 1);
        if let Some(preview) = Self::sample_width(area.width).filter(|_| self.selected.is_some()) {
            let list = list_width(area.width);
            self.draw_choices(Rect::new(body.x, body.y, list, body.height), buf);
            let divider = self
                .views
                .as_ref()
                .map_or("│", |views| views.border_symbol("│"));
            let style = self.style(t::LIST_DIVIDER, muted);
            for y in body.y..body.bottom() {
                buf[(body.x + list, y)].set_symbol(divider).set_style(style);
            }
            self.draw_preview(
                Rect::new(body.x + list + 2, body.y, preview, body.height),
                true,
                buf,
            );
        } else {
            let preview = usize::from(self.selected.is_some()) + self.description(body.width).len();
            let preview = preview
                .min(4)
                .min(usize::from(body.height.saturating_sub(2))) as u16;
            let list = body.height - preview;
            self.draw_choices(Rect::new(body.x, body.y, body.width, list), buf);
            self.draw_preview(
                Rect::new(body.x, body.y + list, body.width, preview),
                false,
                buf,
            );
        }
        put(area, height - 1, &footer, muted, buf);
    }
}

fn list_width(width: u16) -> u16 {
    (width / 5 * 2).clamp(21, 32)
}

fn put(area: Rect, row: u16, text: &str, style: Style, buf: &mut Buffer) {
    if area.width > 0 && row < area.height {
        let text = cut_to_width(&super::render::clean(text), area.width);
        buf.set_stringn(area.x, area.y + row, text, usize::from(area.width), style);
    }
}

/// Example transcript rows drawn with the same renderer as the conversation.
/// The narrow column keeps one prompt, tool and reply under the heading.
pub fn sample_lines(cells: &dyn CellLines, width: u16) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        "Transcript sample",
        Style::new().add_modifier(Modifier::DIM),
    )];
    let compact = width < 50;
    for cell in sample_cells(compact) {
        if compact && matches!(cell, Cell::Reasoning(_) | Cell::Notice(_)) {
            continue;
        }
        lines.extend(cells.lines(&cell, width));
    }
    lines
}

fn sample_cells(compact: bool) -> [Cell; 5] {
    let (prompt, reply) = if compact {
        ("Tidy build.sh.", "Ran `cargo fmt`.")
    } else {
        (
            "Sample prompt: tidy the build script",
            "Removed the repeated step and ran `cargo fmt`.",
        )
    };
    [
        Cell::User(UserCell {
            id: CellId::new(1),
            text: prompt.into(),
            expanded_paste: None,
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
            text: reply.into(),
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
