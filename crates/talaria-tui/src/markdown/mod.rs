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
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamUpdate {
    pub committed: Vec<Line<'static>>,
    pub tail: Vec<Line<'static>>,
}

impl StreamUpdate {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let _ = (&self.committed, &self.tail);
        todo!()
    }
}

impl MarkdownStream {
    #[must_use]
    pub fn new(options: MarkdownOptions) -> Self {
        Self {
            width: options.width,
            color: options.color,
            pending: String::new(),
            base_style: options.base_style,
            theme: options.theme,
        }
    }

    pub fn push(&mut self, delta: &str) -> StreamUpdate {
        self.observe(delta.len());
        todo!()
    }

    pub fn set_width(&mut self, width: u16) -> StreamUpdate {
        let _ = width;
        self.observe(0);
        todo!()
    }

    pub fn finish(&mut self) -> StreamUpdate {
        self.observe(0);
        todo!()
    }

    #[must_use]
    pub fn pending_bytes(&self) -> usize {
        self.observe(0);
        todo!()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn lines_highlighted(&self) -> u64 {
        self.observe(0);
        todo!()
    }

    fn observe(&self, extra: usize) {
        let _ = (
            self.width,
            self.color,
            self.pending.len(),
            self.base_style,
            self.theme.heading1,
            extra,
            blocks::touch(),
            render::touch(),
            table::touch(),
            wrap::touch(),
        );
    }
}

#[must_use]
pub fn render_markdown(text: &str, options: &MarkdownOptions) -> Vec<Line<'static>> {
    let _ = (
        text.len(),
        options.width,
        options.color,
        options.base_style,
        options.theme.heading1,
        blocks::touch(),
        render::touch(),
        table::touch(),
        wrap::touch(),
    );
    todo!()
}
