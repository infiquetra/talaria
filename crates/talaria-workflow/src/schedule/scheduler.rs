use super::{accounting::Accounting, *};
use crate::{
    engine::{AgentRequest, BudgetSnapshot},
    journal::{
        AdmissionGate, GateDecision, GatePermit, GateReason, JournalReader, Record, ResolvedLaunch,
        SettledTurn, journal_key,
    },
    staffing::{Availability, Clock, LimitScope, RosterCache},
};
use chrono::{DateTime, Utc};
use futures_util::future::BoxFuture;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, Weak},
    time::Duration,
};
use tokio::sync::{Notify, mpsc, oneshot};
type LaneId = (String, String);
#[derive(Clone)]
pub struct Scheduler {
    inner: Arc<Inner>,
}
struct Inner {
    cfg: SchedulerConfig,
    rosters: RosterCache,
    path: PathBuf,
    clock: Arc<dyn Clock>,
    runtime: tokio::runtime::Handle,
    state: Mutex<State>,
    wake: Notify,
}
struct Lane {
    width: u32,
    in_flight: u32,
    pause: Option<(Option<DateTime<Utc>>, PauseCause)>,
    forced: BTreeMap<String, u64>,
}
#[derive(Default)]
struct Profile {
    attempted: bool,
    reading: bool,
    last_read: Option<DateTime<Utc>>,
    next_read: Option<DateTime<Utc>>,
    failures: u32,
}
struct Waiting {
    id: u64,
    key: String,
    label: String,
    lane: LaneId,
    model: String,
    reason: Option<WaitReason>,
    tx: oneshot::Sender<GateDecision>,
}
struct State {
    width: u32,
    in_flight: u32,
    lanes: BTreeMap<LaneId, Lane>,
    profiles: BTreeMap<String, Profile>,
    queue: VecDeque<Waiting>,
    next: u64,
    accounting: Accounting,
    offset: u64,
    last_fold: Option<DateTime<Utc>>,
    seen_failures: BTreeSet<(String, u32, u32)>,
    halved: bool,
    budgets: BTreeSet<BudgetKind>,
    subscribers: Vec<mpsc::UnboundedSender<TimedEvent>>,
    journal_error: Option<String>,
}
fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}
fn later(now: DateTime<Utc>, duration: Duration) -> DateTime<Utc> {
    now + chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::MAX)
}
impl Scheduler {
    pub async fn start(
        cfg: SchedulerConfig,
        rosters: RosterCache,
        journal_path: PathBuf,
        clock: Arc<dyn Clock>,
    ) -> Result<(Self, SchedulerEvents), ScheduleError> {
        if !(1..=64).contains(&cfg.limits.max_in_flight)
            || cfg
                .providers
                .values()
                .any(|p| !(1..=64).contains(&p.max_in_flight))
            || cfg.cooldown.is_zero()
            || cfg.roster_min_interval.is_zero()
            || cfg.budget.tokens == Some(0)
            || cfg
                .budget
                .money_usd
                .is_some_and(|v| !v.is_finite() || v <= 0.0)
        {
            return Err(ScheduleError::Config(
                "widths must be 1..=64; budgets and refresh intervals must be positive".into(),
            ));
        }
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| ScheduleError::Config("scheduler needs a Tokio runtime".into()))?;
        rosters.get_or_load(&cfg.run_profile).await?;
        let width = cfg.limits.max_in_flight;
        let profile = cfg.run_profile.clone();
        let now = clock.now();
        let inner = Arc::new(Inner {
            cfg,
            rosters,
            path: journal_path,
            clock,
            runtime,
            state: Mutex::new(State {
                width,
                in_flight: 0,
                lanes: BTreeMap::new(),
                profiles: BTreeMap::from([(
                    profile,
                    Profile {
                        attempted: true,
                        last_read: Some(now),
                        ..Default::default()
                    },
                )]),
                queue: VecDeque::new(),
                next: 0,
                accounting: Accounting::default(),
                offset: 0,
                last_fold: None,
                seen_failures: BTreeSet::new(),
                halved: false,
                budgets: BTreeSet::new(),
                subscribers: vec![],
                journal_error: None,
            }),
            wake: Notify::new(),
        });
        {
            let mut s = inner.lock();
            inner.fold(&mut s, true).map_err(ScheduleError::Journal)?;
        }
        let this = Self { inner };
        let events = this.subscribe();
        let weak = Arc::downgrade(&this.inner);
        tokio::spawn(async move {
            pump(weak).await;
        });
        Ok((this, events))
    }
    pub fn subscribe(&self) -> SchedulerEvents {
        let (tx, rx) = mpsc::unbounded_channel();
        self.inner.lock().subscribers.push(tx);
        SchedulerEvents(rx)
    }
    pub fn budget(&self) -> BudgetSnapshot {
        let mut s = self.inner.lock();
        let now = self.inner.clock.now();
        if s.last_fold
            .is_none_or(|t| (now - t).num_milliseconds() >= 250)
        {
            if let Err(e) = self.inner.fold(&mut s, false) {
                s.journal_error = Some(e);
            }
            self.inner.wake.notify_one();
        }
        self.inner.snapshot(&s)
    }
    pub fn status(&self) -> SchedulerStatus {
        let s = self.inner.lock();
        SchedulerStatus {
            global_width: s.width,
            in_flight: s.in_flight,
            queued: s.queue.len() as u32,
            lanes: s
                .lanes
                .iter()
                .map(|((profile, provider), l)| LaneStatus {
                    profile: profile.clone(),
                    provider: provider.clone(),
                    width: l.width,
                    in_flight: l.in_flight,
                    queued: s
                        .queue
                        .iter()
                        .filter(|w| w.lane == (profile.clone(), provider.clone()))
                        .count() as u32,
                    paused_until: l.pause.as_ref().and_then(|(t, _)| t.map(stamp)),
                    pause_cause: l.pause.as_ref().map(|(_, c)| *c),
                })
                .collect(),
            spend: s.accounting.spend.clone(),
        }
    }
}
impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn lane<'a>(&self, s: &'a mut State, id: &LaneId) -> &'a mut Lane {
        s.lanes.entry(id.clone()).or_insert_with(|| Lane {
            width: self
                .cfg
                .providers
                .get(&id.1)
                .map_or(self.cfg.limits.max_in_flight, |p| p.max_in_flight),
            in_flight: 0,
            pause: None,
            forced: BTreeMap::new(),
        })
    }
    fn event(&self, s: &mut State, event: SchedulerEvent) {
        let timed = TimedEvent {
            at: stamp(self.clock.now()),
            event,
        };
        s.subscribers.retain(|tx| tx.send(timed.clone()).is_ok());
    }
    fn snapshot(&self, s: &State) -> BudgetSnapshot {
        BudgetSnapshot {
            tokens_total: self.cfg.budget.tokens,
            tokens_spent: s.accounting.spend.tokens,
            money_total: self.cfg.budget.money_usd,
            money_spent: s.accounting.spend.money_usd,
        }
    }
    fn reached(&self, s: &State) -> Option<BudgetKind> {
        if self
            .cfg
            .budget
            .tokens
            .is_some_and(|t| s.accounting.spend.tokens >= t)
        {
            Some(BudgetKind::Tokens)
        } else if self
            .cfg
            .budget
            .money_usd
            .is_some_and(|m| s.accounting.spend.money_usd >= m)
        {
            Some(BudgetKind::Money)
        } else {
            None
        }
    }
    fn budget_events(&self, s: &mut State) {
        for kind in [BudgetKind::Tokens, BudgetKind::Money] {
            let reached = match kind {
                BudgetKind::Tokens => self
                    .cfg
                    .budget
                    .tokens
                    .is_some_and(|t| s.accounting.spend.tokens >= t),
                BudgetKind::Money => self
                    .cfg
                    .budget
                    .money_usd
                    .is_some_and(|m| s.accounting.spend.money_usd >= m),
            };
            if reached && s.budgets.insert(kind) {
                self.event(
                    s,
                    SchedulerEvent::BudgetReached {
                        kind,
                        tokens_spent: s.accounting.spend.tokens,
                        tokens_total: self.cfg.budget.tokens,
                        money_spent_usd: s.accounting.spend.money_usd,
                        money_total_usd: self.cfg.budget.money_usd,
                    },
                );
            }
        }
    }
    fn failure(
        &self,
        s: &mut State,
        identity: (&str, u32, u32),
        launch: &ResolvedLaunch,
        reason: &str,
        silent: bool,
    ) {
        let (key, attempt, turn) = identity;
        if !matches!(
            reason,
            "rate_limit" | "upstream_rate_limit" | "overloaded" | "billing"
        ) || !s.seen_failures.insert((key.into(), attempt, turn))
        {
            return;
        }
        let id = (
            launch.profile.clone(),
            launch.provider.clone().unwrap_or_default(),
        );
        if reason == "billing" {
            if !silent {
                self.lane(s, &id).pause = Some((None, PauseCause::OutOfCredit));
                self.event(
                    s,
                    SchedulerEvent::LanePaused {
                        profile: id.0,
                        provider: id.1,
                        until: None,
                        cause: PauseCause::OutOfCredit,
                    },
                );
            }
            return;
        }
        if !s.halved {
            s.halved = true;
            let from = s.width;
            s.width = (from / 2).max(1);
            if !silent {
                self.event(
                    s,
                    SchedulerEvent::CapHalved {
                        scope: CapScope::Global,
                        from,
                        to: s.width,
                    },
                );
            }
        }
        let lane = self.lane(s, &id);
        let from = lane.width;
        lane.width = (from / 2).max(1);
        let to = lane.width;
        if !silent {
            lane.forced
                .entry(launch.model.clone())
                .and_modify(|n| *n = n.saturating_add(1))
                .or_insert(1);
            self.event(
                s,
                SchedulerEvent::CapHalved {
                    scope: CapScope::Lane {
                        profile: id.0,
                        provider: id.1,
                    },
                    from,
                    to,
                },
            );
        }
    }
    fn fold(&self, s: &mut State, silent: bool) -> Result<(), String> {
        let (records, offset) =
            JournalReader::read_from(&self.path, s.offset).map_err(|e| e.to_string())?;
        for line in records {
            match line.record {
                Record::CallIntent {
                    key,
                    attempt,
                    launch,
                    ..
                } => {
                    s.accounting.launches.insert((key, attempt), launch);
                }
                Record::TurnSettled {
                    key,
                    attempt,
                    turn,
                    usage,
                    result_meta,
                    ..
                } => {
                    s.accounting
                        .settle(&key, attempt, turn, usage.as_ref(), &self.cfg.prices);
                    if let Some(l) = s.accounting.launches.get(&(key.clone(), attempt)).cloned() {
                        self.failure(
                            s,
                            (&key, attempt, turn),
                            &l,
                            result_meta["failure_reason"].as_str().unwrap_or_default(),
                            silent,
                        );
                    }
                }
                _ => {}
            }
        }
        s.offset = offset;
        s.last_fold = Some(self.clock.now());
        if !silent {
            self.budget_events(s);
        }
        Ok(())
    }
    fn permit(
        self: &Arc<Self>,
        s: &mut State,
        key: String,
        id: LaneId,
        reattached: bool,
    ) -> GatePermit {
        s.in_flight += 1;
        let lane = self.lane(s, &id);
        lane.in_flight += 1;
        let lane_in_flight = lane.in_flight;
        self.event(
            s,
            SchedulerEvent::Admitted {
                key: key.clone(),
                profile: id.0.clone(),
                provider: id.1.clone(),
                in_flight: s.in_flight,
                lane_in_flight,
                reattached,
            },
        );
        let inner = self.clone();
        GatePermit::new(move || {
            let mut s = inner.lock();
            s.in_flight = s.in_flight.saturating_sub(1);
            let lane = inner.lane(&mut s, &id);
            lane.in_flight = lane.in_flight.saturating_sub(1);
            if let Err(e) = inner.fold(&mut s, false) {
                s.journal_error = Some(e);
            }
            inner.event(
                &mut s,
                SchedulerEvent::Released {
                    key,
                    profile: id.0,
                    provider: id.1,
                },
            );
            inner.drain(&mut s);
            drop(s);
            inner.wake.notify_one();
        })
    }
    fn wait_reason(&self, s: &mut State, w: &Waiting) -> Option<WaitReason> {
        let now = self.clock.now();
        let l = self.lane(s, &w.lane);
        if let Some((until, cause)) = &l.pause {
            return Some(WaitReason::LanePaused {
                until: until.map(stamp),
                cause: *cause,
            });
        }
        if !l.forced.is_empty()
            || s.profiles
                .get(&w.lane.0)
                .is_some_and(|p| p.reading || !p.attempted)
        {
            return Some(WaitReason::LanePaused {
                until: None,
                cause: PauseCause::RateLimited,
            });
        }
        if let Some(r) = self.rosters.cached(&w.lane.0)
            && let Availability::Limited { until, scope } = r.availability(&w.lane.1, &w.model, now)
        {
            let next = until.unwrap_or_else(|| later(now, self.cfg.cooldown));
            let profile = s.profiles.entry(w.lane.0.clone()).or_default();
            let earliest = profile
                .last_read
                .map_or(next, |t| next.max(later(t, self.cfg.roster_min_interval)));
            profile.next_read = Some(profile.next_read.map_or(earliest, |old| old.min(earliest)));
            return Some(match scope {
                LimitScope::Account => WaitReason::ProviderLimited {
                    until: until.map(stamp),
                },
                LimitScope::Model => WaitReason::ModelLimited {
                    until: until.map(stamp),
                },
            });
        }
        // A previously observed reset requires a new roster before admission.
        if s.profiles
            .get(&w.lane.0)
            .and_then(|p| p.next_read)
            .is_some_and(|t| t <= now)
        {
            return Some(WaitReason::LanePaused {
                until: None,
                cause: PauseCause::RateLimited,
            });
        }
        if s.in_flight >= s.width {
            return Some(WaitReason::GlobalCap);
        }
        let l = self.lane(s, &w.lane);
        if l.in_flight >= l.width {
            return Some(WaitReason::LaneCap);
        }
        None
    }
    fn drain(self: &Arc<Self>, s: &mut State) {
        let now = self.clock.now();
        let resumed: Vec<_> = s
            .lanes
            .iter()
            .filter(|(_, l)| {
                l.pause
                    .as_ref()
                    .is_some_and(|(t, _)| t.is_some_and(|t| t <= now))
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in resumed {
            self.lane(s, &id).pause = None;
            self.event(
                s,
                SchedulerEvent::LaneResumed {
                    profile: id.0,
                    provider: id.1,
                },
            );
        }
        let mut blocked = BTreeSet::new();
        let mut remaining = VecDeque::new();
        while let Some(mut w) = s.queue.pop_front() {
            if w.tx.is_closed() {
                continue;
            }
            if let Some(error) = &s.journal_error {
                let _ = w.tx.send(GateDecision::Abort {
                    reason: format!("scheduler journal read failed: {error}"),
                });
                continue;
            }
            let cause = self
                .reached(s)
                .map(|budget| SkipCause::BudgetReached { budget })
                .or_else(|| {
                    self.lane(s, &w.lane)
                        .pause
                        .as_ref()
                        .filter(|(_, c)| *c == PauseCause::OutOfCredit)
                        .map(|_| SkipCause::OutOfCredit)
                });
            if let Some(cause) = cause {
                self.budget_events(s);
                self.event(
                    s,
                    SchedulerEvent::Skipped {
                        key: w.key,
                        label: w.label,
                        cause,
                    },
                );
                let _ = w.tx.send(GateDecision::Skip);
                continue;
            }
            let reason = if blocked.contains(&w.lane) {
                Some(w.reason.clone().unwrap_or(WaitReason::LaneCap))
            } else {
                self.wait_reason(s, &w)
            };
            if let Some(reason) = reason {
                blocked.insert(w.lane.clone());
                if w.reason.as_ref() != Some(&reason) {
                    w.reason = Some(reason.clone());
                    self.event(
                        s,
                        SchedulerEvent::Queued {
                            key: w.key.clone(),
                            label: w.label.clone(),
                            profile: w.lane.0.clone(),
                            provider: w.lane.1.clone(),
                            model: w.model.clone(),
                            reason,
                        },
                    );
                }
                remaining.push_back(w);
            } else {
                let permit = self.permit(s, w.key, w.lane, false);
                if let Err(GateDecision::Proceed(p)) = w.tx.send(GateDecision::Proceed(permit)) {
                    // Defer release until the state lock is available.
                    self.runtime.spawn(async move {
                        drop(p);
                    });
                }
            }
        }
        s.queue = remaining;
    }
}
impl AdmissionGate for Scheduler {
    fn acquire(
        &self,
        request: &AgentRequest,
        launch: &ResolvedLaunch,
        reason: GateReason,
    ) -> BoxFuture<'static, GateDecision> {
        let inner = self.inner.clone();
        let key = journal_key(&request.call_id, launch);
        let label = request.label.clone();
        let id = (
            launch.profile.clone(),
            launch.provider.clone().unwrap_or_default(),
        );
        let model = launch.model.clone();
        Box::pin(async move {
            let (rx, seq) = {
                let mut s = inner.lock();
                if reason == GateReason::Reattach {
                    return GateDecision::Proceed(inner.permit(&mut s, key, id, true));
                }
                if let Err(e) = inner.fold(&mut s, false) {
                    s.journal_error = Some(e);
                }
                let attempted = inner.rosters.cached(&id.0).is_some();
                s.profiles.entry(id.0.clone()).or_insert(Profile {
                    attempted,
                    last_read: attempted.then(|| inner.clock.now()),
                    ..Default::default()
                });
                inner.lane(&mut s, &id);
                let (tx, rx) = oneshot::channel();
                let seq = s.next;
                s.next = s.next.wrapping_add(1);
                s.queue.push_back(Waiting {
                    id: seq,
                    key,
                    label,
                    lane: id,
                    model,
                    reason: None,
                    tx,
                });
                inner.drain(&mut s);
                (rx, seq)
            };
            let _cancel = Cancel {
                inner: Arc::downgrade(&inner),
                seq,
            };
            inner.wake.notify_one();
            rx.await.unwrap_or(GateDecision::Abort {
                reason: "scheduler stopped".into(),
            })
        })
    }
    fn budget(&self) -> BudgetSnapshot {
        Scheduler::budget(self)
    }
    fn retry_after_failure(
        &self,
        request: &AgentRequest,
        launch: &ResolvedLaunch,
        last: &SettledTurn,
        attempt: u32,
    ) -> bool {
        let reason = last.result_meta["failure_reason"]
            .as_str()
            .unwrap_or_default();
        let key = journal_key(&request.call_id, launch);
        let id = (
            launch.profile.clone(),
            launch.provider.clone().unwrap_or_default(),
        );
        let mut s = self.inner.lock();
        if let Err(e) = self.inner.fold(&mut s, false) {
            s.journal_error = Some(e);
            return false;
        }
        self.inner
            .failure(&mut s, (&key, attempt, last.turn), launch, reason, false);
        let allowed = matches!(reason, "rate_limit" | "upstream_rate_limit" | "overloaded")
            && self.inner.reached(&s).is_none()
            && s.journal_error.is_none()
            && !self
                .inner
                .lane(&mut s, &id)
                .pause
                .as_ref()
                .is_some_and(|(_, c)| *c == PauseCause::OutOfCredit);
        if allowed {
            self.inner.event(
                &mut s,
                SchedulerEvent::RetryAuthorised {
                    key,
                    attempt,
                    reason: reason.into(),
                },
            );
        }
        drop(s);
        self.inner.wake.notify_one();
        allowed
    }
}
struct Cancel {
    inner: Weak<Inner>,
    seq: u64,
}
impl Drop for Cancel {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            let mut s = inner.lock();
            s.queue.retain(|w| w.id != self.seq);
            inner.drain(&mut s);
            drop(s);
            inner.wake.notify_one();
        }
    }
}
async fn pump(weak: Weak<Inner>) {
    loop {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        let notified = inner.wake.notified();
        let now = inner.clock.now();
        let (plan, delay) = {
            let mut s = inner.lock();
            inner.drain(&mut s);
            let forced: BTreeSet<_> = s
                .lanes
                .iter()
                .filter(|(_, l)| !l.forced.is_empty())
                .map(|(id, _)| id.0.clone())
                .collect();
            let waiting: BTreeSet<_> = s.queue.iter().map(|w| w.lane.0.clone()).collect();
            let profile = forced
                .iter()
                .chain(waiting.iter())
                .find(|p| {
                    let info = s.profiles.get(*p);
                    info.is_none_or(|p| {
                        !p.reading && (!p.attempted || p.next_read.is_some_and(|t| t <= now))
                    }) || forced.contains(*p) && info.is_none_or(|p| !p.reading)
                })
                .cloned();
            let plan = profile.map(|p| {
                let forced_models: Vec<_> = s
                    .lanes
                    .iter()
                    .filter(|(id, l)| id.0 == p && !l.forced.is_empty())
                    .map(|(id, l)| (id.clone(), l.forced.clone()))
                    .collect();
                let i = s.profiles.entry(p.clone()).or_default();
                let first = !i.attempted;
                i.reading = true;
                i.attempted = true;
                (p, first, forced_models)
            });
            let next = s
                .profiles
                .iter()
                .filter(|(p, _)| waiting.contains(*p))
                .filter_map(|(_, p)| p.next_read)
                .chain(
                    s.lanes
                        .values()
                        .filter_map(|l| l.pause.as_ref().and_then(|(t, _)| *t)),
                )
                .min();
            let delay = next.map(|t| {
                (t - now)
                    .to_std()
                    .unwrap_or(Duration::from_millis(1))
                    .max(Duration::from_millis(1))
            });
            (plan, delay)
        };
        if let Some((profile, first, forced_models)) = plan {
            let force = !forced_models.is_empty();
            let result = if first {
                inner.rosters.get_or_load(&profile).await
            } else {
                inner.rosters.refresh(&profile, force).await
            };
            let now = inner.clock.now();
            let mut s = inner.lock();
            let info = s.profiles.entry(profile.clone()).or_default();
            info.reading = false;
            info.last_read = Some(now);
            info.next_read = None;
            match result {
                Ok(_) => {
                    info.failures = 0;
                    inner.event(
                        &mut s,
                        SchedulerEvent::RosterRefreshed {
                            profile: profile.clone(),
                        },
                    );
                }
                Err(e) => {
                    info.failures = info.failures.saturating_add(1);
                    let persistent = info.failures >= 3;
                    info.next_read = Some(later(now, inner.cfg.cooldown));
                    inner.event(
                        &mut s,
                        SchedulerEvent::RosterReadFailed {
                            profile: profile.clone(),
                            message: e.to_string(),
                            persistent,
                        },
                    );
                }
            }
            for (id, models) in forced_models {
                let limited = inner.rosters.cached(&profile).is_some_and(|r| {
                    models.keys().any(|m| {
                        matches!(r.availability(&id.1, m, now), Availability::Limited { .. })
                    })
                });
                let lane = inner.lane(&mut s, &id);
                for (m, n) in models {
                    if lane.forced.get(&m) == Some(&n) {
                        lane.forced.remove(&m);
                    }
                }
                if !limited
                    && !lane
                        .pause
                        .as_ref()
                        .is_some_and(|(_, c)| *c == PauseCause::OutOfCredit)
                {
                    let until = later(now, inner.cfg.cooldown);
                    lane.pause = Some((Some(until), PauseCause::RateLimited));
                    inner.event(
                        &mut s,
                        SchedulerEvent::LanePaused {
                            profile: id.0,
                            provider: id.1,
                            until: Some(stamp(until)),
                            cause: PauseCause::RateLimited,
                        },
                    );
                }
            }
            inner.drain(&mut s);
            continue;
        }
        if let Some(delay) = delay {
            tokio::select! {_=notified=>{},_=tokio::time::sleep(delay)=>{}}
        } else {
            // Do not keep the scheduler alive while it has no work.
            drop(notified);
            let wake = inner.clone();
            drop(inner);
            tokio::select! {_=wake.wake.notified()=>{},_=tokio::time::sleep(Duration::from_secs(1))=>{if Arc::strong_count(&wake)==1{return;}}}
        }
    }
}
