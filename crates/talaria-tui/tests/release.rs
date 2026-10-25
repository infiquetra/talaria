//! Handing the terminal to another program and taking it back.
//!
//! Behavior contract: issue 228.

mod support;

use support::{ByteSink, Vt, assert_tags_once_in_order, live_view_sections, probe, tagged};
use talaria_tui::live::{LiveSection, LiveView};
use talaria_tui::terminal::{InlineTerminal, ProbeReply, RestoreLink, TerminalCaps};

fn open_term(sink: ByteSink) -> InlineTerminal<ByteSink> {
    InlineTerminal::new(
        sink,
        (80, 24),
        3,
        probe(true, false, Some((0, 23))),
        RestoreLink::detached(),
    )
    .unwrap()
}

fn live_view(sections: &[impl LiveSection]) -> LiveView<'_> {
    LiveView {
        sections: sections.iter().map(|s| s as &dyn LiveSection).collect(),
    }
}

#[test]
fn release_restores_modes_and_queues() {
    let sink = ByteSink::default();
    let mut vt = Vt::new(80, 24);
    let mut term = open_term(sink.clone());
    let sections = live_view_sections(3);
    term.draw_live(&live_view(&sections)).unwrap();
    vt.absorb(&sink);
    assert!(!term.is_released());

    term.release().unwrap();
    assert!(term.is_released());
    let bytes = vt.absorb(&sink);
    assert!(
        bytes.windows(b"?2004l".len()).any(|w| w == b"?2004l"),
        "bracketed paste is off: {bytes:?}"
    );
    assert!(
        bytes.windows(b"?25h".len()).any(|w| w == b"?25h"),
        "the cursor is shown: {bytes:?}"
    );

    let before = vt.rows();
    term.commit_lines(tagged(2)).unwrap();
    term.draw_live(&live_view(&sections)).unwrap();
    assert!(sink.take().is_empty(), "nothing is drawn while released");
    assert_eq!(vt.rows(), before);
}

#[test]
fn reacquire_probes_after_resets_and_writes_queued_once() {
    let sink = ByteSink::default();
    let mut vt = Vt::new(80, 24);
    let mut term = open_term(sink.clone());
    term.release().unwrap();
    sink.take();

    term.commit_lines(tagged(3)).unwrap();
    assert!(sink.take().is_empty(), "commits queue while released");

    let seen = sink.clone();
    let mut calls = 0;
    let mut probe_fn = || {
        calls += 1;
        assert!(!seen.is_empty(), "reset bytes precede the probe call");
        Ok(ProbeReply {
            caps: TerminalCaps {
                synchronized_output: true,
                keyboard_enhancement: false,
            },
            cursor: Some((0, 10)),
            colors: Default::default(),
        })
    };
    term.reacquire(&mut probe_fn).unwrap();
    assert_eq!(calls, 1);
    assert!(!term.is_released());
    vt.absorb(&sink);
    assert_tags_once_in_order(&vt.rows(), 3);
}

#[test]
fn overlay_survives_release() {
    let sink = ByteSink::default();
    let mut vt = Vt::new(80, 24);
    let mut term = open_term(sink.clone());
    term.open_overlay().unwrap();
    vt.absorb(&sink);
    assert!(vt.alternate());

    term.release().unwrap();
    let bytes = vt.absorb(&sink);
    assert!(
        bytes.windows(b"?1049l".len()).any(|w| w == b"?1049l"),
        "the alternate screen is left on release: {bytes:?}"
    );
    assert!(!vt.alternate());

    let mut probe_fn = || {
        Ok(ProbeReply {
            caps: TerminalCaps {
                synchronized_output: true,
                keyboard_enhancement: false,
            },
            cursor: Some((0, 10)),
            colors: Default::default(),
        })
    };
    term.reacquire(&mut probe_fn).unwrap();
    let bytes = vt.absorb(&sink);
    assert!(
        bytes.windows(b"?1049h".len()).any(|w| w == b"?1049h"),
        "the alternate screen is re-entered on reacquire: {bytes:?}"
    );
    assert!(vt.alternate());
}

#[test]
fn released_alert_keeps_most_urgent() {
    let sink = ByteSink::default();
    let mut term = open_term(sink.clone());
    term.release().unwrap();
    sink.take();

    term.alert(b"done-first".to_vec(), 0).unwrap();
    term.alert(b"needs-you".to_vec(), 2).unwrap();
    term.alert(b"done-second".to_vec(), 0).unwrap();
    assert!(sink.take().is_empty(), "alerts hold while released");

    let mut probe_fn = || {
        Ok(ProbeReply {
            caps: TerminalCaps {
                synchronized_output: true,
                keyboard_enhancement: false,
            },
            cursor: Some((0, 10)),
            colors: Default::default(),
        })
    };
    term.reacquire(&mut probe_fn).unwrap();
    let bytes = sink.take();
    assert_eq!(
        bytes
            .windows(b"needs-you".len())
            .filter(|w| *w == &b"needs-you"[..])
            .count(),
        1,
        "one needs-you alert after reacquire: {bytes:?}"
    );
    assert!(
        !bytes
            .windows(b"done-first".len())
            .any(|w| w == &b"done-first"[..]),
        "less urgent alerts are dropped: {bytes:?}"
    );
}
