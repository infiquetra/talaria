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
    route_submit_with_model(text, enabled, None)
}
/// Like [`route_submit`], and names the model and provider the session runs on in a last line
/// of the routed prompt, so the agent staffs the plan with the model the user picked. The line
/// is left out when either name is empty or holds a control character.
pub fn route_submit_with_model(
    text: &str,
    enabled: bool,
    session_model: Option<(&str, &str)>,
) -> SubmitRoute {
    let trimmed = text.trim_start();
    if trimmed.is_empty()
        || trimmed.starts_with(['/', '!'])
        || (!enabled && !contains_keyword(text))
    {
        return SubmitRoute::Plain;
    }
    let mut command_line = format!("{SKILL_COMMAND} {}", text.trim_end());
    if let Some((model, provider)) = session_model
        && [model, provider]
            .iter()
            .all(|name| !name.trim().is_empty() && !name.chars().any(char::is_control))
    {
        command_line.push_str(&format!("\nSession model: {model} on {provider}"));
    }
    SubmitRoute::Workflow {
        command_line,
        one_shot: !enabled,
    }
}
/// Commands that carry a prompt to Hermes without the workflow route.
const BYPASSING: [&str; 9] = [
    "goal",
    "bg",
    "background",
    "queue",
    "q",
    "steer",
    "s",
    "btw",
    "plan",
];
/// `/goal` subcommands that manage a standing goal rather than set one.
const GOAL_SUBCOMMANDS: [&str; 9] = [
    "show", "draft", "gate", "pause", "resume", "clear", "status", "wait", "unwait",
];
/// The command name, as typed, when Ultracode is on and a slash command sends a prompt that
/// Ultracode does not route.
pub fn bypass(text: &str, enabled: bool) -> Option<String> {
    if !enabled {
        return None;
    }
    let line = text.trim().strip_prefix('/')?;
    let (name, arg) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(name, arg)| (name, arg.trim()));
    let lower = name.to_ascii_lowercase();
    if arg.is_empty() || !BYPASSING.contains(&lower.as_str()) {
        return None;
    }
    let first = arg.split_whitespace().next().unwrap_or_default();
    if lower == "goal" && GOAL_SUBCOMMANDS.contains(&first.to_ascii_lowercase().as_str()) {
        return None;
    }
    Some(name.to_owned())
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
    /// Copy an explicit setting from one session to another, and say whether `to` is now on.
    pub fn carry(&mut self, from: &SessionId, to: &SessionId) -> bool {
        if let Some(on) = self.sessions.get(from).copied() {
            self.set(to, on);
        }
        self.is_on(to)
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
    /// What Ultracode does with a prompt, in the words every notice shares.
    pub const SKILL_ROLE: &str = "With Ultracode on, a plain prompt goes to the talaria-workflows skill, and the agent proposes a workflow and asks before starting it.";
    pub const ON: &str = "Ultracode on for this session. With Ultracode on, a plain prompt goes to the talaria-workflows skill, and the agent proposes a workflow and asks before starting it. Slash commands such as /goal still go to Hermes as typed. A started run shows above the composer, and /workflows lists every run. Ultracode stays on in a /branch copy and after a reconnect; other sessions keep their own setting. /ultracode off to stop.";
    pub const ON_REMOTE: &str = "Ultracode on for this session. With Ultracode on, a plain prompt goes to the talaria-workflows skill, and the agent proposes a workflow and asks before starting it. Slash commands such as /goal still go to Hermes as typed. Runs start on the gateway's machine, so this window cannot show them; talaria workflow list there lists them. Ultracode stays on in a /branch copy and after a reconnect; other sessions keep their own setting. /ultracode off to stop.";
    pub const ROUTED: &str = "Ultracode: this prompt goes to the talaria-workflows skill. The agent proposes a workflow and asks before starting it; a started run shows above the composer.";
    pub const ROUTED_REMOTE: &str = "Ultracode: this prompt goes to the talaria-workflows skill. The agent proposes a workflow and asks before starting it. The run starts on the gateway's machine, so this window cannot show it; talaria workflow list there lists it.";
    pub const BRANCH_CARRIED: &str = "Ultracode stays on in this branch.";
    pub const OFF: &str = "Ultracode off for this session.";
    pub const ONE_SHOT: &str = "Ultracode: this prompt goes to the talaria-workflows skill (keyword ultracode). The agent proposes a workflow and asks before starting it.";
    pub const NO_SESSION: &str = "No session to set ultracode on.";
    pub const USAGE: &str = "Usage: /ultracode [on|off|status]";
    pub fn bypassed(command: &str) -> String {
        format!(
            "Ultracode does not apply to /{command}; it goes to Hermes as typed. To send the task to the talaria-workflows skill, send it as a plain prompt without /{command}."
        )
    }
    fn open_with(key: Option<&str>, what: &str) -> String {
        match key {
            Some(key) => format!("Press {key} or type /workflows to see {what}."),
            None => format!("Type /workflows to see {what}."),
        }
    }
    pub fn run_started(name: &str, run_id: &str, key: Option<&str>) -> String {
        format!(
            "Workflow run started: {name} ({run_id}). {}",
            open_with(key, "its agents")
        )
    }
    pub fn run_finished(name: &str, run_id: &str, outcome: &str, key: Option<&str>) -> String {
        format!(
            "Workflow run {outcome}: {name} ({run_id}). {}",
            open_with(key, "the result")
        )
    }
    pub fn offer(profile: &str) -> String {
        format!("Install talaria-workflows into profile {profile} now? [y/N]")
    }
    pub fn missing(profile: &str) -> String {
        format!(
            "Ultracode needs the talaria-workflows skill in profile {profile}. {SKILL_ROLE} Install it with: talaria workflow skill install --profile {profile}"
        )
    }
    pub fn unknown(reason: &str) -> String {
        format!(
            "Cannot check for the talaria-workflows skill yet: {reason}. {SKILL_ROLE} Try again once connected."
        )
    }
    pub fn unavailable(profile: &str) -> String {
        format!(
            "The talaria-workflows skill in profile {profile} could not be loaded. {SKILL_ROLE} Reinstall it with: talaria workflow skill install --force --profile {profile}"
        )
    }
    pub fn collision(profile: &str) -> String {
        format!(
            "A quick or plugin command named /talaria-workflows in profile {profile} hides the skill, so nothing was sent. {SKILL_ROLE} Rename that command, then try again."
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
