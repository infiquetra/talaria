//! The live rows and the process overlay for background work.

use std::cell::{Cell, RefCell};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget, Wrap};
use talaria_core::background::{
    BackgroundCell, BackgroundWork, BgOrigin, BgResult, BgState, FINISHED_ROW_MS, KillState,
    ListState, ProcessId, ProcessRow, ProcessStatus, format_elapsed, list_failure_text,
};
use talaria_core::side::clean_output;
use unicode_segmentation::UnicodeSegmentation;

use crate::history::{grapheme_width, sanitize, sanitize_line, wrap_lines};
use crate::keys::{Key, KeyCode, KeyKind};
use crate::live::LiveSection;
use crate::markdown::{MarkdownOptions, render_markdown};

pub use talaria_core::background::ProcessesIntent;

/// Colours for the live rows, the overlay, and a result cell.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundTheme {
    /// Dim text: footers, elapsed times, closed-view notes.
    pub dim: Style,
    /// A warning note.
    pub warning: Style,
    /// An error note or a failed result.
    pub error: Style,
    /// A finished process that exited cleanly.
    pub ok: Style,
    /// The selected overlay row.
    pub selected: Style,
}

impl Default for BackgroundTheme {
    fn default() -> Self {
        Self {
            dim: Style::default().add_modifier(Modifier::DIM),
            warning: Style::default().fg(Color::Yellow),
            error: Style::default().fg(Color::Red),
            ok: Style::default().fg(Color::Green),
            selected: Style::default().add_modifier(Modifier::REVERSED),
        }
    }
}

/// The live rows above the composer.
pub struct BackgroundSection<'a> {
    /// The session's background work.
    pub work: &'a BackgroundWork,
    /// Width used when a caller asks for height before the area is known.
    pub width_hint: u16,
    /// Colours.
    pub theme: &'a BackgroundTheme,
}

impl LiveSection for BackgroundSection<'_> {
    fn height(&self, width: u16) -> u16 {
        u16::try_from(self.lines(width).len()).unwrap_or(u16::MAX)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        for (offset, line) in (0..area.height).zip(self.lines(area.width)) {
            buf.set_line(area.x, area.y + offset, &line, area.width);
        }
    }
}

impl BackgroundSection<'_> {
    fn lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for task in self.work.bg_tasks().iter().take(2) {
            let prompt = title(&task.text, "", 60);
            let state = match &task.state {
                BgState::Starting => "starting".to_string(),
                BgState::ReplyLost => "reply lost · the answer will still appear".to_string(),
                BgState::Running { accepted_ms, .. } => format!(
                    "running · {}",
                    format_elapsed(self.work.now_ms().saturating_sub(*accepted_ms) / 1_000)
                ),
            };
            lines.push(Line::styled(
                ellipsize(&format!("◌ bg · {prompt} · {state}"), width),
                self.theme.dim,
            ));
        }
        let more = self.work.bg_tasks().len().saturating_sub(2);
        if more > 0 {
            let noun = if more == 1 { "task" } else { "tasks" };
            lines.push(Line::styled(
                ellipsize(&format!("◌ +{more} more background {noun}"), width),
                self.theme.dim,
            ));
        }
        let rows: Vec<_> = self.work.rows().collect();
        let running: Vec<_> = rows.iter().copied().filter(|row| is_running(row)).collect();
        let (text, style) = if let [row] = running.as_slice() {
            let output = if row.live_line.is_empty() {
                "no output yet".to_string()
            } else {
                format!("last: {}", clean(&row.live_line))
            };
            (
                format!(
                    "⚙ {} · {} · {output}",
                    title(&row.command, "background process", 60),
                    elapsed(row, self.work.now_ms(), self.work.wall_s())
                ),
                Style::default(),
            )
        } else if !running.is_empty() {
            let newest = running.last().copied();
            (
                format!(
                    "⚙ {} background processes running · newest: {}",
                    running.len(),
                    newest.map_or_else(String::new, |row| title(
                        &row.command,
                        "background process",
                        60
                    ))
                ),
                Style::default(),
            )
        } else if let Some(row) = rows.iter().copied().find(|row| {
            row.finished_seen_ms
                .is_some_and(|at| self.work.now_ms().saturating_sub(at) < FINISHED_ROW_MS)
        }) {
            (
                format!(
                    "{} {} · {} · ended {} ago",
                    glyph(&row.status),
                    title(&row.command, "background process", 60),
                    label(&row.status),
                    ended_ago(row, self.work.now_ms(), self.work.wall_s())
                ),
                status_style(&row.status, self.theme),
            )
        } else {
            return lines;
        };
        lines.push(Line::styled(
            with_suffix(&text, " · /processes", width),
            style,
        ));
        lines
    }
}

