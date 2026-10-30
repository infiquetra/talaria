//! The status line: one bottom row of native segments plus an optional operator command.

#![forbid(unsafe_code)]

pub mod command;
pub mod document;
pub mod env;
pub mod output;
pub mod segments;

use ratatui::style::{Color, Modifier, Style};

pub use command::{
    CommandLimits, CommandResult, CommandSpec, Decision, ExternalDisplay, MIN_RERUN_GAP,
    RefreshPolicy, SkipReason, StatusCommandService, external_segment, interval_from_config,
    parse_command, run_command,
};
pub use document::{DOCUMENT_FIELDS, StatusDocument};
pub use env::{DENIED_PROVIDER_KEYS, FORWARDED_TALARIA, build_child_env, clean_gateway_url};
pub use output::{ExternalRow, Failure, ParsedOutput, parse_output};
pub use segments::{
    ApprovalsSegment, ExternalSegment, LinePlan, Segment, SessionRef, StatusInputs, StatusSection,
    TokenUsage, VoiceSegment, plan,
};

/// The attach state of the gateway link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connection {
    /// No link.
    #[default]
    Disconnected,
    /// The first link is being made.
    Connecting,
    /// A link is up.
    Connected,
    /// A link was lost and is being made again.
    Reconnecting,
    /// The gateway refused the credentials.
    AuthFailed,
}

/// Whether the client is attached to a live gateway or replaying a recording.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    /// A live gateway.
    #[default]
    Live,
    /// A recorded corpus.
    Replay,
}

/// The state of the focused session's turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnState {
    /// No turn is running.
    #[default]
    Idle,
    /// A reply is streaming.
    Streaming,
    /// A turn is blocked on an open prompt.
    Waiting,
    /// A turn ended interrupted.
    Cancelled,
}

/// The seven tones of the command contract, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// The workflow accent.
    Accent,
    /// The terminal's own foreground.
    Text,
    /// Dim: the default of every plain segment.
    Muted,
    /// Dim: the separator between two segments.
    Separator,
    /// Green.
    Success,
    /// Yellow.
    Warning,
    /// Red.
    Error,
    /// Bold yellow: the operator is needed.
    Attention,
}

impl Tone {
    /// An unknown name becomes `Text`. A command may ask for a tone; it never supplies a style.
    pub fn from_token(name: &str) -> Tone {
        match name {
            "text" => Tone::Text,
            "muted" => Tone::Muted,
            "separator" => Tone::Separator,
            "success" => Tone::Success,
            "warning" => Tone::Warning,
            "error" => Tone::Error,
            "attention" => Tone::Attention,
            _ => Tone::Text,
        }
    }
}

/// One style per tone.
#[derive(Clone, Copy, Debug)]
pub struct StatusPalette {
    styles: [Style; 7],
    accent: Style,
}

impl Default for StatusPalette {
    fn default() -> Self {
        Self {
            accent: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            styles: [
                Style::new(),
                Style::new().add_modifier(Modifier::DIM),
                Style::new().add_modifier(Modifier::DIM),
                Style::new().fg(Color::Green),
                Style::new().fg(Color::Yellow),
                Style::new().fg(Color::Red),
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ],
        }
    }
}

impl StatusPalette {
    /// The style of one tone.
    pub fn style(&self, tone: Tone) -> Style {
        if tone == Tone::Accent {
            self.accent
        } else {
            self.styles[tone_index(tone)]
        }
    }

    pub fn with_accent(mut self, accent: Style) -> Self {
        self.accent = accent;
        self
    }

    /// Styles in `Tone` order.
    pub fn from_styles(styles: [Style; 7]) -> Self {
        Self {
            styles,
            accent: styles[0],
        }
    }
}

fn tone_index(tone: Tone) -> usize {
    match tone {
        Tone::Accent => 0,
        Tone::Text => 0,
        Tone::Muted => 1,
        Tone::Separator => 2,
        Tone::Success => 3,
        Tone::Warning => 4,
        Tone::Error => 5,
        Tone::Attention => 6,
    }
}
