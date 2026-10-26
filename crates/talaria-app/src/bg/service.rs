//! The launch service: runs the launch machine's effects one at a time, in order.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::{SubscribedTurn, SubscribedView};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::Notify;

use crate::bg::conn::LaunchConn;
use crate::bg::link::LaunchLink;
use crate::bg::machine::{
    CallFailure, CwdCheck, LaunchEffect, LaunchIds, LaunchInput, LaunchMachine, Step,
};
use crate::bg::outcome::{LaunchError, LaunchOutcome};
use crate::bg::request::LaunchRequest;
use crate::bg::wire;
use crate::failure;
use crate::fleet::FleetHandle;
use crate::worktree::{self, GitRunner};

/// Launches of one Talaria process that may be starting at once.
const MAX_IN_FLIGHT: usize = 3;

/// A token shared by a launch and the code that can cancel it.
///
/// This is an `Arc<AtomicBool>`, not a utility-crate token. The workspace does not
/// depend on that utility crate, and no merged unit exported a token of its own.
#[derive(Clone, Debug)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    /// A token that has not been cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Cancel every launch holding a clone.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Whether [`Self::cancel`] has been called.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

/// The three fleet registry calls a launch makes. The fleet handle has one owner, so a launch
/// running on its own task reaches it through this.
pub trait LaunchFleet: Send + Sync {
    /// The launched session is running and this launch is its viewer.
    fn observe_subscribed(&self, key: RowKey, view: SubscribedView);
    /// This launch let go of the session.
    fn observe_unsubscribed(&self, key: RowKey);
    /// Read the gateway again now.
    fn refresh_now(&self);
}

impl LaunchFleet for FleetHandle {
    fn observe_subscribed(&self, key: RowKey, view: SubscribedView) {
        FleetHandle::observe_subscribed(self, key, view);
    }

    fn observe_unsubscribed(&self, key: RowKey) {
        FleetHandle::observe_unsubscribed(self, key);
    }

    fn refresh_now(&self) {
        FleetHandle::refresh_now(self);
    }
}

/// Run one launch to a finish.
pub async fn run_launch(
    conn: &LaunchConn,
    git: &GitRunner,
    request: LaunchRequest,
    ids: LaunchIds,
    fleet: Option<&dyn LaunchFleet>,
    cancel: CancelToken,
) -> LaunchOutcome {
    run_staged(conn, git, request, ids, fleet, &cancel, &|_| {}).await
}

