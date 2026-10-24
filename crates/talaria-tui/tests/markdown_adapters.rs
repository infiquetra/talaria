mod markdown_support;

use markdown_support::options;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use talaria_core::session::{AssistantCell, Cell, CellId, ReasoningCell, UserCell};
use talaria_tui::history::{CellLines, PlainCellLines};
use talaria_tui::live::LiveSection;
use talaria_tui::markdown::MarkdownTheme;
use talaria_tui::markdown::cells::MarkdownCellLines;
use talaria_tui::markdown::render_markdown;
use talaria_tui::markdown::section::MarkdownTailSection;

#[test]
fn cell_lines_markdown_and_delegate() {
    let width = 40u16;
    let adapter = MarkdownCellLines::new(true, MarkdownTheme::default());
    let text = "# Title\n\nhello\n";
    let user = Cell::User(UserCell {
        id: CellId::new(1),
        text: text.to_string(),
    });
    let assistant = Cell::Assistant(AssistantCell {
        id: CellId::new(2),
        text: text.to_string(),
        sealed: true,
        interim: false,
        transformed: false,
    });
    let reasoning = Cell::Reasoning(ReasoningCell {
        id: CellId::new(3),
        text: text.to_string(),
        sealed: true,
    });

    assert_eq!(
        adapter.lines(&user, width),
        PlainCellLines.lines(&user, width)
    );

    let opts = options(width);
    assert_eq!(
        adapter.lines(&assistant, width),
        render_markdown(text, &opts)
    );

    let mut dim = opts;
    dim.base_style = Style::default().add_modifier(Modifier::DIM);
    assert_eq!(
        adapter.lines(&reasoning, width),
        render_markdown(text, &dim)
    );
}

#[test]
fn tail_section_height_and_rows() {
    let lines: Vec<Line<'static>> = (0..5)
        .map(|index| Line::from(format!("row {index}")))
        .collect();
    let section = MarkdownTailSection::new(lines);
    assert_eq!(section.height(20), 5);

    let area3 = Rect::new(0, 0, 20, 3);
    let mut short = Buffer::empty(area3);
    section.render(area3, &mut short);
    assert_eq!(row_text(&short, 0, 5), "row 2");
    assert_eq!(row_text(&short, 1, 5), "row 3");
    assert_eq!(row_text(&short, 2, 5), "row 4");

    let area8 = Rect::new(0, 0, 20, 8);
    let mut tall = Buffer::empty(area8);
    let fresh = Buffer::empty(area8);
    section.render(area8, &mut tall);
    for index in 0..5 {
        assert_eq!(row_text(&tall, index, 5), format!("row {index}"));
    }
    for y in 5..8 {
        for x in 0..20 {
            assert_eq!(tall[(x, y)].symbol(), fresh[(x, y)].symbol());
            assert_eq!(tall[(x, y)].style(), fresh[(x, y)].style());
        }
    }
}

fn row_text(buf: &Buffer, y: u16, len: u16) -> String {
    (0..len).map(|x| buf[(x, y)].symbol().to_string()).collect()
}
