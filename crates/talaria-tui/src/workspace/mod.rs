//! Optional full-screen workspace presentation.
pub mod panel;

use crate::{
    checklist::{ChecklistSection, ChecklistStyle, PANEL_MAX_ROWS, SHORT_TERMINAL_ROWS},
    history::CellLines,
    live::{LiveSection, LiveView, max_live_height},
    pager::{Details, TranscriptBody, TranscriptEntry},
};
use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::{
    checklist::Checklist,
    layout::{DOCK_COLUMNS, DOCK_ROWS, MIN_ROWS, PANEL_COLUMNS, PanelVisibility},
};

/// Borrowed workspace contents. The footer keeps its existing section order,
/// excluding the streaming tail. A checklist supplied here is hosted in the panel,
/// or placed above the footer when the panel is dismissed.
pub struct WorkspaceView<'a> {
    pub entries: &'a [TranscriptEntry<'a>],
    pub details: Details,
    pub panel: PanelVisibility,
    pub checklist: Option<&'a Checklist>,
    pub checklist_expanded: bool,
    pub footer: &'a LiveView<'a>,
    /// Session rows for the panel. Absent on the frame-only path.
    pub work: Option<&'a panel::WorkContents<'a>>,
}

/// Full-width footer and independent body allocations. The menu allocation is
/// available for a future menu without making the panel reserve an empty row.
pub struct WorkspaceAreas {
    pub menu: Rect,
    pub transcript: Rect,
    pub panel: Option<Rect>,
    pub footer: Rect,
}

/// Allocate rectangles, keeping overlays over a full-width transcript.
pub fn geometry(
    area: Rect,
    footer_rows: u16,
    menu_rows: u16,
    panel: PanelVisibility,
) -> WorkspaceAreas {
    let menu_rows = if area.height >= MIN_ROWS {
        menu_rows.min(area.height)
    } else {
        0
    };
    let footer_rows = footer_rows.min(area.height.saturating_sub(menu_rows));
    let body = Rect::new(
        area.x,
        area.y + menu_rows,
        area.width,
        area.height - footer_rows - menu_rows,
    );
    let visible = area.height >= MIN_ROWS
        && !body.is_empty()
        && match panel {
            PanelVisibility::Docked => area.width >= DOCK_COLUMNS && area.height >= DOCK_ROWS,
            PanelVisibility::OverlayOpen => true,
            _ => false,
        };
    let docked = visible && panel == PanelVisibility::Docked;
    let panel = visible.then(|| {
        let width = PANEL_COLUMNS.min(body.width);
        Rect::new(body.right() - width, body.y, width, body.height)
    });
    let transcript_width = if docked {
        body.width.saturating_sub(PANEL_COLUMNS)
    } else {
        body.width
    };
    WorkspaceAreas {
        menu: Rect::new(area.x, area.y, area.width, menu_rows),
        transcript: Rect::new(body.x, body.y, transcript_width, body.height),
        panel,
        footer: Rect::new(area.x, body.bottom(), area.width, footer_rows),
    }
}

/// A retained render cache; no application or gateway state lives here.
#[derive(Default)]
pub struct Workspace {
    transcript: TranscriptBody,
}

impl Workspace {
    pub fn clear(&mut self) {
        self.transcript.clear();
    }

    pub fn draw(
        &mut self,
        view: &WorkspaceView<'_>,
        area: Rect,
        buffer: &mut Buffer,
        other: &dyn CellLines,
    ) {
        self.draw_with_style(view, area, buffer, other, &ChecklistStyle::default(), None);
    }

    pub fn draw_with_style(
        &mut self,
        view: &WorkspaceView<'_>,
        area: Rect,
        buffer: &mut Buffer,
        other: &dyn CellLines,
        style: &ChecklistStyle,
        hint: Option<String>,
    ) {
        let visible = geometry(area, 0, 0, view.panel).panel.is_some();
        let max_rows = if area.height < SHORT_TERMINAL_ROWS {
            1
        } else {
            PANEL_MAX_ROWS
        };
        let checklist = view.checklist.filter(|_| !visible).map(|list| {
            ChecklistSection::new(list, view.checklist_expanded, max_rows, hint.clone(), style)
        });
        let mut sections: Vec<&dyn LiveSection> = Vec::new();
        if let Some(checklist) = &checklist {
            sections.push(checklist);
        }
        sections.extend(view.footer.sections.iter().copied());
        let footer = LiveView { sections };
        let rows = footer.layout(area.width, max_live_height(area.height));
        let footer_rows = rows
            .iter()
            .fold(0u16, |height, row| height.saturating_add(row.height));
        let areas = geometry(area, footer_rows, 0, view.panel);
        self.transcript
            .draw(view.entries, view.details, areas.transcript, buffer, other);
        if let Some(area) = areas.panel {
            if let Some(work) = view.work {
                panel::draw_work(area, buffer, work);
            } else {
                panel::draw(
                    area,
                    buffer,
                    view.checklist,
                    view.checklist_expanded,
                    style,
                    hint,
                );
            }
        }
        for (section, row) in footer.sections.iter().zip(rows) {
            if row.height > 0 {
                section.render(
                    Rect::new(
                        areas.footer.x + row.x,
                        areas.footer.y + row.y,
                        row.width,
                        row.height,
                    ),
                    buffer,
                );
            }
        }
    }
}

/// Draw a workspace without retaining its optional render cache.
pub fn draw(view: &WorkspaceView<'_>, area: Rect, buffer: &mut Buffer, other: &dyn CellLines) {
    Workspace::default().draw(view, area, buffer, other);
}

/// The same following body used by the pager.
pub fn draw_body(
    entries: &[TranscriptEntry<'_>],
    details: Details,
    area: Rect,
    buffer: &mut Buffer,
    other: &dyn CellLines,
) {
    crate::pager::draw_transcript_body(entries, details, area, buffer, other);
}
