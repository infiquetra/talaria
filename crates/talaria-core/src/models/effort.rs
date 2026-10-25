//! Reasoning effort ladder. Issue 230, scenario "parse and ladder".

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
    /// Trimmed, any case. Only `none` means off.
    pub fn parse(text: &str) -> Option<Self> {
        let _ = text;
        todo!()
    }

    /// The word Hermes expects.
    pub fn as_wire(self) -> &'static str {
        let _ = self;
        todo!()
    }

    /// The next level, or `Ultra` when already there.
    pub fn up(self) -> Self {
        let _ = self;
        todo!()
    }

    /// The previous level, or `None` when already there.
    pub fn down(self) -> Self {
        let _ = self;
        todo!()
    }
}
