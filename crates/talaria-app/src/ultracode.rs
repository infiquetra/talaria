//! Workflow routing decisions; the application executes every read through its event channel.
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use talaria_core::{
    commands::{CommandCatalog, CommandOutcome, ErrorView, SlashLine, describe_error},
    session::SessionId,
    ultracode::{
        SKILL_COMMAND, SkillAvailability, SkillEpoch, SubmitRoute, UltracodeCommand,
        UltracodeState, follow_up_text, notice, route_submit, skill_availability,
    },
};
use talaria_workflow::runner::{RunStatus, RunSummary};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    FetchCatalog {
        session: SessionId,
        profile: String,
        command: UltracodeCommand,
    },
    RunCommand {
        session: SessionId,
        original_text: String,
        command_line: String,
    },
    Notice(String),
    SubmitCommand {
        text: String,
        display: String,
        submission_id: String,
    },
    RestoreComposer(String),
    InstallSkill {
        session: SessionId,
        profile: String,
    },
    Changed,
}
pub struct Foreground<'a> {
    pub session: &'a SessionId,
    pub profile: &'a str,
}
/// How often the runs folder is read while a routed prompt waits for its run.
const SCAN_EVERY_MS: u64 = 2_000;
/// How long a watch lives after the routed prompt that last armed it. The agent proposes a
/// workflow and asks before starting it, so the run can start long after the prompt; every
/// later routed prompt arms the watch again. A run still active at the end keeps the watch
/// until that run ends, so its end is announced.
const WATCH_WINDOW_MS: u64 = 60 * 60 * 1_000;

/// A session's watch for the runs a routed prompt starts. Runs are matched by profile and
/// start time, so two windows on one profile can both claim a run, and a run that ends
/// says nothing about whether this session's run has started.
#[derive(Debug, Clone)]
struct RunWatch {
    profile: String,
    since_ms: i64,
    armed_at_ms: u64,
    /// Each run seen so far: its last status and its start time.
    seen: HashMap<String, (RunStatus, i64)>,
}
impl RunWatch {
    /// The earliest start time a read must reach: the window's start, or an earlier run
    /// that is still active.
    fn floor_ms(&self) -> i64 {
        self.seen
            .values()
            .filter(|(status, _)| is_active(*status))
            .map(|(_, started)| *started)
            .fold(self.since_ms, i64::min)
    }
}

fn is_active(status: RunStatus) -> bool {
    matches!(status, RunStatus::Running | RunStatus::Parked)
}

