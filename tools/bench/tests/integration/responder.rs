//! The responder answers the capability queries a modern terminal answers.

use talaria_bench::{Emulator, Query, ResponderState, answer_queries};

const ESC: &str = "\x1b";

fn fresh() -> (ResponderState, Emulator) {
    (ResponderState::new(), Emulator::new(48, 160))
}

#[test]
fn cursor_query_gets_position_reply() {
    // The text before the query puts the cursor on column 6 of row 1.
    let (mut state, mut emulator) = fresh();
    let answers = answer_queries(&mut state, b"hello\x1b[6n", &mut emulator);
    assert_eq!(answers.reply, b"\x1b[1;6R".to_vec());
    assert_eq!(answers.seen, vec![Query::CursorPosition]);

    // Bytes after the query in the same chunk must not move the reported cursor:
    // the reply describes the moment the query ended.
    let (mut state, mut emulator) = fresh();
    let answers = answer_queries(&mut state, b"\x1b[6nxyz", &mut emulator);
    assert_eq!(answers.reply, b"\x1b[1;1R".to_vec());
    assert_eq!(answers.seen, vec![Query::CursorPosition]);

    // The responder fed the whole chunk to the emulator itself, rest included.
    assert_eq!(emulator.cursor(), (1, 4));
}

/// Two characters of text, then the four queries the client sends at start-up, then a
/// query for a mode number nobody answers.
fn query_stream() -> Vec<u8> {
    let mut stream = Vec::new();
    stream.extend_from_slice(b"ab");
    stream.extend_from_slice(format!("{ESC}[?2026$p").as_bytes());
    stream.extend_from_slice(format!("{ESC}[?u").as_bytes());
    stream.extend_from_slice(format!("{ESC}[6n").as_bytes());
    stream.extend_from_slice(format!("{ESC}[c").as_bytes());
    stream.extend_from_slice(format!("{ESC}[?9999$p").as_bytes());
    stream
}

fn expected_reply() -> Vec<u8> {
    // Synchronized output known and reset, keyboard flags none, cursor after "ab",
    // primary device attributes. The unknown mode gets no reply.
    let mut reply = Vec::new();
    reply.extend_from_slice(b"\x1b[?2026;2$y");
    reply.extend_from_slice(b"\x1b[?0u");
    reply.extend_from_slice(b"\x1b[1;3R");
    reply.extend_from_slice(b"\x1b[?62;c");
    reply
}

fn expected_seen() -> Vec<Query> {
    vec![
        Query::SynchronizedOutput,
        Query::KeyboardFlags,
        Query::CursorPosition,
        Query::DeviceAttributes,
        Query::Other("CSI ? $ p".to_string()),
    ]
}

/// Pushes the stream in the given pieces and returns every reply byte and every query seen.
fn run_pieces(pieces: &[&[u8]]) -> (Vec<u8>, Vec<Query>, Emulator) {
    let (mut state, mut emulator) = fresh();
    let mut reply = Vec::new();
    let mut seen = Vec::new();
    for piece in pieces {
        let answers = answer_queries(&mut state, piece, &mut emulator);
        reply.extend_from_slice(&answers.reply);
        seen.extend(answers.seen);
    }
    (reply, seen, emulator)
}

#[test]
fn all_queries_get_their_replies_once_even_when_split() {
    let stream = query_stream();

    // One byte at a time.
    let singles: Vec<&[u8]> = stream.chunks(1).collect();
    let (reply, seen, emulator) = run_pieces(&singles);
    assert_eq!(reply, expected_reply(), "byte at a time");
    assert_eq!(seen, expected_seen(), "byte at a time");
    assert_eq!(
        emulator.cursor(),
        (1, 3),
        "queries leave the cursor where it was"
    );

    // Cut in two at every position, including inside a query.
    for cut in 0..=stream.len() {
        let (head, tail) = stream.split_at(cut);
        let (reply, seen, _) = run_pieces(&[head, tail]);
        assert_eq!(reply, expected_reply(), "cut at {cut}");
        assert_eq!(seen, expected_seen(), "cut at {cut}");
    }

    // Whole in one chunk.
    let (reply, seen, _) = run_pieces(&[&stream]);
    assert_eq!(reply, expected_reply(), "one chunk");
    assert_eq!(seen, expected_seen(), "one chunk");
}

#[test]
fn a_query_is_not_answered_again_by_later_chunks() {
    let stream = query_stream();
    let (mut state, mut emulator) = fresh();
    let first = answer_queries(&mut state, &stream, &mut emulator);
    assert_eq!(first.reply, expected_reply());

    for later in [&b"plain text"[..], &b"\x1b[0m"[..], &b"more"[..]] {
        let answers = answer_queries(&mut state, later, &mut emulator);
        assert!(answers.reply.is_empty(), "no reply for {later:?}");
        assert!(answers.seen.is_empty(), "nothing seen in {later:?}");
    }
}