async fn run_staged(
    conn: &LaunchConn,
    git: &GitRunner,
    request: LaunchRequest,
    ids: LaunchIds,
    fleet: Option<&dyn LaunchFleet>,
    cancel: &CancelToken,
    stage: &(dyn Fn(Stage) + Sync),
) -> LaunchOutcome {
    if !conn.reaches(&request.profile) {
        let mismatch = failure::profile_mismatch();
        return LaunchOutcome::NotStarted {
            error: LaunchError {
                what: mismatch.what,
                next_step: mismatch.next_step,
            },
            closed_session: None,
            tree_removed: false,
            tree_kept: None,
        };
    }
    let profile = request.profile.clone();
    let routed = conn.routed_profile(&profile);
    let routed = routed.as_deref();
    let (mut machine, first) = LaunchMachine::start(request, ids);
    let mut queue: VecDeque<LaunchEffect> = first.into();
    let mut told_cancel = false;
    let mut warnings = Vec::new();
    let mut model = None;
    while let Some(effect) = queue.pop_front() {
        // Cancellation reaches the machine between effects, and only before the submit.
        let cancelled = cancel.is_cancelled() && !machine.submit_sent();
        if cancelled && !told_cancel {
            told_cancel = true;
            queue.extend(machine.step(LaunchInput::Cancel));
        }
        let input = match effect {
            LaunchEffect::Describe => LaunchInput::Described(conn.describe()),
            LaunchEffect::PrepareWorktree { folder, slug } => {
                stage(Stage::Fetching);
                match worktree::prepare_worktree(git, &folder, &slug, &profile, now_unix(), cancel)
                    .await
                {
                    Ok((record, mut found)) => {
                        warnings.append(&mut found);
                        LaunchInput::WorktreeReady(record)
                    }
                    Err(error) => LaunchInput::WorktreeFailed(error),
                }
            }
            // A create not yet sent when the launch was cancelled is never sent.
            LaunchEffect::Create { .. } if cancelled => LaunchInput::Failed {
                step: Step::Create,
                failure: CallFailure::NotSent("the launch was cancelled".to_owned()),
            },
            LaunchEffect::Create { new } => {
                stage(Stage::CreatingSession);
                let created = create(conn, &wire::create_params(&new, routed)).await;
                match created {
                    Ok(created) => {
                        model.clone_from(&created.model);
                        LaunchInput::Created(created)
                    }
                    Err(failure) => LaunchInput::Failed {
                        step: Step::Create,
                        failure,
                    },
                }
            }
            LaunchEffect::CheckCwd { sent, reported } => {
                LaunchInput::CwdChecked(compare_cwd(&sent, reported.as_deref()))
            }
            LaunchEffect::RecordSession { record, session_id } => {
                let recorded = worktree::record_session(git, &record, &session_id).await;
                LaunchInput::SessionRecorded {
                    warning: recorded.err().map(|error| error.what),
                }
            }
            LaunchEffect::Submit {
                session_id,
                text,
                submission_id,
            } => {
                stage(Stage::Submitting);
                let params = wire::submit_params(&session_id, &text, &submission_id, routed);
                match conn
                    .call("prompt.submit", params, wire::SUBMIT_TIMEOUT)
                    .await
                {
                    Ok(reply) => match wire::read_submit(&reply) {
                        Ok(reply) => LaunchInput::Submitted(reply),
                        Err(why) => failed(Step::Submit, CallFailure::Malformed(why)),
                    },
                    Err(failure) => failed(Step::Submit, failure),
                }
            }
            LaunchEffect::Detach {
                session_id,
                subscription_id,
                end_reason,
            } => {
                let params = wire::detach_params(&session_id, &subscription_id, end_reason, routed);
                match conn
                    .call("session.detach", params, wire::DETACH_TIMEOUT)
                    .await
                {
                    Ok(_) => LaunchInput::Detached,
                    Err(failure) => failed(Step::Detach, failure),
                }
            }
            LaunchEffect::Rollback(record) => {
                let kept = match worktree::rollback(git, &record).await {
                    Ok(kept) => kept,
                    Err(error) => Some(error),
                };
                LaunchInput::RolledBack { kept }
            }
            LaunchEffect::FleetRunning { session_id } => {
                if let Some(fleet) = fleet {
                    fleet.observe_subscribed(
                        RowKey {
                            profile: profile.clone(),
                            session_id,
                        },
                        running_view(model.clone()),
                    );
                }
                continue;
            }
            LaunchEffect::FleetReleased { session_id } => {
                if let Some(fleet) = fleet {
                    fleet.observe_unsubscribed(RowKey {
                        profile: profile.clone(),
                        session_id,
                    });
                    fleet.refresh_now();
                }
                continue;
            }
            LaunchEffect::Finish(mut outcome) => {
                if let LaunchOutcome::Launched(launched) = &mut outcome {
                    warnings.append(&mut launched.warnings);
                    launched.warnings = warnings;
                }
                return outcome;
            }
        };
        queue.extend(machine.step(input));
    }
    // The machine always ends with a finish; reaching here means it stopped emitting effects.
    LaunchOutcome::NotStarted {
        error: LaunchError {
            what: "the launch stopped before it finished.".to_owned(),
            next_step: "Check talaria fleet list before launching it again.".to_owned(),
        },
        closed_session: None,
        tree_removed: false,
        tree_kept: None,
    }
}

fn failed(step: Step, failure: CallFailure) -> LaunchInput {
    LaunchInput::Failed { step, failure }
}

/// `session.create`, sent again once, identically, when the first reply was lost on a
/// connection that is still the same one. The request id makes the second send safe.
async fn create(
    conn: &LaunchConn,
    params: &serde_json::Value,
) -> Result<crate::bg::machine::CreatedSession, CallFailure> {
    let generation = conn.generation();
    let mut reply = conn
        .call("session.create", params.clone(), wire::CREATE_TIMEOUT)
        .await;
    if matches!(reply, Err(CallFailure::Unknown)) && conn.generation() == generation {
        reply = conn
            .call("session.create", params.clone(), wire::CREATE_TIMEOUT)
            .await;
    }
    let snapshot = reply?;
    wire::read_created(&snapshot).map_err(CallFailure::Malformed)
}

