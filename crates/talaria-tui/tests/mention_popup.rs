//! The mention list above the composer. Issue 224.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use talaria_core::attachments::MentionView;
use talaria_tui::composer::Composer;
use talaria_tui::keys::KeyMap;
use talaria_tui::live::{LiveSection, mention_then_composer};
use talaria_tui::mention::MentionSection;

const FOOTER: &str = "tab complete · enter insert · esc close";

fn view(rows: Vec<(&str, &str)>, selected: Option<usize>, status: Option<&str>) -> MentionView {
    MentionView {
        rows: rows
            .into_iter()
            .map(|(display, meta)| (display.to_string(), meta.to_string()))
            .collect(),
        selected,
        status: status.map(str::to_string),
        footer: FOOTER,
    }
}

fn draw(shown: &MentionView, cols: u16) -> Buffer {
    let section = MentionSection::new(shown);
    let mut terminal = Terminal::new(TestBackend::new(cols, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            let area = Rect::new(0, 0, cols, 24);
            section.render(area, frame.buffer_mut());
        })
        .expect("draw");
    terminal.backend().buffer().clone()
}

fn line(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn lines(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| line(buffer, y))
        .filter(|row| !row.is_empty())
        .collect()
}

fn cut(text: &str, width: usize) -> String {
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    if text.chars().count() >= width {
        out.push('…');
    }
    out
}

/// Searching, rows, no matches, and a refusal, at 80 and 36 columns.
#[test]
fn states_at_80_and_36_columns() {
    let searching = draw(&view(Vec::new(), None, Some("Searching…")), 80);
    let rows = lines(&searching);
    assert_eq!(rows, vec!["Searching…".to_string(), FOOTER.to_string()]);

    let items = view(
        vec![("calc.py", "META"), ("notes.md", "file")],
        Some(1),
        None,
    );
    let drawn = draw(&items, 80);
    let rows = lines(&drawn);
    assert!(rows.iter().any(|row| row.contains("META")));
    assert!(
        rows.iter()
            .any(|row| row.starts_with('›') && row.contains("notes.md"))
    );
    assert_eq!(rows.last().map(String::as_str), Some(FOOTER));
    let selected = rows
        .iter()
        .position(|row| row.starts_with('›'))
        .expect("selected");
    assert!(
        drawn[(0, selected as u16)]
            .modifier
            .contains(Modifier::REVERSED)
    );
    assert!(
        !lines(&drawn)
            .iter()
            .any(|row| row.contains('╭') || row.contains('│'))
    );

    let none = draw(&view(Vec::new(), None, Some("No matches for @zzz")), 80);
    assert_eq!(
        lines(&none),
        vec!["No matches for @zzz".to_string(), FOOTER.to_string()]
    );

    let unavailable = "Completion unavailable: Hermes does not know this session.";
    let failed = draw(&view(Vec::new(), None, Some(unavailable)), 80);
    assert_eq!(lines(&failed)[0], unavailable);

    let narrow = draw(
        &view(vec![("calc.py", "META")], Some(0), Some(unavailable)),
        36,
    );
    let rows = lines(&narrow);
    assert!(rows.iter().all(|row| !row.contains("META")));
    assert!(rows.iter().any(|row| row == &cut(FOOTER, 36)));
    assert!(rows.iter().any(|row| row == &cut(unavailable, 36)));
    assert!(rows.iter().all(|row| row.chars().count() <= 36));
    assert!(
        !rows
            .iter()
            .any(|row| row.contains('╭') || row.contains('─'))
    );

    let mention = MentionSection::new(&items);
    let mut composer = Composer::new(KeyMap::defaults());
    composer.load("see @calc".to_string());
    let section = composer.section(80, 8);
    let placed = mention_then_composer(&mention, &section);
    let rects = placed.layout(80, 20);
    assert_eq!(rects[0].y, 0);
    assert!(rects[0].height > 0);
    assert_eq!(rects[1].y, rects[0].height);

    composer.load("[Image #1] [Image #2]".to_string());
    composer.set_highlights(vec![
        (0..10, Style::default().fg(Color::Cyan)),
        (11..21, Style::default().add_modifier(Modifier::DIM)),
    ]);
    let section = composer.section(80, 8);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
    terminal
        .draw(|frame| section.render(frame.area(), frame.buffer_mut()))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let mut brackets = Vec::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            if buffer[(x, y)].symbol() == "[" {
                brackets.push((x, y));
            }
        }
    }
    assert!(brackets.len() >= 2);
    assert_eq!(buffer[brackets[0]].fg, Color::Cyan);
    assert!(buffer[brackets[1]].modifier.contains(Modifier::DIM));
}
