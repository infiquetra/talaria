//! Optional full-screen workspace presentation.
pub mod panel;
use crate::{
    history::CellLines,
    live::LiveView,
    pager::{Details, TranscriptEntry},
};
use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::{checklist::Checklist, layout::PanelVisibility};

pub struct WorkspaceView<'a> {
    pub entries: &'a [TranscriptEntry<'a>],
    pub details: Details,
    pub panel: PanelVisibility,
    pub checklist: Option<&'a Checklist>,
    pub checklist_expanded: bool,
    pub footer: &'a LiveView<'a>,
}

pub fn draw(_view: &WorkspaceView<'_>, _area: Rect, _buffer: &mut Buffer, _other: &dyn CellLines) {}

pub fn draw_body(
    entries: &[TranscriptEntry<'_>],
    details: Details,
    area: Rect,
    buffer: &mut Buffer,
    other: &dyn CellLines,
) {
    crate::pager::draw_transcript_body(entries, details, area, buffer, other);
}
