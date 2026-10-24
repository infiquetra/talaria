//! The native status line: segment text, priority layout, snapshots and properties.
//!
//! Expected strings are literals from the unit specification, never produced by calling the code
//! under test. Paths are built from parts so no test holds an absolute path.

use proptest::prelude::*;
use proptest::test_runner::{Config, FileFailurePersistence};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use talaria_tui::live::LiveSection;
use talaria_tui::status::{
    Connection, ExternalSegment, Segment, SessionRef, StatusInputs, StatusPalette, StatusSection,
    Tone, plan,
};
use unicode_width::UnicodeWidthStr;

/// Segments from the one dropped last to the one dropped first.
const KEEP_ORDER: [Segment; 8] = [
    Segment::Connection,
    Segment::NeedsYou,
    Segment::Model,
    Segment::Context,
    Segment::Folder,
    Segment::Working,
    Segment::Cost,
    Segment::External,
];

const SHORT_EXTERNAL: &str = "tests: 296 passing";
const LONG_EXTERNAL: &str = "build ok · 5 files changed · 2 warnings";

const FULL_75: &str = " opus-5.5 high │ ~/proj (main) │ ctx 34% │ $0.12 │ ⚑ 2 need you │ 3 working";
const FULL_67: &str = " opus-5.5 high │ ~/proj (main) │ ctx 34% │ ⚑ 2 need you │ 3 working";
const FULL_55: &str = " opus-5.5 high │ ~/proj (main) │ ctx 34% │ ⚑ 2 need you";
const FULL_39: &str = " opus-5.5 high │ ctx 34% │ ⚑ 2 need you";
const FULL_29: &str = " opus-5.5 high │ ⚑ 2 need you";
const FULL_13: &str = " ⚑ 2 need you";

fn path_of(parts: &[&str]) -> String {
    parts.join("/")
}

fn fake_home() -> String {
    path_of(&["", "h", "dev"])
}

/// A connected state with nothing else set, so no segment shows.
fn base() -> StatusInputs {
    StatusInputs {
        connection: Connection::Connected,
        ..StatusInputs::default()
    }
}

/// The worked example `full` of the specification.
fn full() -> StatusInputs {
    StatusInputs {
        connection: Connection::Connected,
        model: Some("opus-5.5".into()),
        reasoning_effort: Some("high".into()),
        reasoning_effort_wire: Some("high".into()),
        working_dir: Some(path_of(&["", "h", "dev", "proj"])),
        home_dir: Some(fake_home()),
        branch: Some("main".into()),
        context_percent: Some(34),
        context_estimated: false,
        cost_usd: Some(0.12),
        cost_status: Some("actual".into()),
        needs_you: 2,
        working: 3,
        session: Some(SessionRef {
            id: "sess-1".into(),
            title: None,
        }),
        ..StatusInputs::default()
    }
}

fn external(text: &str) -> ExternalSegment {
    ExternalSegment {
        text: text.into(),
        tone: Tone::Muted,
    }
}

/// The text of one segment on a very wide line, or `None` when it is hidden.
fn piece(inputs: &StatusInputs, segment: Segment) -> Option<String> {
    plan(inputs, None, 400)
        .pieces
        .into_iter()
        .find(|(seg, _, _)| *seg == segment)
        .map(|(_, text, _)| text)
}

fn piece_with_tone(inputs: &StatusInputs, segment: Segment) -> Option<(String, Tone)> {
    plan(inputs, None, 400)
        .pieces
        .into_iter()
        .find(|(seg, _, _)| *seg == segment)
        .map(|(_, text, tone)| (text, tone))
}

fn line_at(inputs: &StatusInputs, ext: Option<&ExternalSegment>, cols: u16) -> String {
    plan(inputs, ext, cols).text()
}

fn model_of(model: &str, effort: Option<&str>, wire: Option<&str>) -> Option<String> {
    let inputs = StatusInputs {
        model: Some(model.into()),
        reasoning_effort: effort.map(Into::into),
        reasoning_effort_wire: wire.map(Into::into),
        ..base()
    };
    piece(&inputs, Segment::Model)
}

fn context_of(percent: Option<i64>, estimated: bool) -> Option<String> {
    let inputs = StatusInputs {
        context_percent: percent,
        context_estimated: estimated,
        ..base()
    };
    piece(&inputs, Segment::Context)
}

fn cost_of(usd: Option<f64>, status: Option<&str>) -> Option<String> {
    let inputs = StatusInputs {
        cost_usd: usd,
        cost_status: status.map(Into::into),
        ..base()
    };
    piece(&inputs, Segment::Cost)
}

