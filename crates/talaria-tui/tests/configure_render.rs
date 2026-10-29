mod support;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use support::{ByteSink, Vt, probe};
use talaria_core::configure::{ConfigureKey, ConfigureView, Phase, ViewLine};
use talaria_tui::configure::{draw_configure, route_configure};
use talaria_tui::keys::{Key, KeyChord, KeyContext, KeyMap};
use talaria_tui::terminal::{InlineTerminal, RestoreLink};

fn view(phase: Phase, lines: &[&str], footer: &str) -> ConfigureView {
    ConfigureView {
        title: "Configure · profile fixture · provider/model".into(),
        phase,
        lines: lines
            .iter()
            .map(|line| ViewLine {
                text: (*line).into(),
                row: None,
            })
            .collect(),
        confirmation: None,
        footer: footer.into(),
        selected_row: None,
    }
}

fn rig(cols: u16, rows: u16) -> (InlineTerminal<ByteSink>, ByteSink, Vt) {
    let sink = ByteSink::default();
    let mut terminal = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        2,
        probe(false, false, Some((0, rows - 1))),
        RestoreLink::detached(),
    )
    .unwrap();
    let mut vt = Vt::new(cols, rows);
    vt.absorb(&sink);
    terminal.open_overlay().unwrap();
    vt.absorb(&sink);
    (terminal, sink, vt)
}

fn paint(
    terminal: &mut InlineTerminal<ByteSink>,
    sink: &ByteSink,
    vt: &mut Vt,
    view: &ConfigureView,
) -> String {
    terminal
        .draw_overlay(|area, buf| draw_configure(view, area, buf))
        .unwrap();
    vt.absorb(sink);
    vt.rows().join("\n")
}

#[test]
fn phase_snapshots() {
    for (cols, rows) in [(120, 40), (80, 24)] {
        let (mut terminal, sink, mut vt) = rig(cols, rows);
        let cases = [
            view(
                Phase::Gathering,
                &["Starting the configuration agent…"],
                "Esc close",
            ),
            view(
                Phase::Starting,
                &["Starting the configuration agent…"],
                "Esc close",
            ),
            view(
                Phase::Thinking,
                &[
                    "Checking the current settings.",
                    "Asking the configuration agent… 1s",
                ],
                "Esc cancel",
            ),
            view(
                Phase::Question,
                &["Which setting?", "1. Bell", "2. Display"],
                "Choose an answer",
            ),
            view(
                Phase::Review,
                &[
                    "Proposed changes",
                    "[x] set display.bell_on_complete: false → true · may need a restart",
                    "[ ] ! remove MCP server alpha",
                    "  = display.compact",
                    "  ✗ invalid · unknown key",
                    "  → 1 next: open providers (/providers)",
                ],
                "Space select · a apply · d detail · r refine · Esc close",
            ),
            view(
                Phase::Confirming,
                &["Apply 1 change(s) to profile fixture? y apply · n back"],
                "y apply · n back",
            ),
            view(
                Phase::Applying,
                &[
                    "… set display.bell_on_complete",
                    "Applying… wait for it to finish.",
                ],
                "Applying",
            ),
            view(
                Phase::Done,
                &[
                    "Configured profile fixture: 1 applied, 1 failed, 1 unknown, 1 not attempted.",
                    "✓ applied",
                    "~ adjusted",
                    "= no change",
                    "✗ failed",
                    "! conflict",
                    "? unknown",
                    "- not attempted",
                    "Next steps:",
                    "1 next: open providers (/providers)",
                ],
                "Enter close",
            ),
            view(
                Phase::Answer,
                &["The requested setting is already enabled."],
                "r refine · Esc close",
            ),
            view(
                Phase::Cannot,
                &["The request cannot be completed."],
                "r refine · Esc close",
            ),
            view(
                Phase::NoProposal,
                &["No configuration proposal was returned."],
                "r refine · Esc close",
            ),
            view(
                Phase::Failed,
                &["The configuration agent failed."],
                "Esc close",
            ),
            view(
                Phase::Cancelled,
                &["Cancelled. Nothing was changed."],
                "Esc close",
            ),
            view(
                Phase::Closing,
                &["Closing the configuration agent…"],
                "Esc close",
            ),
        ];
        for snapshot in &cases {
            let screen = paint(&mut terminal, &sink, &mut vt, snapshot);
            assert!(screen.contains(&snapshot.title), "{cols}x{rows}: {screen}");
            for line in &snapshot.lines {
                assert!(
                    screen.contains(&line.text),
                    "missing {:?} at {cols}x{rows}: {screen}",
                    line.text
                );
            }
            assert!(screen.contains(&snapshot.footer), "{cols}x{rows}: {screen}");
        }
        let mut many = view(Phase::Review, &[], "Space select · Esc close");
        many.lines = (0..40)
            .map(|row| ViewLine {
                text: format!("[x] setting {row}"),
                row: Some(row),
            })
            .collect();
        many.selected_row = Some(39);
        let screen = paint(&mut terminal, &sink, &mut vt, &many);
        assert!(screen.contains("setting 39"), "{cols}x{rows}: {screen}");
        assert!(screen.contains(&many.footer), "{cols}x{rows}: {screen}");

        let hostile = view(
            Phase::Answer,
            &["safe \u{1b} text \u{202e} end"],
            "Esc close",
        );
        let screen = paint(&mut terminal, &sink, &mut vt, &hostile);
        assert!(!screen.contains('\u{1b}'));
        assert!(!screen.contains('\u{202e}'));
    }
}

