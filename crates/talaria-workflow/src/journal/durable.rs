use super::{replay::CallHistory, *};
use crate::engine::{AgentBackend, AgentOutcome, AgentRequest, BudgetSnapshot};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;

pub struct JournaledBackend<P, T, L, G> {
    inner: Arc<Inner<P, T, L, G>>,
}
struct Inner<P, T, L, G> {
    journal: Arc<RunJournal>,
    port: P,
    policy: T,
    resolver: L,
    gate: G,
    retry: RetryPolicy,
    retried: std::sync::Mutex<std::collections::BTreeSet<String>>,
    queued: std::sync::Mutex<std::collections::BTreeSet<(String, u32)>>,
}
impl<P: GatewayPort, T: TurnPolicy, L: LaunchResolver, G: AdmissionGate>
    JournaledBackend<P, T, L, G>
{
    pub fn new(
        journal: Arc<RunJournal>,
        port: P,
        policy: T,
        resolver: L,
        gate: G,
        retry: RetryPolicy,
    ) -> Self {
        journal.ensure_polling();
        Self {
            inner: Arc::new(Inner {
                journal,
                port,
                policy,
                resolver,
                gate,
                retry,
                retried: std::sync::Mutex::new(std::collections::BTreeSet::new()),
                queued: std::sync::Mutex::new(std::collections::BTreeSet::new()),
            }),
        }
    }
    /// Settle calls whose admission gate was cancelled before any gateway work.
    pub(crate) async fn cancel_queued(&self) -> Result<(), JournalError> {
        // A writer barrier drains intent writes submitted before engine cancellation.
        self.inner
            .journal
            .record_engine_event(&crate::engine::EngineEvent::Log {
                message: "runner: cancelling queued calls".into(),
                workflow_path: vec![],
            })
            .await?;
        let queued = self.inner.queued.lock().map_err(io_error)?.clone();
        for (key, attempt) in queued {
            let history = self.inner.journal.history(&key)?;
            if matches!(history.last().map(|r| &r.record), Some(Record::CallIntent { attempt: stored, .. }) if *stored == attempt)
            {
                self.inner
                    .finish_call(
                        &key,
                        attempt,
                        0,
                        AgentOutcome::Failed {
                            reason: "cancelled before admission".into(),
                        },
                    )
                    .await?;
            }
        }
        Ok(())
    }
}
impl<P: GatewayPort, T: TurnPolicy, L: LaunchResolver, G: AdmissionGate> AgentBackend
    for JournaledBackend<P, T, L, G>
{
    fn run_agent(&self, request: AgentRequest) -> BoxFuture<'static, AgentOutcome> {
        let inner = self.inner.clone();
        Box::pin(async move {
            match tokio::time::timeout(
                Duration::from_secs(30),
                inner.journal.started_durable(&request.call_id),
            )
            .await
            {
                Err(_) => {
                    return AgentOutcome::Abort {
                        reason: "engine events are not being journaled".into(),
                    };
                }
                Ok(Err(e)) => return abort_journal(e),
                Ok(Ok(())) => {}
            }
            match inner.run(request.clone()).await {
                Ok(outcome) => outcome,
                Err(JournalError::RecordTooLarge { t, .. }) => {
                    let result = async {
                        let launch = inner
                            .resolver
                            .resolve(&request)
                            .map_err(JournalError::Manifest)?;
                        let key = journal_key(&request.call_id, &launch);
                        let history = inner.journal.history(&key)?;
                        inner
                            .finish_call(
                                &key,
                                history.attempt(),
                                history.turns(inner.journal.dir().path())?.len() as u32,
                                AgentOutcome::Failed {
                                    reason: format!("journal record too large: {t}"),
                                },
                            )
                            .await
                    }
                    .await;
                    result.unwrap_or_else(abort_journal)
                }
                Err(e) => abort_journal(e),
            }
        })
    }
    fn budget(&self) -> BudgetSnapshot {
        self.inner.gate.budget()
    }
}
fn abort_journal(e: JournalError) -> AgentOutcome {
    AgentOutcome::Abort {
        reason: format!("journal write failed: {e}"),
    }
}
impl<P: GatewayPort, T: TurnPolicy, L: LaunchResolver, G: AdmissionGate> Inner<P, T, L, G> {
    /// A gate refusal ends the call as stopped when the budget caused it.
    fn skipped(&self) -> AgentOutcome {
        self.gate
            .budget_stop()
            .map_or(AgentOutcome::Skipped, |reason| AgentOutcome::Stopped {
                reason,
            })
    }
    async fn finish_call(
        &self,
        key: &str,
        attempt: u32,
        turns: u32,
        outcome: AgentOutcome,
    ) -> Result<AgentOutcome, JournalError> {
        if matches!(outcome, AgentOutcome::Abort { .. }) {
            return Ok(outcome);
        }
        let record = Record::CallFinished {
            key: key.into(),
            attempt,
            turns,
            outcome: CallOutcome::from_agent(outcome.clone()),
        };
        let finished = match self.journal.append(record).await {
            Ok(_) => Ok(outcome),
            Err(JournalError::RecordTooLarge { t, .. }) => {
                let outcome = AgentOutcome::Failed {
                    reason: format!("journal record too large: {t}"),
                };
                self.journal
                    .append(Record::CallFinished {
                        key: key.into(),
                        attempt,
                        turns,
                        outcome: CallOutcome::from_agent(outcome.clone()),
                    })
                    .await
                    .map(|_| outcome)
            }
            Err(e) => Err(e),
        };
        if let Ok(ended) = &finished {
            // Only a failed call can leave work behind on its route.
            self.close_session(key, matches!(ended, AgentOutcome::Failed { .. }))
                .await;
        }
        finished
    }
    /// End the call's session in Hermes once its outcome is recorded; left open, it lingers
    /// until the runner's connection closes. First sweep the call's route, so nothing the run
    /// does not follow keeps working after the call. A failed end changes nothing the run
    /// depends on, so it is only noted in the journal.
    async fn close_session(&self, key: &str, sweep: bool) {
        let Ok(history) = self.journal.history(key) else {
            return;
        };
        let (Some(session), Some((_, launch, _, label))) = (history.session(), history.intent())
        else {
            return;
        };
        if sweep && let Some(stored) = history.stored_session() {
            self.sweep_route(label, launch, &session, &stored).await;
        }
        let profile = launch.profile.clone();
        let failure = match tokio::time::timeout(
            Duration::from_secs(15),
            self.port.close_session(profile, session.clone()),
        )
        .await
        {
            Ok(Ok(())) => return,
            Ok(Err(PortError::Refused { reason, detail, .. })) => detail.unwrap_or(reason),
            Ok(Err(e)) => e.to_string(),
            Err(_) => "no answer within 15 seconds".into(),
        };
        self.note(format!(
            "runner: agent session {session} was not ended in Hermes ({failure}); Hermes ends it when the run's connection closes"
        ))
        .await;
    }
    async fn note(&self, message: String) {
        let _ = self
            .journal
            .record_engine_event(&crate::engine::EngineEvent::Log {
                message,
                workflow_path: vec![],
            })
            .await;
    }
    /// Look for work on the call's route that the run is not following, stop it, and write what
    /// was found to the journal. Hermes resets a conversation that outgrew its model and then
    /// starts the next queued input on the route's fresh stored session, which the run never
    /// bound. That session would keep editing the call's working folder.
    async fn sweep_route(&self, label: &str, launch: &ResolvedLaunch, session: &str, stored: &str) {
        let folder = launch.cwd.as_deref().unwrap_or("its working folder");
        let swept = tokio::time::timeout(
            Duration::from_secs(60),
            self.port
                .sweep_route(launch.profile.clone(), session.into(), stored.into()),
        )
        .await;
        let sweep = match swept {
            Ok(Ok(sweep)) => sweep,
            Ok(Err(e)) => {
                self.note(format!(
                    "{ROUTE_WARNING}agent {label}: could not check whether anything else was still running on session {session} ({e}); look in {folder}"
                ))
                .await;
                return;
            }
            Err(_) => {
                self.note(format!(
                    "{ROUTE_WARNING}agent {label}: no answer within 60 seconds when checking whether anything else was still running on session {session}; look in {folder}"
                ))
                .await;
                return;
            }
        };
        if let (Some(moved), Some(tokens)) = (&sweep.moved_to, sweep.moved_tokens) {
            self.gate
                .count_untracked_usage(&format!("untracked:{moved}"), tokens);
        }
        if !sweep.found_untracked() && sweep.problems.is_empty() {
            return;
        }
        let mut parts = Vec::new();
        if let Some(moved) = &sweep.moved_to {
            parts.push(format!(
                "Hermes moved the conversation onto a new stored session {moved}, which this run did not start"
            ));
        }
        if sweep.cancelled > 0 {
            parts.push(format!(
                "Talaria cancelled {} turn(s) there that it was not tracking",
                sweep.cancelled
            ));
        }
        if let Some(tokens) = sweep.moved_tokens {
            parts.push(format!(
                "Hermes reports {tokens} tokens used there, now counted in the run's spend"
            ));
        }
        if sweep.still_active {
            parts.push(format!(
                "something is still running there and could not be stopped, so it may still be changing files in {folder}"
            ));
        }
        parts.extend(sweep.problems.iter().cloned());
        self.note(format!(
            "{ROUTE_WARNING}agent {label}, session {session}, working folder {folder}: {}",
            parts.join("; ")
        ))
        .await;
    }
    async fn run(&self, request: AgentRequest) -> Result<AgentOutcome, JournalError> {
        let launch = match self.resolver.resolve(&request) {
            Ok(launch) => launch,
            Err(reason) => {
                let key = keys::sha(
                    keys::canonical(&json!({"v":1,"call":request.call_id,"resolver_error":reason}))
                        .as_bytes(),
                )[..32]
                    .to_owned();
                self.journal.encounter(&key)?;
                let h = self.journal.history(&key)?;
                let restart = self.journal.restart_requested(&key, h.attempt())?;
                if !restart
                    && h.execution == self.journal.execution()
                    && let Some(line) = h.last()
                    && let Record::CallFinished { outcome, .. } = &line.record
                {
                    return blobs::outcome(self.journal.dir().path(), outcome, line.seq);
                }
                let attempt =
                    if h.last().is_some() && (h.execution != self.journal.execution() || restart) {
                        h.attempt()
                            .checked_add(1)
                            .ok_or_else(|| io_error("attempt overflow"))?
                    } else {
                        h.attempt()
                    };
                return self
                    .finish_call(&key, attempt, 0, AgentOutcome::Failed { reason })
                    .await;
            }
        };
        let key = journal_key(&request.call_id, &launch);
        self.journal.encounter(&key)?;
        let mut permit: Option<GatePermit> = None;
        let mut runner = PortRunner::new(&self.port, &self.retry);
        let mut next_attempt = None;
        loop {
            let h = self.journal.history(&key)?;
            let old_attempt = h.attempt();
            let restart_requested = self.journal.restart_requested(&key, old_attempt)?;
            let failed_resume = (h.execution != self.journal.execution() || restart_requested)
                && matches!(
                    h.last().map(|r| &r.record),
                    Some(
                        Record::CallFinished {
                            outcome: CallOutcome::Failed { .. } | CallOutcome::Stopped { .. },
                            ..
                        } | Record::CallRetryFinished { .. }
                    )
                );
            let fresh = h.last().is_none() || next_attempt.is_some() || failed_resume;
            let attempt = if let Some(a) = next_attempt.take() {
                a
            } else if failed_resume {
                old_attempt
                    .checked_add(1)
                    .ok_or_else(|| io_error("attempt overflow"))?
            } else {
                old_attempt
            };
            if fresh {
                if self.journal.execution() > 1 && next_attempt.is_none() {
                    tokio::task::yield_now().await;
                }
                // Register in scheduler order before asynchronous journal writes can reorder calls.
                let mut admission = self.gate.acquire(&request, &launch, GateReason::Fresh);
                let ready = futures::poll!(&mut admission);
                // Keep intent durable while the admission gate holds the call.
                self.queued
                    .lock()
                    .map_err(io_error)?
                    .insert((key.clone(), attempt));
                let record = Record::CallIntent {
                    key: key.clone(),
                    attempt,
                    call_id: request.call_id.clone(),
                    label: request.label.clone(),
                    phase: request.phase.clone(),
                    workflow_path: request.workflow_path.clone(),
                    launch: launch.clone(),
                    request_id: request_id(self.journal.dir().run_id(), &key, attempt),
                };
                match self.journal.append(record).await {
                    Err(JournalError::RecordTooLarge { t, .. }) => {
                        return self
                            .finish_call(
                                &key,
                                attempt,
                                0,
                                AgentOutcome::Failed {
                                    reason: format!("journal record too large: {t}"),
                                },
                            )
                            .await;
                    }
                    Err(e) => return Err(e),
                    Ok(_) => {}
                }
                let decision = match ready {
                    std::task::Poll::Ready(decision) => decision,
                    std::task::Poll::Pending => admission.await,
                };
                self.queued
                    .lock()
                    .map_err(io_error)?
                    .remove(&(key.clone(), attempt));
                permit = match decision {
                    GateDecision::Proceed(p) => Some(p),
                    GateDecision::Skip => {
                        return self.finish_call(&key, attempt, 0, self.skipped()).await;
                    }
                    GateDecision::Abort { reason } => return Ok(AgentOutcome::Abort { reason }),
                };
                continue;
            }
            let Some(last) = h.last() else {
                return Err(io_error("missing call state"));
            };
            if let Record::CallFinished { outcome, .. } = &last.record {
                return blobs::outcome(self.journal.dir().path(), outcome, last.seq);
            }
            if matches!(last.record, Record::CallAttention { .. }) {
                let parked = h.attention().ok_or_else(|| io_error("missing attention"))?;
                self.journal.notice(JournalNotice::NeedsOperator {
                    key: key.clone(),
                    label: request.label.clone(),
                    attempt,
                    episode: parked.episode,
                    cause: parked.cause,
                    session_id: parked.session_id.clone(),
                    admission_id: parked.admission_id.clone(),
                });
                drop(permit.take());
                self.journal.await_resolution(&parked).await?;
                continue;
            }
            if let Record::CallResolved {
                resolution, source, ..
            } = &last.record
            {
                let resolution = *resolution;
                let source = *source;
                let parked = h
                    .attention()
                    .ok_or_else(|| io_error("resolution without attention"))?;
                drop(permit.take());
                self.journal.await_resolution(&parked).await?;
                if let (AttentionCause::Unknown, Some(generation), Some(session), Some(admission)) = (
                    parked.cause,
                    parked.execution_generation,
                    parked.session_id.clone(),
                    parked.admission_id.clone(),
                ) {
                    let profile = h
                        .intent()
                        .map_or(launch.profile.clone(), |i| i.1.profile.clone());
                    let subscription = Some((profile.clone(), session.clone()));
                    let resolved = runner
                        .call(
                            || {
                                self.port.resolve_unknown(
                                    profile.clone(),
                                    session.clone(),
                                    admission.clone(),
                                    generation,
                                )
                            },
                            subscription.clone(),
                        )
                        .await;
                    match resolved {
                        Err(StepError::Abort(reason)) => return Ok(AgentOutcome::Abort { reason }),
                        Err(StepError::Port(e)) if e.reason() != "stale_generation" => {
                            if let Some(outcome) =
                                self.handle_error(&key, &h, &e, &mut permit).await?
                            {
                                return Ok(outcome);
                            }
                            continue;
                        }
                        _ => {}
                    }
                    let receipt = match runner
                        .call(
                            || {
                                self.port.receipt(
                                    profile.clone(),
                                    session.clone(),
                                    admission.clone(),
                                )
                            },
                            subscription,
                        )
                        .await
                    {
                        Ok(r) => r,
                        Err(StepError::Abort(reason)) => return Ok(AgentOutcome::Abort { reason }),
                        Err(StepError::Port(e)) => {
                            if let Some(o) = self.handle_error(&key, &h, &e, &mut permit).await? {
                                return Ok(o);
                            }
                            continue;
                        }
                    };
                    if receipt.status != AdmissionState::Terminal {
                        self.park(
                            &key,
                            &h,
                            AttentionCause::Unknown,
                            Some(&receipt),
                            "resolution generation changed",
                            &mut permit,
                        )
                        .await?;
                        continue;
                    }
                    if receipt.result.is_some() {
                        self.settle(&key, attempt, parked.turn.unwrap_or_default(), receipt)
                            .await?;
                        if resolution == Resolution::Abort
                            && h.execution == self.journal.execution()
                        {
                            return Ok(AgentOutcome::Abort {
                                reason: "operator aborted the run".into(),
                            });
                        }
                        continue;
                    }
                }
                if restart_requested && resolution == Resolution::Fail {
                    self.finish_call(
                        &key,
                        attempt,
                        h.turns(self.journal.dir().path())?.len() as u32,
                        AgentOutcome::Failed {
                            reason: "operator requested restart".into(),
                        },
                    )
                    .await?;
                    continue;
                }
                match resolution {
                    Resolution::Fail => {
                        let reason = if source == ResolutionSource::Policy {
                            "on_unknown is fail"
                        } else {
                            match parked.cause {
                                AttentionCause::Unknown => {
                                    "operator marked an unknown turn as failed"
                                }
                                AttentionCause::Lost => "operator marked a lost session as failed",
                                AttentionCause::IdentityConflict => {
                                    "operator marked a conflicting retry as failed"
                                }
                            }
                        };
                        return self
                            .finish_call(
                                &key,
                                attempt,
                                h.turns(self.journal.dir().path())?.len() as u32,
                                AgentOutcome::Failed {
                                    reason: reason.into(),
                                },
                            )
                            .await;
                    }
                    Resolution::Abort
                        if h.execution == self.journal.execution() && !restart_requested =>
                    {
                        return Ok(AgentOutcome::Abort {
                            reason: "operator aborted the run".into(),
                        });
                    }
                    Resolution::Rerun | Resolution::Abort => {
                        next_attempt = Some(
                            attempt
                                .checked_add(1)
                                .ok_or_else(|| io_error("attempt overflow"))?,
                        );
                        continue;
                    }
                }
            }
            let (_, stored_launch, stored_request_id, stored_label) =
                h.intent().ok_or_else(|| io_error("missing call intent"))?;
            if permit.is_none() {
                let reason = if matches!(last.record, Record::CallIntent { .. }) {
                    self.queued
                        .lock()
                        .map_err(io_error)?
                        .insert((key.clone(), attempt));
                    tokio::task::yield_now().await;
                    GateReason::Fresh
                } else {
                    GateReason::Reattach
                };
                let decision = self.gate.acquire(&request, stored_launch, reason).await;
                if reason == GateReason::Fresh {
                    self.queued
                        .lock()
                        .map_err(io_error)?
                        .remove(&(key.clone(), attempt));
                }
                match decision {
                    GateDecision::Proceed(p) => permit = Some(p),
                    GateDecision::Abort { reason } => return Ok(AgentOutcome::Abort { reason }),
                    GateDecision::Skip if reason == GateReason::Fresh => {
                        return self.finish_call(&key, attempt, 0, self.skipped()).await;
                    }
                    GateDecision::Skip => {
                        return Ok(AgentOutcome::Abort {
                            reason: "reattach gate refused existing work".into(),
                        });
                    }
                }
            }
            let profile = stored_launch.profile.clone();
            let session = h.session();
            let result: Result<(), StepError> = match &last.record {
                Record::CallIntent { .. } => {
                    let create = CreateSession {
                        request_id: stored_request_id.into(),
                        launch: stored_launch.clone(),
                        label: stored_label.into(),
                    };
                    match runner
                        .call(|| self.port.create_session(create.clone()), None)
                        .await
                    {
                        Ok(bound) => {
                            runner
                                .attached
                                .insert((profile.clone(), bound.session_id.clone()));
                            self.journal
                                .append(Record::SessionBound {
                                    key: key.clone(),
                                    attempt,
                                    session_id: bound.session_id,
                                    stored_session_id: bound.stored_session_id,
                                })
                                .await?;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                Record::SessionBound { .. } => {
                    if let Some(reason) = self.gate.budget_stop() {
                        drop(permit.take());
                        return self
                            .finish_call(&key, attempt, 0, AgentOutcome::Stopped { reason })
                            .await;
                    }
                    let mut text = self.policy.first_turn(&request, stored_launch);
                    if h.follows_overflow_restart() {
                        text = format!("{OVERFLOW_RESTART_NOTE}\n\n{text}");
                    }
                    self.turn_intent(&key, attempt, 0, text).await?;
                    Ok(())
                }
                Record::TurnIntent {
                    turn,
                    submission_id,
                    text,
                    text_ref,
                    ..
                } => {
                    let text = blobs::text(self.journal.dir().path(), text, text_ref, last.seq)?
                        .ok_or_else(|| corrupt(last.seq, "missing turn text"))?;
                    let session = session.clone().ok_or_else(|| io_error("missing session"))?;
                    let submit = SubmitTurn {
                        profile: profile.clone(),
                        session_id: session.clone(),
                        submission_id: submission_id.clone(),
                        text,
                    };
                    match runner
                        .call(
                            || self.port.submit(submit.clone()),
                            Some((profile.clone(), session)),
                        )
                        .await
                    {
                        Ok(r) => {
                            self.journal
                                .append(Record::TurnAdmitted {
                                    key: key.clone(),
                                    attempt,
                                    turn: *turn,
                                    admission_id: r.admission_id,
                                    status: r.status,
                                    outcome: r.outcome,
                                    execution_generation: r.execution_generation,
                                    authority_epoch: r.authority_epoch,
                                })
                                .await?;
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                Record::TurnAdmitted {
                    turn,
                    admission_id,
                    status,
                    execution_generation,
                    ..
                } => {
                    let session = session.clone().ok_or_else(|| io_error("missing session"))?;
                    if *status == AdmissionState::Unknown {
                        let r = Receipt {
                            admission_id: admission_id.clone(),
                            session_id: session,
                            status: *status,
                            outcome: None,
                            authority_epoch: 0,
                            execution_generation: *execution_generation,
                            result: None,
                            usage: None,
                        };
                        self.park(
                            &key,
                            &h,
                            AttentionCause::Unknown,
                            Some(&r),
                            "admission is unknown",
                            &mut permit,
                        )
                        .await?;
                        continue;
                    }
                    let subscription = Some((profile.clone(), session.clone()));
                    let r = runner
                        .call(
                            || {
                                self.port.receipt(
                                    profile.clone(),
                                    session.clone(),
                                    admission_id.clone(),
                                )
                            },
                            subscription.clone(),
                        )
                        .await;
                    let r = match r {
                        Ok(r)
                            if matches!(
                                r.status,
                                AdmissionState::Queued | AdmissionState::Started
                            ) =>
                        {
                            runner
                                .call(
                                    || {
                                        self.port.wait_settled(
                                            profile.clone(),
                                            session.clone(),
                                            admission_id.clone(),
                                        )
                                    },
                                    subscription,
                                )
                                .await
                        }
                        other => other,
                    };
                    match r {
                        Ok(r) if r.status == AdmissionState::Unknown => {
                            self.park(
                                &key,
                                &h,
                                AttentionCause::Unknown,
                                Some(&r),
                                "admission is unknown",
                                &mut permit,
                            )
                            .await?;
                            Ok(())
                        }
                        Ok(r) if r.status == AdmissionState::Terminal => {
                            self.settle(&key, attempt, *turn, r).await?;
                            Ok(())
                        }
                        Ok(_) => Err(StepError::Port(PortError::Transport(
                            "wait returned an unsettled admission".into(),
                        ))),
                        Err(e) => Err(e),
                    }
                }
                Record::TurnSettled { .. } => {
                    let turns = h.turns(self.journal.dir().path())?;
                    let next = self.policy.after_turn(&request, &turns);
                    // A finished answer is kept; anything else would spend past the budget.
                    if !matches!(
                        next,
                        NextStep::Finish(
                            AgentOutcome::Completed(_) | AgentOutcome::CutShort { .. }
                        )
                    ) && let Some(reason) = self.gate.budget_stop()
                    {
                        drop(permit.take());
                        return self
                            .finish_call(
                                &key,
                                attempt,
                                turns.len() as u32,
                                AgentOutcome::Stopped { reason },
                            )
                            .await;
                    }
                    match next {
                        NextStep::Finish(outcome) => {
                            if matches!(outcome, AgentOutcome::Failed { .. })
                                && let Some(last) = turns.last()
                                && last.result_meta["failure_reason"] == CONTEXT_OVERFLOW
                            {
                                // The count comes from the journal, so a resume keeps it.
                                let used = h.overflow_restarts();
                                if used >= MAX_OVERFLOW_RESTARTS {
                                    let reason = overflow_exhausted(h.overflowed(), used);
                                    return self
                                        .finish_call(
                                            &key,
                                            attempt,
                                            turns.len() as u32,
                                            AgentOutcome::Failed { reason },
                                        )
                                        .await;
                                }
                                drop(permit.take());
                                if self.gate.authorise_restart(
                                    &request,
                                    &launch,
                                    last,
                                    attempt,
                                    used + 1,
                                    MAX_OVERFLOW_RESTARTS,
                                ) {
                                    self.journal
                                        .append(Record::CallRetryFinished {
                                            key: key.clone(),
                                            attempt,
                                            outcome: CallOutcome::from_agent(outcome.clone()),
                                            turns: turns.len() as u32,
                                        })
                                        .await?;
                                    let _ = self
                                        .journal
                                        .record_engine_event(&crate::engine::EngineEvent::Log {
                                            message: format!(
                                                "agent {}: restarted {} of {MAX_OVERFLOW_RESTARTS} after a context overflow",
                                                request.label,
                                                used + 1
                                            ),
                                            workflow_path: request.workflow_path.clone(),
                                        })
                                        .await;
                                    // End the old session and sweep its route first, so nothing
                                    // untracked keeps working in the folder the restart reuses.
                                    self.close_session(&key, true).await;
                                    next_attempt = Some(
                                        attempt
                                            .checked_add(1)
                                            .ok_or_else(|| io_error("attempt overflow"))?,
                                    );
                                    continue;
                                }
                                return self
                                    .finish_call(&key, attempt, turns.len() as u32, outcome)
                                    .await;
                            }
                            if matches!(outcome, AgentOutcome::Failed { .. })
                                && !self.retried.lock().map_err(io_error)?.contains(&key)
                            {
                                drop(permit.take());
                                if let Some(last) = turns.last()
                                    && self
                                        .gate
                                        .retry_after_failure(&request, &launch, last, attempt)
                                {
                                    self.journal
                                        .append(Record::CallRetryFinished {
                                            key: key.clone(),
                                            attempt,
                                            outcome: CallOutcome::from_agent(outcome.clone()),
                                            turns: turns.len() as u32,
                                        })
                                        .await?;
                                    self.close_session(&key, true).await;
                                    self.retried.lock().map_err(io_error)?.insert(key.clone());
                                    next_attempt = Some(
                                        attempt
                                            .checked_add(1)
                                            .ok_or_else(|| io_error("attempt overflow"))?,
                                    );
                                    continue;
                                }
                            }
                            return self
                                .finish_call(&key, attempt, turns.len() as u32, outcome)
                                .await;
                        }
                        NextStep::Turn { .. } if turns.len() >= MAX_TURNS_PER_CALL as usize => {
                            return self
                                .finish_call(
                                    &key,
                                    attempt,
                                    turns.len() as u32,
                                    AgentOutcome::Failed {
                                        reason: "turn limit reached".into(),
                                    },
                                )
                                .await;
                        }
                        NextStep::Turn { text } => {
                            self.turn_intent(&key, attempt, turns.len() as u32, text)
                                .await?;
                            Ok(())
                        }
                    }
                }
                _ => return Err(io_error("unhandled journal state")),
            };
            match result {
                Ok(()) => {}
                Err(StepError::Abort(reason)) => return Ok(AgentOutcome::Abort { reason }),
                Err(StepError::Port(e)) => {
                    if let Some(outcome) = self.handle_error(&key, &h, &e, &mut permit).await? {
                        return Ok(outcome);
                    }
                }
            }
        }
    }
    async fn turn_intent(
        &self,
        key: &str,
        attempt: u32,
        turn: u32,
        text: String,
    ) -> Result<(), JournalError> {
        self.journal
            .append(Record::TurnIntent {
                key: key.into(),
                attempt,
                turn,
                submission_id: submission_id(self.journal.dir().run_id(), key, attempt, turn),
                text: Some(text),
                text_ref: None,
            })
            .await?;
        Ok(())
    }
    async fn settle(
        &self,
        key: &str,
        attempt: u32,
        turn: u32,
        r: Receipt,
    ) -> Result<(), JournalError> {
        self.journal
            .append(settled_record(key, attempt, turn, r))
            .await?;
        Ok(())
    }
    async fn handle_error(
        &self,
        key: &str,
        h: &CallHistory,
        e: &PortError,
        permit: &mut Option<GatePermit>,
    ) -> Result<Option<AgentOutcome>, JournalError> {
        let cause = match e {
            PortError::Refused { reason, field, .. }
                if reason == "admission_conflict"
                    || (reason == "invalid_params" && field.as_deref() == Some("request_id")) =>
            {
                Some(AttentionCause::IdentityConflict)
            }
            PortError::Refused { reason, .. } if reason == "not_found" => {
                Some(AttentionCause::Lost)
            }
            _ => None,
        };
        if let Some(cause) = cause {
            self.park(key, h, cause, None, &e.to_string(), permit)
                .await?;
            return Ok(None);
        }
        let reason = if e.reason() == "queue_full" {
            "queue full".into()
        } else {
            e.to_string()
        };
        self.finish_call(
            key,
            h.attempt(),
            h.turns(self.journal.dir().path())?.len() as u32,
            AgentOutcome::Failed { reason },
        )
        .await
        .map(Some)
    }
    async fn park(
        &self,
        key: &str,
        h: &CallHistory,
        cause: AttentionCause,
        receipt: Option<&Receipt>,
        detail: &str,
        permit: &mut Option<GatePermit>,
    ) -> Result<(), JournalError> {
        let episode = match h.attention() {
            Some(p) => p
                .episode
                .checked_add(1)
                .ok_or_else(|| io_error("episode overflow"))?,
            None => 0,
        };
        let turn = h.current().rev().find_map(|r| match r.record {
            Record::TurnIntent { turn, .. } => Some(turn),
            _ => None,
        });
        let admission = h.current().rev().find_map(|r| match &r.record {
            Record::TurnAdmitted {
                admission_id,
                execution_generation,
                ..
            } => Some((admission_id.clone(), *execution_generation)),
            _ => None,
        });
        self.journal
            .append(Record::CallAttention {
                key: key.into(),
                attempt: h.attempt(),
                episode,
                cause,
                turn,
                admission_id: receipt
                    .map(|r| r.admission_id.clone())
                    .or_else(|| admission.as_ref().map(|a| a.0.clone())),
                execution_generation: receipt
                    .and_then(|r| r.execution_generation)
                    .or_else(|| admission.and_then(|a| a.1)),
                detail: cut(detail, 4096),
            })
            .await?;
        drop(permit.take());
        Ok(())
    }
}
enum StepError {
    Port(PortError),
    Abort(String),
}
struct PortRunner<'a, P> {
    port: &'a P,
    policy: &'a RetryPolicy,
    outage: Option<Instant>,
    failures: usize,
    attached: std::collections::BTreeSet<(String, String)>,
}
impl<'a, P: GatewayPort> PortRunner<'a, P> {
    fn new(port: &'a P, policy: &'a RetryPolicy) -> Self {
        Self {
            port,
            policy,
            outage: None,
            failures: 0,
            attached: std::collections::BTreeSet::new(),
        }
    }
    fn success(&mut self) {
        self.outage = None;
        self.failures = 0;
    }
    async fn call<R>(
        &mut self,
        mut op: impl FnMut() -> BoxFuture<'static, Result<R, PortError>>,
        subscription: Option<(String, String)>,
    ) -> Result<R, StepError> {
        let mut attach = subscription
            .as_ref()
            .is_some_and(|s| !self.attached.contains(s));
        let mut permission_retry = false;
        loop {
            let result = if attach {
                let Some((profile, session)) = &subscription else {
                    return Err(StepError::Abort("missing attach target".into()));
                };
                match self.port.attach(profile.clone(), session.clone()).await {
                    Ok(()) => {
                        self.success();
                        self.attached.insert((profile.clone(), session.clone()));
                        attach = false;
                        continue;
                    }
                    Err(e) => Err(e),
                }
            } else {
                op().await
            };
            match result {
                Ok(value) => {
                    self.success();
                    return Ok(value);
                }
                Err(e)
                    if e.reason() == "permission_denied"
                        && subscription.is_some()
                        && !permission_retry =>
                {
                    permission_retry = true;
                    attach = true;
                }
                Err(e) if e.retryable() => {
                    let started = *self.outage.get_or_insert_with(Instant::now);
                    let remaining = self.policy.max_outage.saturating_sub(started.elapsed());
                    if remaining.is_zero() {
                        return Err(StepError::Abort(format!("gateway unavailable: {e}")));
                    }
                    let millis = [500, 1000, 2000, 4000, 8000]
                        .get(self.failures)
                        .copied()
                        .unwrap_or(15000);
                    self.failures += 1;
                    tokio::time::sleep(Duration::from_millis(millis).min(remaining)).await;
                    if started.elapsed() >= self.policy.max_outage {
                        return Err(StepError::Abort(format!("gateway unavailable: {e}")));
                    }
                    attach = subscription.is_some();
                }
                Err(e) => return Err(StepError::Port(e)),
            }
        }
    }
}

pub(crate) fn settled_record(key: &str, attempt: u32, turn: u32, r: Receipt) -> Record {
    let mut meta = serde_json::Map::new();
    if let Some(result) = r.result.as_ref().and_then(Value::as_object) {
        for field in [
            "completed",
            "failed",
            "interrupted",
            "turn_exit_reason",
            "model",
            "provider",
            "failure_reason",
            "error",
        ] {
            if let Some(v) = result.get(field) {
                if matches!(field, "failure_reason" | "error") && !v.is_string() {
                    continue;
                }
                meta.insert(
                    field.into(),
                    match v {
                        Value::String(s) => Value::String(cut(s, 1024)),
                        _ => v.clone(),
                    },
                );
            }
        }
    }
    let final_text = r
        .result
        .as_ref()
        .and_then(|v| v.get("final_response"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    Record::TurnSettled {
        key: key.into(),
        attempt,
        turn,
        admission_id: r.admission_id,
        outcome: r.outcome.unwrap_or_else(|| "interrupted".into()),
        final_text,
        final_text_ref: None,
        result_meta: Value::Object(meta),
        usage: r.usage,
    }
}

/// Starts every journal note about work found on a route the run was not following; the run
/// result lists these.
pub const ROUTE_WARNING: &str = "runner: untracked work: ";
/// Put at the top of the first prompt of an agent started again after a context overflow.
const OVERFLOW_RESTART_NOTE: &str = "Note: an earlier attempt at this task ran out of context and was stopped. Its working folder still holds whatever it wrote or committed. Check that folder first and continue from it rather than starting over. Keep your notes in files rather than in the conversation.";
fn overflow_exhausted(overflowed: u32, restarts: u32) -> String {
    format!(
        "the agent ran out of context {overflowed} times (the first attempt and {restarts} restarts), so it was not started again"
    )
}
