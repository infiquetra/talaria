//! The `/theme` and `/skin` selector in the live region, and the sample block drawn above it.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
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
    assert_eq!(rows[0], "Appearance");
    assert!(rows[1].starts_with("  Skins Hermes has"));
    assert!(rows[1].contains("Preview  meadow (current)"));
    assert!(rows[2].starts_with("  harbor"));
    assert!(rows[2].contains("Soft greens"));
    assert!(rows[3].starts_with("> meadow (current)"));
    assert!(rows[4].starts_with("  cinder"));
    assert_eq!(
        rows[5],
        "type to filter \u{b7} \u{2191}\u{2193} preview \u{b7} Enter apply \u{b7} Esc cancel"
    );
    assert_eq!(rows.len(), 6);
    assert!(buffer[(0, 3)].modifier.contains(Modifier::BOLD));
    assert!(!buffer[(0, 3)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn choice_section_shows_the_filter_in_the_heading_and_the_note_above_the_list() {
    let mut list = list();
    list.set_note(Some(
        "Previewing changes the Hermes profile's skin until you cancel.".into(),
    ));
    for ch in "ar".chars() {
        list.update(ChoiceKey::Type(ch));
    }
    let (rows, buffer) = draw(&ChoiceSection::new(&list, 40), 160);
    assert!(rows[0].starts_with("Appearance  ·  Filter: ar"));
    assert!(rows[0].ends_with("Current: meadow"));
    assert_eq!(
        rows[1],
        "Previewing changes the Hermes profile's skin until you cancel."
    );
    assert!(buffer[(0, 1)].modifier.contains(Modifier::DIM));
    assert!(rows[2].contains("Preview  harbor"));
    assert!(rows[3].starts_with("> harbor"));
    assert!(rows[3].contains("Cool blue and grey"));
    assert!(rows[4].starts_with("  cinder"));
    assert_eq!(
        rows[5],
        "\u{2191}\u{2193} preview \u{b7} Enter apply \u{b7} Esc cancel"
    );
    assert_eq!(rows.len(), 6);
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
    assert_eq!(rows[1..3].join(" "), note);
    assert!(rows[3].starts_with("  Skins Hermes has"));
    assert_eq!(
        rows.last().map(String::as_str),
        Some("type to filter \u{b7} \u{2191}\u{2193} preview \u{b7} Enter apply \u{b7} Esc cancel")
    );
    assert_eq!(rows.len(), 8);
}

#[test]
fn choice_section_stacks_the_selected_description_at_forty_columns() {
    let (rows, _) = draw(&ChoiceSection::new(&list(), 40), 40);
    assert_eq!(rows[0], "Appearance  ·  type to filter");
    assert_eq!(rows[2], "  harbor");
    assert_eq!(rows[3], "> meadow (current)");
    assert_eq!(rows[5], "Preview  meadow (current)");
    assert_eq!(rows[6], "Soft greens");
}

#[test]
fn a_themed_inline_selection_stays_visible_on_the_terminal_background() {
    let theme = theme("ember");
    let selection = theme.views.inline_selection(
        talaria_tui::theme::surfaces::tokens::LIST_SELECTED,
        ratatui::style::Style::default(),
    );
    let section = ChoiceSection::new(&list(), 40).with_theme(theme.views);
    let (_, buffer) = draw(&section, 80);
    let selected = &buffer[(0, 3)];
    assert_eq!(selected.symbol(), ">");
    assert_eq!(selected.fg, selection.fg.unwrap());
    assert_eq!(selected.bg, Color::Reset);
    assert!(selected.modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn a_scrolled_choice_keeps_its_group_title_and_wraps_the_selected_description() {
    let long = "The selected appearance uses paper colours for a light terminal and customises links, headings and tool output.";
    let mut items = Vec::new();
    items.push(item("configured", "Configured appearance", true));
    for index in 0..=9 {
        items.push(ChoiceItem {
            group: Some("Background settings".into()),
            ..item(&format!("option {index}"), long, false)
        });
    }
    let mut list = ChoiceList::new(items);
    list.update(ChoiceKey::Prev);
    let section = ChoiceSection::new(&list, 24).within_rows(7);
    let (rows, _) = draw(&section, 60);
    assert_eq!(rows.len(), 7);
    assert_eq!(rows[0], "Appearance");
    assert!(rows[1].starts_with("  Background settings"));
    assert!(rows[1].contains("Preview  option 9"));
    assert!(rows.iter().any(|row| row.starts_with("> option 9")));
    let preview = rows[2..6]
        .iter()
        .filter_map(|row| row.split_once('│').map(|(_, text)| text.trim()))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(preview, long);
    assert!(rows[6].contains("Esc cancel"));
}

#[test]
fn the_transcript_sample_and_note_share_the_choice_row_budget() {
    let mut list = list();
    list.set_note(Some("A long note about the appearance. ".repeat(20)));
    let sample = vec![
        ratatui::text::Line::from("Transcript sample"),
        ratatui::text::Line::from("This extra row must fit."),
    ];
    let section = ChoiceSection::new(&list, 24)
        .with_sample(sample)
        .within_rows(6);
    let (rows, _) = draw(&section, 68);
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[0], "Appearance");
    assert!(rows.iter().any(|row| row.contains("> meadow (current)")));
    assert!(!rows.iter().any(|row| row.contains("Transcript sample")));
    assert!(rows[5].contains("Enter apply") && rows[5].contains("Esc cancel"));
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
        "You",
        "Sample prompt: tidy the build script",
        "Hermes",
        "cargo fmt",
        "read_file",
        "Sample notice",
        "\u{2234} reasoning",
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

#[test]
fn choice_preview_at_sixty_columns_keeps_a_tool_and_reply() {
    let theme = theme("ember");
    let width = ChoiceSection::sample_width(60).expect("preview column");
    let sample = sample_lines(
        &ThemedCells {
            theme: &theme,
            transcript_key: None,
        },
        width,
    );
    let mut list = list();
    list.update(ChoiceKey::Next);
    let section = ChoiceSection::new(&list, 24)
        .with_theme(theme.views)
        .with_sample(sample);
    let (rows, _) = draw(&section, 60);
    let text = rows.join("\n");
    assert!(text.contains("Transcript sample"), "{text}");
    assert!(text.contains("Tidy build.sh."), "{text}");
    assert!(text.contains("read_file"), "{text}");
    assert!(text.contains("Ran cargo fmt."), "{text}");
    assert!(text.contains("Esc cancel"), "{text}");
    assert!(rows.len() <= 12, "{text}");
}
