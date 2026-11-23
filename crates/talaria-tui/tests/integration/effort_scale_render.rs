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

const LABELS_GAP_2: &str = "  none  minimal  low  medium  high  xhigh  max  ultra";
const LABELS_GAP_5: &str =
    "  none     minimal     low     medium     high     xhigh     max     ultra";
const HINT: &str = " \u{2190}/\u{2192} adjust \u{b7} Tab Hivecode \u{b7} Ctrl+G new sessions \u{b7} Enter apply \u{b7} Esc cancel";

#[test]
fn the_scale_at_80_columns() {
    let (rows, _) = draw(&section(&moved()), 80);
    // The scale runs from column 2 to 52; xhigh's centre is column 38.
    let rule = format!(
        "  {}\u{25b2}{}    Hivecode off",
        "\u{2500}".repeat(36),
        "\u{2500}".repeat(14)
    );
    assert_eq!(
        rows,
        [
            " Effort \u{b7} model-a".to_owned(),
            format!("  Faster{}Smarter", " ".repeat(38)),
            rule,
            format!("{LABELS_GAP_2}    Tab to toggle"),
            "  Extended thinking for complex work  (current: high)".to_owned(),
            "  Also save for new sessions on work: off".to_owned(),
            HINT.to_owned(),
        ]
    );
    assert_eq!(column(&rows[2], "\u{25b2}"), column(&rows[3], "xhigh") + 2);
}

#[test]
fn the_scale_at_120_columns() {
    let (rows, _) = draw(&section(&moved()), 120);
    assert_eq!(rows[3], format!("{LABELS_GAP_5}    Tab to toggle"));
    assert_eq!(column(&rows[2], "\u{25b2}"), column(&rows[3], "xhigh") + 2);
    assert_eq!(column(&rows[2], "Hivecode off"), column(&rows[3], "Tab"));
    assert_eq!(
        rows[1].chars().count(),
        LABELS_GAP_5.chars().count(),
        "Smarter ends where the scale ends"
    );
    assert_eq!(rows[6], HINT);
}

#[test]
fn the_marker_sits_over_every_level() {
    let mut scale = scale();
    scale.update(ScaleKey::Lowest);
    for _ in 0..8 {
        let (rows, _) = draw(&section(&scale), 80);
        let word = scale.selected().as_wire();
        let labels = &rows[3];
        // A word can appear inside another ("max" is not inside any, "high" is inside
        // "xhigh"), so look for it as a whole word.
        let at = format!(" {word} ");
        let start = column(&format!("{labels} "), &at) + 1;
        let centre = start + word.len() / 2;
        assert_eq!(column(&rows[2], "\u{25b2}"), centre, "{word}");
        scale.update(ScaleKey::Higher);
    }
}

#[test]
fn staged_switches_show_in_the_panel() {
    let mut scale = moved();
    scale.update(ScaleKey::ToggleHivecode);
    scale.update(ScaleKey::ToggleSave);
    let (rows, _) = draw(&section(&scale), 80);
    assert!(
        rows[2].ends_with("    Hivecode on (now off)"),
        "{:?}",
        rows[2]
    );
    assert_eq!(rows[5], "  Also save for new sessions on work: on");
}

#[test]
fn narrow_widths_move_hivecode_to_its_own_row_and_then_compact_the_scale() {
    let (rows, _) = draw(&section(&moved()), 70);
    assert_eq!(
        rows[3],
        "  none    minimal    low    medium    high    xhigh    max    ultra"
    );
    assert!(
        rows.contains(&"  Hivecode off \u{b7} Tab to toggle".to_owned()),
        "{rows:#?}"
    );
    let (rows, _) = draw(&section(&moved()), 40);
    assert_eq!(rows[1], "  \u{25c0} xhigh \u{25b6}   6 of 8");
    assert!(rows.iter().all(|row| !row.contains('\u{2500}')));
    assert!(
        rows.contains(&"  Hivecode off \u{b7} Tab to toggle".to_owned()),
        "{rows:#?}"
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
    let marker = u16::try_from(column(&rows[2], "\u{25b2}")).unwrap();
    assert_eq!(buffer[(marker, 2)].symbol(), "\u{25b2}");
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
    assert!(hint.starts_with(" Alt+H/Alt+L adjust"), "{hint:?}");
    assert!(hint.contains("Alt+G new sessions"), "{hint:?}");
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
        rows.iter().any(|row| row.contains("Effort \u{b7} model-a")),
        "{rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("Also save for new sessions on work")),
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
                .any(|row| row.contains("xhigh") || row.contains('\u{25b2}')),
            "{size}: the scale or its compact form stays: {rows:#?}"
        );
    }
}

#[test]
fn rows_go_in_order_save_note_detail_then_the_ends_row() {
    let width = 80;
    let rows_at = |terminal_rows: u16| {
        let section = EffortScaleSection::new(
            &moved(),
            scale_key_labels(&KeyMap::defaults()),
            terminal_rows,
        );
        drawn(&section, width, section.height(width))
    };
    // Seven rows at 21 and more: everything.
    let full = rows_at(21);
    assert_eq!(full.len(), 7, "{full:#?}");
    // Six: the save line goes first.
    let six = rows_at(18);
    assert!(!six.iter().any(|row| row.contains("Also save")), "{six:#?}");
    assert!(
        six.iter().any(|row| row.contains("Extended thinking")),
        "{six:#?}"
    );
    // Five: then the detail (this level has no note).
    let five = rows_at(15);
    assert!(
        !five.iter().any(|row| row.contains("Extended thinking")),
        "{five:#?}"
    );
    assert!(five.iter().any(|row| row.contains("Smarter")), "{five:#?}");
    // Four: then the "Faster … Smarter" row; the hint row still names every key.
    let four = rows_at(12);
    assert!(!four.iter().any(|row| row.contains("Smarter")), "{four:#?}");
    assert_eq!(four.last().map(String::as_str), Some(HINT), "{four:#?}");
    // Three: the compact scale, with one hint row.
    let three = rows_at(9);
    assert_eq!(
        three,
        [
            " Effort \u{b7} model-a".to_owned(),
            "  \u{25c0} xhigh \u{25b6}   6 of 8".to_owned(),
            " Enter apply \u{b7} Esc cancel".to_owned(),
        ]
    );
}

#[test]
fn a_cut_selector_keeps_its_bottom_rows() {
    let section = EffortScaleSection::new(&moved(), scale_key_labels(&KeyMap::defaults()), 6);
    // The status bar took most of an 8-row terminal: two rows are left for the selector.
    let rows = drawn(&section, 80, 2);
    assert_eq!(
        rows,
        [
            "  \u{25c0} xhigh \u{25b6}   6 of 8".to_owned(),
            " Enter apply \u{b7} Esc cancel".to_owned(),
        ]
    );
}
