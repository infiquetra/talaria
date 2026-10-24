//! Segment text, priority and the one-row layout.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::{Connection, RunMode, StatusPalette, Tone, TurnState};
use crate::live::LiveSection;

#[derive(Clone, Debug)]
pub struct SessionRef {
    pub id: String,
    pub title: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Everything the line shows, handed over by the caller.
#[derive(Clone, Debug, Default)]
pub struct StatusInputs {
    pub connection: Connection,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub reasoning_effort_wire: Option<String>,
    pub working_dir: Option<String>,
    pub home_dir: Option<String>,
    pub branch: Option<String>,
    pub context_percent: Option<i64>,
    pub context_estimated: bool,
    pub cost_usd: Option<f64>,
    pub cost_status: Option<String>,
    pub needs_you: u32,
    pub working: u32,
    pub mode: RunMode,
    pub session: Option<SessionRef>,
    pub turn: TurnState,
    pub pending_prompts: u32,
    pub subagents_active: u32,
    pub subagents_terminal: u32,
    pub usage_tokens: Option<TokenUsage>,
}

#[derive(Clone, Debug)]
pub struct ExternalSegment {
    pub text: String,
    pub tone: Tone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Segment {
    Connection,
    NeedsYou,
    Model,
    Context,
    Folder,
    Working,
    Cost,
    External,
}

impl Segment {
    pub const ALL: [Segment; 8] = [
        Segment::Connection,
        Segment::NeedsYou,
        Segment::Model,
        Segment::Context,
        Segment::Folder,
        Segment::Working,
        Segment::Cost,
        Segment::External,
    ];

    /// 1 is dropped last, 8 first.
    pub fn priority(self) -> u8 {
        todo!("status")
    }

    /// Left to right position, 0 first.
    pub fn position(self) -> u8 {
        todo!("status")
    }
}

#[derive(Clone, Debug)]
pub struct LinePlan {
    pub pieces: Vec<(Segment, String, Tone)>,
}

impl LinePlan {
    pub fn text(&self) -> String {
        todo!("status")
    }

    pub fn width(&self) -> usize {
        todo!("status")
    }
}

pub fn plan(inputs: &StatusInputs, external: Option<&ExternalSegment>, cols: u16) -> LinePlan {
    let _ = (inputs, external, cols);
    todo!("status")
}

/// The one-row live section.
pub struct StatusSection<'a> {
    _inputs: &'a StatusInputs,
    _external: Option<&'a ExternalSegment>,
    _palette: &'a StatusPalette,
}

impl<'a> StatusSection<'a> {
    pub fn new(
        inputs: &'a StatusInputs,
        external: Option<&'a ExternalSegment>,
        palette: &'a StatusPalette,
    ) -> Self {
        Self {
            _inputs: inputs,
            _external: external,
            _palette: palette,
        }
    }
}

impl LiveSection for StatusSection<'_> {
    fn height(&self, width: u16) -> u16 {
        let _ = width;
        todo!("status")
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let _ = (area, buf);
        todo!("status")
    }
}
