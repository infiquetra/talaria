//! Workflow routing decisions; the application executes every read through its event channel.
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use talaria_core::{
    commands::{CommandCatalog, CommandOutcome, ErrorView, SlashLine, describe_error},
    hivecode::{
        HivecodeCommand, HivecodeState, SKILL_COMMAND, SavedChanges, SavedModes, SkillAvailability,
        SkillEpoch, SubmitRoute,
        authoring::{AuthoringEnd, AuthoringStage, AuthoringView, RunFacts, resume},
        follow_up_with_model, notice, route_submit_with_request, session_model_names,
        skill_availability,
    },
    session::{Cell, SessionId, TurnOutcome, TurnState},
};
use talaria_workflow::runner::{RunStatus, RunSummary};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    FetchCatalog {
        session: SessionId,
        profile: String,
        command: HivecodeCommand,
    },
    /// Read the local workflow setup: the installed skill, staffing tiers and built-ins.
    CheckSetup {
        session: SessionId,
        profile: String,
    },
    RunCommand {
        session: SessionId,
        original_text: String,
        restore_text: Option<String>,
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
    /// Replace the installed skill with this Talaria's copy, backing it up first: what
    /// `/hivecode reinstall` asks for.
    ReinstallSkill {
        session: SessionId,
        profile: String,
    },
    Changed,
}
/// The installed skill compared with its install record and this Talaria's copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillCheck {
    Unchanged,
    /// Another program changed the files; `changes` are the inspection's entries.
    Modified {
        changes: Vec<String>,
        digest: String,
    },
    /// No install record, and the files are not this Talaria's copy.
    Differs {
        digest: String,
    },
    /// An unchanged install from an older Talaria.
    Outdated {
        digest: String,
    },
    /// Not installed by Talaria, or not readable: nothing to compare.
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TierCheck {
    Present,
    Missing,
    Unreadable(String),
}
/// What a setup read found on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupFacts {
    pub skill: SkillCheck,
    pub tiers: TierCheck,
    pub missing_builtins: Vec<String>,
}
pub struct Foreground<'a> {
    pub session: &'a SessionId,
    pub profile: &'a str,
}
/// An edit of a queued routed prompt: the row's submission id and text before, and the
/// submission id and text Hermes admits in its place.
pub struct Retype<'a> {
    pub input_id: &'a str,
    pub submission_id: &'a str,
    pub old: &'a str,
    pub new: &'a str,
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

/// A routed prompt's turn while the agent builds a workflow.
#[derive(Debug, Clone)]
struct Authoring {
    /// The prompt's submission: a refused one never starts a turn.
    submission_id: String,
    /// What the person typed: the text of the prompt's cell.
    typed: String,
    /// How many cells the session held when the prompt was sent; its cell comes after them.
    from: usize,
    /// Wall-clock milliseconds when the prompt was sent: a run started since is this turn's.
    since_wall_ms: i64,
    /// When the turn started, in the application's clock.
    started_ms: Option<u64>,
    /// What an earlier turn left waiting for approval, once this turn started after it.
    carried: Option<Carried>,
}

/// A script an earlier routed turn wrote and asked to start, and how long building it took.
#[derive(Debug, Clone)]
struct Carried {
    script: String,
    elapsed_ms: u64,
}

impl Authoring {
    /// Where the prompt's cell is, once it is drawn.
    fn cell(&self, cells: &[Cell]) -> Option<usize> {
        cells
            .iter()
            .enumerate()
            .skip(self.from)
            .rev()
            .find(|(_, cell)| matches!(cell, Cell::User(user) if user.text == self.typed))
            .map(|(at, _)| at)
    }
    /// The turn's step, counting from the script an earlier turn left waiting.
    fn progress(
        &self,
        cells: &[Cell],
        at: usize,
    ) -> talaria_core::hivecode::authoring::AuthoringProgress {
        let script = self.carried.as_ref().map(|carried| carried.script.as_str());
        resume(script, &cells[at + 1..])
    }
    /// How long the workflow has been building: this turn, and any earlier turn it follows.
    fn elapsed_ms(&self, started_ms: u64, now_ms: u64) -> u64 {
        let carried = self
            .carried
            .as_ref()
            .map_or(0, |carried| carried.elapsed_ms);
        carried.saturating_add(now_ms.saturating_sub(started_ms))
    }
}

