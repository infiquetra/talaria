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
    open: Option<(Instant, usize)>,
    tail: Vec<u8>,
    counts: SplitCounts,
}

impl FrameSplitter {
    pub fn new() -> FrameSplitter {
        FrameSplitter::default()
    }

    pub fn push(&mut self, chunk: &[u8], at: Instant) -> Vec<Frame> {
        const OPEN: &[u8] = b"\x1b[?2026h";
        const CLOSE: &[u8] = b"\x1b[?2026l";
        let mut frames = Vec::new();
        for &byte in chunk {
            self.tail.push(byte);
            while !OPEN.starts_with(&self.tail) && !CLOSE.starts_with(&self.tail) {
                self.tail.remove(0);
                self.add_bytes(1);
                if self.tail.is_empty() {
                    break;
                }
            }
            if self.tail == OPEN {
                if self.open.is_some() {
                    self.counts.nested_markers += 1;
                    self.add_bytes(OPEN.len());
                } else {
                    self.open = Some((at, OPEN.len()));
                }
                self.tail.clear();
            } else if self.tail == CLOSE {
                if let Some((start, bytes)) = self.open.take() {
                    frames.push(Frame {
                        bytes: bytes + CLOSE.len(),
                        start,
                        end: at,
                    });
                } else {
                    self.counts.bytes_outside_frames += CLOSE.len() as u64;
                }
                self.tail.clear();
            }
        }
        frames
    }

    pub fn counts(&self) -> SplitCounts {
        let mut counts = self.counts;
        counts.unterminated_frames = u64::from(self.open.is_some());
        if self.open.is_none() {
            counts.bytes_outside_frames += self.tail.len() as u64;
        }
        counts
    }

    fn add_bytes(&mut self, n: usize) {
        if let Some((_, bytes)) = &mut self.open {
            *bytes += n;
        } else {
            self.counts.bytes_outside_frames += n as u64;
        }
    }
}
