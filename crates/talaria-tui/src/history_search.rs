//! History search box: view state over submitted prompts.
//!
//! Pure view state; drawing is a live section. Behavior contract: issue 228.

use std::sync::Arc;
use std::time::SystemTime;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::prompt_history::HistoryEntry;

use crate::keys::{Key, KeyChord};
use crate::keys::{KeyCode, KeyKind};
use crate::live::LiveSection;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use talaria_core::prompt_history::Scope;
use unicode_segmentation::UnicodeSegmentation;

/// What a search key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchOutcome {
    /// Nothing changed.
    Unchanged,
    /// The query, scope, or selection changed.
    Changed,
    /// Load this text into the composer, unsent.
    Accept(String),
    /// Close the box and restore the saved draft.
    Cancel,
}

/// The open search box.
#[derive(Debug)]
pub struct HistorySearch {
    entries: Arc<[HistoryEntry]>,
    lowercase: Vec<String>,
    order: Vec<usize>,
    draft: String,
    query: String,
    scope: Scope,
    selected: usize,
    chords: Vec<KeyChord>,
}

impl HistorySearch {
    /// Open the box over these entries, saving the draft.
    pub fn open(entries: Arc<[HistoryEntry]>, draft: String, search_chords: Vec<KeyChord>) -> Self {
        let lowercase = entries.iter().map(|e| e.text.to_lowercase()).collect();
        let mut order: Vec<_> = (0..entries.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse((entries[i].ts, i)));
        Self {
            entries,
            lowercase,
            order,
            draft,
            query: String::new(),
            scope: Scope::ThisFolder,
            selected: 0,
            chords: search_chords,
        }
    }

    /// Apply one key.
    pub fn handle_key(&mut self, key: Key, now: SystemTime) -> SearchOutcome {
        let _ = now;
        if key.kind == KeyKind::Release {
            return SearchOutcome::Unchanged;
        }
        if key.code == KeyCode::Esc || (key.mods.ctrl && key.code == KeyCode::Char('c')) {
            return SearchOutcome::Cancel;
        }
        let matches = self.matches();
        let older = self.chords.iter().any(|chord| chord.matches(&key));
        match key.code {
            KeyCode::Enter => {
                return matches
                    .get(self.selected)
                    .map_or(SearchOutcome::Unchanged, |&i| {
                        SearchOutcome::Accept(self.entries[i].text.clone())
                    });
            }
            KeyCode::Up if !matches.is_empty() => {
                self.selected = (self.selected + 1).min(matches.len() - 1)
            }
            _ if older && !matches.is_empty() => {
                self.selected = (self.selected + 1).min(matches.len() - 1)
            }
            KeyCode::Down => self.selected = self.selected.saturating_sub(1),
            KeyCode::Tab => {
                self.scope = if self.scope == Scope::ThisFolder {
                    Scope::AllFolders
                } else {
                    Scope::ThisFolder
                };
                self.selected = 0;
            }
            KeyCode::Backspace => {
                let Some((i, _)) = self.query.grapheme_indices(true).next_back() else {
                    return SearchOutcome::Unchanged;
                };
                self.query.truncate(i);
                self.selected = 0;
            }
            KeyCode::Char(c) if !key.mods.ctrl && !key.mods.alt && !c.is_control() => {
                self.query.push(c);
                self.selected = 0;
            }
            _ => return SearchOutcome::Unchanged,
        }
        SearchOutcome::Changed
    }

    /// Append the first pasted line to the query.
    pub fn handle_paste(&mut self, text: &str) {
        self.query.extend(
            text.lines()
                .next()
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_control()),
        );
        self.selected = 0;
    }

    /// The draft saved when the box opened.
    pub fn saved_draft(&self) -> &str {
        &self.draft
    }

    /// The box as a live section replacing the composer.
    pub fn section(&self, now: SystemTime) -> impl LiveSection + '_ {
        HistorySection { search: self, now }
    }

    fn matches(&self) -> Vec<usize> {
        let query = self.query.to_lowercase();
        let mut seen = std::collections::HashSet::new();
        self.order
            .iter()
            .copied()
            .filter(|&i| {
                let e = &self.entries[i];
                (self.scope == Scope::AllFolders || e.this_folder)
                    && self.lowercase[i].contains(&query)
                    && seen.insert(&e.text)
            })
            .collect()
    }
}

/// The search box as a live section. Stub.
#[derive(Debug)]
pub struct HistorySection<'a> {
    search: &'a HistorySearch,
    now: SystemTime,
}

