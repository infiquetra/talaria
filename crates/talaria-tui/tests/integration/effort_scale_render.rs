//! The `/effort` selector in the live region: the scale with its marker, the selected level's
//! description, the staged Hivecode and save switches, and the key hints.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use talaria_core::models::{Effort, EffortScale, ScaleFacts, ScaleKey};
use talaria_tui::composer::Composer;
use talaria_tui::keys::KeyMap;
use talaria_tui::live::{LiveSection, LiveView, max_live_height};
use talaria_tui::palette::{EffortScaleSection, scale_key_labels};

/// A terminal tall enough that the selector draws every row.
const TALL: u16 = 60;

fn scale() -> EffortScale {
    EffortScale::new(ScaleFacts {
        model: Some("model-a".into()),
        current: Some(Effort::High),
        wire: None,
        offered: Some(true),
        can_disable: None,
        hivecode: false,
        profile: "work".into(),
    })
}

/// Opened on high, moved one step up to xhigh.
fn moved() -> EffortScale {
    let mut scale = scale();
    scale.update(ScaleKey::Higher);
    scale
}

fn section(scale: &EffortScale) -> EffortScaleSection {
    EffortScaleSection::new(scale, scale_key_labels(&KeyMap::defaults()), TALL)
}

fn draw(section: &EffortScaleSection, width: u16) -> (Vec<String>, Buffer) {
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

/// The column of a word in a row of one-cell glyphs.
fn column(row: &str, word: &str) -> usize {
    let at = row.find(word).expect("word in row");
    row[..at].chars().count()
}

const HINT: &str = "  ←/→ adjust   Enter apply   Esc cancel";

#[test]
fn full_scale_groups_the_controls_and_names_the_preview_and_applied_levels() {
    for width in [80, 120] {
        let (rows, _) = draw(&section(&moved()), width);
        assert_eq!(
            rows[0],
            " Effort   Preview: xhigh   Current: high · model-a"
        );
        assert!(rows[1].starts_with("  Faster") && rows[1].ends_with("More reasoning"));
        assert_eq!(rows[1].chars().count(), rows[3].chars().count());
        assert_eq!(
            rows[3],
            "  none     minimal     low     medium     high     xhigh     max     ultra"
        );
        assert_eq!(rows[4], "  Extended thinking for complex work");
        assert_eq!(
            rows[5],
            "  Tab Hivecode off    Ctrl+G Save for new sessions off (work)"
        );
        assert_eq!(rows[6], HINT);
    }
    let (rows, _) = draw(&section(&scale()), 80);
    assert!(rows[0].contains("Current: high"));
    assert!(!rows[0].contains("Preview"));
}

#[test]
fn the_marker_sits_over_every_level() {
    let mut scale = scale();
    scale.update(ScaleKey::Lowest);
    for index in 0..8 {
        let (rows, _) = draw(&section(&scale), 80);
        let word = scale.selected().as_wire();
        let labels = &rows[3];
        // A word can appear inside another ("max" is not inside any, "high" is inside
        // "xhigh"), so look for it as a whole word.
        let at = format!(" {word} ");
        let start = column(&format!("{labels} "), &at) + 1;
        let centre = start + word.len() / 2;
        assert_eq!(column(&rows[2], "◆"), centre, "{word}");
        assert_eq!(rows[2].matches('●').count(), index);
        assert_eq!(rows[2].matches('○').count(), 7 - index);
        scale.update(ScaleKey::Higher);
    }
}

#[test]
fn staged_switches_show_in_the_panel() {
    let mut scale = moved();
    scale.update(ScaleKey::ToggleHivecode);
    scale.update(ScaleKey::ToggleSave);
    let (rows, _) = draw(&section(&scale), 80);
    assert_eq!(
        rows[5],
        "  Tab Hivecode on (was off)    Ctrl+G Save for new sessions on (work)"
    );
    let short = drawn(&section(&scale), 40, 4);
    assert!(
        short
            .iter()
            .any(|row| row.contains("Tab Hivecode on  Ctrl+G Save on")),
        "{short:?}"
    );
}

#[test]
fn narrow_widths_stack_controls_and_compact_the_scale() {
    let (rows, _) = draw(&section(&moved()), 40);
    assert_eq!(rows[1], "  ◀ xhigh ▶   6 of 8");
    assert!(rows.iter().any(|row| row == "  Tab Hivecode off"));
    assert!(
        rows.iter()
            .any(|row| row.starts_with("  Ctrl+G Save for new sessions off"))
    );
    for width in [40u16, 53, 54, 70, 78, 79, 80, 120] {
        let (rows, _) = draw(&section(&moved()), width);
        for row in &rows {
            assert!(
                row.chars().count() <= usize::from(width),
                "{width}: {row:?}"
            );
        }
        assert!(rows.iter().any(|row| row.contains("Esc cancel")), "{width}");
        assert!(
            rows.iter().any(|row| row.contains("Extended thinking")),
            "{width}"
        );
    }
}

#[test]
fn without_colour_the_marker_and_bold_label_carry_the_selection() {
    let (rows, buffer) = draw(&section(&moved()), 80);
    let marker = u16::try_from(column(&rows[2], "◆")).unwrap();
    assert_eq!(buffer[(marker, 2)].symbol(), "◆");
    let start = u16::try_from(column(&rows[3], "xhigh")).unwrap();
    for x in start..start + 5 {
        assert!(buffer[(x, 3)].modifier.contains(Modifier::BOLD), "{x}");
    }
    let high = u16::try_from(column(&rows[3], " high ") + 1).unwrap();
    assert!(!buffer[(high, 3)].modifier.contains(Modifier::BOLD));
}

#[test]
fn a_note_takes_its_own_row() {
    let mut scale = EffortScale::new(ScaleFacts {
        model: Some("model-a".into()),
        current: Some(Effort::Max),
        wire: Some("medium".into()),
        offered: Some(true),
        can_disable: None,
        hivecode: false,
        profile: "work".into(),
    });
    scale.update(ScaleKey::Lower);
    let (rows, _) = draw(&section(&scale), 80);
    assert!(
        rows.contains(&"  Hermes sends medium on this route.".to_owned()),
        "{rows:#?}"
    );
    let short = drawn(&section(&scale), 50, 4);
    assert!(
        short
            .iter()
            .any(|row| row.contains("Hermes sends medium on this route."))
    );
}

#[test]
fn the_hints_use_the_keys_the_user_bound() {
    let entries = [
        ("effort_down", vec!["alt+h"]),
        ("effort_up", vec!["alt+l"]),
        ("model_scope", vec!["alt+g"]),
    ]
    .into_iter()
    .map(|(name, chords)| {
        (
            name.to_owned(),
            chords.into_iter().map(str::to_owned).collect(),
        )
    })
    .collect();
    let (map, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    let section = EffortScaleSection::new(&moved(), scale_key_labels(&map), TALL);
    let (rows, _) = draw(&section, 120);
    let hint = rows.last().expect("hint row");
    assert!(hint.starts_with("  Alt+H/Alt+L adjust"), "{hint:?}");
    assert!(
        rows.iter()
            .any(|row| row.contains("Alt+G Save for new sessions"))
    );
}

#[test]
fn names_hermes_reported_cannot_carry_control_characters_into_the_panel() {
    let scale = EffortScale::new(ScaleFacts {
        model: Some("model-a\u{1b}[31m\nsecond".into()),
        current: Some(Effort::Max),
        wire: Some("medium\u{1b}[2J".into()),
        offered: Some(true),
        can_disable: None,
        hivecode: false,
        profile: "work\u{1b}]0;title\u{7}".into(),
    });
    let (rows, _) = draw(&section(&scale), 100);
    let all = rows.join("\n");
    assert!(
        !all.contains('\u{1b}') && !all.contains('\u{7}'),
        "a control character reached the panel: {rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("Current: max · model-a")),
        "{rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("Save for new sessions off (work")),
        "{rows:#?}"
    );
}

/// A fixed-height stand-in for a live section.
struct Rows(u16);

impl LiveSection for Rows {
    fn height(&self, _: u16) -> u16 {
        self.0
    }
    fn render(&self, _: ratatui::layout::Rect, _: &mut Buffer) {}
}

/// The rows the selector draws in the area the live region gives it.
fn drawn(section: &EffortScaleSection, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal
        .draw(|frame| section.render(frame.area(), frame.buffer_mut()))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

#[test]
fn a_short_terminal_keeps_the_composer_the_working_row_and_the_enter_and_esc_hint() {
    // The run loop's order: the tail, the working row, the composer, the selector, the status
    // bar. The live region is the terminal less two rows.
    for (cols, terminal_rows) in [(80u16, 24u16), (80, 16), (80, 12), (40, 12), (20, 12)] {
        let mut composer = Composer::new(KeyMap::defaults());
        let composer = composer.section(cols, 10);
        let (tail, working, status) = (Rows(3), Rows(1), Rows(1));
        let scale = EffortScaleSection::new(
            &moved(),
            scale_key_labels(&KeyMap::defaults()),
            terminal_rows,
        );
        let view = LiveView {
            sections: vec![&tail, &working, &composer, &scale, &status],
        };
        let rects = view.layout(cols, max_live_height(terminal_rows));
        let size = format!("{cols}x{terminal_rows}");
        assert_eq!(rects[1].height, 1, "{size}: the working row keeps its row");
        assert_eq!(
            rects[2].height,
            composer.height(cols),
            "{size}: the composer keeps its rows"
        );
        assert_eq!(
            rects[3].height,
            scale.height(cols),
            "{size}: the selector fits"
        );
        assert!(
            scale.height(cols) <= (terminal_rows / 3).max(3),
            "{size}: within a third of the terminal"
        );
        let rows = drawn(&scale, cols, rects[3].height);
        assert!(
            rows[0].contains("Effort"),
            "{size}: the title stays: {rows:#?}"
        );
        let hint = rows.last().expect("hint row");
        assert!(
            hint.contains("Enter") && hint.contains("Esc"),
            "{size}: the last row names Enter and Esc: {rows:#?}"
        );
        assert!(
            rows.iter()
                .any(|row| row.contains("xhigh") || row.contains('◆')),
            "{size}: the scale or its compact form stays: {rows:#?}"
        );
    }
}

#[test]
fn a_clipped_selector_reflows_around_the_selection_and_the_exit_keys() {
    let section = section(&moved());
    for (height, expected) in [
        (1, vec!["  ◀ xhigh ▶   6 of 8"]),
        (2, vec!["  ◀ xhigh ▶   6 of 8", HINT]),
        (
            3,
            vec![
                " Effort   Preview: xhigh   Current: high · model-a",
                "  ◀ xhigh ▶   6 of 8",
                HINT,
            ],
        ),
    ] {
        assert_eq!(drawn(&section, 80, height), expected);
    }
    let five = drawn(&section, 80, 5);
    assert!(five.iter().any(|row| row.contains('◆')));
    assert!(
        five.iter().any(
            |row| row.contains("Tab Hivecode off") && row.contains("Save for new sessions off")
        )
    );
}

#[test]
fn custom_theme_colours_reach_both_halves_of_the_scale_and_the_keys() {
    use crate::view_colours_support as support;
    use ratatui::style::Color;
    use talaria_app::theme_file::{ColorValue, StyleFields, builtin};
    let mut file = builtin("homebrew").unwrap();
    for (token, rgb) in [
        ("effort.track", (181, 195, 221)),
        ("effort.selected", (242, 188, 145)),
    ] {
        file.elements.insert(
            token.to_owned(),
            StyleFields {
                fg: Some(ColorValue::Rgb(rgb.0, rgb.1, rgb.2)),
                ..Default::default()
            },
        );
    }
    let theme = support::resolve(Some(&file));
    let (_, buf) = draw(&section(&moved()).with_theme(theme.theme.views), 80);
    for (text, token) in [
        ("━", "effort.track"),
        ("◆", "effort.selected"),
        ("xhigh", "effort.selected"),
        ("○", "list.muted"),
        ("Tab", "dialog.key"),
    ] {
        let cell = support::at_text(&buf, text);
        assert_eq!(Some(cell.fg), theme.elements[token].fg, "{text}");
        assert_eq!(cell.bg, Color::Reset);
    }
}
