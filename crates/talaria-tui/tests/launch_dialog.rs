//! The background-launch dialog. Expected frames are the layout rules, written by hand.
//! Step 0 leaves the buffer empty, so the frame compare fails.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_tui::keys::{KeyChord, KeyContext};
use talaria_tui::launch_dialog::{
    DialogAction, DialogDefaults, DialogMode, DialogSubmit, LaunchDialog,
};

const WORKTREE_76: &str = "\
╭─ New background session in a worktree ───────────────────────────────────╮
│ Prompt   › fix the flaky test in tests/net.rs                            │
│ Profile    fixture                                         ← → to change │
│ Folder     ~/proj                                                        │
│ Model      profile default                                               │
│ Worktree   fix-the-flaky-test-in-tests-net-rs                            │
│                                                                          │
│ enter launch · tab next · shift+tab back · esc close                     │
╰──────────────────────────────────────────────────────────────────────────╯";

const WORKTREE_120: &str = "\
╭─ New background session in a worktree ───────────────────────────────────────────────────────────────────────────────╮
│ Prompt   › fix the flaky test in tests/net.rs                                                                        │
│ Profile    fixture                                                                                     ← → to change │
│ Folder     ~/proj                                                                                                    │
│ Model      profile default                                                                                           │
│ Worktree   fix-the-flaky-test-in-tests-net-rs                                                                        │
│                                                                                                                      │
│ enter launch · tab next · shift+tab back · esc close                                                                 │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯";

const PLAIN_76: &str = "\
╭─ New background session ─────────────────────────────────────────────────╮
│ Prompt   › fix the flaky test in tests/net.rs                            │
│ Profile    fixture                                         ← → to change │
│ Folder     ~/proj                                                        │
│ Model      profile default                                               │
│                                                                          │
│ enter launch · tab next · shift+tab back · esc close                     │
╰──────────────────────────────────────────────────────────────────────────╯";

fn slug(text: &str) -> String {
    if text == "fix the flaky test in tests/net.rs" {
        "fix-the-flaky-test-in-tests-net-rs".to_owned()
    } else {
        String::new()
    }
}

fn defaults() -> DialogDefaults {
    DialogDefaults {
        profiles: vec!["fixture".to_owned(), "default".to_owned()],
        profile: "fixture".to_owned(),
        profile_note: None,
        folder: PathBuf::from("~/proj"),
        draft: Some("fix the flaky test in tests/net.rs".to_owned()),
        model_placeholder: "profile default".to_owned(),
        slug,
    }
}

fn chord(text: &str) -> KeyChord {
    KeyChord::parse(text, KeyContext::Fleet).unwrap_or_else(|_| panic!("chord {text}"))
}

