//! The microphone segment and the line under the composer.

use std::time::Duration;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::status::{Tone, VoiceSegment};

fn segment(text: String, tone: Tone, read_aloud: bool) -> VoiceSegment {
    let text = if read_aloud {
        format!("{text} +read")
    } else {
        text
    };
    VoiceSegment { text, tone }
}

/// Project the current session's phase onto the status line.
#[must_use]
pub fn segment_for(view: &talaria_core::voice::VoiceView) -> Option<VoiceSegment> {
    use talaria_core::voice::VoicePhase;
    match view.phase {
        VoicePhase::Starting | VoicePhase::Recording => {
            recording_segment(view.clock.unwrap_or_default(), view.read_aloud)
        }
        VoicePhase::Ready | VoicePhase::Stopping | VoicePhase::Transcribing
            if view.label.is_some() =>
        {
            label_segment(view.label.as_deref().unwrap_or_default(), view.read_aloud)
        }
        VoicePhase::Ready => ready_segment(view.read_aloud),
        VoicePhase::Stopping | VoicePhase::Transcribing => transcribing_segment(view.read_aloud),
        VoicePhase::Off if view.read_aloud => read_only_segment(),
        _ => None,
    }
}

/// `REC m:ss` for Starting or Recording.
#[must_use]
pub fn recording_segment(elapsed: Duration, read_aloud: bool) -> Option<VoiceSegment> {
    let seconds = elapsed.as_secs();
    Some(segment(
        format!("REC {}:{:02}", seconds / 60, seconds % 60),
        Tone::Warning,
        read_aloud,
    ))
}

/// `voice`, plus ` +read` when reading aloud.
#[must_use]
pub fn ready_segment(read_aloud: bool) -> Option<VoiceSegment> {
    Some(segment("voice".into(), Tone::Muted, read_aloud))
}

/// `transcribing…`, plus ` +read` when reading aloud.
#[must_use]
pub fn transcribing_segment(read_aloud: bool) -> Option<VoiceSegment> {
    Some(segment("transcribing…".into(), Tone::Muted, read_aloud))
}

/// A gateway label that does not replace `REC`.
#[must_use]
pub fn label_segment(label: &str, read_aloud: bool) -> Option<VoiceSegment> {
    let label: String = label
        .chars()
        .filter(|ch| !ch.is_control())
        .take(24)
        .collect();
    if label.is_empty() {
        None
    } else {
        Some(segment(label, Tone::Muted, read_aloud))
    }
}

/// Off, with read-aloud on: the word `read`.
#[must_use]
pub fn read_only_segment() -> Option<VoiceSegment> {
    Some(segment("read".into(), Tone::Muted, false))
}

/// `heard: ` plus the partial, newlines as spaces, cut to `columns`.
#[must_use]
pub fn partial_line(text: &str, columns: usize) -> Option<String> {
    if columns == 0 {
        return None;
    }
    let text: String = text
        .chars()
        .filter_map(|ch| match ch {
            '\n' | '\r' | '\t' => Some(' '),
            ch if ch.is_control() => None,
            ch => Some(ch),
        })
        .collect();
    let line = format!("heard: {text}");
    if UnicodeWidthStr::width(line.as_str()) <= columns {
        return Some(line);
    }
    let mut out = String::new();
    let mut width = 0;
    for grapheme in line.graphemes(true) {
        let next = UnicodeWidthStr::width(grapheme);
        if width + next > columns.saturating_sub(1) {
            break;
        }
        width += next;
        out.push_str(grapheme);
    }
    out.push('…');
    Some(out)
}

/// A transient, dim line below the composer; it is never written to scrollback.
pub struct PartialSection {
    text: String,
}

impl PartialSection {
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
        }
    }
}

impl crate::live::LiveSection for PartialSection {
    fn height(&self, width: u16) -> u16 {
        u16::from(width > 0)
    }

    fn render(&self, area: ratatui::layout::Rect, buffer: &mut ratatui::buffer::Buffer) {
        if area.height == 0 {
            return;
        }
        if let Some(line) = partial_line(&self.text, usize::from(area.width)) {
            buffer.set_stringn(
                area.x,
                area.y,
                line,
                usize::from(area.width),
                ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::DIM),
            );
        }
    }
}
