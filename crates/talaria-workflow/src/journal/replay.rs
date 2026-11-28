use super::*;
use crate::engine::{CallId, EngineConfig, EngineEvent, ReplayEntry, ReplayLog};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionCause {
    Unknown,
    Lost,
    IdentityConflict,
    /// A stop request ended the agent's turn, and the runner did not send that stop as part of
    /// stopping the run.
    Stopped,
}
impl AttentionCause {
    /// The answers an operator may give a call parked for this cause, as the command line
    /// spells them.
    pub fn choices(self) -> &'static [&'static str] {
        match self {
            Self::Stopped => &["fail", "rerun", "abort", "use-reply"],
            _ => &["fail", "rerun", "abort"],
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Lost => "lost",
            Self::IdentityConflict => "identity_conflict",
            Self::Stopped => "stopped",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReplaySummary {
    pub resume_count: u32,
    pub finished: usize,
    pub in_flight: usize,
    pub parked: Vec<ParkedCall>,
    pub orphans: Vec<OrphanCall>,
    pub running_orphans: Vec<OrphanCall>,
    pub settled_usage: Vec<TurnUsage>,
    pub unsettled: Vec<UnsettledTurn>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ParkedCall {
    pub key: String,
    pub label: String,
    pub attempt: u32,
    pub episode: u32,
    pub cause: AttentionCause,
    pub turn: Option<u32>,
    pub session_id: Option<String>,
    pub admission_id: Option<String>,
    pub execution_generation: Option<i64>,
    /// Why the call was parked, in plain words.
    pub detail: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OrphanCall {
    pub key: String,
    pub label: String,
    pub attempt: u32,
    pub last_record: String,
    pub session_id: Option<String>,
    pub admission_id: Option<String>,
    pub last_status: Option<AdmissionState>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TurnUsage {
    pub key: String,
    pub attempt: u32,
    pub turn: u32,
    pub admission_id: String,
    pub outcome: String,
    pub usage: Option<Value>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct UnsettledTurn {
    pub key: String,
    pub attempt: u32,
    pub turn: u32,
    pub admission_id: String,
    pub last_status: AdmissionState,
}
#[derive(Clone, Debug, PartialEq)]
pub enum JournalNotice {
    NeedsOperator {
        key: String,
        label: String,
        attempt: u32,
        episode: u32,
        cause: AttentionCause,
        session_id: Option<String>,
        admission_id: Option<String>,
        /// Why the call was parked, in plain words.
        detail: String,
    },
    Resolved {
        key: String,
        attempt: u32,
        episode: u32,
        resolution: Resolution,
    },
    TurnSettled {
        key: String,
        attempt: u32,
        turn: u32,
        admission_id: String,
        usage: Option<Value>,
    },
    Warning(JournalWarning),
    /// A note about an agent's route (untracked work found, or a route that could not be
    /// checked), shown live as it is journaled.
    RouteNote {
        message: String,
    },
    /// A stopped call the operator answered with use-reply now waits for the next reply on its
    /// session. Nothing ends the wait but that reply.
    WaitingForReply {
        key: String,
        label: String,
        session_id: String,
    },
}
#[derive(Clone, Debug, Default)]
pub(crate) struct CallHistory {
    pub records: Vec<RecordLine>,
    pub execution: u32,
}
impl CallHistory {
    pub fn last(&self) -> Option<&RecordLine> {
        self.records.last()
    }
    pub fn attempt(&self) -> u32 {
        self.last()
            .and_then(|r| r.record.identity())
            .map_or(0, |(_, a)| a)
    }
    pub fn current(&self) -> impl DoubleEndedIterator<Item = &RecordLine> {
        let a = self.attempt();
        self.records
            .iter()
            .filter(move |r| r.record.identity().is_some_and(|(_, n)| n == a))
    }
    pub fn intent(&self) -> Option<(&CallId, &ResolvedLaunch, &str, &str)> {
        self.current().find_map(|r| match &r.record {
            Record::CallIntent {
                call_id,
                launch,
                request_id,
                label,
                ..
            } => Some((call_id, launch, request_id.as_str(), label.as_str())),
            _ => None,
        })
    }
    pub fn report_baseline(&self) -> Option<&str> {
        self.current().find_map(|r| match &r.record {
            Record::CallIntent {
                report_baseline, ..
            } => report_baseline.as_deref(),
            _ => None,
        })
    }
    pub fn session(&self) -> Option<String> {
        self.current().find_map(|r| match &r.record {
            Record::SessionBound { session_id, .. } => Some(session_id.clone()),
            _ => None,
        })
    }
    /// What a route check needs: the call's label and launch, the session id the run holds and
    /// the stored session it was bound to. `None` when the journal holds no intent or no bound
    /// session for this call, in which case the route cannot be checked.
    pub fn route(&self) -> Option<(&str, &ResolvedLaunch, String, String)> {
        let (_, launch, _, label) = self.intent()?;
        Some((label, launch, self.session()?, self.stored_session()?))
    }
    pub fn stored_session(&self) -> Option<String> {
        self.current().find_map(|r| match &r.record {
            Record::SessionBound {
                stored_session_id, ..
            } => Some(stored_session_id.clone()),
            _ => None,
        })
    }
    pub fn attention(&self) -> Option<ParkedCall> {
        self.current().rev().find_map(|r| match &r.record {
            Record::CallAttention {
                key,
                attempt,
                episode,
                cause,
                turn,
                admission_id,
                execution_generation,
                detail,
                ..
            } => Some(ParkedCall {
                key: key.clone(),
                label: self.intent().map_or("", |t| t.3).into(),
                attempt: *attempt,
                episode: *episode,
                cause: *cause,
                turn: *turn,
                session_id: self.session(),
                admission_id: admission_id.clone(),
                execution_generation: *execution_generation,
                detail: detail.clone(),
            }),
            _ => None,
        })
    }
    /// Every admission this call has made or adopted, across all its attempts, and every
    /// other admission the port had seen on its session when it parked. None of them can be
    /// the reply a stopped call waits for: that reply comes after the stop.
    pub fn known_admissions(&self) -> Vec<String> {
        let mut known: Vec<String> = Vec::new();
        let mut add = |id: &String| {
            if !known.contains(id) {
                known.push(id.clone());
            }
        };
        for r in &self.records {
            match &r.record {
                Record::TurnAdmitted { admission_id, .. }
                | Record::TurnSettled { admission_id, .. } => add(admission_id),
                Record::CallAttention {
                    admission_id,
                    seen_admissions,
                    ..
                } => {
                    admission_id
                        .iter()
                        .chain(seen_admissions)
                        .for_each(&mut add);
                }
                _ => {}
            }
        }
        known
    }
    /// The attempts whose last settled turn failed with a context overflow.
    fn overflowed_attempts(&self) -> BTreeSet<u32> {
        let mut last = BTreeMap::new();
        for r in &self.records {
            if let Record::TurnSettled {
                attempt,
                result_meta,
                ..
            } = &r.record
            {
                last.insert(*attempt, result_meta["failure_reason"] == CONTEXT_OVERFLOW);
            }
        }
        last.into_iter()
            .filter_map(|(attempt, overflowed)| overflowed.then_some(attempt))
            .collect()
    }
    /// How many times this call was started again after its agent ran out of context, across
    /// every attempt and every execution of the run. A restart is an attempt that ended with a
    /// retry finish after a context overflow, so the count survives a resume.
    pub fn overflow_restarts(&self) -> u32 {
        let overflowed = self.overflowed_attempts();
        self.records
            .iter()
            .filter(|r| {
                matches!(&r.record, Record::CallRetryFinished { attempt, .. } if overflowed.contains(attempt))
            })
            .count() as u32
    }
    /// How many attempts of this call ended because the agent ran out of context.
    pub fn overflowed(&self) -> u32 {
        self.overflowed_attempts().len() as u32
    }
    /// Whether the previous attempt's last settled turn ran out of context, however that
    /// attempt ended (a restart, or a failure that a resume runs again). The current attempt's
    /// first prompt then carries the note about the earlier work in its folder.
    pub fn previous_attempt_overflowed(&self) -> bool {
        self.attempt()
            .checked_sub(1)
            .is_some_and(|previous| self.overflowed_attempts().contains(&previous))
    }
    /// The current attempt's settled turns, one per turn number. A turn settles twice only when
    /// an operator answered a stopped turn with `use_reply`: the adopted reply replaces the
    /// stopped settlement.
    pub fn turns(&self, dir: &Path) -> Result<Vec<SettledTurn>, JournalError> {
        let mut turns: Vec<SettledTurn> = Vec::new();
        for turn in self.settlements(dir)? {
            match turns.iter_mut().find(|t| t.turn == turn.turn) {
                Some(slot) => *slot = turn,
                None => turns.push(turn),
            }
        }
        Ok(turns)
    }
    fn settlements(&self, dir: &Path) -> Result<Vec<SettledTurn>, JournalError> {
        self.current()
            .filter_map(|r| match &r.record {
                Record::TurnSettled {
                    turn,
                    outcome,
                    final_text,
                    final_text_ref,
                    result_meta,
                    usage,
                    ..
                } => Some(
                    blobs::text(dir, final_text, final_text_ref, r.seq).map(|text| SettledTurn {
                        turn: *turn,
                        outcome: outcome.clone(),
                        final_text: text,
                        result_meta: result_meta.clone(),
                        usage: usage.clone(),
                    }),
                ),
                _ => None,
            })
            .collect()
    }
}
#[derive(Default)]
pub(crate) struct Index {
    pub records: Vec<RecordLine>,
    pub calls: BTreeMap<String, CallHistory>,
    pub execution: u32,
    pub started: BTreeSet<CallId>,
    pub encountered: BTreeSet<String>,
    pub replay_keys: BTreeMap<CallId, String>,
    pub resolving: BTreeSet<(String, u32, u32)>,
    pub restart_attempts: BTreeMap<String, u32>,
    pub active: bool,
    pub closed: bool,
    pub failure: Option<String>,
    next_delivery: u64,
    refused: usize,
}
impl Index {
    /// A recovered report is still a failure, but it already has a durable script delivery.
    pub fn kept_report(&self, call: &CallId) -> Option<(&str, &CallOutcome, u64)> {
        self.calls.iter().find_map(|(key, h)| {
            let line = h.last()?;
            let Record::CallFinished {
                outcome: outcome @ CallOutcome::FailedWithReport { .. },
                ..
            } = &line.record
            else {
                return None;
            };
            (h.intent()?.0 == call).then_some((key.as_str(), outcome, line.seq))
        })
    }
    /// Check a record against its call's history before it is written, with the same rules
    /// `apply` uses. Only call records are checked; the sequence is the writer's.
    pub fn check_next(&self, record: &Record) -> Result<(), JournalError> {
        let n = self.records.len() as u64 + 1;
        record.validate(n)?;
        if let Some((key, attempt)) = record.identity() {
            match self.calls.get(key) {
                Some(h) => validate_transition(h, record, n)?,
                None if attempt != 0 || !is_start(record) => {
                    return Err(corrupt(n, "attempt must start with intent or short finish"));
                }
                None => {}
            }
        }
        Ok(())
    }
    pub fn apply(&mut self, line: RecordLine, dir: &Path) -> Result<(), JournalError> {
        let n = line.seq;
        if n != self.records.len() as u64 + 1 {
            return Err(corrupt(n, "sequence gap"));
        }
        if n == 1 && !matches!(line.record, Record::RunStarted { .. }) {
            return Err(corrupt(n, "first record is not run_started"));
        }
        line.record.validate(n)?;
        for r in line.record.refs() {
            blobs::check_size(dir, r, n)?;
        }
        match &line.record {
            Record::RunStarted { .. } if n != 1 => {
                return Err(corrupt(n, "run_started must be first"));
            }
            Record::RunResumed { resume_count, .. } => {
                if *resume_count != self.execution + 1 {
                    return Err(corrupt(n, "resume count gap"));
                }
                self.execution = *resume_count;
                self.next_delivery = 0;
                self.refused = 0;
                self.started.clear();
                self.encountered.clear();
                self.active = false;
            }
            _ => {}
        }
        if let Some(event) = blobs::event(dir, &line.record, n)? {
            match event {
                EngineEvent::AgentStarted { request, replayed } => {
                    if !replayed {
                        self.started.insert(request.call_id);
                    } else if let Some(key) = self.replay_keys.get(&request.call_id) {
                        self.encountered.insert(key.clone());
                    }
                    self.active = true;
                }
                EngineEvent::AgentFinished {
                    completion_seq: Some(seq),
                    ..
                } => {
                    if seq != self.next_delivery {
                        return Err(corrupt(n, "completion sequence gap"));
                    }
                    self.next_delivery += 1;
                }
                EngineEvent::AgentRefused { .. } => self.refused += 1,
                _ => {}
            }
        }
        if let Some((key, attempt)) = line.record.identity() {
            if let Some(h) = self.calls.get(key) {
                validate_transition(h, &line.record, n)?;
            } else if attempt != 0 || !is_start(&line.record) {
                return Err(corrupt(n, "attempt must start with intent or short finish"));
            }
            let key = key.to_owned();
            let h = self.calls.entry(key).or_default();
            h.execution = self.execution;
            h.records.push(line.clone());
        }
        self.records.push(line);
        Ok(())
    }
    pub fn summary(&self) -> ReplaySummary {
        let mut result = ReplaySummary {
            resume_count: self.execution,
            ..Default::default()
        };
        let mut admissions: BTreeMap<(String, u32, u32, String), AdmissionState> = BTreeMap::new();
        for (key, h) in &self.calls {
            let Some(last) = h.last() else { continue };
            if matches!(
                &last.record,
                Record::CallFinished {
                    outcome: CallOutcome::Completed { .. } | CallOutcome::Skipped,
                    ..
                }
            ) {
                result.finished += 1;
            } else if matches!(&last.record, Record::CallAttention { .. }) {
                if let Some(p) = h.attention() {
                    result.parked.push(p);
                }
            } else if h.intent().is_some() && !matches!(&last.record, Record::CallFinished { .. }) {
                result.in_flight += 1;
            }
            let mut admission = None;
            let mut status = None;
            for r in &h.records {
                match &r.record {
                    Record::TurnAdmitted {
                        attempt,
                        turn,
                        admission_id,
                        status: s,
                        ..
                    } => {
                        admissions.insert((key.clone(), *attempt, *turn, admission_id.clone()), *s);
                        if *attempt == h.attempt() {
                            admission = Some(admission_id.clone());
                            status = Some(*s);
                        }
                    }
                    Record::CallAttention {
                        attempt,
                        turn: Some(turn),
                        admission_id: Some(id),
                        cause: AttentionCause::Unknown,
                        ..
                    } => {
                        if let Some(s) =
                            admissions.get_mut(&(key.clone(), *attempt, *turn, id.clone()))
                        {
                            *s = AdmissionState::Unknown;
                        }
                        if *attempt == h.attempt() {
                            status = Some(AdmissionState::Unknown);
                        }
                    }
                    Record::TurnSettled {
                        attempt,
                        turn,
                        admission_id,
                        outcome,
                        usage,
                        ..
                    } => {
                        admissions.remove(&(key.clone(), *attempt, *turn, admission_id.clone()));
                        result.settled_usage.push(TurnUsage {
                            key: key.clone(),
                            attempt: *attempt,
                            turn: *turn,
                            admission_id: admission_id.clone(),
                            outcome: outcome.clone(),
                            usage: usage.clone(),
                        });
                        if *attempt == h.attempt() {
                            status = Some(AdmissionState::Terminal);
                        }
                    }
                    _ => {}
                }
            }
            if self.active && !self.encountered.contains(key) {
                let orphan = OrphanCall {
                    key: key.clone(),
                    label: h.intent().map_or("", |t| t.3).into(),
                    attempt: h.attempt(),
                    last_record: last.record.kind().into(),
                    session_id: h.session(),
                    admission_id: admission,
                    last_status: status,
                };
                if matches!(
                    status,
                    Some(AdmissionState::Queued | AdmissionState::Started)
                ) {
                    result.running_orphans.push(orphan.clone());
                }
                result.orphans.push(orphan);
            }
        }
        result.unsettled = admissions
            .into_iter()
            .map(
                |((key, attempt, turn, admission_id), last_status)| UnsettledTurn {
                    key,
                    attempt,
                    turn,
                    admission_id,
                    last_status,
                },
            )
            .collect();
        result
    }
    pub fn finish_summary(&self) -> FinishSummary {
        let mut summary = FinishSummary {
            refused: self.refused,
            ..FinishSummary::default()
        };
        for key in &self.encountered {
            if let Some(last) = self.calls.get(key).and_then(CallHistory::last) {
                match &last.record {
                    Record::CallFinished { outcome, .. } => match outcome {
                        CallOutcome::Completed { .. } => summary.completed += 1,
                        CallOutcome::Failed { .. } | CallOutcome::FailedWithReport { .. } => {
                            summary.failed += 1
                        }
                        CallOutcome::Skipped => summary.skipped += 1,
                        CallOutcome::Stopped { .. } => summary.stopped += 1,
                    },
                    Record::CallAttention { .. } => summary.parked += 1,
                    _ => summary.unfinished += 1,
                }
            }
        }
        summary
    }
    pub fn config(
        &mut self,
        dir: &RunDir,
        resolver: &dyn LaunchResolver,
    ) -> Result<EngineConfig, JournalError> {
        let m = dir.manifest();
        let mut config = EngineConfig::new(m.run_id.as_str(), m.started_at_ms, m.seed);
        config.args = m.args.clone();
        if self.execution == 0 {
            return Ok(config);
        }
        let mut history: Vec<CallId> = Vec::new();
        let mut deliveries = Vec::new();
        let mut diverged = false;
        let mut requests = BTreeMap::new();
        let mut logged_keys: BTreeMap<CallId, String> = BTreeMap::new();
        for line in &self.records {
            if matches!(line.record, Record::RunResumed { .. }) {
                merge_history(&mut history, &mut deliveries, diverged);
                diverged = false;
            }
            if let Record::CallIntent { key, call_id, .. } = &line.record {
                logged_keys.insert(call_id.clone(), key.clone());
            }
            if let Some(event) = blobs::event(dir.path(), &line.record, line.seq)? {
                match event {
                    EngineEvent::AgentStarted { request, .. } => {
                        requests.insert(request.call_id.clone(), request);
                    }
                    EngineEvent::AgentFinished {
                        call_id,
                        completion_seq: Some(_),
                        ..
                    } => deliveries.push(call_id),
                    EngineEvent::ReplayDiverged { .. } => diverged = true,
                    _ => {}
                }
            }
        }
        merge_history(&mut history, &mut deliveries, diverged);
        let mut entries = Vec::new();
        self.replay_keys.clear();
        for call_id in history {
            // The worker's worktree can be removed after recovery. Its report is in the journal.
            if let Some((key, outcome, seq)) = self.kept_report(&call_id) {
                entries.push(ReplayEntry {
                    call_id: call_id.clone(),
                    outcome: blobs::outcome(dir.path(), outcome, seq)?,
                });
                self.replay_keys.insert(call_id, key.to_owned());
                continue;
            }
            let Some(request) = requests.get(&call_id) else {
                continue;
            };
            let Ok(launch) = resolver.resolve(request) else {
                continue;
            };
            let key = journal_key(&call_id, &launch);
            let matches = logged_keys.get(&call_id).is_some_and(|k| k == &key)
                || self.calls.get(&key).is_some_and(|h| {
                    matches!(
                        h.last().map(|r| &r.record),
                        Some(Record::CallFinished {
                            outcome: CallOutcome::Skipped,
                            ..
                        })
                    )
                });
            if !matches {
                continue;
            }
            if let Some(line) = self.calls.get(&key).and_then(CallHistory::last)
                && let Record::CallFinished {
                    outcome: outcome @ (CallOutcome::Completed { .. } | CallOutcome::Skipped),
                    ..
                } = &line.record
            {
                entries.push(ReplayEntry {
                    call_id: call_id.clone(),
                    outcome: blobs::outcome(dir.path(), outcome, line.seq)?,
                });
                self.replay_keys.insert(call_id, key);
            }
        }
        config.replay = Some(ReplayLog { entries });
        Ok(config)
    }
}
fn merge_history(history: &mut Vec<CallId>, deliveries: &mut Vec<CallId>, diverged: bool) {
    let agrees = history.iter().zip(deliveries.iter()).all(|(a, b)| a == b);
    if diverged || !agrees || deliveries.len() > history.len() {
        *history = std::mem::take(deliveries);
    } else {
        deliveries.clear();
    }
}
fn is_start(r: &Record) -> bool {
    matches!(
        r,
        Record::CallIntent { .. }
            | Record::CallFinished {
                outcome: CallOutcome::Skipped | CallOutcome::Failed { .. },
                turns: 0,
                ..
            }
    )
}
fn validate_transition(h: &CallHistory, next: &Record, line: u64) -> Result<(), JournalError> {
    // Retry finishes obey precisely the failed-finish grammar.
    if let Record::CallRetryFinished {
        key,
        attempt,
        outcome,
        turns,
    } = next
    {
        return validate_transition(
            h,
            &Record::CallFinished {
                key: key.clone(),
                attempt: *attempt,
                outcome: outcome.clone(),
                turns: *turns,
            },
            line,
        );
    }

    let Some(last) = h.last() else { return Ok(()) };
    let a = h.attempt();
    let next_a = next.identity().map_or(a, |(_, a)| a);
    if next_a != a {
        let due = matches!(
            &last.record,
            Record::CallFinished {
                outcome: CallOutcome::Failed { .. } | CallOutcome::Stopped { .. },
                ..
            } | Record::CallRetryFinished { .. }
                | Record::CallReportRestartRequested { .. }
                | Record::CallResolved {
                    resolution: Resolution::Rerun | Resolution::Abort,
                    ..
                }
        );
        if due && a.checked_add(1) == Some(next_a) && is_start(next) {
            return Ok(());
        }
        return Err(corrupt(line, "attempt gap or forbidden retry"));
    }
    let failed = matches!(
        next,
        Record::CallFinished {
            outcome: CallOutcome::Failed { .. } | CallOutcome::Stopped { .. },
            ..
        }
    );
    let attention = matches!(next, Record::CallAttention { .. });
    let allowed = match &last.record {
        Record::CallFinished {
            outcome: CallOutcome::FailedWithReport { .. },
            ..
        } => {
            matches!(next, Record::CallReportRestartRequested { .. })
        }
        Record::CallIntent { .. } => {
            matches!(
                next,
                Record::SessionBound { .. }
                    | Record::CallFinished {
                        outcome: CallOutcome::Skipped,
                        turns: 0,
                        ..
                    }
            ) || attention
                || failed
        }
        Record::SessionBound { .. } => {
            matches!(next, Record::TurnIntent { turn: 0, .. }) || attention || failed
        }
        Record::TurnIntent { turn, .. } => {
            matches!(next,Record::TurnAdmitted{turn:t,..} if t==turn) || attention || failed
        }
        Record::TurnAdmitted {
            turn, admission_id, ..
        } => {
            matches!(next,Record::TurnSettled{turn:t,admission_id:id,..} if t==turn && id==admission_id)
                || attention
                || failed
        }
        Record::TurnSettled { turn, .. } => {
            matches!(next, Record::CallFinished { .. })
                || attention
                || (*turn == 0 && matches!(next, Record::TurnIntent { turn: 1, .. }))
        }
        Record::CallAttention { episode, .. } => {
            matches!(next,Record::CallResolved{episode:e,..} if e==episode)
        }
        Record::CallResolved {
            episode,
            resolution,
            ..
        } => {
            let parked = h.attention();
            // An adopted reply settles the stopped turn again, under the admission another
            // client made on the same session.
            let adopted = *resolution == Resolution::UseReply
                && parked
                    .as_ref()
                    .is_some_and(|p| p.cause == AttentionCause::Stopped);
            failed
                || matches!(next,Record::CallAttention{episode:e,..} if episode.checked_add(1)==Some(*e))
                || matches!(next,Record::TurnSettled{turn,admission_id,..} if parked.as_ref().is_some_and(|p|p.turn==Some(*turn)&&(adopted||p.admission_id.as_ref()==Some(admission_id))))
        }
        _ => false,
    };
    if !allowed {
        return Err(corrupt(
            line,
            format!("{} cannot follow {}", next.kind(), last.record.kind()),
        ));
    }
    if let Record::CallAttention {
        episode,
        turn,
        admission_id,
        ..
    } = next
    {
        let previous = h.attention();
        let expected = previous
            .as_ref()
            .map_or(Some(0), |p| p.episode.checked_add(1));
        if expected != Some(*episode) {
            return Err(corrupt(line, "attention episode gap"));
        }
        if let Some(t) = turn
            && !h
                .current()
                .any(|r| matches!(&r.record,Record::TurnIntent{turn:n,..} if n==t))
        {
            return Err(corrupt(line, "attention names an absent turn"));
        }
        if let Some(id) = admission_id
            && !h
                .current()
                .any(|r| matches!(&r.record,Record::TurnAdmitted{admission_id:a,..} if a==id))
        {
            return Err(corrupt(line, "attention names an absent admission"));
        }
    }
    if let Record::CallFinished { turns, .. } = next {
        let settled = h
            .current()
            .filter_map(|r| match r.record {
                Record::TurnSettled { turn, .. } => Some(turn),
                _ => None,
            })
            .collect::<BTreeSet<_>>()
            .len();
        if *turns as usize != settled {
            return Err(corrupt(
                line,
                "finished turn count differs from settlements",
            ));
        }
    }
    Ok(())
}