#[test]
fn confirmations_stay_visible_over_a_full_proposal() {
    for (cols, rows) in [(80, 24), (40, 12)] {
        let (mut terminal, sink, mut vt) = rig(cols, rows);
        for selected in [0, 19] {
            for (phase, prompt, footer, words) in [
                (
                    Phase::Confirming,
                    "Apply 20 change(s), 5 removal(s)?",
                    "y apply · n back",
                    ["20 change(s)", "5 removal(s)"],
                ),
                (
                    Phase::Applying,
                    "Change main model?",
                    "y apply · n skip",
                    ["Change main model?", "y apply"],
                ),
                (
                    Phase::Review,
                    "Discard this proposal?",
                    "y discard · n back",
                    ["Discard this proposal?", "y discard"],
                ),
            ] {
                let mut snapshot = view(phase, &["Summary", "Details"], footer);
                snapshot.lines.extend((0..20).map(|row| ViewLine {
                    text: format!("[x] setting {row}"),
                    row: Some(row),
                }));
                snapshot.confirmation = Some(prompt.into());
                snapshot.selected_row = Some(selected);
                let screen = paint(&mut terminal, &sink, &mut vt, &snapshot);
                for word in words {
                    assert!(
                        screen.contains(word),
                        "{cols}x{rows}: missing {word:?}: {screen}"
                    );
                }
                assert!(screen.contains(footer), "{screen}");
            }
        }
    }
}

fn key(chord: &str) -> Key {
    KeyChord::parse(chord, KeyContext::Configure)
        .unwrap()
        .to_key()
}

#[test]
fn handoff_numbers_survive_conflicts_in_default_and_rebound_keys() {
    for (blocked, surviving) in [("1", "2"), ("4", "5")] {
        let mut entries = BTreeMap::new();
        entries.insert("configure_apply".to_owned(), vec![blocked.to_owned()]);
        let (map, _) = KeyMap::from_entries(&entries);
        let number = surviving.chars().next().unwrap();
        assert_eq!(
            route_configure(&map, &key(surviving)),
            Some(ConfigureKey::Char(number))
        );
    }
    let mut entries = BTreeMap::new();
    entries.insert(
        "configure_open_next".to_owned(),
        vec!["q".into(), "w".into(), "e".into()],
    );
    entries.insert("configure_apply".to_owned(), vec!["w".into()]);
    let (map, _) = KeyMap::from_entries(&entries);
    assert_eq!(
        route_configure(&map, &key("e")),
        Some(ConfigureKey::Char('3'))
    );
}

#[test]
fn keys_layout_first_frame_and_idle() {
    let map = KeyMap::defaults();
    for (chord, expected) in [
        ("up", ConfigureKey::Up),
        ("down", ConfigureKey::Down),
        ("pageup", ConfigureKey::PageUp),
        ("pagedown", ConfigureKey::PageDown),
        ("home", ConfigureKey::Home),
        ("end", ConfigureKey::End),
        ("space", ConfigureKey::Space),
        ("enter", ConfigureKey::Enter),
        ("esc", ConfigureKey::Esc),
        ("a", ConfigureKey::Char('a')),
        ("d", ConfigureKey::Char('d')),
        ("r", ConfigureKey::Char('r')),
        ("y", ConfigureKey::Char('y')),
        ("n", ConfigureKey::Char('n')),
        ("1", ConfigureKey::Char('1')),
        ("9", ConfigureKey::Char('9')),
    ] {
        assert_eq!(
            route_configure(&map, &key(chord)),
            Some(expected),
            "{chord}"
        );
    }
    let mut entries = BTreeMap::new();
    entries.insert("configure_down".to_owned(), vec!["j".to_owned()]);
    let (rebound, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(
        route_configure(&rebound, &key("j")),
        Some(ConfigureKey::Down)
    );
    assert_ne!(
        route_configure(&rebound, &key("down")),
        Some(ConfigureKey::Down)
    );

    let (mut terminal, sink, mut vt) = rig(120, 40);
    let start = Instant::now();
    let gathering = view(
        Phase::Gathering,
        &["Starting the configuration agent…"],
        "Esc close",
    );
    let screen = paint(&mut terminal, &sink, &mut vt, &gathering);
    assert!(screen.contains("Starting the configuration agent…"));
    assert!(start.elapsed() <= Duration::from_millis(50));
    let stats = terminal
        .draw_overlay(|area, buf| draw_configure(&gathering, area, buf))
        .unwrap();
    assert!(!stats.drew);
    assert_eq!(stats.bytes, 0);
    assert!(sink.is_empty());

    for phase in [Phase::Thinking, Phase::Applying] {
        let first = view(phase, &["Elapsed 1s"], "Esc close");
        let screen = paint(&mut terminal, &sink, &mut vt, &first);
        assert!(screen.contains("Elapsed 1s"));
        let stats = terminal
            .draw_overlay(|area, buf| draw_configure(&first, area, buf))
            .unwrap();
        assert!(!stats.drew);
        let mut next = first;
        next.lines[0].text = "Elapsed 2s".into();
        let screen = paint(&mut terminal, &sink, &mut vt, &next);
        assert!(screen.contains("Elapsed 2s"));
        assert!(!screen.contains("Elapsed 1s"));
    }
}
