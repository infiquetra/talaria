//! The `/agents` and `/tasks` argument. Pure. Issue #242.

/// What `/agents` was asked to do. `Unknown` holds the whole trimmed argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentsArg {
    Open,
    Pause,
    Resume,
    Unknown(String),
}

/// Parse the typed argument: trims; empty gives `Open`; `pause` and `resume`
/// match in any case; anything else gives `Unknown`.
#[must_use]
pub fn parse_agents_arg(arg: &str) -> AgentsArg {
    let arg = arg.trim();
    if arg.is_empty() {
        AgentsArg::Open
    } else if arg.eq_ignore_ascii_case("pause") {
        AgentsArg::Pause
    } else if arg.eq_ignore_ascii_case("resume") {
        AgentsArg::Resume
    } else {
        AgentsArg::Unknown(arg.to_owned())
    }
}

impl AgentsArg {
    /// The refusal for `Unknown`: `Unknown /agents option: <arg cut to 40>.
    /// Use /agents, /agents pause or /agents resume.`
    #[must_use]
    pub fn unknown_text(&self) -> Option<String> {
        match self {
            Self::Unknown(arg) => Some(format!(
                "Unknown /agents option: {}. Use /agents, /agents pause or /agents resume.",
                super::row::cut(arg, 40)
            )),
            _ => None,
        }
    }
}
