//! The splitter cuts the output stream into synchronized-output frames.

use std::time::{Duration, Instant};

use talaria_bench::{Frame, FrameSplitter, SplitCounts};

const OPEN: &[u8] = b"\x1b[?2026h";
const CLOSE: &[u8] = b"\x1b[?2026l";

/// Stray bytes, a frame, stray bytes, a stray closing marker, a frame holding a nested
/// opening marker, and a last frame that is never closed.
///
/// Byte positions (half open):
///   0..5     stray text with an escape sequence that is not a marker
///   5..13    opening marker of the first frame (last byte 12)
///   13..15   frame text
///   15..23   closing marker (last byte 22)
///   23..25   stray text
///   25..33   stray closing marker
///   33..41   opening marker of the second frame (last byte 40)
///   41..42   frame text
///   42..50   nested opening marker
///   50..51   frame text
///   51..59   closing marker (last byte 58)
///   59..67   opening marker of a frame that never closes
///   67..69   its text
fn stream() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"a\x1b[0m");
    bytes.extend_from_slice(OPEN);
    bytes.extend_from_slice(b"xy");
    bytes.extend_from_slice(CLOSE);
    bytes.extend_from_slice(b"de");
    bytes.extend_from_slice(CLOSE);
    bytes.extend_from_slice(OPEN);
    bytes.extend_from_slice(b"p");
    bytes.extend_from_slice(OPEN);
    bytes.extend_from_slice(b"q");
    bytes.extend_from_slice(CLOSE);
    bytes.extend_from_slice(OPEN);
    bytes.extend_from_slice(b"zz");
    bytes
}

/// Index of the last byte of the marker that opens frame one and closes it, then frame two.
const FIRST_OPENED_AT: usize = 12;
const FIRST_CLOSED_AT: usize = 22;
const SECOND_OPENED_AT: usize = 40;
const SECOND_CLOSED_AT: usize = 58;

/// Pushes the pieces, each stamped by the position of its last byte in the whole stream
/// (plus the base), and returns every frame found and the counts.
fn run(pieces: &[&[u8]], base: Instant) -> (Vec<Frame>, SplitCounts) {
    let mut splitter = FrameSplitter::new();
    let mut frames = Vec::new();
    let mut end_index = 0usize;
    for piece in pieces {
        end_index += piece.len();
        let at = base + Duration::from_millis((end_index as u64).saturating_sub(1));
        frames.extend(splitter.push(piece, at));
    }
    (frames, splitter.counts())
}

fn assert_expected(frames: &[Frame], counts: SplitCounts, base: Instant, what: &str) {
    assert_eq!(frames.len(), 2, "{what}: two complete frames");
    // Sizes include both markers; the nested marker is ordinary bytes inside the frame.
    assert_eq!(frames[0].bytes, 18, "{what}: first frame size");
    assert_eq!(frames[1].bytes, 26, "{what}: second frame size");
    assert!(frames[0].start <= frames[0].end, "{what}");
    assert!(frames[1].start <= frames[1].end, "{what}");
    let at = |index: usize| base + Duration::from_millis(index as u64);
    assert_eq!(frames[0].start, at(FIRST_OPENED_AT), "{what}: first start");
    assert_eq!(frames[0].end, at(FIRST_CLOSED_AT), "{what}: first end");
    assert_eq!(
        frames[1].start,
        at(SECOND_OPENED_AT),
        "{what}: second start"
    );
    assert_eq!(frames[1].end, at(SECOND_CLOSED_AT), "{what}: second end");
    assert_eq!(
        counts,
        SplitCounts {
            // The five stray bytes at the start (an escape sequence that is not a marker),
            // "de" and the stray closing marker: 5 + 2 + 8.
            bytes_outside_frames: 15,
            nested_markers: 1,
            unterminated_frames: 1,
        },
        "{what}: counts"
    );
}

#[test]
fn frames_split_at_markers_and_strays_are_counted() {
    let whole = stream();
    assert_eq!(whole.len(), 69, "the layout in the comment above");
    let base = Instant::now();

    // A single chunk: stamps are all the position of the final byte, so only sizes and
    // counts are comparable.
    let mut splitter = FrameSplitter::new();
    let frames = splitter.push(&whole, base);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].bytes, 18);
    assert_eq!(frames[1].bytes, 26);
    assert_eq!(frames[0].start, base);
    assert_eq!(frames[1].end, base);
    assert_eq!(
        splitter.counts(),
        SplitCounts {
            bytes_outside_frames: 15,
            nested_markers: 1,
            unterminated_frames: 1
        }
    );

    // One byte at a time: every marker is cut at every position, and each frame starts and
    // ends at the arrival of the chunk holding the marker's last byte.
    let singles: Vec<&[u8]> = whole.chunks(1).collect();
    let (frames, counts) = run(&singles, base);
    assert_expected(&frames, counts, base, "byte at a time");

    // Cut in two at every position.
    for cut in 1..whole.len() {
        let (head, tail) = whole.split_at(cut);
        let later = base + Duration::from_secs(1);
        let mut splitter = FrameSplitter::new();
        let mut frames = splitter.push(head, base);
        frames.extend(splitter.push(tail, later));
        assert_eq!(frames.len(), 2, "cut at {cut}");
        assert_eq!(frames[0].bytes, 18, "cut at {cut}");
        assert_eq!(frames[1].bytes, 26, "cut at {cut}");
        let stamp = |last_byte: usize| if last_byte < cut { base } else { later };
        assert_eq!(frames[0].start, stamp(FIRST_OPENED_AT), "cut at {cut}");
        assert_eq!(frames[0].end, stamp(FIRST_CLOSED_AT), "cut at {cut}");
        assert_eq!(frames[1].start, stamp(SECOND_OPENED_AT), "cut at {cut}");
        assert_eq!(frames[1].end, stamp(SECOND_CLOSED_AT), "cut at {cut}");
        assert_eq!(
            splitter.counts(),
            SplitCounts {
                bytes_outside_frames: 15,
                nested_markers: 1,
                unterminated_frames: 1
            },
            "cut at {cut}"
        );
    }
}

#[test]
fn a_stream_that_ends_between_frames_has_no_open_frame() {
    let mut bytes = b"abc".to_vec();
    bytes.extend_from_slice(OPEN);
    bytes.extend_from_slice(b"payload");
    bytes.extend_from_slice(CLOSE);
    let singles: Vec<&[u8]> = bytes.chunks(1).collect();
    let (frames, counts) = run(&singles, Instant::now());
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].bytes, 8 + 7 + 8);
    assert_eq!(
        counts,
        SplitCounts {
            bytes_outside_frames: 3,
            nested_markers: 0,
            unterminated_frames: 0
        }
    );
}
