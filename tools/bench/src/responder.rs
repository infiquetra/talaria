//! Answers the capability queries a modern terminal answers.

use crate::screen::Emulator;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    CursorPosition,
    KeyboardFlags,
    SynchronizedOutput,
    DeviceAttributes,
    Other(String),
}

#[derive(Clone, Debug, Default)]
pub struct Answers {
    pub reply: Vec<u8>,
    pub seen: Vec<Query>,
}

#[derive(Debug, Default)]
pub struct ResponderState {
    _trailing: Vec<u8>,
}

impl ResponderState {
    pub fn new() -> ResponderState {
        ResponderState::default()
    }
}

/// Feeds `chunk` to the emulator itself, piece by piece, and replies to each query once.
pub fn answer_queries(
    state: &mut ResponderState,
    chunk: &[u8],
    emulator: &mut Emulator,
) -> Answers {
    let mut answers = Answers::default();
    for &byte in chunk {
        emulator.feed(&[byte]);
        if byte == 0x1b {
            state._trailing.clear();
            state._trailing.push(byte);
            continue;
        }
        if state._trailing.is_empty() {
            continue;
        }
        state._trailing.push(byte);
        if state._trailing.len() > 256 {
            state._trailing.clear();
            continue;
        }
        let sequence = state._trailing.as_slice();
        let csi = sequence.starts_with(b"\x1b[");
        let osc = sequence.starts_with(b"\x1b]");
        if sequence.len() < 3 || !(csi || osc) {
            if sequence.len() >= 3 {
                state._trailing.clear();
            }
            continue;
        }
        let query = match sequence {
            b"\x1b[6n" => Some(Query::CursorPosition),
            b"\x1b[?u" => Some(Query::KeyboardFlags),
            b"\x1b[?2026$p" => Some(Query::SynchronizedOutput),
            b"\x1b[c" => Some(Query::DeviceAttributes),
            _ if csi
                && (byte == b'n' || byte == b'c' || (byte == b'p' && sequence.contains(&b'$'))) =>
            {
                let kind: String = sequence[2..]
                    .iter()
                    .filter(|b| !b.is_ascii_digit() && **b != b';')
                    .map(|&b| b as char)
                    .collect();
                Some(Query::Other(format!(
                    "CSI {}",
                    kind.chars()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                )))
            }
            _ if osc && byte == b'?' => Some(Query::Other("OSC ?".into())),
            _ => None,
        };
        if let Some(query) = query {
            match &query {
                Query::CursorPosition => {
                    let (row, col) = emulator.cursor();
                    answers
                        .reply
                        .extend_from_slice(format!("\x1b[{row};{col}R").as_bytes());
                }
                Query::KeyboardFlags => answers.reply.extend_from_slice(b"\x1b[?0u"),
                Query::SynchronizedOutput => answers.reply.extend_from_slice(b"\x1b[?2026;2$y"),
                Query::DeviceAttributes => answers.reply.extend_from_slice(b"\x1b[?62;c"),
                Query::Other(_) => {}
            }
            answers.seen.push(query);
            state._trailing.clear();
        } else if (csi && (0x40..=0x7e).contains(&byte)) || (osc && byte == 7) {
            state._trailing.clear();
        }
    }
    answers
}
