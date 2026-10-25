//! Context, usage and compaction blocks (#234).

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use talaria_core::context::breakdown::Breakdown;
use talaria_core::context::compact::CompactResult;
use talaria_core::context::text::ReadError;
use talaria_core::context::tracker::{CompactingLine, ContextNotice};
use talaria_core::context::usage::{ContextFigure, UsageReport};

use crate::live::LiveSection;

/// Styles for the context blocks. `categories` follows Hermes's display order.
#[derive(Clone, Debug)]
pub struct ContextStyle {
    pub header: Style,
    pub dim: Style,
    pub error: Style,
    pub tick: Style,
    pub categories: [Style; 9],
    pub free: Style,
}

impl ContextStyle {
    /// The full-colour style.
    pub fn standard() -> Self {
        unimplemented!("#234")
    }

    /// No colours; bold and dim kept.
    pub fn plain() -> Self {
        unimplemented!("#234")
    }
}

/// The `/context` block for a breakdown.
pub fn context_block(
    _typed: &str,
    _b: &Breakdown,
    _width: u16,
    _s: &ContextStyle,
) -> Vec<Line<'static>> {
    unimplemented!("#234")
}

/// The `/context` block before the agent exists, with an optional last reading.
pub fn prebuild_block(
    _typed: &str,
    _last: Option<&ContextFigure>,
    _width: u16,
    _s: &ContextStyle,
) -> Vec<Line<'static>> {
    unimplemented!("#234")
}

/// The `/usage` block for a report.
pub fn usage_block(
    _typed: &str,
    _r: &UsageReport,
    _width: u16,
    _s: &ContextStyle,
) -> Vec<Line<'static>> {
    unimplemented!("#234")
}

/// The `/compact` block for a result.
pub fn compact_block(
    _typed: &str,
    _r: &CompactResult,
    _width: u16,
    _s: &ContextStyle,
) -> Vec<Line<'static>> {
    unimplemented!("#234")
}

/// A one-row notice block, with no typed line.
pub fn notice_block(_n: &ContextNotice, _width: u16, _s: &ContextStyle) -> Vec<Line<'static>> {
    unimplemented!("#234")
}

/// The error block for a failed `/context` or `/usage`.
pub fn error_block(
    _typed: &str,
    _command: &str,
    _method: &str,
    _sid: &str,
    _e: &ReadError,
    _width: u16,
    _s: &ContextStyle,
) -> Vec<Line<'static>> {
    unimplemented!("#234")
}

/// The live compacting line section.
pub struct CompactingSection {
    pub line: Option<CompactingLine>,
    pub now_ms: u64,
}

impl LiveSection for CompactingSection {
    fn height(&self, _width: u16) -> u16 {
        unimplemented!("#234")
    }

    fn render(&self, _area: Rect, _buf: &mut ratatui::buffer::Buffer) {
        unimplemented!("#234")
    }
}
