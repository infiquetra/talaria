//! Two Hermes settings that quietly break long work, and the words that say so.
//!
//! `compression.in_place: false` makes Hermes end a long conversation and start a child session.
//! Hermes ignores a child of a session whose source is `tool`, which is what every workflow agent
//! uses, so the agent can never compress and ends in a context overflow. `agent.max_turns` with a
//! number stops every ordinary session on the profile after that many steps; Hermes's default is
//! no cap. This module reads the effective configuration Hermes serves and names those two states.

use serde_json::Value;

/// The setting whose `false` value stops workflow agents compressing their conversations.
pub const COMPRESSION_KEY: &str = "compression.in_place";
/// The setting whose number stops a session after that many steps.
pub const STEP_CAP_KEY: &str = "agent.max_turns";

/// Spellings Hermes reads as "no cap" (`_UNLIMITED_SPELLINGS` in `hermes_cli/config.py`).
const NO_CAP_WORDS: [&str; 9] = [
    "none",
    "null",
    "unlimited",
    "infinite",
    "infinity",
    "inf",
    "\u{221e}",
    "-1",
    "0",
];

/// One harmful setting found in a profile's configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Finding {
    /// `compression.in_place` is `false`.
    CompressionNotInPlace,
    /// `agent.max_turns` is a finite number of steps.
    StepCap { turns: u64 },
}

impl Finding {
    /// The exact configuration key the finding is about.
    #[must_use]
    pub fn key(&self) -> &'static str {
        match self {
            Self::CompressionNotInPlace => COMPRESSION_KEY,
            Self::StepCap { .. } => STEP_CAP_KEY,
        }
    }

    /// What is wrong, in one sentence, for a doctor row.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::CompressionNotInPlace => format!(
                "{COMPRESSION_KEY} is false, so workflow agents cannot compress their conversations on this profile and a long-running agent will fail with a context overflow"
            ),
            Self::StepCap { turns } => format!(
                "{STEP_CAP_KEY} is {turns}, so ordinary sessions on this profile stop after {turns} steps"
            ),
        }
    }

    /// What to change, in one sentence for a doctor row. `profile` names the Hermes profile in the
    /// command. Doctor text drops anything that starts with a slash as a path and cuts a row at
    /// 200 characters, so this names no slash command and stays short.
    #[must_use]
    pub fn fix(&self, profile: &str) -> String {
        match self {
            Self::CompressionNotInPlace => format!(
                "Set {COMPRESSION_KEY} to true, Hermes's default: hermes -p {profile} config set {COMPRESSION_KEY} true"
            ),
            Self::StepCap { .. } => format!(
                "Hermes's default is no cap. Remove it with: hermes -p {profile} config set {STEP_CAP_KEY} 0"
            ),
        }
    }

    /// The short notice a session shows once: what will happen and what to change.
    #[must_use]
    pub fn notice(&self, profile: &str) -> String {
        match self {
            Self::CompressionNotInPlace => format!(
                "The Hermes setting {COMPRESSION_KEY} is false on profile {profile}. Workflow agents cannot compress their conversations there, so a long-running agent will fail with a context overflow. Set {COMPRESSION_KEY} to true (Hermes's default): open /config {COMPRESSION_KEY}, or run hermes -p {profile} config set {COMPRESSION_KEY} true."
            ),
            Self::StepCap { turns } => format!(
                "The Hermes setting {STEP_CAP_KEY} is {turns} on profile {profile}, so a session there stops after {turns} steps. Hermes's default is no cap. To remove it, open /config {STEP_CAP_KEY} and clear the value, or run hermes -p {profile} config set {STEP_CAP_KEY} 0."
            ),
        }
    }

    /// The warning `talaria workflow run` prints before any agent starts. Only the compression
    /// setting affects workflow agents: they send their own step cap, so `agent.max_turns` does
    /// not reach them.
    #[must_use]
    pub fn workflow_warning(&self, profile: &str) -> Option<String> {
        match self {
            Self::CompressionNotInPlace => Some(format!(
                "warning: {COMPRESSION_KEY} is false on profile {profile}, so workflow agents cannot compress their conversations and a long-running agent will fail with a context overflow. The run starts anyway. Set {COMPRESSION_KEY} to true (Hermes's default): hermes -p {profile} config set {COMPRESSION_KEY} true."
            )),
            Self::StepCap { .. } => None,
        }
    }
}

/// What a check of one profile found, for the doctor report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ConfigFacts {
    /// No check ran, for example because the gateway was not ready.
    #[default]
    NotTried,
    /// The configuration could not be read; the reason is short and has no private detail.
    Unknown { reason: String },
    /// The configuration was read; the list holds the harmful settings, possibly none.
    Read(Vec<Finding>),
}

/// The harmful settings in the effective configuration Hermes serves for a profile.
///
/// A missing key means Hermes's default, which is harmless for both settings.
#[must_use]
pub fn findings(effective: &Value) -> Vec<Finding> {
    let mut found = Vec::new();
    if effective["compression"]["in_place"] == Value::Bool(false) {
        found.push(Finding::CompressionNotInPlace);
    }
    if let Some(turns) = step_cap(effective) {
        found.push(Finding::StepCap { turns });
    }
    found
}

/// The finite step cap Hermes would apply, reading the value the way `resolve_turn_limit` does.
/// A root level `max_turns` counts only when `agent` has no `max_turns` of its own.
fn step_cap(effective: &Value) -> Option<u64> {
    let raw = match effective["agent"]
        .as_object()
        .and_then(|a| a.get("max_turns"))
    {
        Some(raw) => raw,
        None => &effective["max_turns"],
    };
    let steps = match raw {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f as i64))
            .or_else(|| n.as_u64().map(|u| i64::try_from(u).unwrap_or(i64::MAX)))?,
        Value::String(text) => {
            let word = text.trim().to_lowercase();
            if word.is_empty() || NO_CAP_WORDS.contains(&word.as_str()) {
                return None;
            }
            word.parse::<i64>().ok().or_else(|| {
                word.parse::<f64>()
                    .ok()
                    .filter(|f| f.is_finite())
                    .map(|f| f as i64)
            })?
        }
        _ => return None,
    };
    u64::try_from(steps).ok().filter(|steps| *steps > 0)
}
