//! Start-up and restore of the inline terminal: which modes it switches on, where it places the
//! viewport, that it undoes everything exactly once, that it never queries the terminal again and
//! that it creates no input thread without a terminal.

mod support;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ratatui::style::Style;
use support::{Alac, ByteSink, live_view_sections, probe, tagged};
use talaria_tui::live::{LiveSection, LiveView, PlainTextSection};
use talaria_tui::terminal::{
    InlineTerminal, RestoreLink, TerminalError, TerminalGuard, probe_startup, spawn_input,
};

/// Names the mode of the helper test that runs in a child process.
const CHILD_VARIABLE: &str = "TALARIA_TUI_CHILD";

/// Value of [`CHILD_VARIABLE`] that makes the helper test do its work.
const CHILD_MODE: &str = "not_a_terminal";

/// Printed by the child when every check passed.
const CHILD_MARKER: &str = "NOT_A_TERMINAL_OK";

/// How many times `needle` occurs in `haystack`.
fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

/// Position of the first `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A live view that borrows the given sections.
fn view_of(sections: &[PlainTextSection]) -> LiveView<'_> {
    LiveView {
        sections: sections
            .iter()
            .map(|section| section as &dyn LiveSection)
            .collect(),
    }
}

/// Printable form of some bytes for failure messages.
fn shown(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace('\x1b', "ESC")
}

#[test]
fn setup_enables_paste_and_never_mouse() {
    let mouse_modes: [&[u8]; 6] = [
        b"\x1b[?1000h",
        b"\x1b[?1002h",
        b"\x1b[?1003h",
        b"\x1b[?1005h",
        b"\x1b[?1006h",
        b"\x1b[?1015h",
    ];
    let probes = [
        probe(false, false, Some((0, 23))),
        probe(true, true, Some((0, 23))),
    ];
    for reply in probes {
        let sink = ByteSink::default();
        let mut term =
            InlineTerminal::new(sink.clone(), (80, 24), 2, reply, RestoreLink::detached())
                .expect("new");
        let setup = sink.take();
        assert_eq!(
            count(&setup, b"\x1b[?2004h"),
            1,
            "bracketed paste is switched on once: {}",
            shown(&setup)
        );
        assert_eq!(
            count(&setup, b"\x1b[?25l"),
            1,
            "the cursor is hidden once: {}",
            shown(&setup)
        );

        // The mouse stays off through a whole session, not only at start-up.
        let sections = live_view_sections(2);
        let view = view_of(&sections);
        term.draw_live(&view).expect("draw");
        term.commit_lines(tagged(3)).expect("commit");
        term.resize(80, 30).expect("resize");
        term.draw_live(&view).expect("draw after resize");
        term.open_overlay().expect("open overlay");
        term.draw_overlay(|_, buf| {
            buf.set_string(0, 0, "OVERLAY", Style::default());
        })
        .expect("draw overlay");
        term.close_overlay().expect("close overlay");
        term.draw_live(&view).expect("draw after close");
        term.restore().expect("restore");

        let mut all = setup;
        all.extend(sink.take());
        for mode in mouse_modes {
            assert_eq!(
                count(&all, mode),
                0,
                "mouse mode {} must never be written: {}",
                shown(mode),
                shown(&all)
            );
        }
    }
}

#[test]
fn keyboard_flags_pushed_only_when_supported() {
    for keyboard in [true, false] {
        for sync in [true, false] {
            let sink = ByteSink::default();
            let mut term = InlineTerminal::new(
                sink.clone(),
                (80, 24),
                2,
                probe(sync, keyboard, Some((0, 23))),
                RestoreLink::detached(),
            )
            .expect("new");
            let setup = sink.take();
            let pushed = count(&setup, b"\x1b[>1u");
            assert_eq!(
                pushed,
                usize::from(keyboard),
                "keyboard {keyboard}, sync {sync}: push count after new: {}",
                shown(&setup)
            );
            assert_eq!(
                count(&setup, b"\x1b[<1u"),
                0,
                "nothing is popped before restore: {}",
                shown(&setup)
            );

            term.restore().expect("restore");
            let restored = sink.take();
            assert_eq!(
                count(&restored, b"\x1b[<1u"),
                usize::from(keyboard),
                "keyboard {keyboard}, sync {sync}: pop count on restore: {}",
                shown(&restored)
            );
            assert_eq!(
                count(&restored, b"\x1b[>1u"),
                0,
                "restore does not push: {}",
                shown(&restored)
            );

            term.restore().expect("second restore");
            assert!(
                sink.is_empty(),
                "a second restore writes nothing, got {}",
                shown(&sink.take())
            );
        }
    }
}