pub struct HivecodeService {
    state: HivecodeState,
    profiles: HashMap<SessionId, String>,
    asked: HashSet<SessionId>,
    kept: HashSet<SessionId>,
    pending: HashMap<SessionId, String>,
    /// The skill change last reported for each profile, by digest.
    warned_skill: HashMap<String, String>,
    /// The missing setup last reported for each profile.
    warned_setup: HashMap<String, String>,
    /// Sessions that routed a prompt in this process.
    routed: HashSet<SessionId>,
    /// Sessions whose interrupted routed turn left its line, which says Hivecode is still on,
    /// before Hermes answered the interrupt.
    ended_interrupted: HashSet<SessionId>,
    /// Sessions whose interrupt Hermes answered while a routed turn was building: the line that
    /// turn leaves says Hivecode is still on.
    interrupt_answered: HashSet<SessionId>,
    /// Sessions whose next open says nothing, because a notice about the switch is already shown.
    announced: HashSet<SessionId>,
    /// True until the first session opens when Talaria was started with Hivecode on: that
    /// session is turned on even if it was saved as off.
    launch_on: bool,
    remote: bool,
    watches: HashMap<SessionId, RunWatch>,
    tracked: HashMap<SessionId, RunSummary>,
    /// Each session's routed prompts, oldest first: the front one's turn runs or comes next.
    authoring: HashMap<SessionId, VecDeque<Authoring>>,
    /// The script each session's last routed turn wrote and left waiting for approval.
    carried: HashMap<SessionId, Carried>,
    next_scan_ms: Option<u64>,
    scanning: bool,
}
impl Default for HivecodeService {
    fn default() -> Self {
        Self::new(false)
    }
}
impl HivecodeService {
    pub fn new(default_on: bool) -> Self {
        Self {
            state: HivecodeState::new(default_on),
            profiles: HashMap::new(),
            asked: HashSet::new(),
            kept: HashSet::new(),
            pending: HashMap::new(),
            warned_skill: HashMap::new(),
            warned_setup: HashMap::new(),
            routed: HashSet::new(),
            ended_interrupted: HashSet::new(),
            interrupt_answered: HashSet::new(),
            announced: HashSet::new(),
            launch_on: default_on,
            remote: false,
            watches: HashMap::new(),
            tracked: HashMap::new(),
            authoring: HashMap::new(),
            carried: HashMap::new(),
            next_scan_ms: None,
            scanning: false,
        }
    }
    pub fn state(&self) -> &HivecodeState {
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
    /// Start from the settings an earlier run saved.
    pub fn restore(&mut self, saved: SavedModes) {
        self.state.restore(saved);
    }
    /// The runtime session belongs to stored session `stored`. True when it opens with
    /// Hivecode on and nothing has said so yet.
    pub fn adopt(&mut self, session: &SessionId, stored: &str) -> bool {
        let mut on = self.state.adopt(session, stored);
        if std::mem::take(&mut self.launch_on) && !on {
            self.state.set(session, true);
            on = true;
        }
        let announced = self.announced.remove(session);
        on && !announced
    }
    /// A notice about the session's switch is already shown; its next open says nothing.
    pub fn mark_announced(&mut self, session: &SessionId) {
        self.announced.insert(session.clone());
    }
    /// Hermes moved the session to a new stored id.
    pub fn rotate(&mut self, session: &SessionId, stored: &str) {
        self.state.rotate(session, stored);
    }
    /// The changes to merge into the settings file, when a setting changed or a session with
    /// one was opened.
    pub fn take_saved(&mut self, now_ms: i64) -> Option<SavedChanges> {
        self.state.take_saved(now_ms)
    }
    /// A read of the local setup, for a session on a local gateway.
    pub fn setup_check(&self, session: &SessionId) -> Option<Action> {
        if self.remote {
            return None;
        }
        Some(Action::CheckSetup {
            session: session.clone(),
            profile: self
                .profiles
                .get(session)
                .map_or("default", String::as_str)
                .into(),
        })
    }
    /// Report what a setup read found, each finding once until it changes. A changed or older
    /// skill names `/hivecode reinstall`; no typed line is ever taken as an answer.
    pub fn on_setup(
        &mut self,
        _session: &SessionId,
        profile: &str,
        facts: SetupFacts,
    ) -> Vec<Action> {
        let mut actions = vec![];
        let tiers = match facts.tiers {
            TierCheck::Present => None,
            TierCheck::Missing => Some(notice::NO_TIERS.to_owned()),
            TierCheck::Unreadable(reason) => {
                Some(format!("staffing.toml could not be read ({reason})"))
            }
        };
        if tiers.is_none() && facts.missing_builtins.is_empty() {
            self.warned_setup.remove(profile);
        } else {
            let key = format!("{tiers:?}{:?}", facts.missing_builtins);
            if self.warned_setup.get(profile) != Some(&key) {
                self.warned_setup.insert(profile.into(), key);
                if let Some(reason) = &tiers {
                    actions.push(Action::Notice(notice::setup_tiers(profile, reason)));
                }
                if !facts.missing_builtins.is_empty() {
                    actions.push(Action::Notice(notice::setup_builtins(
                        &facts.missing_builtins,
                    )));
                }
            }
        }
        let (warnings, digest) = match facts.skill {
            SkillCheck::Modified { changes, digest } => (
                vec![
                    notice::skill_changed(profile, &changes),
                    notice::skill_lock(profile),
                ],
                digest,
            ),
            SkillCheck::Differs { digest } => (vec![notice::skill_differs(profile)], digest),
            SkillCheck::Outdated { digest } => (vec![notice::skill_outdated(profile)], digest),
            SkillCheck::Unchanged => {
                self.warned_skill.remove(profile);
                return actions;
            }
            SkillCheck::Unknown => return actions,
        };
        if self.warned_skill.get(profile) == Some(&digest) {
            return actions;
        }
        self.warned_skill.insert(profile.into(), digest);
        actions.extend(warnings.into_iter().map(Action::Notice));
        actions
    }
    /// The session routed a prompt, so an interrupt there says Hivecode is still on.
    pub fn note_routed(&mut self, session: &SessionId) {
        self.routed.insert(session.clone());
    }
    /// A routed prompt was sent while the session held `cells` cells, at wall-clock `wall_ms`:
    /// its turn shows the building block. A prompt sent while an earlier routed turn runs waits
    /// behind it.
    pub fn begin_authoring(
        &mut self,
        session: &SessionId,
        submission_id: &str,
        typed: &str,
        cells: usize,
        wall_ms: i64,
    ) {
        self.ended_interrupted.remove(session);
        self.interrupt_answered.remove(session);
        self.authoring
            .entry(session.clone())
            .or_default()
            .push_back(Authoring {
                submission_id: submission_id.to_owned(),
                typed: typed.to_owned(),
                from: cells,
                since_wall_ms: wall_ms,
                started_ms: None,
                carried: None,
            });
    }
    /// The submission was refused, so its prompt starts no turn.
    pub fn submission_failed(&mut self, submission_id: &str) {
        for queue in self.authoring.values_mut() {
            queue.retain(|authoring| authoring.submission_id != submission_id);
        }
        self.authoring.retain(|_, queue| !queue.is_empty());
    }
    /// The run the routed turn started, once the runs watch found it.
    fn authoring_run(&self, session: &SessionId, authoring: &Authoring) -> Option<&RunSummary> {
        self.tracked(session)
            .filter(|run| run.started_at_ms >= authoring.since_wall_ms)
    }
    /// Follow the routed turn: note when it starts, and when it ends give the one line it
    /// leaves in the transcript.
    pub fn observe_authoring(
        &mut self,
        session: &SessionId,
        cells: &[Cell],
        turn: &TurnState,
        now_ms: u64,
    ) -> Option<String> {
        let running = *turn == TurnState::Running;
        let queue = self.authoring.get_mut(session)?;
        let Some(started_ms) = queue.front()?.started_ms else {
            if !running {
                return None;
            }
            // The turn that runs belongs to the newest prompt cell. Routed prompts queued in
            // front of its own never got a turn, so they are dropped.
            let newest = cells.iter().rposition(|cell| matches!(cell, Cell::User(_)));
            let own = queue
                .iter()
                .position(|authoring| newest.is_some() && authoring.cell(cells) == newest)?;
            queue.drain(..own);
            if let Some(front) = queue.front_mut() {
                front.started_ms = Some(now_ms);
                front.carried = self.carried.remove(session);
            }
            return None;
        };
        if running {
            return None;
        }
        let authoring = queue.pop_front()?;
        if queue.is_empty() {
            self.authoring.remove(session);
        }
        let at = authoring.cell(cells)?;
        let end = match turn {
            TurnState::Finished(TurnOutcome::Completed) => AuthoringEnd::Completed,
            TurnState::Finished(TurnOutcome::Interrupted) => AuthoringEnd::Interrupted,
            _ => AuthoringEnd::Failed,
        };
        let progress = authoring.progress(cells, at);
        let elapsed_ms = authoring.elapsed_ms(started_ms, now_ms);
        // A script written and not started waits for the person's answer: the next routed turn
        // goes on from it.
        if end == AuthoringEnd::Completed
            && self.authoring_run(session, &authoring).is_none()
            && progress.stage != AuthoringStage::Running
            && let Some(script) = &progress.script
        {
            self.carried.insert(
                session.clone(),
                Carried {
                    script: script.clone(),
                    elapsed_ms,
                },
            );
        }
        let run = self.authoring_run(session, &authoring).map(|run| RunFacts {
            name: &run.name,
            status: run.status.as_str(),
            agents: run.agents_total_known,
        });
        let found = run.is_some();
        let mut line = notice::authoring_ended(&progress, end, elapsed_ms, run);
        // A remote gateway starts the run where this window cannot follow it.
        if self.remote
            && !found
            && end == AuthoringEnd::Completed
            && progress.stage == AuthoringStage::Running
        {
            line.push_str(&format!(" {}", notice::REMOTE_RUN_TAIL));
        }
        // An interrupt does not end the mode: this line says so, in place of a separate one.
        let answered = self.interrupt_answered.remove(session);
        if end == AuthoringEnd::Interrupted && self.state.is_on(session) {
            line.push_str(&format!(" {}", notice::STILL_ON_TAIL));
            if !answered {
                self.ended_interrupted.insert(session.clone());
            }
        }
        Some(line)
    }
    /// The building block, while the routed turn runs and no run it started is tracked: from
    /// then on the run dock shows in its place.
    pub fn authoring_view(
        &self,
        session: &SessionId,
        cells: &[Cell],
        now_ms: u64,
    ) -> Option<AuthoringView> {
        let authoring = self.authoring.get(session)?.front()?;
        let started_ms = authoring.started_ms?;
        if self.authoring_run(session, authoring).is_some() {
            return None;
        }
        let at = authoring.cell(cells)?;
        Some(AuthoringView {
            progress: authoring.progress(cells, at),
            elapsed_ms: authoring.elapsed_ms(started_ms, now_ms),
        })
    }
    /// What Hermes's answer to an interrupt adds: that Hivecode is still on, unless the
    /// interrupted routed turn's own line says so.
    pub fn after_interrupt(&mut self, session: &SessionId) -> Option<Action> {
        if self.ended_interrupted.remove(session) {
            return None;
        }
        let building = self
            .authoring
            .get(session)
            .and_then(VecDeque::front)
            .is_some_and(|authoring| authoring.started_ms.is_some());
        if building && self.state.is_on(session) {
            self.interrupt_answered.insert(session.clone());
            return None;
        }
        (self.routed.contains(session) && self.state.is_on(session))
            .then(|| Action::Notice(notice::STILL_ON.into()))
    }
    pub fn handle_local_command(
        &mut self,
        arg: &str,
        foreground: Option<Foreground<'_>>,
    ) -> Vec<Action> {
        let command = match HivecodeCommand::parse(arg) {
            Ok(c) => c,
            Err(_) => return vec![Action::Notice(notice::USAGE.into())],
        };
        let Some(Foreground { session, profile }) = foreground else {
            return vec![Action::Notice(notice::NO_SESSION.into())];
        };
        self.bind(session, profile);
        if command == HivecodeCommand::Reinstall {
            // The reinstalled skill goes whole with the next routed prompt.
            return vec![if self.remote {
                Action::Notice(notice::reinstall_remote(profile))
            } else {
                Action::ReinstallSkill {
                    session: session.clone(),
                    profile: profile.into(),
                }
            }];
        }
        if command == HivecodeCommand::Off
            || (command == HivecodeCommand::Toggle && self.state.is_on(session))
        {
            self.state.set(session, false);
            // A script left waiting is not carried into a routed turn after Hivecode is on
            // again. Prompts already sent as workflows keep their blocks.
            self.carried.remove(session);
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
        command: HivecodeCommand,
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
        if command == HivecodeCommand::Status {
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
            SkillAvailability::Missing if self.remote => notice::missing_remote(profile),
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
        self.intercept_submit_with_model(session, text, text, None)
    }
    /// As [`Self::intercept_submit`], with the model and provider the session runs on.
    pub fn intercept_submit_with_model(
        &self,
        session: &SessionId,
        draft: &str,
        request: &str,
        session_model: Option<(&str, &str)>,
    ) -> Option<Vec<Action>> {
        match route_submit_with_request(draft, request, self.state.is_on(session), session_model) {
            SubmitRoute::Plain => None,
            SubmitRoute::Workflow {
                command_line,
                one_shot,
            } => {
                // The building block shows once the turn starts; no line is printed here.
                let mut actions = vec![];
                // The conversation holds the skill already: the prompt goes with a reminder,
                // and names the session model again when it changed since it last went.
                if !one_shot && self.state.skill_loaded(session) {
                    let names = session_model_names(session_model)
                        .filter(|names| self.state.named_model(session) != Some(names.as_str()));
                    actions.push(Action::SubmitCommand {
                        text: follow_up_with_model(request, names.as_deref()),
                        display: draft.into(),
                        submission_id: crate::commands::fresh_submission_id(),
                    });
                    return Some(actions);
                }
                actions.push(Action::RunCommand {
                    session: session.clone(),
                    original_text: draft.into(),
                    restore_text: (request != draft).then(|| request.to_owned()),
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
                    notice::missing_remote(profile)
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
    /// A routed prompt sent `text`: note the session model it named.
    pub fn model_sent(&mut self, session: &SessionId, text: &str) {
        self.state.model_sent(session, text);
    }
    /// A routed prompt that named the session model never started: the next one names it.
    pub fn forget_model(&mut self, session: &SessionId) {
        self.state.forget_model(session);
    }
    /// A queued routed prompt was edited: its turn's block follows the edited text. The prompt
    /// is found by the submission id its queue row carries; only a row without one is matched
    /// by its text, oldest first. Hermes admits the edit under its new submission id at the back
    /// of the queue, so the prompt's entry moves there too.
    pub fn retype_authoring(&mut self, session: &SessionId, edit: &Retype<'_>) {
        let Some(queue) = self.authoring.get_mut(session) else {
            return;
        };
        // A retried edit was already applied.
        if queue
            .iter()
            .any(|authoring| authoring.submission_id == edit.submission_id)
        {
            return;
        }
        let Some(at) = queue.iter().position(|authoring| {
            authoring.started_ms.is_none()
                && if edit.input_id.is_empty() {
                    authoring.typed == edit.old
                } else {
                    authoring.submission_id == edit.input_id
                }
        }) else {
            return;
        };
        if let Some(mut authoring) = queue.remove(at) {
            authoring.submission_id = edit.submission_id.to_owned();
            edit.new.clone_into(&mut authoring.typed);
            queue.push_back(authoring);
        }
    }
    /// The notice a session that opens with Hivecode on prints.
    pub fn opened_notice(&self) -> &'static str {
        if self.remote {
            notice::OPENED_ON_REMOTE
        } else {
            notice::OPENED_ON
        }
    }
    /// Say which conversation the session holds now, before a prompt is routed.
    pub fn note_epoch(&mut self, session: &SessionId, epoch: &SkillEpoch) {
        self.state.note_epoch(session, epoch);
    }
    /// The session's conversation may have lost the skill: its next routed prompt sends it whole.
    pub fn rearm(&mut self, session: &SessionId) {
        self.state.rearm(session);
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
                            notice::run_outcome(row.status.as_str()),
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
