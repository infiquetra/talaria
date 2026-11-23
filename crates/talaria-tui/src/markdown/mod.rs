//! Styled lines for streaming Markdown. Nothing here writes a control sequence.

mod blocks;
mod render;
mod sanitize;
mod table;
mod theme;
mod wrap;

pub mod cells;
#[doc(hidden)]
pub mod highlight;
pub mod section;

pub use highlight::warm_up;
pub use sanitize::sanitize_untrusted;
pub use theme::MarkdownTheme;

use crate::theme::surfaces::ViewStyles;
use ratatui::style::Style;
use ratatui::text::Line;

#[derive(Clone, Debug)]
pub struct MarkdownOptions {
    pub width: u16,
    pub theme: MarkdownTheme,
    pub color: bool,
    pub base_style: Style,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            width: 80,
            theme: MarkdownTheme::default(),
            color: true,
            base_style: Style::default(),
        }
    }
}

pub struct MarkdownStream {
    width: u16,
    color: bool,
    pending: String,
    base_style: Style,
    theme: MarkdownTheme,
    held_cr: bool,
    emitted_any: bool,
    code: highlight::CodeCache,
    views: ViewStyles,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamUpdate {
    pub committed: Vec<Line<'static>>,
    pub tail: Vec<Line<'static>>,
}

impl StreamUpdate {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.committed.is_empty() && self.tail.is_empty()
    }
}

impl MarkdownStream {
    #[must_use]
    pub fn new(options: MarkdownOptions) -> Self {
        Self::new_with_theme(options, ViewStyles::default())
    }

    #[must_use]
    pub fn new_with_theme(options: MarkdownOptions, views: ViewStyles) -> Self {
        Self {
            width: options.width.max(8),
            color: options.color,
            pending: String::new(),
            base_style: options.base_style,
            theme: options.theme,
            held_cr: false,
            emitted_any: false,
            code: highlight::CodeCache::default(),
            views,
        }
    }

    pub fn push(&mut self, delta: &str) -> StreamUpdate {
        if delta.is_empty() {
            return StreamUpdate::default();
        }
        if self.held_cr {
            self.held_cr = false;
            if !delta.starts_with('\n') {
                self.pending.push('␍');
            }
        }
        let delta = if let Some(delta) = delta.strip_suffix('\r') {
            self.held_cr = true;
            delta
        } else {
            delta
        };
        self.pending.push_str(&sanitize_untrusted(delta));
        let complete_end = self.pending.rfind('\n').map_or(0, |pos| pos + 1);
        let blocks = blocks::scan(&self.pending[..complete_end]);
        let closed = blocks
            .iter()
            .enumerate()
            .take_while(|(index, block)| index + 1 < blocks.len() || block.closed_fence)
            .count();
        let options = self.options();
        let mut committed = Vec::new();
        for block in blocks.iter().take(closed) {
            let lines = render::render(
                &self.pending[block.range.clone()],
                &options,
                &mut self.code,
                &self.views,
            );
            if !lines.is_empty() {
                if self.emitted_any {
                    committed.push(Line::default());
                }
                committed.extend(lines);
                self.emitted_any = true;
            }
        }
        if closed > 0 {
            let boundary = if let Some(open) = blocks.get(closed) {
                self.pending[..open.range.start]
                    .rfind('\n')
                    .map_or(0, |pos| pos + 1)
            } else {
                let end = blocks[closed - 1].range.end;
                self.pending[end..]
                    .find('\n')
                    .map_or(end, |pos| end + pos + 1)
            };
            self.pending.drain(..boundary);
        }
        let tail = self.tail();
        if self.pending.is_empty() {
            self.code.clear_blocks();
        }
        StreamUpdate { committed, tail }
    }

    pub fn set_width(&mut self, width: u16) -> StreamUpdate {
        self.width = width.max(8);
        StreamUpdate {
            committed: Vec::new(),
            tail: self.tail(),
        }
    }

    pub fn finish(&mut self) -> StreamUpdate {
        if self.held_cr {
            self.pending.push('␍');
            self.held_cr = false;
        }
        let committed = self.tail();
        self.pending.clear();
        self.emitted_any = false;
        self.code.clear_blocks();
        StreamUpdate {
            committed,
            tail: Vec::new(),
        }
    }

    #[must_use]
    pub fn pending_bytes(&self) -> usize {
        self.pending.len()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn lines_highlighted(&self) -> u64 {
        self.code.lines_highlighted()
    }

    fn options(&self) -> MarkdownOptions {
        MarkdownOptions {
            width: self.width,
            color: self.color,
            base_style: self.base_style,
            theme: self.theme.clone(),
        }
    }

    fn tail(&mut self) -> Vec<Line<'static>> {
        let options = self.options();
        let mut lines = render::render(&self.pending, &options, &mut self.code, &self.views);
        self.code.retain_rendered();
        if self.emitted_any && !lines.is_empty() {
            lines.insert(0, Line::default());
        }
        lines
    }
}

/// How many leading bytes of a streaming Markdown text are whole top-level blocks that later
/// text cannot change. A block counts once the next block has started on a later line, or when
/// it is a code fence whose closing line has arrived; the open last block never counts, so a
/// table, a list or a fence is always counted whole. The length ends at the start of a line, so
/// what follows starts with the open block, and rendering the two parts apart gives the rows of
/// one rendering with a blank row between them. Zero when no block is closed yet.
#[must_use]
pub fn closed_blocks_len(text: &str) -> usize {
    // Scan what will be rendered, and only whole lines: a partial line is no evidence yet.
    let clean = sanitize_untrusted(text);
    let complete_end = clean.rfind('\n').map_or(0, |pos| pos + 1);
    let blocks = blocks::scan(&clean[..complete_end]);
    let closed = blocks
        .iter()
        .enumerate()
        .take_while(|(index, block)| index + 1 < blocks.len() || block.closed_fence)
        .count();
    if closed == 0 {
        return 0;
    }
    let boundary = match blocks.get(closed) {
        Some(open) => clean[..open.range.start]
            .rfind('\n')
            .map_or(0, |pos| pos + 1),
        None => {
            let end = blocks[closed - 1].range.end;
            clean[end..].find('\n').map_or(end, |pos| end + pos + 1)
        }
    };
    // Sanitizing keeps every line break, so the same count of them ends the raw prefix.
    match clean[..boundary].matches('\n').count() {
        0 => 0,
        lines => text
            .match_indices('\n')
            .nth(lines - 1)
            .map_or(0, |(at, _)| at + 1),
    }
}

#[must_use]
pub fn render_markdown(text: &str, options: &MarkdownOptions) -> Vec<Line<'static>> {
    render_markdown_with_theme(text, options, &ViewStyles::default())
}

/// Render with an explicit syntax palette, without changing the legacy options.
#[must_use]
pub fn render_markdown_with_theme(
    text: &str,
    options: &MarkdownOptions,
    views: &ViewStyles,
) -> Vec<Line<'static>> {
    render::render(
        &sanitize_untrusted(text),
        options,
        &mut highlight::CodeCache::default(),
        views,
    )
}
