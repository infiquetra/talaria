//! Per-session workflow routing and the notices explaining it.
use crate::{
    commands::{CommandCatalog, CommandSource, SkillOrigin},
    session::SessionId,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
pub mod authoring;
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
/// How long a saved setting is kept after its session was last opened or changed.
pub const KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// One stored session's saved setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedMode {
    pub on: bool,
    /// Wall-clock milliseconds when the session was last opened or changed.
    pub seen_ms: i64,
}
/// The settings saved across restarts, keyed by stored session id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedModes {
    sessions: BTreeMap<String, SavedMode>,
}
#[derive(Serialize, Deserialize)]
struct SavedFile {
    version: u32,
    sessions: BTreeMap<String, SavedMode>,
}
impl SavedModes {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let file: SavedFile = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if file.version != 1 {
            return Err(format!("unknown version {}", file.version));
        }
        Ok(Self {
            sessions: file.sessions,
        })
    }
    pub fn to_json(&self) -> String {
        let file = SavedFile {
            version: 1,
            sessions: self.sessions.clone(),
        };
        serde_json::to_string_pretty(&file).unwrap_or_default() + "\n"
    }
    /// Drop sessions not seen within [`KEEP_MS`] of `now_ms`.
    pub fn prune(&mut self, now_ms: i64) {
        self.sessions
            .retain(|_, mode| now_ms.saturating_sub(mode.seen_ms) <= KEEP_MS);
    }
    /// Apply one window's changes to the file as it is now. A changed setting replaces the
    /// saved one; a session that was only opened keeps the saved setting, which another window
    /// may have changed, and is marked seen. Every other window's entry is kept, and sessions
    /// not seen for 30 days are dropped.
    pub fn merge(&mut self, changes: &SavedChanges) {
        for (stored, change) in &changes.sessions {
            match self.sessions.get_mut(stored) {
                Some(mode) if !change.set => mode.seen_ms = changes.now_ms,
                _ => {
                    self.sessions.insert(
                        stored.clone(),
                        SavedMode {
                            on: change.on,
                            seen_ms: changes.now_ms,
                        },
                    );
                }
            }
        }
        self.prune(changes.now_ms);
    }
}
/// What one Talaria window changed since its last save, by stored session id. Several windows
/// share the saved file, so a save merges these into the file instead of replacing it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedChanges {
    sessions: BTreeMap<String, SavedChange>,
    now_ms: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SavedChange {
    /// This window's setting for the session.
    on: bool,
    /// True when this window changed the setting; false when it only opened the session.
    set: bool,
}
impl SavedChanges {
    /// The stored session ids these changes touch.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.sessions.keys().map(String::as_str)
    }
    /// Keep only the sessions `keep` accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.sessions.retain(|stored, _| keep(stored));
    }
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}
#[derive(Debug, Clone, Default)]
pub struct UltracodeState {
    default_on: bool,
    /// Settings made before the session's stored id was known.
    sessions: HashMap<SessionId, bool>,
    /// Sessions whose conversation holds the whole skill, and the conversation it went into.
    skill: HashMap<SessionId, SkillEpoch>,
    /// The stored session each runtime session id belongs to.
    bound: HashMap<SessionId, String>,
    /// Settings by stored session id; these are saved across restarts.
    stored: BTreeMap<String, SavedMode>,
    /// Stored ids opened or changed since the last save; true for a changed setting.
    touched: BTreeMap<String, bool>,
}
impl UltracodeState {
    pub fn new(default_on: bool) -> Self {
        Self {
            default_on,
            ..Self::default()
        }
    }
    /// Start from the settings saved by an earlier run.
    pub fn restore(&mut self, saved: SavedModes) {
        self.stored = saved.sessions;
    }
    pub fn is_on(&self, id: &SessionId) -> bool {
        self.explicit(id).unwrap_or(self.default_on)
    }
    /// The setting made for the session, if any.
    fn explicit(&self, id: &SessionId) -> Option<bool> {
        match self.bound.get(id) {
            Some(stored) => self.stored.get(stored).map(|mode| mode.on),
            None => self.sessions.get(id).copied(),
        }
    }
    pub fn set(&mut self, id: &SessionId, on: bool) {
        match self.bound.get(id).cloned() {
            Some(stored) => self.save(stored, on),
            None => {
                self.sessions.insert(id.clone(), on);
            }
        }
    }
    fn save(&mut self, stored: String, on: bool) {
        let seen_ms = self.stored.get(&stored).map_or(0, |mode| mode.seen_ms);
        self.stored
            .insert(stored.clone(), SavedMode { on, seen_ms });
        self.touched.insert(stored, true);
    }
    /// The runtime session `id` belongs to stored session `stored`: its setting is the stored
    /// session's from now on. A setting made for `id` before, such as a branch's copy of its
    /// parent's, moves to the stored session. Says whether the session is on.
    pub fn adopt(&mut self, id: &SessionId, stored: &str) -> bool {
        if stored.is_empty() {
            return self.is_on(id);
        }
        // A session compacted while Talaria showed another one comes back under a new stored
        // id. Its setting follows from the stored id the runtime session was bound to, or from
        // the id Talaria asked to resume, which a resume by id answers with as its runtime id.
        let previous = self
            .bound
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.as_str().to_owned());
        let compacted = (previous != stored
            && !self.stored.contains_key(stored)
            && !self.sessions.contains_key(id))
        .then(|| self.stored.get(&previous).copied())
        .flatten();
        self.bound.insert(id.clone(), stored.to_owned());
        if let Some(on) = self.sessions.remove(id) {
            self.save(stored.to_owned(), on);
        } else if let Some(mode) = compacted {
            self.save(stored.to_owned(), mode.on);
        } else if self.stored.contains_key(stored) {
            self.touched.entry(stored.to_owned()).or_insert(false);
        }
        self.is_on(id)
    }
    /// Hermes moved the session to a new stored id, as a compaction does: the setting follows.
    pub fn rotate(&mut self, id: &SessionId, stored: &str) {
        let Some(old) = self.bound.get(id).cloned() else {
            return;
        };
        if old == stored || stored.is_empty() {
            return;
        }
        self.bound.insert(id.clone(), stored.to_owned());
        if let Some(mode) = self.stored.get(&old).copied() {
            self.save(stored.to_owned(), mode.on);
        }
    }
    /// The changes to save, when a session was opened or changed since the last save: each is
    /// marked seen at `now_ms`, and sessions not seen for 30 days are dropped.
    pub fn take_saved(&mut self, now_ms: i64) -> Option<SavedChanges> {
        if self.touched.is_empty() {
            return None;
        }
        let mut changes = SavedChanges {
            sessions: BTreeMap::new(),
            now_ms,
        };
        for (stored, set) in std::mem::take(&mut self.touched) {
            if let Some(mode) = self.stored.get_mut(&stored) {
                mode.seen_ms = now_ms;
                changes
                    .sessions
                    .insert(stored, SavedChange { on: mode.on, set });
            }
        }
        let mut kept = SavedModes {
            sessions: std::mem::take(&mut self.stored),
        };
        kept.prune(now_ms);
        self.stored = kept.sessions;
        Some(changes)
    }
    /// Copy an explicit setting from one session to another, and say whether `to` is now on.
    pub fn carry(&mut self, from: &SessionId, to: &SessionId) -> bool {
        if let Some(on) = self.explicit(from) {
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
    use super::authoring::{
        AuthoringEnd, AuthoringProgress, AuthoringStage, RunFacts, STEPS, elapsed_text,
    };
    /// What Ultracode does with a prompt, in the words every notice shares.
    pub const SKILL_ROLE: &str = "With Ultracode on, a plain prompt is handled as a workflow: the agent proposes a workflow and asks before starting it.";
    pub const ON: &str = "Ultracode on for this session. With Ultracode on, a plain prompt is handled as a workflow: the agent proposes a workflow and asks before starting it. Slash commands such as /goal still go to Hermes as typed. A started run shows above the composer, and /workflows lists every run. Ultracode stays on in a /branch copy, after a reconnect, and when the session is reopened or resumed after a restart; other sessions keep their own setting. /ultracode off to stop.";
    pub const ON_REMOTE: &str = "Ultracode on for this session. With Ultracode on, a plain prompt is handled as a workflow: the agent proposes a workflow and asks before starting it. Slash commands such as /goal still go to Hermes as typed. Runs start on the gateway's machine, so this window cannot show them; talaria workflow list there lists them. Ultracode stays on in a /branch copy, after a reconnect, and when the session is reopened or resumed after a restart; other sessions keep their own setting. /ultracode off to stop.";
    /// The building block's first row while a routed turn builds a workflow.
    pub const AUTHORING_TITLE: &str = "Ultracode · building a workflow";
    /// The block's second row: the step in plain words, then the script once written.
    pub fn authoring_step(progress: &AuthoringProgress) -> String {
        let mut text = format!("Step {} of {STEPS}: {}", progress.step(), progress.label());
        if let Some(script) = &progress.script {
            text.push_str(&format!(" · {script}"));
        }
        text
    }
    /// The block as one row, on a short screen.
    pub fn authoring_short(progress: &AuthoringProgress, elapsed_ms: u64) -> String {
        format!(
            "Ultracode · {} · {}",
            progress.label(),
            elapsed_text(elapsed_ms)
        )
    }
    /// The one line a routed turn leaves in the transcript when it ends.
    pub fn authoring_ended(
        progress: &AuthoringProgress,
        end: AuthoringEnd,
        elapsed_ms: u64,
        run: Option<RunFacts<'_>>,
    ) -> String {
        let elapsed = elapsed_text(elapsed_ms);
        let stopped = |why: &str| {
            let script = progress
                .script
                .as_ref()
                .map_or_else(String::new, |script| format!(" ({script})"));
            format!(
                "Ultracode stopped: {why} while {}{script} after {elapsed}. No workflow started.",
                progress.label()
            )
        };
        // The turn sent the command that starts the run, but Talaria has not found the run: on
        // a remote gateway it never can, and locally the runs folder is read every 2 seconds.
        let started = progress.stage == AuthoringStage::Running;
        let started_then_stopped = |why: &str| {
            let target = progress.script.as_deref().unwrap_or("the workflow");
            format!(
                "Ultracode stopped: {why} after starting {target}, {elapsed} in. The workflow may still be running."
            )
        };
        match (end, run, &progress.script) {
            (_, Some(run), _) => {
                let active = matches!(run.status, "running" | "parked");
                let state = match run.status {
                    "running" => "is running".to_owned(),
                    "parked" => "is waiting for an answer".to_owned(),
                    other => other.to_owned(),
                };
                // The count is the agents the run's events name so far; none yet says nothing.
                let agents = match (run.agents, active) {
                    (0, _) => String::new(),
                    (1, true) => " (1 agent so far)".to_owned(),
                    (1, false) => " (1 agent)".to_owned(),
                    (count, true) => format!(" ({count} agents so far)"),
                    (count, false) => format!(" ({count} agents)"),
                };
                format!(
                    "Ultracode built the workflow in {elapsed}: {} {state}{agents}.",
                    run.name
                )
            }
            (AuthoringEnd::Interrupted, None, _) if started => started_then_stopped("interrupted"),
            (AuthoringEnd::Failed, None, _) if started => started_then_stopped("the turn failed"),
            (AuthoringEnd::Interrupted, None, _) => stopped("interrupted"),
            (AuthoringEnd::Failed, None, _) => stopped("the turn failed"),
            (_, None, script) if started => format!(
                "Ultracode built the workflow in {elapsed} and started {}.",
                script.as_deref().unwrap_or("the run")
            ),
            (_, None, Some(script)) => format!(
                "Ultracode wrote the workflow script {script} in {elapsed} and is waiting for your approval to start it."
            ),
            (_, None, None) => {
                format!("Ultracode finished in {elapsed} without writing a workflow script.")
            }
        }
    }
    pub const BRANCH_CARRIED: &str = "Ultracode stays on in this branch.";
    /// Printed when a session opens with Ultracode on.
    pub const OPENED_ON: &str = "Ultracode is on in this session: a plain prompt is handled as a workflow. /ultracode off to stop.";
    /// Printed after an interrupt in a session that routed a prompt.
    pub const STILL_ON: &str =
        "Ultracode is still on: your next plain prompt is handled as a workflow too.";
    /// Why the staffing tiers are missing when no table names one.
    pub const NO_TIERS: &str = "no tiers are configured";
    pub const OFF: &str = "Ultracode off for this session.";
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
    /// The Hermes command that stops agents, and Hermes's background review, changing the skill.
    /// It always names the profile: without `-p`, Hermes runs in the profile chosen with
    /// `hermes profile use`, which may not be the one Talaria checked.
    pub fn lock_command(profile: &str) -> String {
        format!("hermes -p {profile} curator lock talaria-workflows")
    }
    /// `changes` are the inspection's `changed:`, `added:` and `removed:` entries.
    pub fn skill_changed(profile: &str, changes: &[String]) -> String {
        let files = changes
            .iter()
            .map(|change| match change.split_once(':') {
                Some((kind, path)) => format!("{kind} {path}"),
                None => change.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "The talaria-workflows skill in profile {profile} was changed by another program after Talaria installed it ({files}). This is most often Hermes's self-improvement review, which rewrites skills it has used. With changed instructions the agent may not follow Talaria's instructions, for example by building the project itself instead of proposing a workflow. To stop Hermes changing it again after a reinstall, run: {}",
            lock_command(profile)
        )
    }
    pub fn skill_outdated(profile: &str) -> String {
        format!(
            "The talaria-workflows skill in profile {profile} is older than the copy in this Talaria, so the agent follows older instructions."
        )
    }
    pub fn reinstall_offer(profile: &str) -> String {
        format!(
            "Reinstall Talaria's talaria-workflows skill into profile {profile} now? The installed copy is backed up first. [y/N]"
        )
    }
    pub fn reinstall_declined(profile: &str) -> String {
        format!(
            "Kept the installed talaria-workflows skill. Reinstall it later with: talaria workflow skill install --force --profile {profile}"
        )
    }
    /// `tiers` says why no staffing tier is usable, when none is.
    pub fn setup_missing(profile: &str, tiers: Option<&str>, builtins: &[String]) -> String {
        let mut text = "Workflow setup is incomplete, so the agent must write each workflow by hand and name every model itself.".to_owned();
        if let Some(reason) = tiers {
            text.push_str(&format!(
                " Staffing tiers: {reason}. Add [tiers.<name>] tables with a model and provider to staffing.toml in Talaria's home folder (~/.talaria unless TALARIA_CONFIG_DIR is set), then check them with: talaria workflow roster --profile {profile}"
            ));
        }
        if !builtins.is_empty() {
            text.push_str(&format!(
                " Built-in workflows not installed: {}. Install them with: talaria workflow builtins install",
                builtins.join(", ")
            ));
        }
        text
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
