//! Per-session workflow routing and the notices explaining it.
use crate::{
    commands::{CommandCatalog, CommandSource, SkillOrigin},
    session::SessionId,
};
use std::collections::HashMap;
pub const SKILL_COMMAND: &str = "/talaria-workflows";
/// The model-facing line in front of a routed prompt once the skill is in the conversation.
/// It never shows on screen, live or replayed.
pub const FOLLOW_UP_REMINDER: &str = "[Talaria workflow mode is on: the talaria-workflows skill already loaded in this conversation applies to this message.]";
/// The start of the message Hermes builds for a `/talaria-workflows` invocation, and the
/// markers around the person's request in it: `build_skill_invocation_message` and
/// `append_user_instruction` in Hermes's `agent/skill_commands.py` at the pinned revision.
const SCAFFOLD_HEAD: &str = "[IMPORTANT: The user has invoked the \"talaria-workflows\" skill,";
const SCAFFOLD_LOADED: &str = "The full skill content is loaded below.]";
const SCAFFOLD_REQUEST: &str =
    "The user has provided the following instruction alongside the skill invocation: ";
const SCAFFOLD_RUNTIME_NOTE: &str = "\n\n[Runtime note:";
/// What a routed prompt sends once the skill is in the conversation: the reminder, then what
/// the person typed.
pub fn follow_up_text(typed: &str) -> String {
    format!("{FOLLOW_UP_REMINDER}\n\n{typed}")
}
/// What the person typed, when `text` is what a routed prompt sent the model: the whole skill
/// with the request at its end, or the reminder in front of it. `None` for any other text.
pub fn typed_text(text: &str) -> Option<&str> {
    if let Some(rest) = text.strip_prefix(FOLLOW_UP_REMINDER) {
        return rest.strip_prefix("\n\n");
    }
    if !text.starts_with(SCAFFOLD_HEAD) || !text.contains(SCAFFOLD_LOADED) {
        return None;
    }
    // The skill body may quote the marker, so the person's request follows the last one.
    let rest = &text[text.rfind(SCAFFOLD_REQUEST)? + SCAFFOLD_REQUEST.len()..];
    Some(
        rest.find(SCAFFOLD_RUNTIME_NOTE)
            .map_or(rest, |end| &rest[..end]),
    )
}
/// True when `text` carries the whole skill: Hermes's expansion of a routed prompt, not the
/// reminder that stands in for it once the skill is in the conversation.
pub fn loads_skill(text: &str) -> bool {
    !text.starts_with(FOLLOW_UP_REMINDER) && typed_text(text).is_some()
}
/// What the screen shows for text a prompt sent: what the person typed for a routed prompt,
/// the text itself otherwise.
pub fn shown_text(text: &str) -> &str {
    typed_text(text).unwrap_or(text)
}
/// The request of a replayed `/talaria-workflows <request>` row. Hermes writes a routed
/// prompt that way when it replays a session.
pub fn replayed_request(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(SKILL_COMMAND)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(rest.trim()).filter(|request| !request.is_empty())
}
/// `typed` with the model-facing part of `sent` in front, so an edited routed prompt stays
/// routed. Any other `sent` gives `typed` alone.
pub fn retyped(sent: &str, typed: &str) -> String {
    match typed_text(sent) {
        Some(old) if sent.ends_with(old) => format!("{}{typed}", &sent[..sent.len() - old.len()]),
        _ => typed.to_owned(),
    }
}
/// The conversation a full copy of the skill went into: the stored session, and how many
/// times Hermes had compacted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEpoch {
    pub stored: String,
    pub compressions: i64,
}
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
    /// Sessions whose conversation holds the whole skill, and the conversation it went into.
    skill: HashMap<SessionId, SkillEpoch>,
}
impl UltracodeState {
    pub fn new(default_on: bool) -> Self {
        Self {
            default_on,
            sessions: HashMap::new(),
            skill: HashMap::new(),
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
        if let Some(epoch) = self.skill.get(from).cloned() {
            self.skill.insert(to.clone(), epoch);
        }
        self.is_on(to)
    }
    /// True when the session's conversation already holds the whole skill, so a routed prompt
    /// sends the reminder instead.
    pub fn skill_loaded(&self, id: &SessionId) -> bool {
        self.skill.contains_key(id)
    }
    pub fn skill_sent(&mut self, id: &SessionId, epoch: SkillEpoch) {
        self.skill.insert(id.clone(), epoch);
    }
    /// Forget the skill when the session's conversation is no longer the one it went into: a
    /// new stored id, or a compaction that may have dropped it.
    pub fn note_epoch(&mut self, id: &SessionId, epoch: &SkillEpoch) {
        if self.skill.get(id).is_some_and(|sent| sent != epoch) {
            self.skill.remove(id);
        }
    }
    /// Send the whole skill with the session's next routed prompt.
    pub fn rearm(&mut self, id: &SessionId) {
        self.skill.remove(id);
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
    pub const SKILL_ROLE: &str = "With Ultracode on, a plain prompt is handled as a workflow: the agent proposes a workflow and asks before starting it.";
    pub const ON: &str = "Ultracode on for this session. With Ultracode on, a plain prompt is handled as a workflow: the agent proposes a workflow and asks before starting it. Slash commands such as /goal still go to Hermes as typed. A started run shows above the composer, and /workflows lists every run. Ultracode stays on in a /branch copy and after a reconnect; other sessions keep their own setting. /ultracode off to stop.";
    pub const ON_REMOTE: &str = "Ultracode on for this session. With Ultracode on, a plain prompt is handled as a workflow: the agent proposes a workflow and asks before starting it. Slash commands such as /goal still go to Hermes as typed. Runs start on the gateway's machine, so this window cannot show them; talaria workflow list there lists them. Ultracode stays on in a /branch copy and after a reconnect; other sessions keep their own setting. /ultracode off to stop.";
    /// The one line a routed prompt prints.
    pub const ROUTED: &str = "Ultracode: handling this prompt as a workflow.";
    pub const ROUTED_REMOTE: &str =
        "Ultracode: handling this prompt as a workflow on the gateway's machine.";
    pub const BRANCH_CARRIED: &str = "Ultracode stays on in this branch.";
    pub const OFF: &str = "Ultracode off for this session.";
    pub const ONE_SHOT: &str = "Ultracode (keyword ultracode): handling this prompt as a workflow.";
    /// Names the routed request in an error Talaria words for a slash command.
    pub const REQUEST: &str = "the workflow request";
    pub const NO_SESSION: &str = "No session to set ultracode on.";
    pub const USAGE: &str = "Usage: /ultracode [on|off|status]";
    pub fn bypassed(command: &str) -> String {
        format!(
            "Ultracode does not apply to /{command}; it goes to Hermes as typed. To have the task handled as a workflow, send it as a plain prompt without /{command}."
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
            "A quick or plugin command with the talaria-workflows skill's name in profile {profile} hides the skill, so nothing was sent. {SKILL_ROLE} Rename that command, then try again."
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
