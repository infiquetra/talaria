//! The start-up probe: how a reply is parsed, and how it is read from a descriptor.
//!
//! The parse tests feed byte strings and compare with literal expected answers. The read test
//! drives the reader through a pipe, which is the only place these tests depend on time.

mod support;

use std::os::fd::OwnedFd;
use std::thread;
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::write;
use rustix::pipe::pipe;
use support::probe;
use talaria_tui::terminal::{ProbeReply, parse_probe_reply, read_probe_reply};

/// Cursor report for row 24, column 1, which is (0, 23) when 0-based.
const CURSOR_BOTTOM_LEFT: &[u8] = b"\x1b[24;1R";
/// Keyboard flags reply: the terminal has the kitty protocol.
const KEYBOARD_SUPPORTED: &[u8] = b"\x1b[?1u";
/// Synchronized output mode report: mode 2026 is set.
const SYNC_SET: &[u8] = b"\x1b[?2026;1$y";
/// Device attributes reply with a question mark, the end marker.
const MARKER_WITH_QUESTION: &[u8] = b"\x1b[?62;22c";

/// Join byte strings into one reply.
fn join(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn nothing() -> ProbeReply {
    ProbeReply::default()
}

#[test]
fn parse_probe_reply_reads_every_answer() {
    // The same answers in three different orders, the end marker last.
    let everything = probe(true, true, Some((0, 23)));
    let orders: [Vec<u8>; 3] = [
        join(&[
            CURSOR_BOTTOM_LEFT,
            KEYBOARD_SUPPORTED,
            SYNC_SET,
            MARKER_WITH_QUESTION,
        ]),
        join(&[
            SYNC_SET,
            KEYBOARD_SUPPORTED,
            CURSOR_BOTTOM_LEFT,
            MARKER_WITH_QUESTION,
        ]),
        join(&[
            KEYBOARD_SUPPORTED,
            CURSOR_BOTTOM_LEFT,
            SYNC_SET,
            MARKER_WITH_QUESTION,
        ]),
    ];
    for (index, bytes) in orders.iter().enumerate() {
        assert_eq!(
            parse_probe_reply(bytes),
            (everything, true),
            "order {index}: {bytes:?}"
        );
    }

    // Noise between and around the replies is ignored.
    let noisy = join(&[
        b"noise",
        CURSOR_BOTTOM_LEFT,
        b"\r\n~~",
        KEYBOARD_SUPPORTED,
        b"junk junk",
        SYNC_SET,
        b"xyz",
        MARKER_WITH_QUESTION,
    ]);
    assert_eq!(parse_probe_reply(&noisy), (everything, true));

    // The cursor report is 1-based on the wire and 0-based in the answer: column first.
    let cursor_cases: [(&[u8], (u16, u16)); 4] = [
        (b"\x1b[1;1R", (0, 0)),
        (b"\x1b[5;10R", (9, 4)),
        (b"\x1b[24;80R", (79, 23)),
        (b"\x1b[100;250R", (249, 99)),
    ];
    for (report, expected) in cursor_cases {
        let bytes = join(&[report, MARKER_WITH_QUESTION]);
        assert_eq!(
            parse_probe_reply(&bytes),
            (probe(false, false, Some(expected)), true),
            "cursor report {report:?}"
        );
    }

    // Synchronized output is supported for 1, 2 and 3 only; 0 and 4 mean it is not.
    for ps in 0..=4u8 {
        let report = format!("\x1b[?2026;{ps}$y");
        let bytes = join(&[report.as_bytes(), MARKER_WITH_QUESTION]);
        let expected = matches!(ps, 1..=3);
        assert_eq!(
            parse_probe_reply(&bytes),
            (probe(expected, false, None), true),
            "mode report {report:?}"
        );
    }

    // Any keyboard flags value, including zero, is a keyboard reply.
    for flags in [0u8, 1, 3, 31] {
        let report = format!("\x1b[?{flags}u");
        let bytes = join(&[report.as_bytes(), MARKER_WITH_QUESTION]);
        assert_eq!(
            parse_probe_reply(&bytes),
            (probe(false, true, None), true),
            "keyboard reply {report:?}"
        );
    }

    // The end marker is accepted with or without the question mark.
    for marker in [&b"\x1b[?62;22c"[..], b"\x1b[1;2c", b"\x1b[c", b"\x1b[?6c"] {
        assert_eq!(
            parse_probe_reply(marker),
            (nothing(), true),
            "marker {marker:?}"
        );
        let bytes = join(&[CURSOR_BOTTOM_LEFT, SYNC_SET, marker]);
        assert_eq!(
            parse_probe_reply(&bytes),
            (probe(true, false, Some((0, 23))), true),
            "replies then marker {marker:?}"
        );
    }
}

#[test]
fn parse_probe_reply_without_end_marker_means_unsupported() {
    // Without the end marker nothing counts: every answer is unsupported and the cursor unknown,
    // even when some replies did arrive.
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("cursor only", CURSOR_BOTTOM_LEFT.to_vec()),
        ("truncated keyboard reply", b"\x1b[?20".to_vec()),
        ("truncated marker", b"\x1b[?62;22".to_vec()),
        ("truncated mode report", b"\x1b[?2026;1$".to_vec()),
        (
            "all three answers, no marker",
            join(&[CURSOR_BOTTOM_LEFT, KEYBOARD_SUPPORTED, SYNC_SET]),
        ),
        ("only noise", b"hello world".to_vec()),
    ];
    for (name, bytes) in cases {
        assert_eq!(parse_probe_reply(&bytes), (nothing(), false), "{name}");
    }
}

