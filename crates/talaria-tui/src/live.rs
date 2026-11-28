//! Pure live region description: sections, layout and the height cap.

use crate::status::{StatusPalette, Tone};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::history::word_wrapped;
use crate::theme::surfaces::{ViewStyles, tokens as t};

/// One block of the live region.
pub trait LiveSection {
    fn height(&self, width: u16) -> u16;
    fn render(&self, area: Rect, buf: &mut Buffer);
}

/// A small set of entry points for an empty, connected conversation.
pub struct ConversationStart {
    accent: Style,
    text: Style,
    muted: Style,
    compact: bool,
    chrome: ViewStyles,
}

impl ConversationStart {
    pub fn new(palette: &StatusPalette) -> Self {
        Self {
            accent: palette.style(Tone::Accent).add_modifier(Modifier::BOLD),
            text: palette.style(Tone::Text).add_modifier(Modifier::BOLD),
            muted: palette.style(Tone::Muted),
            compact: false,
            chrome: ViewStyles::default(),
        }
    }

    pub fn from_views(views: &ViewStyles) -> Self {
        let mut start = Self::new(&StatusPalette::default());
        start.chrome = views.clone();
        start.accent = views
            .inline(t::STATUS_ACCENT, start.accent)
            .add_modifier(Modifier::BOLD);
        start.text = views
            .inline(t::WORKSPACE_EMPTY, start.text)
            .add_modifier(Modifier::BOLD);
        start.muted = views.inline(t::WORKSPACE_EMPTY, start.muted);
        start
    }

    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    fn compact_lines(&self, width: u16) -> [Line<'static>; 2] {
        let brand = Line::from(vec![
            Span::styled(" ╱╱ ", self.accent),
            Span::styled("Talaria", self.text),
        ]);
        let mut actions = Line::from("    ");
        for route in ["/resume", "/workflows", "/model"] {
            if actions.width() + route.len() > usize::from(width) {
                break;
            }
            actions.spans.push(Span::styled(route, self.accent));
            actions.spans.push(Span::raw("   "));
        }
        let _ = actions.spans.pop();
        [brand, actions]
    }

    fn wide_lines(&self) -> impl Iterator<Item = Line<'static>> + '_ {
        // Give the wing and the command column independent weight and a shared baseline.
        let wing = [
            "",
            "       ╭━━━━━━╸   ",
            "    ╭━━╯          ",
            " ╭━━┻━━━━━╸       ",
            " ╰━━┳━━━╸         ",
            "    ╰━╸           ",
        ];
        let routes = [
            ("/resume", "Resume a session"),
            ("/workflows", "Browse workflows"),
            ("/model", "Choose a model"),
        ];
        wing.into_iter().enumerate().map(move |(row, feather)| {
            let feather = self.chrome.frame_text(feather);
            let feather = match self.chrome.border_preset() {
                crate::theme::BorderPreset::Ascii => {
                    feather.replace(['━', '╸'], "-").replace(['┻', '┳'], "+")
                }
                crate::theme::BorderPreset::None => " ".repeat(feather.width()),
                _ => feather,
            };
            let mut line = Line::from(Span::styled(feather, self.accent));
            if row == 1 {
                line.spans.push(Span::styled("Talaria", self.text));
            } else if let Some((command, label)) =
                row.checked_sub(3).and_then(|row| routes.get(row))
            {
                line.spans
                    .push(Span::styled(format!("{command:<13}"), self.accent));
                line.spans.push(Span::styled(*label, self.muted));
            }
            line
        })
    }
}

impl LiveSection for ConversationStart {
    fn height(&self, width: u16) -> u16 {
        match width {
            0..=19 => 0,
            52.. if !self.compact => 7,
            _ => 3,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if self.height(area.width) == 0 || area.height == 0 {
            return;
        }
        if self.height(area.width) == 7 && area.height >= 7 {
            for (y, line) in (area.y..area.bottom()).zip(self.wide_lines()) {
                buf.set_line(area.x, y, &line, area.width);
            }
        } else {
            for (y, line) in (area.y..area.bottom()).zip(self.compact_lines(area.width)) {
                buf.set_line(area.x, y, &line, area.width);
            }
        }
    }
}

/// The sections of the live region, first (the streaming tail) to last.
pub struct LiveView<'a> {
    pub sections: Vec<&'a dyn LiveSection>,
}