/// Lines for one finished background-task cell.
#[must_use]
pub fn background_cell_lines(
    cell: &BackgroundCell,
    width: u16,
    theme: &BackgroundTheme,
) -> Vec<Line<'static>> {
    let header = match cell.origin {
        BgOrigin::Here => format!(
            "bg · {}",
            title(cell.prompt.as_deref().unwrap_or_default(), "", 60)
        ),
        BgOrigin::Elsewhere => format!("bg · {} · started in another window", clean(&cell.task_id)),
    };
    let mut lines = wrap_lines(vec![Line::from(header)], width);
    match &cell.result {
        BgResult::Failed(text) => lines.extend(body(
            &format!("The background task failed: {}", clean(text)),
            width,
            theme.error,
        )),
        BgResult::Answer(text) => {
            let answer = clean_output(text);
            if answer.trim().is_empty() {
                lines.extend(body("(no answer)", width, theme.dim));
            } else {
                let options = MarkdownOptions {
                    width: width.saturating_sub(2).max(1),
                    color: theme.warning.fg.is_some()
                        || theme.error.fg.is_some()
                        || theme.ok.fg.is_some(),
                    ..MarkdownOptions::default()
                };
                lines.extend(render_markdown(&answer, &options).into_iter().map(indent));
            }
        }
    }
    lines.extend(body(
        &format!(
            "background task {} · not part of the conversation",
            clean(&cell.task_id)
        ),
        width,
        theme.dim,
    ));
    lines
}

/// The process overlay: selection, scroll, follow, and a confirm that expires.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessesView {
    selected: RefCell<Option<ProcessId>>,
    scroll: usize,
    follow: bool,
    confirm: Option<Confirmation>,
    note: Option<String>,
    // Geometry and the last selected position belong to the view, not the session.
    pane_height: Cell<u16>,
    max_scroll: Cell<usize>,
    selected_index: Cell<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Confirmation {
    key: char,
    until_ms: u64,
    process_id: Option<ProcessId>,
}

impl ProcessesView {
    /// An overlay with no selection and follow on.
    #[must_use]
    pub fn new() -> Self {
        Self {
            follow: true,
            ..Self::default()
        }
    }

    /// Reset transient view state and restore the session's remembered selection.
    pub fn reopen(&mut self, work: &BackgroundWork) {
        *self = Self::new();
        *self.selected.get_mut() = work.selected_process().cloned();
    }

