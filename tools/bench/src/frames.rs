//! Splits the output stream into synchronized-output frames.

use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Frame {
    pub bytes: usize,
    pub start: Instant,
    pub end: Instant,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SplitCounts {
    pub bytes_outside_frames: u64,
    pub nested_markers: u64,
    pub unterminated_frames: u64,
}

#[derive(Debug, Default)]
pub struct FrameSplitter {
    _open: bool,
}

impl FrameSplitter {
    pub fn new() -> FrameSplitter {
        FrameSplitter::default()
    }

    pub fn push(&mut self, chunk: &[u8], at: Instant) -> Vec<Frame> {
        let _ = (chunk, at);
        todo!("bench")
    }

    pub fn counts(&self) -> SplitCounts {
        todo!("bench")
    }
}