fn frame_text(buffer: &Buffer, area: Rect) -> String {
    let mut lines = Vec::new();
    for y in area.y..area.y.saturating_add(area.height) {
        let mut line = String::new();
        let mut x = area.x;
        while x < area.x.saturating_add(area.width) {
            line.push_str(buffer[(x, y)].symbol());
            x = x.saturating_add(1);
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn vt100_text(text: &str, cols: u16, rows: u16) -> String {
    let mut parser = vt100::Parser::new(rows, cols, 0);
    let mut bytes = Vec::new();
    for line in text.split('\n') {
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\r');
        bytes.push(b'\n');
    }
    parser.process(&bytes);
    let mut lines = Vec::new();
    for row in parser.screen().rows(0, cols) {
        lines.push(row.trim_end().to_owned());
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn assert_frame(dialog: &LaunchDialog, screen: Rect, expected: &str) {
    let mut buffer = Buffer::empty(screen);
    let area = dialog.area(screen);
    dialog.draw(area, &mut buffer);
    let got = frame_text(&buffer, area);
    let _screen = vt100_text(&got, area.width, area.height);
    assert_eq!(got, expected, "empty buffer");
}

#[test]
fn renders_at_80_and_160() {
    let dialog = LaunchDialog::open(DialogMode::Worktree, defaults());
    let narrow = Rect::new(0, 0, 80, 24);
    assert_frame(&dialog, narrow, WORKTREE_76);
    let wide = Rect::new(0, 0, 160, 48);
    assert_frame(&dialog, wide, WORKTREE_120);
    assert_eq!(
        dialog.area(narrow),
        Rect {
            x: 2,
            y: 2,
            width: 76,
            height: 9
        }
    );
    assert_eq!(
        dialog.area(wide),
        Rect {
            x: 20,
            y: 2,
            width: 120,
            height: 9
        }
    );
}

#[test]
fn keys_edit_cycle_launch_and_close() {
    let mut dialog = LaunchDialog::open(DialogMode::Plain, defaults());
    assert_frame(&dialog, Rect::new(0, 0, 80, 24), PLAIN_76);
    assert_eq!(dialog.handle_key(chord("tab")), DialogAction::None);
    assert_eq!(dialog.handle_key(chord("shift+tab")), DialogAction::None);
    let launched = dialog.handle_key(chord("enter"));
    assert_eq!(
        launched,
        DialogAction::Launch(DialogSubmit {
            prompt: "fix the flaky test in tests/net.rs".to_owned(),
            profile: "fixture".to_owned(),
            folder: "~/proj".to_owned(),
            model: None,
            worktree: false,
            worktree_name: None,
        })
    );
    let closed = dialog.handle_key(chord("esc"));
    assert!(matches!(closed, DialogAction::Close { .. }));
}

#[test]
fn invalid_busy_launching_failed_states() {
    let screen = Rect::new(0, 0, 80, 24);
    let mut dialog = LaunchDialog::open(DialogMode::Worktree, defaults());
    dialog.show_invalid("Write a prompt first.");
    assert_frame_contains(&dialog, screen, "Write a prompt first.");
    dialog.show_invalid("The folder does not exist: repo/missing.");
    assert_frame_contains(&dialog, screen, "The folder does not exist: repo/missing.");
    dialog.show_invalid("The folder must be an absolute path or start with ~.");
    assert_frame_contains(
        &dialog,
        screen,
        "The folder must be an absolute path or start with ~.",
    );
    dialog.show_invalid("The worktree name needs at least one ASCII letter or digit.");
    assert_frame_contains(
        &dialog,
        screen,
        "The worktree name needs at least one ASCII letter or digit.",
    );
    dialog.show_invalid(
        "Three launches from this Talaria are still starting; wait for one to finish.",
    );
    assert_frame_contains(
        &dialog,
        screen,
        "Three launches from this Talaria are still starting; wait for one to finish.",
    );
    dialog.show_error("the gateway refused.", "Check the options you passed.");
    assert_frame_contains(&dialog, screen, "Could not launch: the gateway refused.");
    assert_frame_contains(&dialog, screen, "Check the options you passed.");
    for stage in [
        "Connecting to Hermes…",
        "Fetching origin…",
        "Creating worktree…",
        "Starting session…",
        "Sending prompt…",
    ] {
        dialog.show_progress(stage);
        assert_frame_contains(&dialog, screen, stage);
    }
}

#[test]
fn untrusted_text_is_drawn_literally() {
    let raw = "hello \u{1b} there";
    let stand_in = talaria_tui::history::sanitize(raw);
    let mut values = defaults();
    values.draft = Some(raw.to_owned());
    let dialog = LaunchDialog::open(DialogMode::Plain, values);
    let screen = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(screen);
    let area = dialog.area(screen);
    dialog.draw(area, &mut buffer);
    let got = frame_text(&buffer, area);
    assert!(got.contains(stand_in.as_ref()), "empty buffer");
}

fn assert_frame_contains(dialog: &LaunchDialog, screen: Rect, needle: &str) {
    let mut buffer = Buffer::empty(screen);
    let area = dialog.area(screen);
    dialog.draw(area, &mut buffer);
    let got = frame_text(&buffer, area);
    let _screen = vt100_text(&got, area.width, area.height);
    assert!(got.contains(needle), "empty buffer, missing {needle}");
}
