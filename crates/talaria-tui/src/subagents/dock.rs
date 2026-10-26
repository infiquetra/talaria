//! The live subagent dock and shared row styles.
use crate::{checklist::cut_to_width, history::sanitize, live::LiveSection};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
};
use talaria_core::subagents::{RowState, Spawning, SubagentCounts, SubagentRow, SubagentTree};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug)]
pub struct SubagentStyle {
    pub header: Style,
    pub hint: Style,
    pub running: Style,
    pub queued: Style,
    pub completed: Style,
    pub failed: Style,
    pub interrupted: Style,
    pub gone: Style,
}
impl Default for SubagentStyle {
    fn default() -> Self {
        Self {
            header: Style::new().bold(),
            hint: Style::new().dim(),
            running: Style::new().fg(Color::Cyan),
            queued: Style::new().dim(),
            completed: Style::new().fg(Color::Green),
            failed: Style::new().fg(Color::Red),
            interrupted: Style::new().fg(Color::Yellow),
            gone: Style::new().dim(),
        }
    }
}
impl SubagentStyle {
    pub fn plain() -> Self {
        Self {
            running: Style::new(),
            completed: Style::new(),
            failed: Style::new(),
            interrupted: Style::new(),
            ..Self::default()
        }
    }
    pub(super) fn state(&self, state: RowState) -> Style {
        match state {
            RowState::Running => self.running,
            RowState::Queued => self.queued,
            RowState::Completed => self.completed,
            RowState::Failed(_) => self.failed,
            RowState::Interrupted => self.interrupted,
            RowState::Gone => self.gone,
        }
    }
}
pub(super) fn literal(text: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for c in text.chars() {
        if matches!(c, '\r' | '\n' | '\t') {
            if !separator {
                output.push(' ');
            }
            separator = true;
        } else {
            output.push(c);
            separator = false;
        }
    }
    sanitize(&output).into_owned()
}
pub(super) fn activity(row: &SubagentRow) -> String {
    if row.stop_requested {
        return "stopping".into();
    }
    row.activity
        .back()
        .map(|a| literal(&a.text))
        .unwrap_or_else(|| {
            if row.state == RowState::Queued {
                "queued"
            } else {
                "starting"
            }
            .into()
        })
}
pub(super) fn styled_row(
    text: String,
    glyph: char,
    state: RowState,
    width: u16,
    style: &SubagentStyle,
    selected: bool,
) -> Line<'static> {
    let text = cut_to_width(&text, width);
    let base = if selected {
        Style::new().reversed()
    } else {
        Style::new()
    };
    if let Some(at) = text.find(glyph) {
        let end = at + glyph.len_utf8();
        let mut line = Line::from(vec![
            Span::styled(text[..at].to_owned(), base),
            Span::styled(text[at..end].to_owned(), base.patch(style.state(state))),
            Span::styled(
                text[end..].to_owned(),
                if selected { base.bold() } else { base },
            ),
        ]);
        if let Some(at) = text.rfind(" · ") {
            dim_suffix(&mut line, &text[at..], base.patch(style.hint));
        }
        line
    } else {
        Line::styled(text, base)
    }
}
pub(super) fn dim_suffix(line: &mut Line<'static>, suffix: &str, style: Style) {
    if suffix.is_empty() {
        return;
    }
    if let Some(last) = line.spans.last_mut()
        && let Some(prefix) = last.content.strip_suffix(suffix)
    {
        last.content = prefix.to_owned().into();
        line.spans.push(Span::styled(suffix.to_owned(), style));
    }
}
pub struct SubagentDock<'a> {
    tree: &'a SubagentTree,
    counts: SubagentCounts,
    spawning: Spawning,
    open_hint: Option<&'a str>,
    style: &'a SubagentStyle,
    max_rows: u16,
    terminal_rows: u16,
}
impl<'a> SubagentDock<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tree: &'a SubagentTree,
        counts: SubagentCounts,
        _key_label: Option<&'a str>,
        spawning: Spawning,
        open_hint: Option<&'a str>,
        style: &'a SubagentStyle,
        max_rows: u16,
        terminal_rows: u16,
    ) -> Self {
        Self {
            tree,
            counts,
            spawning,
            open_hint,
            style,
            max_rows,
            terminal_rows,
        }
    }
    pub fn lines(&self, width: u16) -> Vec<Line<'static>> {
        if width == 0 || self.max_rows == 0 || self.counts.live == 0 {
            return vec![];
        }
        let mut base = format!(
            "⎇ Subagents · {} running",
            self.counts.live.saturating_sub(self.counts.queued)
        );
        if self.counts.queued > 0 {
            base.push_str(&format!(" · {} queued", self.counts.queued));
        }
        let paused = if matches!(self.spawning, Spawning::Paused(_)) {
            " · spawning paused"
        } else {
            ""
        };
        let hint = self
            .open_hint
            .map(|s| format!(" · {} to open", literal(s)))
            .unwrap_or_default();
        let full = format!("{base}{paused}{hint}");
        let header = if full.width() <= usize::from(width) {
            full
        } else if format!("{base}{paused}").width() <= usize::from(width) {
            format!("{base}{paused}")
        } else {
            base
        };
        let mut lines = vec![Line::styled(
            cut_to_width(&header, width),
            self.style.header,
        )];
        if self.terminal_rows < 16 {
            return lines;
        }
        let live: Vec<_> = self
            .tree
            .rows_in_order()
            .into_iter()
            .filter(|r| self.tree.row(r.index).state.is_live())
            .collect();
        for entry in live.iter().take(usize::from(self.max_rows)) {
            let row = self.tree.row(entry.index);
            let prefix = format!(
                "{}{} {} · ",
                " ".repeat(2 + 2 * usize::from(entry.depth.min(3))),
                row.state.glyph(),
                literal(&row.goal)
            );
            let activity_width = usize::from(width)
                .saturating_sub(prefix.width())
                .max(10)
                .min(usize::from(u16::MAX)) as u16;
            let text = format!("{prefix}{}", cut_to_width(&activity(row), activity_width));
            lines.push(styled_row(
                text,
                row.state.glyph(),
                row.state,
                width,
                self.style,
                false,
            ));
        }
        if live.len() > usize::from(self.max_rows) {
            lines.push(Line::styled(
                cut_to_width(
                    &format!("  … +{} more", live.len() - usize::from(self.max_rows)),
                    width,
                ),
                self.style.hint,
            ));
        }
        lines
    }
}
impl LiveSection for SubagentDock<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.lines(width).len()).unwrap_or(u16::MAX)
    }
    fn render(&self, area: Rect, buf: &mut Buffer) {
        for (y, line) in (0..area.height).zip(self.lines(area.width)) {
            buf.set_line(area.x, area.y + y, &line, area.width);
        }
    }
}