#[test]
fn restore_undoes_every_mode_once() {
    // Overlay open, both capabilities, viewport top 22: every undo in its place.
    let sink = ByteSink::default();
    let link = RestoreLink::detached();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(true, true, Some((0, 23))),
        link.clone(),
    )
    .expect("new");
    assert_eq!(term.viewport().y, 22, "the viewport is bottom anchored");
    term.open_overlay().expect("open overlay");
    sink.take();

    term.restore().expect("restore");
    let restored = sink.take();
    let with_overlay: &[u8] =
        b"\x1b[?1049l\x1b[r\x1b[23;1H\x1b[J\x1b[?2004l\x1b[<1u\x1b[0m\x1b[?25h";
    assert_eq!(
        shown(&restored),
        shown(with_overlay),
        "restore sequence with an overlay open"
    );

    term.restore().expect("second restore");
    assert!(
        sink.is_empty(),
        "a second restore writes nothing, got {}",
        shown(&sink.take())
    );
    let mut late = Vec::new();
    let wrote = link.restore_into(&mut late).expect("restore_into");
    assert!(!wrote, "restore_into after restore reports nothing written");
    assert!(late.is_empty(), "restore_into after restore writes nothing");

    // The other order: the link restores first, then the terminal has nothing left to do.
    let sink = ByteSink::default();
    let link = RestoreLink::detached();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(true, true, Some((0, 23))),
        link.clone(),
    )
    .expect("new");
    sink.take();
    let mut out = Vec::new();
    let wrote = link.restore_into(&mut out).expect("restore_into");
    assert!(wrote, "the first restore_into writes the sequence");
    let without_overlay: &[u8] = b"\x1b[r\x1b[23;1H\x1b[J\x1b[?2004l\x1b[<1u\x1b[0m\x1b[?25h";
    assert_eq!(
        shown(&out),
        shown(without_overlay),
        "restore sequence through the link"
    );
    assert!(
        sink.is_empty(),
        "restore_into writes only to the given writer"
    );
    term.restore().expect("restore after the link");
    assert!(
        sink.is_empty(),
        "restore after the link writes nothing, got {}",
        shown(&sink.take())
    );
    let mut again = Vec::new();
    assert!(
        !link.restore_into(&mut again).expect("restore_into twice"),
        "a second restore_into reports nothing written"
    );
    assert!(again.is_empty());

    // Without capabilities nothing is popped, and the viewport top follows the probe.
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(false, false, Some((0, 3))),
        RestoreLink::detached(),
    )
    .expect("new");
    sink.take();
    term.restore().expect("restore");
    let restored = sink.take();
    let plain: &[u8] = b"\x1b[r\x1b[4;1H\x1b[J\x1b[?2004l\x1b[0m\x1b[?25h";
    assert_eq!(
        shown(&restored),
        shown(plain),
        "restore sequence without capabilities"
    );

    // A screen that is too small never placed a viewport, so there is nothing to clear.
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 2),
        2,
        probe(false, true, Some((0, 1))),
        RestoreLink::detached(),
    )
    .expect("new");
    sink.take();
    term.restore().expect("restore");
    let restored = sink.take();
    let small: &[u8] = b"\x1b[r\x1b[?2004l\x1b[<1u\x1b[0m\x1b[?25h";
    assert_eq!(
        shown(&restored),
        shown(small),
        "restore sequence on a screen that is too small"
    );
}

