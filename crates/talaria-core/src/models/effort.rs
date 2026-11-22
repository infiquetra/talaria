//! Reasoning effort ladder.

/// The eight levels, low to high. `None` means thinking is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

impl Effort {
    /// Every level, low to high, as Hermes orders them (`EFFORT_LADDER` in
    /// `agent/reasoning_effort.py`).
    pub const ALL: [Self; 8] = [
        Self::None,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Xhigh,
        Self::Max,
        Self::Ultra,
    ];

    /// One plain line about the level, for the `/effort` selector. It names no cost: Talaria
    /// has no measured cost per level.
    pub fn describe(self) -> &'static str {
        match self {
            Self::None => "Thinking off: the model answers straight away",
            Self::Minimal => "The least thinking the model allows",
            Self::Low => "Quick answers for simple tasks",
            Self::Medium => "Balanced speed and reasoning; Hermes uses it when nothing is set",
            Self::High => "More thinking for harder tasks",
            Self::Xhigh => "Extended thinking for complex work",
            Self::Max => "The most thinking most models accept",
            Self::Ultra => "Beyond max where the route accepts it; Hermes lowers it elsewhere",
        }
    }

    /// Trimmed, any case. Only `none` means off.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "minimal" => Some(Self::Minimal),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "xhigh" => Some(Self::Xhigh),
            "max" => Some(Self::Max),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }

    /// The word Hermes expects.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }

    /// The next level, or `Ultra` when already there.
    pub fn up(self) -> Self {
        match self {
            Self::None => Self::Minimal,
            Self::Minimal => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Xhigh,
            Self::Xhigh => Self::Max,
            Self::Max | Self::Ultra => Self::Ultra,
        }
    }

    /// The previous level, or `None` when already there.
    pub fn down(self) -> Self {
        match self {
            Self::None | Self::Minimal => Self::None,
            Self::Low => Self::Minimal,
            Self::Medium => Self::Low,
            Self::High => Self::Medium,
            Self::Xhigh => Self::High,
            Self::Max => Self::Xhigh,
            Self::Ultra => Self::Max,
        }
    }
}
