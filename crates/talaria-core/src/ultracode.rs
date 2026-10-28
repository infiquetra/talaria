//! Per-session workflow routing and the notices explaining it.
use crate::{
    commands::{CommandCatalog, CommandSource, SkillOrigin},
    session::SessionId,
};
use std::collections::HashMap;
pub const SKILL_COMMAND: &str = "/talaria-workflows";
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitRoute {
    Plain,
    Workflow {
        command_line: String,
        one_shot: bool,
    },
}
pub fn route_submit(text: &str, enabled: bool) -> SubmitRoute {
    let trimmed = text.trim_start();
    if trimmed.is_empty()
        || trimmed.starts_with(['/', '!'])
        || (!enabled && !contains_keyword(text))
    {
        return SubmitRoute::Plain;
    }
    SubmitRoute::Workflow {
        command_line: format!("{SKILL_COMMAND} {}", text.trim_end()),
        one_shot: !enabled,
    }
}
pub fn contains_keyword(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .any(|word| word.eq_ignore_ascii_case("ultracode"))
}
#[derive(Debug, Clone, Default)]
pub struct UltracodeState {
    default_on: bool,
    sessions: HashMap<SessionId, bool>,
}
impl UltracodeState {
    pub fn new(default_on: bool) -> Self {
        Self {
            default_on,
            sessions: HashMap::new(),
        }
    }
    pub fn is_on(&self, id: &SessionId) -> bool {
        self.sessions.get(id).copied().unwrap_or(self.default_on)
    }
    pub fn set(&mut self, id: &SessionId, on: bool) {
        self.sessions.insert(id.clone(), on);
    }
    pub fn toggle(&mut self, id: &SessionId) -> bool {
        let on = !self.is_on(id);
        self.set(id, on);
        on
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UltracodeCommand {
    Toggle,
    On,
    Off,
    Status,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageError;
impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Usage: /ultracode [on|off|status]")
    }
}
impl std::error::Error for UsageError {}
impl UltracodeCommand {
    pub fn parse(arg: &str) -> Result<Self, UsageError> {
        match arg.trim().to_ascii_lowercase().as_str() {
            "" => Ok(Self::Toggle),
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            "status" => Ok(Self::Status),
            _ => Err(UsageError),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillAvailability {
    Installed { origin: String },
    Missing,
    Collision,
    Unknown { reason: String },
}
pub fn skill_availability(catalog: &CommandCatalog) -> SkillAvailability {
    if catalog.has_dispatch_collision(SKILL_COMMAND) {
        return SkillAvailability::Collision;
    }
    match catalog.lookup(SKILL_COMMAND) {
        Some(entry) => match &entry.source {
            CommandSource::Skill { origin } => SkillAvailability::Installed {
                origin: match origin {
                    SkillOrigin::Hub => "hub".into(),
                    SkillOrigin::Bundled => "bundled".into(),
                    SkillOrigin::Local => "local".into(),
                    SkillOrigin::Other(s) => s.clone(),
                },
            },
            _ => SkillAvailability::Collision,
        },
        None => SkillAvailability::Missing,
    }
}
pub mod notice {
    use super::SkillAvailability;
    pub const ON: &str = "Ultracode on for this session: substantive prompts run as Talaria workflows through the talaria-workflows skill. /ultracode off to stop.";
    pub const OFF: &str = "Ultracode off for this session.";
    pub const ONE_SHOT: &str = "This prompt runs as a workflow (keyword ultracode).";
    pub const NO_SESSION: &str = "No session to set ultracode on.";
    pub const USAGE: &str = "Usage: /ultracode [on|off|status]";
    pub fn missing(profile: &str) -> String {
        format!(
            "Ultracode needs the talaria-workflows skill in profile {profile}. Install it with: talaria workflow skill install --profile {profile}"
        )
    }
    pub fn unknown(reason: &str) -> String {
        format!(
            "Cannot check for the talaria-workflows skill yet: {reason}. Try again once connected."
        )
    }
    pub fn unavailable(profile: &str) -> String {
        format!(
            "The talaria-workflows skill in profile {profile} could not be loaded. Reinstall it with: talaria workflow skill install --force --profile {profile}"
        )
    }
    pub fn collision(profile: &str) -> String {
        format!(
            "A quick or plugin command named /talaria-workflows in profile {profile} hides the skill, so nothing was sent. Rename that command, then try again."
        )
    }
    pub fn status(on: bool, profile: &str, skill: &SkillAvailability) -> String {
        let skill = match skill {
            SkillAvailability::Installed { origin } => format!("installed ({origin})"),
            SkillAvailability::Missing => format!("not found in profile {profile}"),
            SkillAvailability::Collision => "unknown (command hides the skill)".into(),
            SkillAvailability::Unknown { reason } => format!("unknown ({reason})"),
        };
        format!(
            "Ultracode is {} for this session. Skill: {skill}.",
            if on { "on" } else { "off" }
        )
    }
}
