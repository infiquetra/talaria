use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Rect};
use talaria_core::memory::{
    CardView, ConfirmView, CuratorRowView, FieldRowView, FollowView, MemoryEffect, MemoryInput,
    MemoryKey, MemoryState, MemoryView, RowView, Section,
};
use talaria_tui::keys::{Key, KeyChord, KeyContext, KeyMap};
use talaria_tui::memory::{draw_memory, route_memory};

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn render(view: &MemoryView, width: u16, height: u16) -> String {
    let sink = Sink::default();
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(sink.clone()),
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, width, height)),
        },
    )
    .unwrap();
    terminal
        .draw(|frame| draw_memory(view, frame.area(), frame.buffer_mut()))
        .unwrap();
    let mut parser = vt100::Parser::new(height, width, 0);
    parser.process(&sink.0.lock().unwrap());
    parser.screen().contents()
}

// Compare full words across wrapped lines without prescribing padding or placement.
fn assert_text(screen: &str, expected: &str) {
    let actual = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    let expected = expected.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        actual.contains(&expected),
        "missing {expected:?} in {screen:?}"
    );
}

fn row(marker: char, name: &str, description: &str, label: &str) -> RowView {
    RowView {
        marker,
        name: name.into(),
        description: description.into(),
        label: label.into(),
    }
}

fn view() -> MemoryView {
    MemoryView {
        title: "Memory · profile fixture".into(),
        banner: None,
        providers: Section::Ready(vec![
            row('●', "built-in only", "Hermes's own MEMORY.md and USER.md", "in use"),
            row('○', "byterover", "ByteRover — persistent knowledge tree", "not available here"),
            row('○', "retaindb", "RetainDB — cloud memory API", "needs settings"),
        ]),
        active_text: "active: built-in only".into(),
        files: Section::Ready(vec![
            row(' ', "Agent memory (MEMORY.md)", "", "1.2 KB"),
            row(' ', "User profile (USER.md)", "", "empty"),
        ]),
        curator: Section::Ready(CuratorRowView {
            status: "active".into(),
            schedule: "Runs every 7 days after 2 h idle. Marks unused skills stale after 14 days, archives after 30.".into(),
            last_run: "Last run or first check: 2026-10-20 14:03 UTC".into(),
        }),
        cursor: 0,
        card: None,
        confirm: None,
        following: None,
        editing: false,
        footer: vec![("enter".into(), "open".into()), ("u".into(), "use".into()), ("ctrl+r".into(), "reload".into()), ("esc".into(), "close".into())],
    }
}

fn card() -> CardView {
    CardView {
        name: "retaindb".into(),
        label: "needs settings".into(),
        description: "RetainDB — cloud memory API with hybrid search and 7 memory types.".into(),
        needs: vec!["Needs environment value: RETAINDB_API_KEY".into()],
        fields: vec![FieldRowView {
            label: "Api Key *".into(),
            value: "not set".into(),
            pending: false,
            selected: true,
        }],
        setup: Some("Set up outside Talaria: hermes -p fixture memory setup retaindb".into()),
        result_lines: vec![],
    }
}

fn key(chord: &str) -> Key {
    KeyChord::parse(chord, KeyContext::Memory).unwrap().to_key()
}

const SIZES: [(u16, u16); 3] = [(100, 30), (80, 24), (60, 16)];

