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
    let _ = (state, chunk, emulator);
    todo!("bench")
}