/// Case name, where the emulator's cursor is, what the probe reports and the expected top.
type Placement = (&'static str, (u16, u16), Option<(u16, u16)>, u16);

#[test]
fn viewport_placed_from_probe_or_at_bottom() {
    let cases: [Placement; 5] = [
        ("cursor mid line", (5, 3), Some((5, 3)), 4),
        ("cursor at column 0", (0, 3), Some((0, 3)), 3),
        ("column 0 on the last row", (0, 23), Some((0, 23)), 22),
        ("mid line on the last row", (5, 23), Some((5, 23)), 22),
        ("cursor unknown", (0, 3), None, 22),
    ];
    for (name, emulator_cursor, probe_cursor, top) in cases {
        let mut alac = Alac::with_shell_rows(80, 24, emulator_cursor);
        let sink = ByteSink::default();
        let term = InlineTerminal::new(
            sink.clone(),
            (80, 24),
            2,
            probe(false, false, probe_cursor),
            RestoreLink::detached(),
        )
        .expect("new");
        alac.absorb(&sink);

        let viewport = term.viewport();
        assert_eq!(viewport.y, top, "{name}: viewport top");
        assert_eq!(viewport.x, 0, "{name}: viewport left");
        assert_eq!(viewport.width, 80, "{name}: viewport width");
        assert_eq!(viewport.height, 2, "{name}: viewport height");

        let rows = alac.history_and_screen();
        for shell in ["shell 1", "shell 2", "shell 3"] {
            let seen = rows.iter().filter(|row| row.as_str() == shell).count();
            assert_eq!(
                seen, 1,
                "{name}: `{shell}` must be kept exactly once, rows: {rows:?}"
            );
        }
    }
}

#[test]
fn no_terminal_query_after_startup() {
    let queries: [&[u8]; 4] = [b"\x1b[6n", b"\x1b[?u", b"$p", b"\x1b[c"];
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (80, 24),
        2,
        probe(true, true, Some((0, 23))),
        RestoreLink::detached(),
    )
    .expect("new");

    let first = live_view_sections(2);
    let second = [
        PlainTextSection::new("changed"),
        PlainTextSection::new("rows"),
    ];
    term.draw_live(&view_of(&first)).expect("first draw");
    term.commit_lines(tagged(4)).expect("first commit");
    term.draw_live(&view_of(&second)).expect("second draw");
    term.commit_lines(tagged(2)).expect("second commit");
    term.draw_live(&view_of(&first)).expect("third draw");
    term.resize(100, 30).expect("resize");
    term.draw_live(&view_of(&first)).expect("draw after resize");
    term.open_overlay().expect("open overlay");
    term.draw_overlay(|_, buf| {
        buf.set_string(0, 0, "OVERLAY", Style::default());
    })
    .expect("draw overlay");
    term.commit_lines(tagged(1)).expect("commit during overlay");
    term.close_overlay().expect("close overlay");
    term.draw_live(&view_of(&first)).expect("draw after close");

    let bytes = sink.take();
    assert!(!bytes.is_empty(), "the session wrote something");
    for query in queries {
        assert_eq!(
            find(&bytes, query),
            None,
            "query {} must not be written after start-up: {}",
            shown(query),
            shown(&bytes)
        );
    }
}

#[test]
fn input_thread_not_created_without_a_terminal() {
    let exe = std::env::current_exe().expect("current executable");
    let output = Command::new(exe)
        .args([
            "--exact",
            "not_a_terminal_child",
            "--ignored",
            "--nocapture",
        ])
        .env(CHILD_VARIABLE, CHILD_MODE)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn the child test process");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "child exited with {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    assert!(
        stdout.contains(CHILD_MARKER),
        "child did not print its marker\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        !output.stdout.contains(&0x1b),
        "the child wrote an escape byte to a stream that is not a terminal: {}",
        shown(&output.stdout)
    );
}

/// Runs in a child process with standard input closed. It does nothing unless the parent test
/// asked for it, so a plain run with `--ignored` stays harmless.
#[test]
#[ignore = "runs only as the child of input_thread_not_created_without_a_terminal"]
fn not_a_terminal_child() {
    if std::env::var(CHILD_VARIABLE).as_deref() != Ok(CHILD_MODE) {
        return;
    }

    assert!(
        matches!(TerminalGuard::acquire(), Err(TerminalError::NotATerminal)),
        "acquire without a terminal must say NotATerminal"
    );

    let started = Instant::now();
    let probed = probe_startup(Duration::from_millis(200));
    let elapsed = started.elapsed();
    assert!(
        matches!(probed, Err(TerminalError::NotATerminal)),
        "probe_startup without a terminal must say NotATerminal"
    );
    assert!(
        elapsed < Duration::from_millis(50),
        "probe_startup must give up at once without a terminal, took {elapsed:?}"
    );

    assert!(
        spawn_input(|_| false).is_none(),
        "no input thread without a terminal"
    );

    println!("{CHILD_MARKER}");
}
