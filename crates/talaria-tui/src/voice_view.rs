//! The microphone segment and the line under the composer.
//!
//! The drawers are stubs: they return nothing, so the snapshot test fails by assertion.

use std::time::Duration;

use crate::status::VoiceSegment;

/// `REC m:ss` for Starting or Recording. The stub has no text.
#[must_use]
pub fn recording_segment(elapsed: Duration, read_aloud: bool) -> Option<VoiceSegment> {
    let _ = (elapsed, read_aloud);
    None
}

/// `voice`, plus ` +read` when reading aloud. The stub has no text.
#[must_use]
pub fn ready_segment(read_aloud: bool) -> Option<VoiceSegment> {
    let _ = read_aloud;
    None
}

/// `transcribing…`, plus ` +read` when reading aloud. The stub has no text.
#[must_use]
pub fn transcribing_segment(read_aloud: bool) -> Option<VoiceSegment> {
    let _ = read_aloud;
    None
}

/// A gateway label that does not replace `REC`. The stub has no text.
#[must_use]
pub fn label_segment(label: &str, read_aloud: bool) -> Option<VoiceSegment> {
    let _ = (label, read_aloud);
    None
}

/// Off, with read-aloud on: the word `read`. The stub has no text.
#[must_use]
pub fn read_only_segment() -> Option<VoiceSegment> {
    None
}

/// `heard: ` plus the partial, newlines as spaces, cut to `columns`. The stub has no line.
#[must_use]
pub fn partial_line(text: &str, columns: usize) -> Option<String> {
    let _ = (text, columns);
    None
}
