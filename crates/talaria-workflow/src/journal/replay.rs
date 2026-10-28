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
    pub fn session(&self) -> Option<String> {
        self.current().find_map(|r| match &r.record {
            Record::SessionBound { session_id, .. } => Some(session_id.clone()),
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
            }),
            _ => None,
        })
    }
    pub fn turns(&self, dir: &Path) -> Result<Vec<SettledTurn>, JournalError> {
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
}
impl Index {
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
        let mut summary = FinishSummary::default();
        for key in &self.encountered {
            if let Some(last) = self.calls.get(key).and_then(CallHistory::last) {
                match &last.record {
                    Record::CallFinished { outcome, .. } => match outcome {
                        CallOutcome::Completed { .. } => summary.completed += 1,
                        CallOutcome::Failed { .. } => summary.failed += 1,
                        CallOutcome::Skipped => summary.skipped += 1,
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
                outcome: CallOutcome::Failed { .. },
                ..
            } | Record::CallRetryFinished { .. }
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
            outcome: CallOutcome::Failed { .. },
            ..
        }
    );
    let attention = matches!(next, Record::CallAttention { .. });
    let allowed = match &last.record {
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
        Record::CallResolved { episode, .. } => {
            let parked = h.attention();
            failed
                || matches!(next,Record::CallAttention{episode:e,..} if episode.checked_add(1)==Some(*e))
                || matches!(next,Record::TurnSettled{turn,admission_id,..} if parked.is_some_and(|p|p.turn==Some(*turn)&&p.admission_id.as_ref()==Some(admission_id)))
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
            .filter(|r| matches!(r.record, Record::TurnSettled { .. }))
            .count();
        if *turns as usize != settled {
            return Err(corrupt(
                line,
                "finished turn count differs from settlements",
            ));
        }
    }
    Ok(())
}