impl LiveView<'_> {
    /// One rectangle per section, relative to the viewport. Over `max_height`, the later
    /// sections keep their rows and the first gives way; when the later sections alone are too
    /// tall, the first gets none and rows go to the sections from the last upward.
    pub fn layout(&self, width: u16, max_height: u16) -> Vec<Rect> {
        let wanted: Vec<u16> = self
            .sections
            .iter()
            .map(|section| section.height(width))
            .collect();
        let total: u32 = wanted.iter().map(|&height| u32::from(height)).sum();
        let rest: u32 = wanted.iter().skip(1).map(|&height| u32::from(height)).sum();
        let max = u32::from(max_height);

        let mut given = wanted.clone();
        if total > max {
            if let Some(first) = given.first_mut() {
                // At most `max_height`, so it fits.
                *first = u16::try_from(max.saturating_sub(rest)).unwrap_or(0);
            }
            if rest > max {
                let mut remaining = max_height;
                for (index, height) in given.iter_mut().enumerate().skip(1).rev() {
                    *height = wanted[index].min(remaining);
                    remaining -= *height;
                }
            }
        }

        let mut y: u16 = 0;
        given
            .into_iter()
            .map(|height| {
                let rect = Rect::new(0, y, width, height);
                y = y.saturating_add(height);
                rect
            })
            .collect()
    }

    /// How many rows [`Self::layout`] gives the section at `index`. Past the first section it
    /// measures only that section and the ones after it, which take their rows first.
    pub fn rows_of(&self, index: usize, width: u16, max_height: u16) -> u16 {
        if index == 0 {
            return self
                .layout(width, max_height)
                .first()
                .map_or(0, |rect| rect.height);
        }
        let Some(section) = self.sections.get(index) else {
            return 0;
        };
        let after: u32 = self.sections[index + 1..]
            .iter()
            .map(|later| u32::from(later.height(width)))
            .sum();
        let room = u32::from(max_height).saturating_sub(after);
        section
            .height(width)
            .min(u16::try_from(room).unwrap_or(u16::MAX))
    }
}

/// The mention list above the composer. A short screen keeps the composer first.
#[must_use]
pub fn mention_then_composer<'a>(
    mention: &'a dyn LiveSection,
    composer: &'a dyn LiveSection,
) -> LiveView<'a> {
    LiveView {
        sections: vec![mention, composer],
    }
}

/// `rows - 2`, and 0 below three rows.
pub fn max_live_height(rows: u16) -> u16 {
    rows.saturating_sub(2)
}

/// How many of `len` rows a live tail cut to its last `max` rows does not draw: the first row
/// kept gives way to the cut marker, so its row counts too. `None` when they all fit, or when no
/// row is left to hold the marker.
pub(crate) fn tail_hidden(len: usize, max: usize) -> Option<usize> {
    (max > 0 && len > max).then(|| len - max + 1)
}

/// The text of a cut tail's first row: how many earlier rows are not in scrollback yet and the
/// key that shows them now, or the transcript view when no key opens it. `more` says the count
/// is a floor, because a long reply was already shortened before the cut.
pub fn tail_cut_text(hidden: usize, more: bool, transcript_key: Option<&str>) -> String {
    let (noun, verb, them) = if hidden == 1 && !more {
        ("row", "is", "it")
    } else {
        ("rows", "are", "them")
    };
    let count = if more {
        format!("more than {hidden}")
    } else {
        hidden.to_string()
    };
    let shows = match transcript_key {
        Some(key) => format!("{key} shows {them} now"),
        None => format!("the transcript view shows {them}"),
    };
    format!("\u{2191} {count} earlier {noun} {verb} not in scrollback yet \u{b7} {shows}")
}

/// Plain text split on newlines and wrapped to the width.
pub struct PlainTextSection {
    text: String,
    cached: std::sync::Mutex<Option<(u16, Vec<Line<'static>>)>>,
}

impl PlainTextSection {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cached: std::sync::Mutex::new(None),
        }
    }

    fn with_rows<R>(&self, width: u16, f: impl FnOnce(&[Line<'static>]) -> R) -> R {
        let mut guard = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if guard.as_ref().is_none_or(|(w, _)| *w != width) {
            let rows = word_wrapped(&self.text, width)
                .into_iter()
                .map(Line::from)
                .collect();
            *guard = Some((width, rows));
        }
        // An empty cache is filled above before its rows are borrowed.
        let (_, rows) = guard.as_ref().expect("wrapped rows were initialized");
        f(rows)
    }
}

impl LiveSection for PlainTextSection {
    fn height(&self, width: u16) -> u16 {
        self.with_rows(width, |rows| u16::try_from(rows.len()).unwrap_or(u16::MAX))
    }

    /// Draws the last rows that fit, one buffer row each, so nothing is clipped.
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.with_rows(area.width, |rows| {
            let skip = rows.len().saturating_sub(usize::from(area.height));
            for (offset, row) in (0..area.height).zip(rows.iter().skip(skip)) {
                buf.set_line(area.x, area.y + offset, row, area.width);
            }
        });
    }
}

/// What one draw did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub drew: bool,
    pub bytes: usize,
}
