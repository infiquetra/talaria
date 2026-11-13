use ratatui::{buffer::Buffer, layout::Rect, style::Modifier};
use talaria_core::layout::PanelVisibility;
use talaria_tui::{
    history::PlainCellLines,
    live::{LiveView, PlainTextSection},
    pager::Details,
    workspace::{self, WorkspaceView, menu::MenuState},
};

const HEADING: &str = "Talaria   Session   Model   View   Voice   Settings   Help";

fn raw(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn paint(width: u16, height: u16, state: &MenuState) -> Buffer {
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, height));
    state.draw(buffer.area, &mut buffer);
    buffer
}

fn assert_heading(buffer: &Buffer) {
    let row = raw(buffer, 0);
    assert!(row.starts_with(HEADING), "{row}");
    assert!(row[HEADING.len()..].chars().all(|ch| ch == ' '), "{row}");
    assert_eq!(row.chars().count(), usize::from(buffer.area.width));
}

fn view_is_marked(buffer: &Buffer) -> bool {
    let row = raw(buffer, 0);
    let Some(start) = row.find("View") else {
        return false;
    };
    let end = start + "View".len();
    let reversed = (start..end).any(|index| {
        buffer[(index as u16, 0)]
            .modifier
            .contains(Modifier::REVERSED)
    });
    if reversed {
        return true;
    }
    row[..start]
        .chars()
        .rev()
        .find(|ch| !ch.is_whitespace())
        .is_some_and(|ch| matches!(ch, '>' | '▸' | '['))
}

#[test]
fn row_text_and_width() {
    assert_eq!(HEADING.chars().count(), 58);
    for width in [60, 80, 120] {
        let buffer = paint(width, 3, &MenuState::shown());
        assert_heading(&buffer);
        assert!(
            !raw(&buffer, 1).contains("Session"),
            "the bar is one row at width {width}"
        );
    }
}

#[test]
fn row_spans_the_panel() {
    let areas = workspace::geometry(Rect::new(0, 0, 120, 40), 0, 1, PanelVisibility::Docked);
    assert_eq!(areas.menu.x, 0);
    assert_eq!(areas.menu.width, 120);
    let buffer = paint(120, 1, &MenuState::shown());
    assert_heading(&buffer);
    assert_eq!(buffer[(100, 0)].symbol(), " ");
}

#[test]
fn row_has_no_voice_or_model_text() {
    let status = PlainTextSection::new("gpt-test · connected · listening · read-aloud");
    let footer = LiveView {
        sections: vec![&status],
    };
    let view = WorkspaceView {
        entries: &[],
        details: Details::Expanded,
        panel: PanelVisibility::Hidden,
        menu: MenuState::shown(),
        checklist: None,
        checklist_expanded: false,
        footer: &footer,
        work: None,
    };
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
    workspace::draw(&view, buffer.area, &mut buffer, &PlainCellLines);
    let row = raw(&buffer, 0);
    assert!(row.starts_with(HEADING), "{row}");
    for word in [
        "gpt-test",
        "connected",
        "listening",
        "read-aloud",
        "recording",
    ] {
        assert!(
            !row.to_lowercase().contains(word),
            "{word} belongs in the status row: {row}"
        );
    }
    let frame = (0..buffer.area.height)
        .map(|y| raw(&buffer, y))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(frame.contains("listening"), "{frame}");
}

#[test]
fn collapses_below_60_columns() {
    for width in [59, 40, 20] {
        let row = raw(&paint(width, 8, &MenuState::shown()), 0);
        assert!(row.contains("Talaria"), "{width}: {row}");
        assert!(row.contains("Menu"), "{width}: {row}");
        assert!(!row.contains("Session"), "{width}: {row}");
        assert!(!row.contains("Settings"), "{width}: {row}");
    }
}

#[test]
fn reserves_no_row_below_16_rows() {
    let state = MenuState::shown();
    let tall = paint(80, 16, &state);
    assert!(
        raw(&tall, 0).contains("Talaria"),
        "sixteen rows show the menu row"
    );
    let areas = workspace::geometry(Rect::new(0, 0, 80, 15), 0, 1, PanelVisibility::Hidden);
    assert_eq!(areas.menu.height, 0);
    let mut short = Buffer::empty(Rect::new(0, 0, 80, 15));
    state.draw(areas.menu, &mut short);
    assert!(!raw(&short, 0).contains("Talaria"));
}

#[test]
fn focus_is_not_colour_only() {
    for color in [true, false] {
        let mut state = MenuState::shown();
        state.menu_index = 2;
        state.color = color;
        let buffer = paint(80, 1, &state);
        let row = raw(&buffer, 0);
        assert!(row.contains("View"), "color {color}: {row}");
        assert!(
            view_is_marked(&buffer),
            "color {color}: focus needs reverse video or a text marker: {row}"
        );
        let session = row.find("Session").expect("Session heading");
        assert!(
            !buffer[(session as u16, 0)]
                .modifier
                .contains(Modifier::REVERSED),
            "color {color}: other headings stay unmarked"
        );
    }
}
