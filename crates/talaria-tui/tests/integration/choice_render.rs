//! The `/theme` and `/skin` selector in the live region, and the sample block drawn above it.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use talaria_core::appearance::{
    ColorDepth, DepthSource, Motion, MotionSource, Polarity, PolaritySource,
};
use talaria_core::commands::{ChoiceItem, ChoiceKey, ChoiceList};
use talaria_tui::live::LiveSection;
use talaria_tui::palette::{ChoiceSection, sample_lines};
use talaria_tui::theme::cells::ThemedCells;
use talaria_tui::theme::{Theme, ThemeInputs, resolve_with_theme_file};

fn item(id: &str, description: &str, current: bool) -> ChoiceItem {
    ChoiceItem {
        id: id.into(),
        name: id.into(),
        description: description.into(),
        group: Some("Skins Hermes has".into()),
        current,
    }
}

fn list() -> ChoiceList {
    ChoiceList::new(vec![
        item("harbor", "Cool blue and grey", false),
        item("meadow", "Soft greens", true),
        item("cinder", "Warm reds on charcoal", false),
    ])
}

fn draw(section: &ChoiceSection, width: u16) -> (Vec<String>, Buffer) {
    let height = section.height(width);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|frame| section.render(frame.area(), frame.buffer_mut()))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let rows = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect();
    (rows, buffer)
}

#[test]
fn choice_section_draws_name_description_current_marker_and_footer() {
    let section = ChoiceSection::new(&list(), 40);
    let (rows, buffer) = draw(&section, 100);
    assert_eq!(rows[0], "  Skins Hermes has");
    assert_eq!(rows[1], "  harbor            Cool blue and grey");
    assert_eq!(rows[2], "  meadow (current)  Soft greens");
    assert_eq!(rows[3], "  cinder            Warm reds on charcoal");
    assert_eq!(
        rows[4],
        "type to filter \u{b7} \u{2191}\u{2193} preview \u{b7} Enter apply \u{b7} Esc cancel"
    );
    assert_eq!(rows.len(), 5);
    // The current entry is selected at open and drawn reversed.
    assert!(buffer[(0, 2)].modifier.contains(Modifier::REVERSED));
    assert!(!buffer[(0, 1)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn choice_section_shows_the_note_above_the_list_and_the_filter_in_the_footer() {
    let mut list = list();
    list.set_note(Some(
        "Previewing changes the Hermes profile's skin until you cancel.".into(),
    ));
    for ch in "ar".chars() {
        list.update(ChoiceKey::Type(ch));
    }
    let (rows, buffer) = draw(&ChoiceSection::new(&list, 40), 160);
    assert_eq!(
        rows[0],
        "Previewing changes the Hermes profile's skin until you cancel."
    );
    assert!(buffer[(0, 0)].modifier.contains(Modifier::DIM));
    assert_eq!(rows[2], "  harbor  Cool blue and grey");
    assert_eq!(rows[3], "  cinder  Warm reds on charcoal");
    assert_eq!(
        rows[4],
        "filter: ar \u{b7} \u{2191}\u{2193} preview \u{b7} Enter apply \u{b7} Esc cancel"
    );
    assert_eq!(rows.len(), 5);
}

#[test]
fn choice_section_keeps_the_whole_note_and_the_key_hints_at_eighty_columns() {
    let note = "Skins do not show under the current theme (follow_hermes_skin = false); \
                previewing still changes the Hermes profile's skin until you cancel.";
    let mut list = list();
    list.set_note(Some(note.into()));
    let section = ChoiceSection::new(&list, 40).with_theme(theme("ember").views.clone());
    let (rows, _) = draw(&section, 80);
    // The note wraps onto rows of its own; no word of it is cut.
    assert_eq!(rows[..2].join(" "), note);
    assert_eq!(rows[2], "  Skins Hermes has");
    assert_eq!(
        rows.last().map(String::as_str),
        Some("type to filter \u{b7} \u{2191}\u{2193} preview \u{b7} Enter apply \u{b7} Esc cancel")
    );
    assert_eq!(rows.len(), 7);
}

#[test]
fn choice_section_drops_description_below_fifty_columns() {
    let (rows, _) = draw(&ChoiceSection::new(&list(), 40), 40);
    assert_eq!(rows[1], "  harbor");
    assert_eq!(rows[2], "  meadow (current)");
}

fn theme(id: &str) -> Theme {
    let file = talaria_app::theme_file::builtin(id).expect("built-in theme");
    resolve_with_theme_file(
        &ThemeInputs {
            skin: None,
            skipped: &[],
            follow_hermes_skin: true,
            polarity: (Polarity::Dark, PolaritySource::Default),
            depth: (ColorDepth::TrueColor, DepthSource::Config),
            motion: (Motion::Full, MotionSource::Default),
            terminal: Default::default(),
        },
        Some(&file),
    )
    .theme
}

#[test]
fn sample_block_shows_each_kind_of_row_in_the_candidate_theme() {
    let ember = theme("ember");
    let paper = theme("paper");
    let ember_lines = sample_lines(
        &ThemedCells {
            theme: &ember,
            transcript_key: None,
        },
        60,
    );
    let paper_lines = sample_lines(
        &ThemedCells {
            theme: &paper,
            transcript_key: None,
        },
        60,
    );
    let text = |lines: &[ratatui::text::Line<'_>]| -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect()
    };
    let shown = text(&ember_lines).join("\n");
    for expected in [
        "> Sample prompt: tidy the build script",
        "cargo fmt",
        "read_file",
        "Sample notice",
        "\u{2234} thinking",
    ] {
        assert!(
            shown.contains(expected),
            "missing {expected:?} in:\n{shown}"
        );
    }
    // The same rows, drawn from the same cell style functions, in each theme's own colours.
    assert_eq!(text(&ember_lines), text(&paper_lines));
    let styles = |lines: &[ratatui::text::Line<'_>]| -> Vec<ratatui::style::Style> {
        lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| line.style.patch(span.style)))
            .collect()
    };
    assert_ne!(styles(&ember_lines), styles(&paper_lines));
}