/// The view a just-admitted session has before the fleet's first read of it.
fn running_view(model: Option<String>) -> SubscribedView {
    SubscribedView {
        turn: SubscribedTurn::Running { started_at: None },
        needs: Vec::new(),
        activity: None,
        cost_usd: None,
        model,
        title: None,
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

/// Compare the folder that was sent with the folder the snapshot reported.
///
/// `None` or `""` is [`CwdCheck::Missing`]. A path that cannot be canonicalised is
/// [`CwdCheck::Differs`]. Equal canonical paths are [`CwdCheck::Matches`].
#[must_use]
pub fn compare_cwd(sent: &Path, reported: Option<&str>) -> CwdCheck {
    let Some(reported) = reported.filter(|reported| !reported.is_empty()) else {
        return CwdCheck::Missing;
    };
    match (std::fs::canonicalize(sent), std::fs::canonicalize(reported)) {
        (Ok(sent), Ok(reported)) if sent == reported => CwdCheck::Matches,
        _ => CwdCheck::Differs,
    }
}

/// What the fleet list should show while a launch is in flight, and when it finishes.
#[derive(Clone, Debug, PartialEq, Eq)]
// A finished launch carries the full outcome. Tests match the field by name, so it stays unboxed.
#[allow(clippy::large_enum_variant)]
pub enum LaunchNotice {
    /// A stage changed.
    Progress { launch: u64, stage: Stage },
    /// The launch finished.
    Finished { launch: u64, outcome: LaunchOutcome },
}

/// The stage a progress notice names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Opening the link.
    Connecting,
    /// Fetching the default branch.
    Fetching,
    /// Creating the worktree.
    CreatingWorktree,
    /// Creating the session.
    CreatingSession,
    /// Sending the prompt.
    Submitting,
}

/// Three launches of this process are already starting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Busy;

struct Shared {
    home: PathBuf,
    git: GitRunner,
    notify: Box<dyn Fn(LaunchNotice) + Send + Sync>,
    in_flight: Mutex<BTreeMap<u64, CancelToken>>,
    finished: Notify,
}

/// Launches started from one Talaria process. Each runs on its own task and connection, so a
/// launch never waits on the screen and the screen never waits on a launch.
pub struct LaunchService {
    shared: Arc<Shared>,
    next: AtomicU64,
    // Launches run on a runtime of their own, so they progress whatever the caller's runtime
    // is doing, including a caller that has none.
    runtime: Option<Runtime>,
    handle: Handle,
}

impl LaunchService {
    /// A service that notifies `notify` as launches progress.
    ///
    /// # Panics
    ///
    /// When the operating system refuses the threads the launch runtime needs.
    #[must_use]
    pub fn new(
        home: PathBuf,
        git: GitRunner,
        notify: impl Fn(LaunchNotice) + Send + Sync + 'static,
    ) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("talaria-launch")
            .enable_all()
            .build()
            .expect("the launch runtime starts");
        let handle = runtime.handle().clone();
        Self {
            shared: Arc::new(Shared {
                home,
                git,
                notify: Box::new(notify),
                in_flight: Mutex::new(BTreeMap::new()),
                finished: Notify::new(),
            }),
            next: AtomicU64::new(1),
            runtime: Some(runtime),
            handle,
        }
    }

    /// Start a launch and return at once. Refused with [`Busy`] while three are starting.
    pub fn launch(
        &self,
        request: LaunchRequest,
        foreground_profile: String,
        fleet: Option<Arc<dyn LaunchFleet>>,
    ) -> Result<u64, Busy> {
        let cancel = CancelToken::new();
        let id = {
            let mut in_flight = lock(&self.shared.in_flight);
            if in_flight.len() >= MAX_IN_FLIGHT {
                return Err(Busy);
            }
            let id = self.next.fetch_add(1, Ordering::Relaxed);
            in_flight.insert(id, cancel.clone());
            id
        };
        let shared = Arc::clone(&self.shared);
        self.handle.spawn(async move {
            let outcome =
                launch_on_link(&shared, id, request, &foreground_profile, fleet, &cancel).await;
            lock(&shared.in_flight).remove(&id);
            (shared.notify)(LaunchNotice::Finished {
                launch: id,
                outcome,
            });
            shared.finished.notify_waiters();
        });
        Ok(id)
    }

    /// Cancel launches still starting and wait up to `limit`; returns the ids still running.
    pub async fn shutdown(&self, limit: Duration) -> Vec<u64> {
        for cancel in lock(&self.shared.in_flight).values() {
            cancel.cancel();
        }
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let finished = self.shared.finished.notified();
            let left: Vec<u64> = lock(&self.shared.in_flight).keys().copied().collect();
            if left.is_empty() || tokio::time::Instant::now() >= deadline {
                return left;
            }
            let _ = tokio::time::timeout_at(deadline, finished).await;
        }
    }
}

impl Drop for LaunchService {
    fn drop(&mut self) {
        // Dropping a runtime blocks, which an async caller may not do; launches still running
        // stop here, and closing their sockets detaches without an end reason.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

async fn launch_on_link(
    shared: &Shared,
    id: u64,
    request: LaunchRequest,
    foreground_profile: &str,
    fleet: Option<Arc<dyn LaunchFleet>>,
    cancel: &CancelToken,
) -> LaunchOutcome {
    let notify = |stage| (shared.notify)(LaunchNotice::Progress { launch: id, stage });
    notify(Stage::Connecting);
    let link = match LaunchLink::open(&shared.home, foreground_profile).await {
        Ok(link) => link,
        Err(error) => {
            return LaunchOutcome::NotStarted {
                error,
                closed_session: None,
                tree_removed: false,
                tree_kept: None,
            };
        }
    };
    let conn = LaunchConn::Link(link);
    let ids = LaunchIds {
        request_id: crate::launch::new_request_id(),
        submission_id: crate::commands::fresh_submission_id(),
    };
    let outcome = run_staged(
        &conn,
        &shared.git,
        request,
        ids,
        fleet.as_deref(),
        cancel,
        &notify,
    )
    .await;
    conn.close().await;
    outcome
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