fn folder_of(dir: Option<&str>, home: Option<&str>, branch: Option<&str>) -> Option<String> {
    let inputs = StatusInputs {
        working_dir: dir.map(Into::into),
        home_dir: home.map(Into::into),
        branch: branch.map(Into::into),
        ..base()
    };
    piece(&inputs, Segment::Folder)
}

/// Draw the section into a `width` by 1 test terminal.
fn draw(inputs: &StatusInputs, ext: Option<&ExternalSegment>, width: u16) -> Terminal<TestBackend> {
    let palette = StatusPalette::default();
    let section = StatusSection::new(inputs, ext, &palette);
    let mut terminal = Terminal::new(TestBackend::new(width, 1)).expect("test terminal");
    terminal
        .draw(|frame| section.render(frame.area(), frame.buffer_mut()))
        .expect("draw");
    terminal
}

fn row_symbols(buffer: &Buffer) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, 0)].symbol())
        .collect::<String>()
}

fn rendered_row(inputs: &StatusInputs, ext: Option<&ExternalSegment>, width: u16) -> String {
    let terminal = draw(inputs, ext, width);
    row_symbols(terminal.backend().buffer())
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

// ---------------------------------------------------------------------------------------------
// Segment text
// ---------------------------------------------------------------------------------------------

#[test]
fn model_label_strips_provider_path_and_joins_effort() {
    // The provider path is dropped: only the text after the last slash is kept.
    assert_eq!(
        model_of("openrouter/anthropic/opus-5.5", Some("high"), None).as_deref(),
        Some("opus-5.5 high")
    );
    assert_eq!(
        model_of("minimax-m3.1-flash", None, None).as_deref(),
        Some("minimax-m3.1-flash")
    );
    // Unset effort, empty or absent, adds nothing.
    assert_eq!(model_of("x", Some(""), None).as_deref(), Some("x"));
    // The effort is trimmed and lower-cased.
    assert_eq!(
        model_of("x", Some("  High "), None).as_deref(),
        Some("x high")
    );
    // An effort of none is shown literally.
    assert_eq!(model_of("x", Some("none"), None).as_deref(), Some("x none"));
    // A wire level that differs from the effort shows as effort arrow wire.
    assert_eq!(
        model_of("x", Some("ultra"), Some("max")).as_deref(),
        Some("x ultra→max")
    );
    // An equal wire level, in any letter case, shows no arrow.
    assert_eq!(
        model_of("x", Some("high"), Some("high")).as_deref(),
        Some("x high")
    );
    assert_eq!(
        model_of("x", Some("high"), Some("HIGH")).as_deref(),
        Some("x high")
    );
    // An empty wire level shows no arrow.
    assert_eq!(
        model_of("x", Some("high"), Some("")).as_deref(),
        Some("x high")
    );
    // No effort means no effort and no arrow, whatever the wire level holds.
    assert_eq!(model_of("x", None, Some("max")).as_deref(), Some("x"));
    assert_eq!(model_of("x", Some(""), Some("max")).as_deref(), Some("x"));
    // An empty model, or a path with nothing after the slash, hides the segment.
    assert_eq!(model_of("", Some("high"), None), None);
    assert_eq!(model_of("provider/", Some("high"), None), None);
    assert_eq!(piece(&base(), Segment::Model), None);
}

#[test]
fn context_hidden_until_reported_estimate_marked_and_clamped() {
    assert_eq!(context_of(None, false), None);
    assert_eq!(context_of(None, true), None);
    assert_eq!(context_of(Some(34), false).as_deref(), Some("ctx 34%"));
    assert_eq!(context_of(Some(34), true).as_deref(), Some("ctx ~34%"));
    assert_eq!(context_of(Some(0), false).as_deref(), Some("ctx 0%"));
    assert_eq!(context_of(Some(100), false).as_deref(), Some("ctx 100%"));
    assert_eq!(context_of(Some(140), false).as_deref(), Some("ctx 100%"));
    assert_eq!(context_of(Some(140), true).as_deref(), Some("ctx ~100%"));
    assert_eq!(
        context_of(Some(i64::MAX), false).as_deref(),
        Some("ctx 100%")
    );
    assert_eq!(context_of(Some(-5), false).as_deref(), Some("ctx 0%"));
    assert_eq!(context_of(Some(i64::MIN), false).as_deref(), Some("ctx 0%"));
}

#[test]
fn cost_follows_status_and_never_invents_zero() {
    assert_eq!(
        cost_of(Some(0.12), Some("actual")).as_deref(),
        Some("$0.12")
    );
    assert_eq!(
        cost_of(Some(0.12), Some("estimated")).as_deref(),
        Some("~$0.12")
    );
    assert_eq!(cost_of(Some(0.12), None).as_deref(), Some("$0.12"));
    // Above zero and under one cent.
    assert_eq!(
        cost_of(Some(0.004), Some("actual")).as_deref(),
        Some("<$0.01")
    );
    assert_eq!(
        cost_of(Some(0.004), Some("estimated")).as_deref(),
        Some("~<$0.01")
    );
    assert_eq!(cost_of(Some(0.009), None).as_deref(), Some("<$0.01"));
    assert_eq!(
        cost_of(Some(0.01), Some("actual")).as_deref(),
        Some("$0.01")
    );
    // A reported zero is shown as zero; only a missing figure is hidden.
    assert_eq!(cost_of(Some(0.0), Some("actual")).as_deref(), Some("$0.00"));
    assert_eq!(cost_of(Some(0.0), None).as_deref(), Some("$0.00"));
    // Two decimals, no thousands separator.
    assert_eq!(
        cost_of(Some(1234.5), Some("actual")).as_deref(),
        Some("$1234.50")
    );
    // Hidden: negative, not finite, status unknown or included, missing.
    assert_eq!(cost_of(Some(-0.5), Some("actual")), None);
    assert_eq!(cost_of(Some(-0.5), None), None);
    assert_eq!(cost_of(Some(f64::NAN), Some("actual")), None);
    assert_eq!(cost_of(Some(f64::NAN), None), None);
    assert_eq!(cost_of(Some(f64::INFINITY), Some("actual")), None);
    assert_eq!(cost_of(Some(f64::NEG_INFINITY), None), None);
    assert_eq!(cost_of(Some(0.12), Some("included")), None);
    assert_eq!(cost_of(Some(0.12), Some("unknown")), None);
    assert_eq!(cost_of(Some(0.0), Some("included")), None);
    assert_eq!(cost_of(Some(0.0), Some("unknown")), None);
    assert_eq!(cost_of(None, Some("actual")), None);
    assert_eq!(cost_of(None, None), None);
}

#[test]
fn folder_abbreviates_home_and_caps_long_paths() {
    let home = fake_home();
    let home = Some(home.as_str());

    // The home folder itself, and a path under it.
    let at_home = fake_home();
    assert_eq!(folder_of(Some(&at_home), home, None).as_deref(), Some("~"));
    let child = path_of(&["", "h", "dev", "proj"]);
    assert_eq!(
        folder_of(Some(&child), home, None).as_deref(),
        Some("~/proj")
    );
    assert_eq!(
        folder_of(Some(&child), home, Some("main")).as_deref(),
        Some("~/proj (main)")
    );
    assert_eq!(
        folder_of(Some(&at_home), home, Some("main")).as_deref(),
        Some("~ (main)")
    );

    // A trailing slash on the home folder is ignored.
    let home_slash = format!("{}/", fake_home());
    assert_eq!(
        folder_of(Some(&child), Some(&home_slash), None).as_deref(),
        Some("~/proj")
    );

    // A sibling that merely shares the prefix is not abbreviated.
    let sibling = path_of(&["", "h", "dev2", "x"]);
    assert_eq!(
        folder_of(Some(&sibling), home, None).as_deref(),
        Some(sibling.as_str())
    );

    // Without a home folder the path is shown as given.
    assert_eq!(
        folder_of(Some(&child), None, None).as_deref(),
        Some(child.as_str())
    );

    // A path of exactly 32 columns is kept whole.
    let exact = format!("/{}", "a".repeat(31));
    assert_eq!(
        folder_of(Some(&exact), None, None).as_deref(),
        Some(exact.as_str())
    );

    // A longer path keeps the most trailing components that fit in 32 columns after "…/".
    let long = path_of(&[
        "",
        "work",
        "alpha-service",
        "components",
        "status-line",
        "render",
        "leaf",
    ]);
    assert_eq!(display_width(&long), 54);
    assert_eq!(
        folder_of(Some(&long), None, None).as_deref(),
        Some("…/status-line/render/leaf")
    );

    // The tilde counts as a component of a path under the home folder.
    let under_home = path_of(&["", "h", "dev", &"a".repeat(20), &"b".repeat(20)]);
    assert_eq!(
        folder_of(Some(&under_home), home, None).as_deref(),
        Some(format!("…/{}", "b".repeat(20)).as_str())
    );

    // When even the last component does not fit: "…/", its first 29 columns, "…".
    let wide_leaf = path_of(&["", "w", &"x".repeat(40)]);
    assert_eq!(
        folder_of(Some(&wide_leaf), None, None).as_deref(),
        Some(format!("…/{}…", "x".repeat(29)).as_str())
    );
    let one_over = format!("/{}", "a".repeat(32));
    assert_eq!(
        folder_of(Some(&one_over), None, None).as_deref(),
        Some(format!("…/{}…", "a".repeat(29)).as_str())
    );

    // A branch longer than 24 columns keeps 23 columns and an ellipsis inside the 24.
    let proj = path_of(&["", "w", "proj"]);
    let long_branch = format!("feature/{}", "abcdefghijklmnopqrstuv");
    assert_eq!(display_width(&long_branch), 30);
    assert_eq!(
        folder_of(Some(&proj), None, Some(&long_branch)).as_deref(),
        Some(format!("{proj} (feature/abcdefghijklmno…)").as_str())
    );
    // A branch of exactly 24 columns is kept whole; 25 columns is cut.
    let branch_24 = format!("feature/{}", "abcdefghijklmnop");
    assert_eq!(display_width(&branch_24), 24);
    assert_eq!(
        folder_of(Some(&proj), None, Some(&branch_24)).as_deref(),
        Some(format!("{proj} ({branch_24})").as_str())
    );
    let branch_25 = format!("{branch_24}q");
    assert_eq!(
        folder_of(Some(&proj), None, Some(&branch_25)).as_deref(),
        Some(format!("{proj} (feature/abcdefghijklmno…)").as_str())
    );

    // An empty branch is no branch; a branch without a folder stands alone.
    assert_eq!(
        folder_of(Some(&proj), None, Some("")).as_deref(),
        Some(proj.as_str())
    );
    assert_eq!(
        folder_of(None, home, Some("main")).as_deref(),
        Some("(main)")
    );
    assert_eq!(
        folder_of(None, None, Some("main")).as_deref(),
        Some("(main)")
    );
    // Neither folder nor branch: no segment.
    assert_eq!(folder_of(None, home, None), None);
    assert_eq!(folder_of(None, None, Some("")), None);
}

#[test]
fn needs_you_working_and_connection_segments() {
    let with = |needs_you: u32, working: u32| StatusInputs {
        needs_you,
        working,
        ..base()
    };
    assert_eq!(piece(&with(0, 0), Segment::NeedsYou), None);
    assert_eq!(
        piece(&with(1, 0), Segment::NeedsYou).as_deref(),
        Some("⚑ 1 needs you")
    );
    assert_eq!(
        piece(&with(2, 0), Segment::NeedsYou).as_deref(),
        Some("⚑ 2 need you")
    );
    assert_eq!(
        piece(&with(12, 0), Segment::NeedsYou).as_deref(),
        Some("⚑ 12 need you")
    );
    assert_eq!(piece(&with(0, 0), Segment::Working), None);
    assert_eq!(
        piece(&with(0, 1), Segment::Working).as_deref(),
        Some("1 working")
    );
    assert_eq!(
        piece(&with(0, 3), Segment::Working).as_deref(),
        Some("3 working")
    );

    // Needs you is the attention tone.
    let (_, tone) = piece_with_tone(&with(2, 0), Segment::NeedsYou).expect("needs you shown");
    assert_eq!(tone, Tone::Attention);

    let connection = |state: Connection| {
        piece_with_tone(
            &StatusInputs {
                connection: state,
                ..StatusInputs::default()
            },
            Segment::Connection,
        )
    };
    assert_eq!(connection(Connection::Connected), None);
    assert_eq!(
        connection(Connection::Disconnected),
        Some(("disconnected".to_string(), Tone::Error))
    );
    assert_eq!(
        connection(Connection::Connecting),
        Some(("connecting…".to_string(), Tone::Warning))
    );
    assert_eq!(
        connection(Connection::Reconnecting),
        Some(("reconnecting…".to_string(), Tone::Warning))
    );
    assert_eq!(
        connection(Connection::AuthFailed),
        Some(("auth failed".to_string(), Tone::Error))
    );

    // The default state is disconnected and says so.
    assert_eq!(line_at(&StatusInputs::default(), None, 80), " disconnected");
}

#[test]
fn external_text_is_trimmed() {
    let ext = external("  tests: 296 passing \t");
    let plan = plan(&base(), Some(&ext), 100);
    let pieces: Vec<_> = plan
        .pieces
        .iter()
        .map(|(s, t, _)| (*s, t.as_str()))
        .collect();
    assert_eq!(pieces, [(Segment::External, SHORT_EXTERNAL)]);
    assert_eq!(plan.text(), " tests: 296 passing");
}

// ---------------------------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------------------------

#[test]
fn priority_drops_lowest_first_at_each_width() {
    let inputs = full();
    let cases: [(u16, &str); 11] = [
        (75, FULL_75),
        (74, FULL_67),
        (67, FULL_67),
        (66, FULL_55),
        (55, FULL_55),
        (54, FULL_39),
        (39, FULL_39),
        (38, FULL_29),
        (29, FULL_29),
        (28, FULL_13),
        (13, FULL_13),
    ];
    for (cols, expected) in cases {
        let line = plan(&inputs, None, cols);
        assert_eq!(line.text(), expected, "line at {cols} columns");
        assert!(
            line.width() <= usize::from(cols),
            "plan width {} exceeds {cols} columns",
            line.width()
        );
        assert!(display_width(&line.text()) <= usize::from(cols));
    }
    // Wider than needed changes nothing.
    assert_eq!(line_at(&inputs, None, 200), FULL_75);
}

#[test]
fn last_segment_is_cut_with_an_ellipsis_and_never_overflows() {
    let inputs = full();
    // One segment left that still does not fit: characters while they fit, then "…".
    assert_eq!(line_at(&inputs, None, 10), " ⚑ 2 need…");
    assert_eq!(display_width(&line_at(&inputs, None, 10)), 10);
    assert_eq!(line_at(&inputs, None, 3), " ⚑…");
    // The one available column holds only the ellipsis.
    assert_eq!(line_at(&inputs, None, 2), " …");
    // No columns to draw in.
    for cols in [1u16, 0] {
        let line = plan(&inputs, None, cols);
        assert!(line.pieces.is_empty(), "pieces at {cols} columns");
        assert_eq!(line.text(), "");
    }

    // A plain text segment is cut by character.
    let ext = external("abcdefghij");
    assert_eq!(line_at(&base(), Some(&ext), 6), " abcd…");
    assert_eq!(line_at(&base(), Some(&ext), 11), " abcdefghij");
    assert_eq!(line_at(&base(), Some(&ext), 10), " abcdefgh…");

    // A two-column character that does not fit whole ends the cut.
    let wide = external("会话会话");
    assert_eq!(line_at(&base(), Some(&wide), 9), " 会话会话");
    assert_eq!(line_at(&base(), Some(&wide), 6), " 会话…");
    assert_eq!(line_at(&base(), Some(&wide), 5), " 会…");
    assert_eq!(line_at(&base(), Some(&wide), 4), " 会…");
    assert_eq!(line_at(&base(), Some(&wide), 3), " …");

    // No width ever overflows, for the full line with an external segment.
    let ext = external(SHORT_EXTERNAL);
    for cols in 0u16..=200 {
        let line = plan(&inputs, Some(&ext), cols);
        assert!(
            display_width(&line.text()) <= usize::from(cols),
            "text overflows at {cols} columns"
        );
        assert!(
            line.width() <= usize::from(cols),
            "plan width overflows at {cols}"
        );
    }

    // Drawing into zero and one column never panics and leaves no text.
    for width in [0u16, 1] {
        let area = Rect::new(0, 0, width, 1);
        let mut buf = Buffer::empty(area);
        let palette = StatusPalette::default();
        StatusSection::new(&inputs, Some(&ext), &palette).render(area, &mut buf);
        assert!(
            row_symbols(&buf).trim().is_empty(),
            "text drawn at {width} columns"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------------------------------

#[test]
fn snapshot_160_columns() {
    let inputs = full();
    let ext = external(SHORT_EXTERNAL);
    let terminal = draw(&inputs, Some(&ext), 160);
    let buffer = terminal.backend().buffer();
    let row = row_symbols(buffer);
    assert_eq!(
        row.trim_end(),
        " opus-5.5 high │ ~/proj (main) │ ctx 34% │ $0.12 │ ⚑ 2 need you │ 3 working │ tests: 296 passing"
    );
    // The row is padded with blanks to the full width.
    assert_eq!(UnicodeWidthStr::width(row.as_str()), 160);

    // A 39 column external text also fits at 160 columns.
    let long = external(LONG_EXTERNAL);
    assert_eq!(
        rendered_row(&inputs, Some(&long), 160).trim_end(),
        " opus-5.5 high │ ~/proj (main) │ ctx 34% │ $0.12 │ ⚑ 2 need you │ 3 working │ build ok · 5 files changed · 2 warnings"
    );

    // Styles: the needs-you text is bold yellow and the separator is dim.
    let flag = row
        .chars()
        .position(|c| c == '⚑')
        .expect("needs-you flag drawn");
    let flag = u16::try_from(flag).expect("column");
    let cell = &buffer[(flag, 0)];
    assert_ne!(
        cell.fg,
        Color::Reset,
        "needs-you cell has the default colour"
    );
    assert_eq!(cell.fg, Color::Yellow);
    assert!(cell.modifier.contains(Modifier::BOLD));
    let bar = row.chars().position(|c| c == '│').expect("separator drawn");
    let bar = u16::try_from(bar).expect("column");
    assert!(buffer[(bar, 0)].modifier.contains(Modifier::DIM));
}

#[test]
fn snapshot_100_columns() {
    let inputs = full();
    // 96 columns fit in 100.
    let ext = external(SHORT_EXTERNAL);
    assert_eq!(
        rendered_row(&inputs, Some(&ext), 100).trim_end(),
        " opus-5.5 high │ ~/proj (main) │ ctx 34% │ $0.12 │ ⚑ 2 need you │ 3 working │ tests: 296 passing"
    );
    // A 39 column external text would make the line 117 columns: it is dropped, the rest stays.
    let long = external(LONG_EXTERNAL);
    let terminal = draw(&inputs, Some(&long), 100);
    let row = row_symbols(terminal.backend().buffer());
    assert_eq!(row.trim_end(), FULL_75);
    assert_eq!(UnicodeWidthStr::width(row.as_str()), 100);
}

#[test]
fn snapshot_60_columns() {
    let inputs = full();
    // Cost and working are dropped, the external text went first.
    let ext = external(SHORT_EXTERNAL);
    let terminal = draw(&inputs, Some(&ext), 60);
    let row = row_symbols(terminal.backend().buffer());
    assert_eq!(row.trim_end(), FULL_55);
    assert_eq!(UnicodeWidthStr::width(row.as_str()), 60);
    let long = external(LONG_EXTERNAL);
    assert_eq!(rendered_row(&inputs, Some(&long), 60).trim_end(), FULL_55);
    assert_eq!(rendered_row(&inputs, None, 60).trim_end(), FULL_55);
}

// ---------------------------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------------------------

fn config() -> Config {
    Config {
        cases: 128,
        failure_persistence: Some(Box::new(FileFailurePersistence::Off)),
        ..Config::default()
    }
}

/// Text from a small alphabet with wide characters and a combining mark, never empty and never
/// blank.
fn text_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec!['a', 'b', 'z', '_', '会', 'é', '\u{301}', '😀']),
        0..10,
    )
    .prop_map(|tail| {
        let mut text = String::from("a");
        text.extend(tail);
        text
    })
}

fn connection_strategy() -> impl Strategy<Value = Connection> {
    prop::sample::select(vec![
        Connection::Disconnected,
        Connection::Connecting,
        Connection::Connected,
        Connection::Reconnecting,
        Connection::AuthFailed,
    ])
}

fn scenario_strategy() -> impl Strategy<Value = (StatusInputs, Option<ExternalSegment>)> {
    let head = (
        connection_strategy(),
        prop::option::of(text_strategy()),
        prop::option::of(prop::sample::select(vec!["", "high", "ultra", "none"])),
        prop::option::of(prop::sample::select(vec!["", "high", "max"])),
        prop::option::of(text_strategy()),
        prop::option::of(text_strategy()),
    );
    let tail = (
        prop::option::of(-20i64..150),
        any::<bool>(),
        prop::option::of(prop::sample::select(vec![
            0.0,
            0.004,
            0.12,
            12.5,
            1234.5,
            -1.0,
            f64::NAN,
        ])),
        prop::option::of(prop::sample::select(vec![
            "actual",
            "estimated",
            "unknown",
            "included",
        ])),
        0u32..6,
        0u32..6,
        prop::option::of(text_strategy()),
    );
    (head, tail).prop_map(
        |(
            (connection, model, effort, wire, working_dir, branch),
            (context, estimated, cost, status, needs_you, working, ext),
        )| {
            let inputs = StatusInputs {
                connection,
                model,
                reasoning_effort: effort.map(Into::into),
                reasoning_effort_wire: wire.map(Into::into),
                working_dir,
                branch,
                context_percent: context,
                context_estimated: estimated,
                cost_usd: cost,
                cost_status: status.map(Into::into),
                needs_you,
                working,
                ..StatusInputs::default()
            };
            let ext = ext.map(|text| ExternalSegment {
                text,
                tone: Tone::Muted,
            });
            (inputs, ext)
        },
    )
}

fn keep_rank(segment: Segment) -> usize {
    KEEP_ORDER
        .iter()
        .position(|s| *s == segment)
        .expect("every segment is in the list")
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn prop_visible_segments_are_a_priority_prefix_within_width(
        (inputs, ext) in scenario_strategy(),
        cols in 0u16..=200,
    ) {
        // The segments that exist, from a line too wide to drop anything.
        let wide = plan(&inputs, ext.as_ref(), u16::MAX);
        let mut shown: Vec<Segment> = wide.pieces.iter().map(|(s, _, _)| *s).collect();
        shown.sort_by_key(|s| keep_rank(*s));
        let texts: Vec<usize> = shown
            .iter()
            .map(|s| {
                let text = wide.pieces.iter().find(|(seg, _, _)| seg == s).map(|(_, t, _)| t).unwrap();
                display_width(text)
            })
            .collect();

        let line = plan(&inputs, ext.as_ref(), cols);
        prop_assert!(line.width() <= usize::from(cols));
        prop_assert!(display_width(&line.text()) <= usize::from(cols));

        let mut visible: Vec<Segment> = line.pieces.iter().map(|(s, _, _)| *s).collect();
        // Pieces are drawn left to right by position, so each segment appears once.
        let drawn = visible.clone();
        visible.sort_by_key(|s| keep_rank(*s));
        visible.dedup();
        prop_assert_eq!(visible.len(), drawn.len());

        let k = visible.len();
        prop_assert!(k <= shown.len());
        prop_assert_eq!(&visible[..], &shown[..k]);

        let avail = usize::from(cols.saturating_sub(1));
        if k == 0 {
            prop_assert!(avail == 0 || shown.is_empty());
        }
        // The prefix is the longest one that fits: one more segment would not.
        if k >= 1 && k < shown.len() {
            let need: usize = texts[..=k].iter().sum::<usize>() + 3 * k;
            prop_assert!(need > avail, "segment {} would also have fitted", k);
        }
    }

    #[test]
    fn prop_widening_never_removes_a_segment(
        (inputs, ext) in scenario_strategy(),
        a in 0u16..=200,
        b in 0u16..=200,
    ) {
        let (narrow, wide) = (a.min(b), a.max(b));
        let at_narrow = plan(&inputs, ext.as_ref(), narrow);
        let at_wide = plan(&inputs, ext.as_ref(), wide);
        for (segment, text, _) in &at_narrow.pieces {
            let other = at_wide.pieces.iter().find(|(s, _, _)| s == segment);
            prop_assert!(other.is_some(), "{:?} vanished when widening {} to {}", segment, narrow, wide);
            // Only a single cut segment may differ, and only in its text.
            if at_narrow.pieces.len() > 1 {
                prop_assert_eq!(&other.unwrap().1, text);
            }
        }
        prop_assert!(at_narrow.pieces.len() <= at_wide.pieces.len());
    }
}

// ---------------------------------------------------------------------------------------------
// Outside text and the shape of the section
// ---------------------------------------------------------------------------------------------

fn forbidden(c: char) -> bool {
    let code = u32::from(c);
    code < 0x20
        || code == 0x7f
        || (0x80..=0x9f).contains(&code)
        || (0x202a..=0x202e).contains(&code)
        || (0x2066..=0x2069).contains(&code)
}

#[test]
fn untrusted_text_is_shown_literally() {
    let inputs = StatusInputs {
        model: Some("ev\x1b[2Jil\x07\u{202e}x\ty".into()),
        reasoning_effort: Some("high\x07".into()),
        working_dir: Some("/w/\x1b[31mred\tx".into()),
        branch: Some("br\x1b]0;t\x07\u{202e}x".into()),
        ..base()
    };
    let ext = external("ex\x1b[2J\x07\u{202e}\ty");

    // Stand-ins from the sanitising table: ESC, BEL, bidirectional override, tab.
    let esc = "\u{241b}";
    let bel = "\u{2407}";
    let bidi = "‹U+202E›";
    let model = format!("ev{esc}[2Jil{bel}{bidi}x    y high{bel}");
    let folder = format!("/w/{esc}[31mred    x (br{esc}]0;t{bel}{bidi}x)");
    let external_text = format!("ex{esc}[2J{bel}{bidi}    y");

    let line = plan(&inputs, Some(&ext), 400);
    let texts: Vec<(Segment, String)> = line
        .pieces
        .iter()
        .map(|(s, t, _)| (*s, t.clone()))
        .collect();
    assert!(
        texts.contains(&(Segment::Model, model.clone())),
        "model in {texts:?}"
    );
    assert!(
        texts.contains(&(Segment::Folder, folder.clone())),
        "folder in {texts:?}"
    );
    assert!(
        texts.contains(&(Segment::External, external_text.clone())),
        "external in {texts:?}"
    );
    assert!(
        !line.text().chars().any(forbidden),
        "plan text holds a control character"
    );

    // Every cell of the drawn buffer is free of control and direction characters.
    let terminal = draw(&inputs, Some(&ext), 400);
    let buffer = terminal.backend().buffer();
    for x in 0..buffer.area.width {
        let symbol = buffer[(x, 0)].symbol();
        assert!(
            !symbol.chars().any(forbidden),
            "cell {x} holds {symbol:?}, a control or direction character"
        );
    }
    let row = row_symbols(buffer);
    assert!(row.contains(&model));
    assert!(row.contains(&folder));
    assert!(row.contains(&external_text));
}

#[test]
fn section_is_one_row_and_blank_when_empty() {
    let inputs = base();
    let palette = StatusPalette::default();
    let section = StatusSection::new(&inputs, None, &palette);
    for width in [0u16, 1, 2, 10, 80, 200] {
        assert_eq!(section.height(width), 1, "height at {width} columns");
    }
    // A section with content is also one row.
    let busy = full();
    assert_eq!(StatusSection::new(&busy, None, &palette).height(120), 1);
    assert_eq!(StatusSection::new(&busy, None, &palette).height(5), 1);

    // Nothing to show: the row is blank, even over cells that held text before.
    let area = Rect::new(0, 0, 20, 1);
    let mut buf = Buffer::empty(area);
    buf.set_string(0, 0, "xxxxxxxxxxxxxxxxxxxx", Style::default());
    section.render(area, &mut buf);
    assert_eq!(row_symbols(&buf), " ".repeat(20));

    let terminal = draw(&inputs, None, 80);
    assert_eq!(row_symbols(terminal.backend().buffer()), " ".repeat(80));

    // The same section drawn with content pads the rest of the row with blanks.
    let mut buf = Buffer::empty(Rect::new(0, 0, 30, 1));
    buf.set_string(0, 0, "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", Style::default());
    StatusSection::new(&busy, None, &palette).render(Rect::new(0, 0, 30, 1), &mut buf);
    assert_eq!(row_symbols(&buf), format!("{FULL_29} "));
}

// ---------------------------------------------------------------------------------------------
// Tones and the palette
// ---------------------------------------------------------------------------------------------

const TONES: [Tone; 7] = [
    Tone::Text,
    Tone::Muted,
    Tone::Separator,
    Tone::Success,
    Tone::Warning,
    Tone::Error,
    Tone::Attention,
];

#[test]
fn tone_names_map_exactly_and_unknown_becomes_text() {
    let names = [
        "text",
        "muted",
        "separator",
        "success",
        "warning",
        "error",
        "attention",
    ];
    for (name, tone) in names.into_iter().zip(TONES) {
        assert_eq!(Tone::from_token(name), tone, "token {name}");
    }
    for unknown in [
        "",
        "red",
        "#ff0000",
        "<b>bold</b>",
        "\x1b[31m",
        "warning ",
        "info",
    ] {
        assert_eq!(Tone::from_token(unknown), Tone::Text, "token {unknown:?}");
    }
}

#[test]
fn palette_follows_tone_order_and_default_uses_terminal_colours() {
    let styles = [
        Style::new().fg(Color::Red),
        Style::new().fg(Color::Green),
        Style::new().fg(Color::Blue),
        Style::new().fg(Color::Cyan),
        Style::new().fg(Color::Magenta),
        Style::new().fg(Color::White),
        Style::new().fg(Color::LightRed),
    ];
    let palette = StatusPalette::from_styles(styles);
    for (tone, style) in TONES.into_iter().zip(styles) {
        assert_eq!(palette.style(tone), style, "{tone:?}");
    }

    // The widget uses the palette it is given.
    let inputs = StatusInputs {
        needs_you: 2,
        ..base()
    };
    let section = StatusSection::new(&inputs, None, &palette);
    let mut terminal = Terminal::new(TestBackend::new(40, 1)).expect("test terminal");
    terminal
        .draw(|frame| section.render(frame.area(), frame.buffer_mut()))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let row = row_symbols(buffer);
    assert_eq!(row.trim_end(), FULL_13);
    let flag = row.chars().position(|c| c == '⚑').expect("flag");
    assert_eq!(
        buffer[(u16::try_from(flag).expect("column"), 0)].fg,
        Color::LightRed
    );

    // Defaults: the terminal's own colours, named ANSI colours and modifiers only.
    let defaults = StatusPalette::default();
    assert_eq!(defaults.style(Tone::Text).fg, None);
    assert!(
        defaults
            .style(Tone::Muted)
            .add_modifier
            .contains(Modifier::DIM)
    );
    assert!(
        defaults
            .style(Tone::Separator)
            .add_modifier
            .contains(Modifier::DIM)
    );
    assert_eq!(defaults.style(Tone::Success).fg, Some(Color::Green));
    assert_eq!(defaults.style(Tone::Warning).fg, Some(Color::Yellow));
    assert_eq!(defaults.style(Tone::Error).fg, Some(Color::Red));
    assert_eq!(defaults.style(Tone::Attention).fg, Some(Color::Yellow));
    assert!(
        defaults
            .style(Tone::Attention)
            .add_modifier
            .contains(Modifier::BOLD)
    );
}