    /// Handle one key. `now_ms` is the app clock, used for the confirm window.
    #[must_use]
    pub fn handle_key(&mut self, key: Key, work: &BackgroundWork, now_ms: u64) -> ProcessesIntent {
        if key.kind == KeyKind::Release {
            return ProcessesIntent::None;
        }
        let previous_confirm = self.confirm.take();
        self.note = None;
        if key.mods.alt || key.mods.super_key || key.mods.other {
            return ProcessesIntent::None;
        }
        if key.mods.ctrl {
            return if key.code == KeyCode::Char('c') {
                ProcessesIntent::Close
            } else {
                ProcessesIntent::None
            };
        }
        let rows: Vec<_> = work.rows().collect();
        let index = self.index(&rows);
        let selected = rows.get(index).copied();
        if let Some(row) = selected {
            *self.selected.get_mut() = Some(row.id.clone());
            self.selected_index.set(index);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => ProcessesIntent::Close,
            KeyCode::Char('r') => ProcessesIntent::Refresh,
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Down | KeyCode::Char('j') => {
                let next = if matches!(key.code, KeyCode::Up | KeyCode::Char('k')) {
                    index.saturating_sub(1)
                } else {
                    index.saturating_add(1).min(rows.len().saturating_sub(1))
                };
                if next != index
                    && let Some(row) = rows.get(next)
                {
                    *self.selected.get_mut() = Some(row.id.clone());
                    self.selected_index.set(next);
                    self.scroll = 0;
                    self.follow = true;
                    return ProcessesIntent::Select(row.id.clone());
                }
                ProcessesIntent::None
            }
            KeyCode::PageUp | KeyCode::PageDown => {
                let position = if self.follow {
                    self.max_scroll.get()
                } else {
                    self.scroll
                };
                let page = usize::from(self.pane_height.get().saturating_sub(1).max(1));
                self.scroll = if key.code == KeyCode::PageUp {
                    position.saturating_sub(page)
                } else {
                    position.saturating_add(page).min(self.max_scroll.get())
                };
                self.follow = false;
                ProcessesIntent::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.scroll = 0;
                self.follow = false;
                ProcessesIntent::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.follow = true;
                ProcessesIntent::None
            }
            KeyCode::Char('d') => {
                if let Some(row) = selected {
                    if !is_running(row) {
                        return ProcessesIntent::Dismiss(row.id.clone());
                    }
                    self.note =
                        Some("A running process cannot be dismissed. Kill it first with x.".into());
                }
                ProcessesIntent::None
            }
            KeyCode::Char('x') if key.kind == KeyKind::Press => {
                if let Some(row) = selected {
                    if !is_running(row) {
                        self.note = Some("This process has already ended.".into());
                    } else if matches!(row.kill, KillState::Sending { .. }) {
                        self.note =
                            Some("Already killing this process; waiting for Hermes.".into());
                    } else if confirmed(previous_confirm.as_ref(), 'x', Some(&row.id), now_ms) {
                        return ProcessesIntent::Kill(row.id.clone());
                    } else {
                        self.confirm = Some(Confirmation {
                            key: 'x',
                            until_ms: now_ms.saturating_add(3_000),
                            process_id: Some(row.id.clone()),
                        });
                        self.note = Some(format!(
                            "Press x again within 3 s to kill: {}",
                            title(&row.command, "background process", 60)
                        ));
                    }
                }
                ProcessesIntent::None
            }
            KeyCode::Char('S') if key.kind == KeyKind::Press => {
                let count = rows.iter().filter(|row| is_running(row)).count();
                if count == 0 {
                    self.note = Some("No background processes are running.".into());
                } else if confirmed(previous_confirm.as_ref(), 'S', None, now_ms) {
                    return ProcessesIntent::StopAll;
                } else {
                    self.confirm = Some(Confirmation {
                        key: 'S',
                        until_ms: now_ms.saturating_add(3_000),
                        process_id: None,
                    });
                    let target = if count == 1 {
                        "the 1 running process".to_string()
                    } else {
                        format!("all {count} running processes")
                    };
                    self.note = Some(format!(
                        "Press S again within 3 s to stop {target} of this session (also interrupts its background delegations)."
                    ));
                }
                ProcessesIntent::None
            }
            _ => ProcessesIntent::None,
        }
    }

    /// Draw the overlay into `buf` for `area`.
    pub fn render(
        &self,
        work: &BackgroundWork,
        now_ms: u64,
        wall_s: f64,
        area: Rect,
        buf: &mut Buffer,
        theme: &BackgroundTheme,
    ) {
        Clear.render(area, buf);
        if area.width < 40 || area.height < 10 {
            Paragraph::new("Widen the terminal to at least 40 columns and 10 rows to see background processes. Esc closes.")
                .wrap(Wrap { trim: true })
                .render(area, buf);
            return;
        }
        let rows: Vec<_> = work.rows().collect();
        let running = rows.iter().filter(|row| is_running(row)).count();
        let mut header = Line::from(format!(
            "Background processes · {running} running · {} finished",
            rows.len() - running
        ));
        if let ListState::Stale { since_ms, .. } = work.list_state() {
            header.spans.push(Span::styled(
                format!(
                    " · list from {} ago",
                    format_elapsed(now_ms.saturating_sub(*since_ms) / 1_000)
                ),
                theme.warning,
            ));
        }
        buf.set_line(area.x, area.y, &header, area.width);
        let list_height = (area.height / 3)
            .min(8)
            .min(u16::try_from(rows.len()).unwrap_or(u16::MAX));
        let index = self.index(&rows);
        self.selected_index.set(index);
        let first = index.saturating_sub(usize::from(list_height.saturating_sub(1)));
        for (offset, row) in (0..list_height).zip(rows.iter().skip(first)) {
            let selected = first + usize::from(offset) == index;
            let status = if matches!(row.kill, KillState::Sending { .. }) {
                "killing…".to_string()
            } else {
                label(&row.status)
            };
            let age = if is_running(row) {
                elapsed(row, now_ms, wall_s)
            } else {
                format!("ended {}", ended_ago(row, now_ms, wall_s))
            };
            let prefix = format!(
                "{} {} {:<12} {:>10}   ",
                if selected { '▸' } else { ' ' },
                glyph(&row.status),
                status,
                age
            );
            let command = title(&row.command, "background process", usize::MAX);
            let text = format!(
                "{prefix}{}",
                ellipsize(&command, area.width.saturating_sub(columns(&prefix)))
            );
            let style = if selected {
                status_style(&row.status, theme).patch(theme.selected)
            } else {
                status_style(&row.status, theme)
            };
            buf.set_line(
                area.x,
                area.y + 1 + offset,
                &Line::styled(text, style),
                area.width,
            );
        }
        let separator_y = area.y + 1 + list_height;
        separator(area.x, separator_y, area.width, buf, theme.dim);
        let pane = Rect::new(
            area.x,
            separator_y + 1,
            area.width,
            area.height.saturating_sub(list_height + 5),
        );
        self.pane_height.set(pane.height);
        let selected = rows.get(index).copied();
        if let ListState::Stopped { .. } = work.list_state() {
            Paragraph::new(sanitize(
                &list_failure_text(work.list_state(), now_ms).unwrap_or_default(),
            ))
            .style(theme.warning)
            .wrap(Wrap { trim: true })
            .render(pane, buf);
        } else if let Some(row) = selected {
            self.render_output(work, row, pane, buf, theme);
        } else {
            let mut lines = body_plain(
                "No background processes in this session.",
                pane.width,
                Style::default(),
            );
            lines.extend(body_plain("When the agent runs a command with the terminal tool and background=true, it appears here with its output.", pane.width, theme.dim));
            draw_lines(lines, pane, buf);
        }
        separator(area.x, area.bottom() - 3, area.width, buf, theme.dim);
        let own_note = self.note.as_deref().filter(|_| {
            self.confirm
                .as_ref()
                .is_none_or(|confirm| now_ms <= confirm.until_ms)
        });
        let row_note = selected.and_then(|row| match &row.kill {
            KillState::Note { text } => Some(text.as_str()),
            _ => None,
        });
        let sending = selected
            .filter(|row| matches!(row.kill, KillState::Sending { .. }))
            .map(|row| format!("Killing {}…", title(&row.command, "background process", 60)));
        let failure = list_failure_text(work.list_state(), now_ms);
        let note = own_note
            .or(work.overlay_note())
            .or(row_note)
            .or(sending.as_deref())
            .or(failure.as_deref())
            .unwrap_or_default();
        buf.set_line(
            area.x,
            area.bottom() - 2,
            &Line::styled(ellipsize(&clean(note), area.width), theme.warning),
            area.width,
        );
        let footer = with_suffix(
            "↑↓ select · PgUp PgDn scroll · End follow · x kill · S stop all · d dismiss · r refresh",
            " · esc close",
            area.width,
        );
        buf.set_line(
            area.x,
            area.bottom() - 1,
            &Line::styled(footer, theme.dim),
            area.width,
        );
    }

