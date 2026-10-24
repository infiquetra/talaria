//! The status line: one bottom row of native segments plus an optional operator command.

pub mod command;
pub mod document;
pub mod env;
pub mod output;
pub mod segments;

use ratatui::style::Style;

pub use command::{
    CommandLimits, CommandResult, CommandSpec, Decision, ExternalDisplay, MIN_RERUN_GAP,
    RefreshPolicy, SkipReason, StatusCommandService, external_segment, interval_from_config,
    parse_command, run_command,
};
pub use document::{DOCUMENT_FIELDS, StatusDocument};
pub use env::{DENIED_PROVIDER_KEYS, FORWARDED_TALARIA, build_child_env, clean_gateway_url};
pub use output::{ExternalRow, Failure, ParsedOutput, parse_output};
pub use segments::{
    ExternalSegment, LinePlan, Segment, SessionRef, StatusInputs, StatusSection, TokenUsage, plan,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Connection {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    AuthFailed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RunMode {
    #[default]
    Live,
    Replay,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TurnState {
    #[default]
    Idle,
    Streaming,
    Waiting,
    Cancelled,
}

/// The seven tones of the command contract, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Text,
    Muted,
    Separator,
    Success,
    Warning,
    Error,
    Attention,
}

impl Tone {
    /// An unknown name becomes `Text`.
    pub fn from_token(name: &str) -> Tone {
        let _ = name;
        todo!("status")
    }
}

/// One style per tone.
pub struct StatusPalette {
    _private: (),
}

impl Default for StatusPalette {
    fn default() -> Self {
        todo!("status")
    }
}

impl StatusPalette {
    pub fn style(&self, tone: Tone) -> Style {
        let _ = tone;
        todo!("status")
    }

    /// Styles in `Tone` order.
    pub fn from_styles(styles: [Style; 7]) -> Self {
        let _ = styles;
        todo!("status")
    }
}