impl LiveSection for HistorySection<'_> {
    fn height(&self, width: u16) -> u16 {
        if width < 20 {
            2
        } else {
            3 + self.search.matches().len().clamp(1, 5) as u16
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let s = self.search;
        let narrow = area.width < 20;
        let scope = if s.scope == Scope::ThisFolder {
            "this folder · tab: all folders"
        } else {
            "all folders · tab: this folder"
        };
        let mut rows = Vec::new();
        if !narrow {
            rows.push(Line::raw(format!("╭ search history · {scope}")));
        }
        rows.push(Line::raw(format!(
            "{}› {}▏",
            if narrow { "" } else { "│ " },
            crate::history::sanitize(&s.query)
        )));
        let matches = s.matches();
        if matches.is_empty() {
            let message = if s.entries.is_empty() {
                "No prompt history yet.".into()
            } else {
                format!(
                    "No earlier prompt matches \"{}\".",
                    s.query.chars().take(30).collect::<String>()
                )
            };
            rows.push(Line::styled(
                crate::history::sanitize(&message).into_owned(),
                Style::default().add_modifier(Modifier::DIM),
            ));
        } else {
            let start = if narrow {
                s.selected
            } else {
                s.selected.saturating_sub(4)
            };
            for (position, &i) in
                matches
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(if narrow { 1 } else { 5 })
            {
                let e = &s.entries[i];
                let first = crate::history::sanitize(e.text.lines().next().unwrap_or(""));
                let query = s.query.to_lowercase();
                let found = if query.is_empty() {
                    None
                } else {
                    first.to_lowercase().find(&query)
                };
                let prefix = if narrow {
                    ""
                } else if position == s.selected {
                    "│ ▸ "
                } else {
                    "│   "
                };
                let mut spans = vec![Span::raw(prefix)];
                let mut at = 0;
                for c in first.chars() {
                    let end = at + c.to_lowercase().collect::<String>().len();
                    let bold = found.is_some_and(|begin| at < begin + query.len() && end > begin);
                    spans.push(Span::styled(
                        c.to_string(),
                        if bold {
                            Style::default().add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    ));
                    at = end;
                }
                if !narrow {
                    let extra = e.text.lines().count().saturating_sub(1);
                    if extra > 0 {
                        spans.push(Span::raw(format!(" (+{extra} lines)")));
                    }
                    spans.push(Span::raw(format!("  {}", age(self.now, e.ts))));
                }
                let mut line = Line::from(spans);
                if position == s.selected {
                    line = line.style(Style::default().add_modifier(Modifier::REVERSED));
                }
                rows.push(line);
            }
        }
        if !narrow {
            rows.push(Line::raw("╰ enter: put in composer · esc: cancel"));
        }
        for (i, line) in rows.iter().take(area.height as usize).enumerate() {
            let line = if narrow {
                clip(line, usize::from(area.width))
            } else {
                let top = i == 0;
                let bottom = i == rows.len() - 1;
                let mut line = line.clone();
                if !top && !bottom && !line.to_string().starts_with('│') {
                    line.spans.insert(0, Span::raw("│ "));
                }
                let mut line = clip(&line, usize::from(area.width - 1));
                let padding = usize::from(area.width - 1).saturating_sub(line.width());
                line.spans.push(Span::raw(if top || bottom {
                    "─".repeat(padding)
                } else {
                    " ".repeat(padding)
                }));
                line.spans.push(Span::raw(if top {
                    "╮"
                } else if bottom {
                    "╯"
                } else {
                    "│"
                }));
                line
            };
            buf.set_line(area.x, area.y + i as u16, &line, area.width);
        }
    }
}

fn clip(line: &Line<'_>, width: usize) -> Line<'static> {
    if line.width() <= width {
        return Line::from(
            line.spans
                .iter()
                .map(|span| Span::styled(span.content.to_string(), span.style))
                .collect::<Vec<_>>(),
        )
        .style(line.style);
    }
    let mut remaining = width.saturating_sub(1);
    let mut spans = Vec::new();
    'spans: for span in &line.spans {
        let mut text = String::new();
        for character in span.content.chars() {
            let size = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
            if size > remaining {
                spans.push(Span::styled(text, span.style));
                break 'spans;
            }
            text.push(character);
            remaining -= size;
        }
        spans.push(Span::styled(text, span.style));
    }
    if width > 0 {
        spans.push(Span::raw("…"));
    }
    Line::from(spans).style(line.style)
}

fn age(now: SystemTime, ts: i64) -> String {
    let seconds = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(ts.max(0) as u64);
    let (n, unit) = match seconds {
        0..60 => return "now".into(),
        60..3600 => (seconds / 60, "m"),
        3600..86400 => (seconds / 3600, "h"),
        86400..604800 => (seconds / 86400, "d"),
        _ => (seconds / 604800, "w"),
    };
    format!("{n}{unit} ago")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipsis_never_splits_a_wide_character() {
        let line = clip(&Line::raw("界界x"), 4);
        assert_eq!(line.to_string(), "界…");
        assert!(line.width() <= 4);
    }
}