    fn index(&self, rows: &[&ProcessRow]) -> usize {
        let mut selected = self.selected.borrow_mut();
        let index = selected
            .as_ref()
            .and_then(|id| rows.iter().position(|row| &row.id == id))
            .unwrap_or_else(|| self.selected_index.get().min(rows.len().saturating_sub(1)));
        *selected = rows.get(index).map(|row| row.id.clone());
        self.selected_index.set(index);
        index
    }

    fn render_output(
        &self,
        work: &BackgroundWork,
        row: &ProcessRow,
        area: Rect,
        buf: &mut Buffer,
        theme: &BackgroundTheme,
    ) {
        let mut lines = body_plain(
            &format!(
                "$ {}",
                title(&row.command, "background process", usize::MAX)
            ),
            area.width,
            theme.dim,
        );
        if row.agent_closed_view {
            lines.extend(body_plain(
                "The agent closed this view; the process keeps running.",
                area.width,
                theme.dim,
            ));
        }
        let output = work.output(&row.id);
        if output.is_some_and(|output| output.trimmed_front()) {
            lines.extend(body_plain(
                "… earlier output dropped",
                area.width,
                theme.dim,
            ));
        }
        let room = usize::from(area.height).saturating_sub(lines.len());
        let max_chars = 65_536.max(usize::from(area.height) * usize::from(area.width) * 4);
        let text = output
            .map(|output| output.visible_tail(max_chars))
            .unwrap_or_default();
        let output_lines = if text.trim().is_empty() {
            body("(no output yet)", area.width, theme.dim)
        } else {
            body(&text, area.width, Style::default())
        };
        let max_scroll = output_lines.len().saturating_sub(room);
        self.max_scroll.set(max_scroll);
        let skip = if self.follow {
            max_scroll
        } else {
            self.scroll.min(max_scroll)
        };
        lines.extend(output_lines.into_iter().skip(skip).take(room));
        draw_lines(lines, area, buf);
    }
}

fn confirmed(
    confirm: Option<&Confirmation>,
    key: char,
    id: Option<&ProcessId>,
    now_ms: u64,
) -> bool {
    confirm.is_some_and(|confirm| {
        confirm.key == key && confirm.process_id.as_ref() == id && now_ms <= confirm.until_ms
    })
}