/// The four replies a capable terminal sends, end marker last.
fn full_reply() -> Vec<u8> {
    join(&[
        CURSOR_BOTTOM_LEFT,
        KEYBOARD_SUPPORTED,
        SYNC_SET,
        MARKER_WITH_QUESTION,
    ])
}

fn write_all(fd: &OwnedFd, bytes: &[u8]) {
    let written = write(fd, bytes).expect("write to the pipe");
    assert_eq!(written, bytes.len(), "the pipe takes a short reply whole");
}

/// Whether the descriptor has bytes waiting right now.
fn has_bytes_waiting(fd: &OwnedFd) -> bool {
    let mut fds = [PollFd::new(fd, PollFlags::IN)];
    let zero = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    poll(&mut fds, Some(&zero)).expect("poll the pipe");
    fds[0].revents().contains(PollFlags::IN)
}

#[test]
fn read_probe_reply_stops_at_marker_or_deadline() {
    let deadline = Duration::from_millis(200);

    // The four replies are already waiting: the end marker ends the read at once. The write end
    // stays open, so only the marker can stop it.
    let (read_end, write_end) = pipe().expect("pipe");
    write_all(&write_end, &full_reply());
    let started = Instant::now();
    let reply = read_probe_reply(&read_end, deadline).expect("read the full reply");
    let elapsed = started.elapsed();
    assert_eq!(reply, probe(true, true, Some((0, 23))));
    assert!(
        elapsed < Duration::from_millis(100),
        "a full reply took {elapsed:?}"
    );
    drop(write_end);

    // Silence with the write end open: defaults, after the deadline and not much after it.
    let (read_end, write_end) = pipe().expect("pipe");
    let started = Instant::now();
    let reply = read_probe_reply(&read_end, deadline).expect("read silence");
    let elapsed = started.elapsed();
    assert_eq!(reply, nothing());
    assert!(
        elapsed >= Duration::from_millis(200),
        "silence ended early after {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(1000),
        "silence took {elapsed:?}"
    );
    drop(write_end);

    // A cursor report only, then a keyboard reply arrives at 220 ms, after the deadline. There
    // was no marker, so the answer is the defaults, and the drain has taken the late reply.
    let (read_end, write_end) = pipe().expect("pipe");
    write_all(&write_end, CURSOR_BOTTOM_LEFT);
    let late = thread::spawn(move || {
        thread::sleep(Duration::from_millis(220));
        write_all(&write_end, KEYBOARD_SUPPORTED);
        // Hand the write end back so the pipe stays open while the test looks at it.
        write_end
    });
    let reply = read_probe_reply(&read_end, deadline).expect("read a late reply");
    assert_eq!(reply, nothing());
    let write_end = late.join().expect("late writer thread");
    assert!(
        !has_bytes_waiting(&read_end),
        "the late reply was left in the pipe for the key reader"
    );
    drop(write_end);

    // The write end closed with nothing written: end of file ends the read at once.
    let (read_end, write_end) = pipe().expect("pipe");
    drop(write_end);
    let started = Instant::now();
    let reply = read_probe_reply(&read_end, deadline).expect("read end of file");
    let elapsed = started.elapsed();
    assert_eq!(reply, nothing());
    assert!(
        elapsed < Duration::from_millis(100),
        "end of file took {elapsed:?}"
    );
}