#[test]
fn draws_every_screen() {
    let mut current = view();
    current.banner = Some("Loading memory settings for profile fixture…".into());
    current.providers = Section::Loading;
    current.files = Section::Loading;
    current.curator = Section::Loading;
    for (width, height) in SIZES {
        let screen = render(&current, width, height);
        assert_text(&screen, "Loading memory settings for profile fixture…");
    }

    current = view();
    current.curator =
        Section::Failed("Could not read curator status: unavailable. ctrl+r tries again.".into());
    for (width, height) in SIZES {
        let screen = render(&current, width, height);
        assert_text(&screen, "built-in only");
        assert_text(
            &screen,
            "Could not read curator status: unavailable. ctrl+r tries again.",
        );
    }

    current = view();
    for (width, height) in SIZES {
        let screen = render(&current, width, height);
        for text in [
            "Memory · profile fixture",
            "MEMORY PROVIDER",
            "active: built-in only",
            "built-in only",
            "byterover",
            "not available here",
            "retaindb",
            "needs settings",
            "BUILT-IN MEMORY",
            "Agent memory (MEMORY.md)",
            "User profile (USER.md)",
            "SKILL CURATOR",
        ] {
            assert_text(&screen, text);
        }
        let provider_row = screen
            .lines()
            .find(|line| line.contains("byterover"))
            .unwrap();
        if width == 100 {
            assert!(provider_row.contains("persistent knowledge tree"));
        } else {
            assert!(!screen.contains("persistent knowledge tree"));
            if width < 80 {
                assert!(!provider_row.contains("not available here"));
                let lines: Vec<_> = screen.lines().collect();
                let row = lines
                    .iter()
                    .position(|line| line.contains("byterover"))
                    .unwrap();
                assert!(lines.get(row + 1).unwrap().contains("not available here"));
            } else {
                assert!(provider_row.contains("not available here"));
            }
        }
    }

    current.card = Some(card());
    for (width, height) in SIZES {
        let screen = render(&current, width, height);
        for text in [
            "retaindb",
            "needs settings",
            "Needs environment value: RETAINDB_API_KEY",
            "Api Key *",
            "not set",
            "Set up outside Talaria: hermes -p fixture memory setup retaindb",
        ] {
            assert_text(&screen, text);
        }
    }
    current.editing = true;
    current.card.as_mut().unwrap().fields[0].value = "•••••••• (new)".into();
    for (width, height) in SIZES {
        let screen = render(&current, width, height);
        assert_text(&screen, "•••••••• (new)");
        assert!(!screen.contains("secret-sentinel"));
    }
    current.editing = false;
    current.card.as_mut().unwrap().result_lines =
        vec!["Api Key: stored".into(), "Project: not saved".into()];
    for (width, height) in SIZES {
        let screen = render(&current, width, height);
        assert_text(&screen, "Api Key: stored");
        assert_text(&screen, "Project: not saved");
    }

    let confirms: [(&[&str], &str); 5] = [
        (
            &["Switch the memory provider for profile fixture from built-in only to retaindb?"],
            "Switch",
        ),
        (
            &[
                "Saving also makes retaindb the memory provider for profile fixture (now built-in only).",
                "Hermes also stores the default of every setting you left unset.",
            ],
            "Save and use",
        ),
        (
            &[
                "Delete MEMORY.md for profile fixture? This cannot be undone.",
                "Only Hermes's built-in memory is deleted; data held by retaindb is not touched.",
            ],
            "Delete",
        ),
        (
            &[
                "Run the skill curator for profile fixture now?",
                "It marks unused skills stale and archives old ones; hermes -p fixture curator restore <skill> brings one back. It runs even while paused. With curator.consolidate on, it also merges and rewrites skills using the auxiliary model.",
            ],
            "Run",
        ),
        (&["Discard 1 unsaved change(s) to retaindb?"], "Discard"),
    ];
    for (lines, button) in confirms {
        current.confirm = Some(ConfirmView {
            lines: lines.iter().map(|line| (*line).into()).collect(),
            buttons: ["Cancel".into(), button.into()],
            selected: 0,
        });
        for (width, height) in SIZES {
            let screen = render(&current, width, height);
            for line in lines {
                assert_text(&screen, line);
            }
            assert_text(&screen, "Cancel");
            assert_text(&screen, button);
        }
    }
    current.confirm = None;
    current.card = None;
    current.following = Some(FollowView {
        elapsed_s: 12,
        lines: vec!["checking skills".into()],
    });
    for (width, height) in SIZES {
        let screen = render(&current, width, height);
        assert_text(&screen, "Curator running · 12 s");
        assert_text(&screen, "checking skills");
    }
    current.following = None;
    for banner in [
        "Curator run finished.",
        "Curator run failed (exit 1).",
        "Deleted MEMORY.md for profile fixture.",
    ] {
        current.banner = Some(banner.into());
        for (width, height) in SIZES {
            assert_text(&render(&current, width, height), banner);
        }
    }
    let too_small = render(&current, 59, 16);
    assert_text(
        &too_small,
        "Make the terminal at least 60 columns wide and 16 rows tall to show memory settings.",
    );
    assert!(!too_small.contains("MEMORY PROVIDER"));
}

#[test]
fn escape_closes_overlay() {
    let now = std::time::Instant::now();
    let mut state = MemoryState::new();
    state.update(
        MemoryInput::Open {
            profile: "fixture".into(),
        },
        now,
    );
    let key = route_memory(&state.view(now), &KeyMap::defaults(), &key("esc")).unwrap();
    let effects = state.update(MemoryInput::Key(key), now);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, MemoryEffect::CloseOverlay))
    );
}

#[test]
fn keys_resolve_through_memory_context() {
    let current = view();
    let map = KeyMap::defaults();
    for (chord, expected) in [
        ("up", MemoryKey::Up),
        ("down", MemoryKey::Down),
        ("enter", MemoryKey::Open),
        ("u", MemoryKey::Use),
        ("d", MemoryKey::Delete),
        ("D", MemoryKey::DeleteBoth),
        ("p", MemoryKey::Pause),
        ("R", MemoryKey::Run),
        ("s", MemoryKey::Settings),
        ("k", MemoryKey::CuratorSettings),
        ("ctrl+r", MemoryKey::Reload),
        ("c", MemoryKey::Copy),
        ("ctrl+s", MemoryKey::Save),
        ("esc", MemoryKey::Close),
    ] {
        assert_eq!(
            route_memory(&current, &map, &key(chord)),
            Some(expected),
            "{chord}"
        );
    }
    let mut entries = BTreeMap::new();
    entries.insert("memory_down".into(), vec!["j".into()]);
    let (rebound, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(
        route_memory(&current, &rebound, &key("j")),
        Some(MemoryKey::Down)
    );
    assert_eq!(route_memory(&current, &rebound, &key("down")), None);
    let mut editing = view();
    editing.editing = true;
    assert_eq!(
        route_memory(&editing, &rebound, &key("j")),
        Some(MemoryKey::Char('j'))
    );
    let mut confirming = view();
    confirming.confirm = Some(ConfirmView {
        lines: vec!["Confirm?".into()],
        buttons: ["Cancel".into(), "Delete".into()],
        selected: 0,
    });
    for chord in ["u", "d", "D", "p", "R", "s", "k", "ctrl+r", "ctrl+s"] {
        assert_eq!(
            route_memory(&confirming, &map, &key(chord)),
            None,
            "{chord}"
        );
    }
    assert_eq!(
        route_memory(&confirming, &map, &key("right")),
        Some(MemoryKey::Right)
    );
}

#[test]
fn hermes_text_is_sanitised() {
    let mut current = view();
    if let Section::Ready(rows) = &mut current.providers {
        rows[1].description = "bad\u{1b}[31mred\u{202e}text".into();
    }
    let screen = render(&current, 100, 30);
    assert!(screen.contains("␛[31mred‹U+202E›text"), "{screen}");
    assert!(!screen.contains('\u{1b}'));
    assert!(!screen.contains('\u{202e}'));
}