fn clean(text: &str) -> String {
    sanitize(&clean_output(text)).into_owned()
}

fn title(text: &str, fallback: &str, max_chars: usize) -> String {
    let cleaned = clean_output(text);
    let first = sanitize(cleaned.lines().next().unwrap_or_default());
    let first = first.as_ref();
    let first = if first.is_empty() { fallback } else { first };
    if first.chars().count() > max_chars {
        format!(
            "{}…",
            first
                .chars()
                .take(max_chars.saturating_sub(1))
                .collect::<String>()
        )
    } else {
        first.to_string()
    }
}

fn columns(text: &str) -> u16 {
    text.graphemes(true).fold(0u16, |width, grapheme| {
        width.saturating_add(grapheme_width(grapheme))
    })
}

fn ellipsize(text: &str, width: u16) -> String {
    if columns(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0u16;
    for grapheme in text.graphemes(true) {
        let next = used.saturating_add(grapheme_width(grapheme));
        if next > width.saturating_sub(1) {
            break;
        }
        result.push_str(grapheme);
        used = next;
    }
    result.push('…');
    result
}

fn with_suffix(text: &str, suffix: &str, width: u16) -> String {
    if columns(text).saturating_add(columns(suffix)) <= width {
        return format!("{text}{suffix}");
    }
    let reserved = columns(suffix);
    if width > reserved.saturating_add(5) {
        format!("{}{suffix}", ellipsize(text, width - reserved))
    } else {
        ellipsize(text, width)
    }
}

fn body_plain(text: &str, width: u16, style: Style) -> Vec<Line<'static>> {
    wrap_lines(
        clean(text)
            .split('\n')
            .map(|line| Line::styled(line.to_string(), style))
            .collect(),
        width,
    )
}

fn indent(line: Line<'static>) -> Line<'static> {
    let style = line.style;
    let mut spans = vec![Span::raw("  ")];
    spans.extend(line.spans);
    sanitize_line(Line::from(spans).style(style))
}

fn body(text: &str, width: u16, style: Style) -> Vec<Line<'static>> {
    body_plain(text, width.saturating_sub(2), style)
        .into_iter()
        .map(indent)
        .collect()
}

fn draw_lines(lines: Vec<Line<'static>>, area: Rect, buf: &mut Buffer) {
    for (offset, line) in (0..area.height).zip(lines) {
        buf.set_line(area.x, area.y + offset, &line, area.width);
    }
}

fn separator(x: u16, y: u16, width: u16, buf: &mut Buffer, style: Style) {
    buf.set_line(
        x,
        y,
        &Line::styled("─".repeat(usize::from(width)), style),
        width,
    );
}

fn is_running(row: &ProcessRow) -> bool {
    row.status == ProcessStatus::Running
}

fn glyph(status: &ProcessStatus) -> char {
    match status {
        ProcessStatus::Running => '●',
        ProcessStatus::Done => '✓',
        ProcessStatus::Killed => '■',
        ProcessStatus::Ended => '?',
        _ => '✗',
    }
}

fn label(status: &ProcessStatus) -> String {
    match status {
        ProcessStatus::Running => "running".into(),
        ProcessStatus::Done => "exit 0".into(),
        ProcessStatus::Failed { code } => format!("exit {code}"),
        ProcessStatus::FailedToStart => "failed to start".into(),
        ProcessStatus::Killed => "killed".into(),
        ProcessStatus::Lost => "lost".into(),
        ProcessStatus::Ended => "ended".into(),
    }
}

fn status_style(status: &ProcessStatus, theme: &BackgroundTheme) -> Style {
    match status {
        ProcessStatus::Running => Style::default(),
        ProcessStatus::Done => theme.ok,
        ProcessStatus::Failed { .. } | ProcessStatus::FailedToStart | ProcessStatus::Lost => {
            theme.error
        }
        ProcessStatus::Killed | ProcessStatus::Ended => theme.dim,
    }
}

fn elapsed(row: &ProcessRow, now_ms: u64, wall_s: f64) -> String {
    format_elapsed(row.started_epoch_s.map_or_else(
        || now_ms.saturating_sub(row.first_seen_ms) / 1_000,
        |started| seconds_since(wall_s, started),
    ))
}

fn ended_ago(row: &ProcessRow, now_ms: u64, wall_s: f64) -> String {
    format_elapsed(row.exited_at_s.map_or_else(
        || now_ms.saturating_sub(row.finished_seen_ms.unwrap_or(now_ms)) / 1_000,
        |ended| seconds_since(wall_s, ended),
    ))
}

fn seconds_since(now: f64, then: f64) -> u64 {
    (now - then).max(0.0).floor() as u64
}