pub struct UltracodeService {
    state: UltracodeState,
    profiles: HashMap<SessionId, String>,
    asked: HashSet<SessionId>,
    kept: HashSet<SessionId>,
    pending: HashMap<SessionId, String>,
    remote: bool,
    watches: HashMap<SessionId, RunWatch>,
    tracked: HashMap<SessionId, RunSummary>,
    next_scan_ms: Option<u64>,
    scanning: bool,
}
impl Default for UltracodeService {
    fn default() -> Self {
        Self::new(false)
    }
}
impl UltracodeService {
    pub fn new(default_on: bool) -> Self {
        Self {
            state: UltracodeState::new(default_on),
            profiles: HashMap::new(),
            asked: HashSet::new(),
            kept: HashSet::new(),
            pending: HashMap::new(),
            remote: false,
            watches: HashMap::new(),
            tracked: HashMap::new(),
            next_scan_ms: None,
            scanning: false,
        }
    }
    pub fn state(&self) -> &UltracodeState {
        &self.state
    }
    pub fn set_remote(&mut self, remote: bool) {
        self.remote = remote;
    }
    pub fn record_shown(&mut self, session: &SessionId) {
        let profile = self.profiles.get(session).map_or("default", String::as_str);
        self.pending.insert(session.clone(), profile.to_owned());
        self.asked.insert(session.clone());
        self.kept.remove(session);
    }
    pub fn show_kept(&mut self, session: &SessionId) -> Option<String> {
        if !self.kept.contains(session) {
            return None;
        }
        let profile = self.profiles.get(session).map_or("default", String::as_str);
        let text = notice::offer(profile);
        self.record_shown(session);
        Some(text)
    }
    pub fn answer_install(&mut self, session: &SessionId, line: &str) -> Option<Vec<Action>> {
        let profile = self.pending.remove(session)?;
        Some(vec![if line.trim() == "y" {
            Action::InstallSkill {
                session: session.clone(),
                profile,
            }
        } else {
            Action::Notice(notice::missing(&profile))
        }])
    }
    pub fn bind(&mut self, session: &SessionId, profile: &str) {
        self.profiles.insert(session.clone(), profile.into());
    }
    pub fn handle_local_command(
        &mut self,
        arg: &str,
        foreground: Option<Foreground<'_>>,
    ) -> Vec<Action> {
        let command = match UltracodeCommand::parse(arg) {
            Ok(c) => c,
            Err(_) => return vec![Action::Notice(notice::USAGE.into())],
        };
        let Some(Foreground { session, profile }) = foreground else {
            return vec![Action::Notice(notice::NO_SESSION.into())];
        };
        self.bind(session, profile);
        if command == UltracodeCommand::Off
            || (command == UltracodeCommand::Toggle && self.state.is_on(session))
        {
            self.state.set(session, false);
            vec![Action::Notice(notice::OFF.into()), Action::Changed]
        } else {
            vec![Action::FetchCatalog {
                session: session.clone(),
                profile: profile.into(),
                command,
            }]
        }
    }
    pub fn on_catalog_reply(
        &mut self,
        session: &SessionId,
        profile: &str,
        command: UltracodeCommand,
        result: Result<CommandCatalog, String>,
    ) -> Vec<Action> {
        self.bind(session, profile);
        let availability = match result {
            Ok(c) => skill_availability(&c),
            Err(reason) => SkillAvailability::Unknown { reason },
        };
        if !matches!(availability, SkillAvailability::Missing) {
            self.kept.remove(session);
        }
        if command == UltracodeCommand::Status {
            return vec![Action::Notice(notice::status(
                self.state.is_on(session),
                profile,
                &availability,
            ))];
        }
        let text = match availability {
            SkillAvailability::Installed { .. } => {
                self.state.set(session, true);
                // Typing the command again sends the whole skill with the next prompt.
                self.state.rearm(session);
                let text = if self.remote {
                    notice::ON_REMOTE
                } else {
                    notice::ON
                };
                return vec![Action::Notice(text.into()), Action::Changed];
            }
            SkillAvailability::Missing if self.remote => format!(
                "{}. The skill must be installed on the machine that runs the gateway.",
                notice::missing(profile)
            ),
            SkillAvailability::Missing if !self.asked.contains(session) => {
                self.kept.insert(session.clone());
                notice::offer(profile)
            }
            SkillAvailability::Missing => notice::missing(profile),
            SkillAvailability::Collision => notice::collision(profile),
            SkillAvailability::Unknown { reason } => notice::unknown(&reason),
        };
        vec![Action::Notice(text)]
    }
    pub fn intercept_submit(&self, session: &SessionId, text: &str) -> Option<Vec<Action>> {
        match route_submit(text, self.state.is_on(session)) {
            SubmitRoute::Plain => None,
            SubmitRoute::Workflow {
                command_line,
                one_shot,
            } => {
                let mut actions = vec![];
                if one_shot {
                    actions.push(Action::Notice(notice::ONE_SHOT.into()));
                } else if self.remote {
                    actions.push(Action::Notice(notice::ROUTED_REMOTE.into()));
                } else {
                    actions.push(Action::Notice(notice::ROUTED.into()));
                }
                // The conversation holds the skill already: the prompt goes with a reminder.
                if !one_shot && self.state.skill_loaded(session) {
                    actions.push(Action::SubmitCommand {
                        text: follow_up_text(text),
                        display: text.into(),
                        submission_id: crate::commands::fresh_submission_id(),
                    });
                    return Some(actions);
                }
                actions.push(Action::RunCommand {
                    session: session.clone(),
                    original_text: text.into(),
                    command_line,
                });
                Some(actions)
            }
        }
    }
    pub fn on_command_outcome(
        &self,
        session: &SessionId,
        original_text: &str,
        outcome: Result<Value, ErrorView>,
    ) -> Vec<Action> {
        let profile = self.profiles.get(session).map_or("default", String::as_str);
        let failure = match outcome {
            Ok(value) if value["type"] == "skill" && value["name"] == "talaria-workflows" => {
                if let Some(text) = value["message"].as_str().filter(|s| !s.trim().is_empty()) {
                    let mut actions = vec![];
                    if let Some(notice) = value["notice"].as_str().filter(|s| !s.trim().is_empty())
                    {
                        actions.push(Action::Notice(notice.into()));
                    }
                    // The cell shows what the person typed, never Hermes's `display`, which
                    // names the skill command.
                    actions.push(Action::SubmitCommand {
                        text: text.into(),
                        display: original_text.into(),
                        submission_id: crate::commands::fresh_submission_id(),
                    });
                    return actions;
                }
                notice::unavailable(profile)
            }
            Ok(_) => notice::collision(profile),
            Err(ErrorView::Rpc {
                reason: Some(reason),
                ..
            }) if reason == "unsupported_command" => {
                if self.remote {
                    format!(
                        "{}. The skill must be installed on the machine that runs the gateway.",
                        notice::missing(profile)
                    )
                } else if self.asked.contains(session) {
                    notice::missing(profile)
                } else {
                    notice::offer(profile)
                }
            }
            Err(ErrorView::Rpc {
                reason: Some(reason),
                ..
            }) if reason == "command_unavailable" => notice::unavailable(profile),
            Err(error) => {
                let line = SlashLine {
                    name: SKILL_COMMAND.trim_start_matches('/').into(),
                    arg: original_text.into(),
                };
                match describe_error(&line, &error, None) {
                    CommandOutcome::Refused { message, .. }
                    | CommandOutcome::Failed { message }
                    | CommandOutcome::OutcomeUnknown { message } => {
                        message.replace(SKILL_COMMAND, notice::REQUEST)
                    }
                    _ => "The workflow command did not return a skill.".into(),
                }
            }
        };
        vec![
            Action::Notice(failure),
            Action::RestoreComposer(original_text.into()),
        ]
    }
    /// The whole skill went into the session's conversation `epoch`.
    pub fn skill_sent(&mut self, session: &SessionId, epoch: SkillEpoch) {
        self.state.skill_sent(session, epoch);
    }
    /// Say which conversation the session holds now, before a prompt is routed.
    pub fn note_epoch(&mut self, session: &SessionId, epoch: &SkillEpoch) {
        self.state.note_epoch(session, epoch);
    }
    pub fn status_input(&self, foreground: Option<&SessionId>) -> bool {
        foreground.is_some_and(|id| self.state.is_on(id))
    }
    /// Copy the toggle from one session to another; true when `to` is now on.
    pub fn carry(&mut self, from: &SessionId, to: &SessionId) -> bool {
        if let Some(profile) = self.profiles.get(from).cloned() {
            self.profiles.entry(to.clone()).or_insert(profile);
        }
        self.state.carry(from, to)
    }
    /// Watch the local runs folder for a run of `profile` that starts at or after `since_ms`,
    /// wall clock. Arming an existing watch opens a fresh window: runs that already ended are
    /// forgotten, and runs still active stay so their end is announced. A remote gateway
    /// starts runs on its own machine, so nothing is watched.
    pub fn arm(&mut self, session: &SessionId, profile: &str, since_ms: i64, now_ms: u64) {
        if self.remote {
            return;
        }
        let watch = self
            .watches
            .entry(session.clone())
            .or_insert_with(|| RunWatch {
                profile: profile.into(),
                since_ms,
                armed_at_ms: now_ms,
                seen: HashMap::new(),
            });
        watch.since_ms = since_ms;
        watch.armed_at_ms = now_ms;
        watch.seen.retain(|_, (status, _)| is_active(*status));
        if !self.scanning {
            self.next_scan_ms = Some(self.next_scan_ms.map_or(now_ms, |at| at.min(now_ms)));
        }
    }
    /// When the next read of the runs folder is due, while any session is watching.
    pub fn next_scan_ms(&self) -> Option<u64> {
        if self.scanning || self.watches.is_empty() {
            return None;
        }
        self.next_scan_ms
    }
    /// Start a read when one is due: the earliest start time any watch accepts.
    pub fn scan_due(&mut self, now_ms: u64) -> Option<i64> {
        let due = self.next_scan_ms()?;
        if now_ms < due {
            return None;
        }
        self.scanning = true;
        self.watches.values().map(RunWatch::floor_ms).min()
    }
    /// Fold one read of the runs folder into every watch: a notice when a run starts and one
    /// when it ends, each once.
    pub fn on_runs(
        &mut self,
        rows: &[RunSummary],
        now_ms: u64,
        key: Option<&str>,
    ) -> Vec<(SessionId, Action)> {
        self.scanning = false;
        let mut actions = vec![];
        let mut ended = vec![];
        for (session, watch) in &mut self.watches {
            let mut changed = false;
            for row in rows
                .iter()
                .filter(|row| row.profile == watch.profile && row.status != RunStatus::Unreadable)
            {
                let before = watch.seen.get(&row.run_id).map(|(status, _)| *status);
                // A run already seen is followed to its end even if it started before the window.
                if before.is_none() && row.started_at_ms < watch.since_ms {
                    continue;
                }
                // A folder whose runner has not taken its lock yet reads as interrupted.
                if before.is_none() && row.status == RunStatus::Interrupted {
                    continue;
                }
                let active = is_active(row.status);
                if before.is_none() && active {
                    actions.push((
                        session.clone(),
                        Action::Notice(notice::run_started(&row.name, &row.run_id, key)),
                    ));
                }
                if !active && before.is_none_or(is_active) {
                    actions.push((
                        session.clone(),
                        Action::Notice(notice::run_finished(
                            &row.name,
                            &row.run_id,
                            row.status.as_str(),
                            key,
                        )),
                    ));
                }
                watch
                    .seen
                    .insert(row.run_id.clone(), (row.status, row.started_at_ms));
                let latest = self.tracked.get(session).is_none_or(|old| {
                    old.run_id == row.run_id || row.started_at_ms >= old.started_at_ms
                });
                if latest && self.tracked.get(session) != Some(row) {
                    self.tracked.insert(session.clone(), row.clone());
                    changed = true;
                }
            }
            if changed {
                actions.push((session.clone(), Action::Changed));
            }
            // A run that ends may be another window's, so only the window ends a watch.
            let any_active = watch.seen.values().any(|(status, _)| is_active(*status));
            if !any_active && now_ms.saturating_sub(watch.armed_at_ms) > WATCH_WINDOW_MS {
                ended.push(session.clone());
            }
        }
        for session in ended {
            self.watches.remove(&session);
        }
        self.next_scan_ms = (!self.watches.is_empty()).then(|| now_ms + SCAN_EVERY_MS);
        actions
    }
    /// The latest run a session's watch found, active or finished.
    pub fn tracked(&self, session: &SessionId) -> Option<&RunSummary> {
        self.tracked.get(session)
    }
    /// The session's run while it is running or waiting for an answer.
    pub fn active_run(&self, session: &SessionId) -> Option<&RunSummary> {
        self.tracked(session).filter(|run| is_active(run.status))
    }
}
