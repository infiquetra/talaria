//! Pull-request status, polled sparingly. Issue #245.
//!
//! The service reads time only through the clock it is given and starts due refreshes only
//! in [`PrStatusService::tick`]. A refresh runs to its end on the calling thread, on a
//! runtime of its own, so a caller on the input thread must hand `tick` and `refresh_now`
//! to a blocking task.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use talaria_core::directory::rows::RowKey;
use talaria_core::review::{GhReady, PrStatus, pr_chunks};

use crate::review::gh::GhRunner;
use crate::worktree::{GitRunner, main_root_of, records_in};

/// The least time between two refreshes of one repository.
const REFRESH_EVERY: Duration = Duration::from_secs(60);

/// A label older than this is drawn muted.
const STALE_AFTER: Duration = Duration::from_secs(5 * 60);

/// How long a successful readiness answer is reused.
const READY_FOR: Duration = Duration::from_secs(10 * 60);

/// How long a local git read may run.
const GIT_READ: Duration = Duration::from_secs(10);

/// One known repository.
#[derive(Default)]
struct Repo {
    /// When the last refresh started, whatever came of it.
    last_attempt: Option<Instant>,
    /// A refresh was asked for and has not started.
    due_now: bool,
    /// A refresh is running.
    in_flight: bool,
    /// `owner/name`, and when `gh` last said so.
    ready: Option<(String, Instant)>,
    /// The latest status per branch, and when the request that produced it finished.
    labels: BTreeMap<String, (PrStatus, Instant)>,
    /// The branch each known row works on.
    rows: BTreeMap<RowKey, String>,
    /// Branches reviewed in this process.
    reviewed: BTreeSet<String>,
    /// The foreground folder's branch, read at the last refresh.
    status_branch: Option<String>,
}

#[derive(Default)]
struct State {
    fleet: bool,
    reviews: BTreeSet<PathBuf>,
    /// The foreground folder the status line follows, and its repository once resolved.
    status_folder: Option<PathBuf>,
    status_root: Option<PathBuf>,
    repos: BTreeMap<PathBuf, Repo>,
}

/// The pull-request status service.
pub struct PrStatusService {
    gh: GhRunner,
    git: GitRunner,
    clock: Arc<dyn Fn() -> Instant + Send + Sync>,
    notify: Box<dyn Fn(PrStatusNotice) + Send + Sync>,
    state: Mutex<State>,
}

impl std::fmt::Debug for PrStatusService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PrStatusService")
            .finish_non_exhaustive()
    }
}

impl PrStatusService {
    /// Serve status with this clock and notice sink.
    pub fn new(
        gh: GhRunner,
        git: GitRunner,
        clock: Arc<dyn Fn() -> Instant + Send + Sync>,
        notify: impl Fn(PrStatusNotice) + Send + Sync + 'static,
    ) -> PrStatusService {
        PrStatusService {
            gh,
            git,
            clock,
            notify: Box::new(notify),
            state: Mutex::new(State::default()),
        }
    }

    /// Start the refreshes that are due, and wait for them. The timer calls this once a
    /// second from a blocking task.
    pub fn tick(&self) {
        self.resolve_status_root();
        let now = (self.clock)();
        let due: Vec<PathBuf> = {
            let mut state = self.lock();
            let wanted = state.wanted_roots();
            wanted
                .into_iter()
                .filter(|root| {
                    state.repos.get_mut(root).is_some_and(|repo| {
                        let elapsed = repo
                            .last_attempt
                            .map(|at| now.saturating_duration_since(at));
                        let due = repo.due_now || elapsed.is_none_or(|time| time >= REFRESH_EVERY);
                        let due = due && !repo.in_flight;
                        if due {
                            claim(repo, now);
                        }
                        due
                    })
                })
                .collect()
        };
        for root in due {
            self.refresh_blocking(&root);
        }
    }

    /// Note that a surface wants status, or stopped wanting it. Nothing runs here: a new
    /// demand makes the repository due, and the next tick refreshes it.
    pub fn want(&self, demand: PrDemand) {
        let mut state = self.lock();
        match demand {
            PrDemand::Fleet(open) => {
                if open && !state.fleet {
                    for repo in state.repos.values_mut() {
                        repo.due_now = true;
                    }
                }
                state.fleet = open;
            }
            PrDemand::Review(root, open) => {
                if open {
                    let fresh = state.reviews.insert(root.clone());
                    let repo = state.repos.entry(root).or_default();
                    repo.due_now |= fresh;
                } else {
                    state.reviews.remove(&root);
                }
            }
            PrDemand::StatusLine(folder) => {
                if state.status_folder != folder {
                    state.status_folder = folder;
                    state.status_root = None;
                }
            }
        }
    }

    /// Refresh one repository at once, and re-run the `gh` readiness check with it. When a
    /// refresh of that repository is already running, the next tick runs another.
    pub fn refresh_now(&self, main_root: &Path) {
        let now = (self.clock)();
        let claimed = {
            let mut state = self.lock();
            let repo = state.repos.entry(main_root.to_path_buf()).or_default();
            repo.ready = None;
            if repo.in_flight {
                repo.due_now = true;
                false
            } else {
                claim(repo, now);
                true
            }
        };
        if claimed {
            self.refresh_blocking(main_root);
        }
    }

    /// The known labels by row.
    pub fn labels(&self) -> BTreeMap<RowKey, PrStatus> {
        let state = self.lock();
        let mut labels = BTreeMap::new();
        for repo in state.repos.values() {
            for (row, branch) in &repo.rows {
                if let Some((status, _)) = repo.labels.get(branch) {
                    labels.insert(row.clone(), status.clone());
                }
            }
        }
        labels
    }

    /// The known status of one branch, and whether it is stale.
    pub fn for_branch(&self, main_root: &Path, branch: &str) -> Option<(PrStatus, bool)> {
        let now = (self.clock)();
        let state = self.lock();
        let (status, at) = state.repos.get(main_root)?.labels.get(branch)?;
        Some((
            status.clone(),
            now.saturating_duration_since(*at) > STALE_AFTER,
        ))
    }

    /// The status line's label: the foreground folder's branch, when a pull request is
    /// known for it, and whether it is stale.
    pub fn foreground_label(&self) -> Option<(PrStatus, bool)> {
        let (root, branch) = {
            let state = self.lock();
            let root = state.status_root.clone()?;
            let branch = state.repos.get(&root)?.status_branch.clone()?;
            (root, branch)
        };
        self.for_branch(&root, &branch)
    }

    /// Tell the service about a repository: the launch folder's, the foreground session's,
    /// or one a tree was launched in. The fleet view refreshes every known repository.
    pub fn know(&self, main_root: &Path) {
        self.lock()
            .repos
            .entry(main_root.to_path_buf())
            .or_default();
    }

    /// Tell the service that `branch` was reviewed in this process, and which row works on
    /// it when that is known. The next refresh looks the branch up.
    pub fn note_branch(&self, main_root: &Path, branch: &str, row: Option<RowKey>) {
        let mut state = self.lock();
        let repo = state.repos.entry(main_root.to_path_buf()).or_default();
        repo.reviewed.insert(branch.to_owned());
        if let Some(row) = row {
            repo.rows.insert(row, branch.to_owned());
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Find the repository of the foreground folder, once per folder.
    fn resolve_status_root(&self) {
        let folder = {
            let state = self.lock();
            match (&state.status_folder, &state.status_root) {
                (Some(folder), None) => folder.clone(),
                _ => return,
            }
        };
        let found = block_on(async { main_root_of(&self.git, &folder).await.ok() }).flatten();
        let mut state = self.lock();
        if state.status_folder.as_deref() != Some(folder.as_path()) {
            return;
        }
        if let Some(root) = found {
            state.repos.entry(root.clone()).or_default().due_now = true;
            state.status_root = Some(root);
        }
    }

    fn refresh_blocking(&self, root: &Path) {
        let changed = block_on(self.refresh(root));
        {
            let mut state = self.lock();
            if let Some(repo) = state.repos.get_mut(root) {
                repo.in_flight = false;
            }
        }
        if changed == Some(true) {
            (self.notify)(PrStatusNotice::Updated);
        }
    }

    /// One refresh of one repository. Whether the labels changed.
    async fn refresh(&self, root: &Path) -> bool {
        let (cached, folder) = {
            let state = self.lock();
            let cached = state.repos.get(root).and_then(|repo| repo.ready.clone());
            let folder = (state.status_root.as_deref() == Some(root))
                .then(|| state.status_folder.clone())
                .flatten();
            (cached, folder)
        };
        let now = (self.clock)();
        let repo_name = match cached {
            Some((name, at)) if now.saturating_duration_since(at) < READY_FOR => name,
            _ => match self.gh.readiness(root).await {
                GhReady::Ready { repo } => {
                    self.with_repo(root, |entry| entry.ready = Some((repo.clone(), now)));
                    repo
                }
                _ => return false,
            },
        };

        let records = records_in(&self.git, root).await;
        let status_branch = match folder {
            Some(folder) => self.current_branch(&folder).await,
            None => None,
        };
        let branches = self.with_repo(root, |entry| {
            entry.status_branch = status_branch;
            for record in &records {
                if let Some(id) = &record.session_id {
                    let row = RowKey {
                        profile: record.profile.clone(),
                        session_id: id.clone(),
                    };
                    entry.rows.insert(row, record.branch.clone());
                }
            }
            // Reviewed branches first, then the foreground branch, then every record's:
            // the first 300 are the ones that matter most.
            let mut branches: Vec<String> = entry.reviewed.iter().cloned().collect();
            let mapped = |branch: &String| {
                entry.reviewed.contains(branch) || records.iter().any(|r| &r.branch == branch)
            };
            if let Some(branch) = entry.status_branch.clone().filter(mapped) {
                branches.push(branch);
            }
            branches.extend(records.iter().map(|record| record.branch.clone()));
            branches
        });
        let branches = branches.unwrap_or_default();

        let mut changed = false;
        for chunk in pr_chunks(&branches) {
            let Ok(statuses) = self.gh.pr_statuses(root, &repo_name, &chunk).await else {
                continue;
            };
            let finished = (self.clock)();
            changed |= self
                .with_repo(root, |entry| apply(entry, &chunk, statuses, finished))
                .unwrap_or(false);
        }
        changed
    }

    async fn current_branch(&self, folder: &Path) -> Option<String> {
        let output = self
            .git
            .run_read_only(folder, &["branch", "--show-current"], GIT_READ)
            .await
            .ok()?;
        let branch = output.stdout.trim();
        (output.status == 0 && !branch.is_empty()).then(|| branch.to_owned())
    }

    fn with_repo<R>(&self, root: &Path, job: impl FnOnce(&mut Repo) -> R) -> Option<R> {
        let mut state = self.lock();
        state.repos.get_mut(root).map(job)
    }
}

impl State {
    /// The repositories that have demand: every known one while the fleet view is open,
    /// the reviewed ones, and the status line's.
    fn wanted_roots(&self) -> Vec<PathBuf> {
        if self.fleet {
            return self.repos.keys().cloned().collect();
        }
        let mut roots: BTreeSet<PathBuf> = self.reviews.clone();
        roots.extend(self.status_root.clone());
        roots
            .into_iter()
            .filter(|root| self.repos.contains_key(root))
            .collect()
    }
}

/// Mark a refresh as started. The caller has checked that none is running.
fn claim(repo: &mut Repo, now: Instant) {
    repo.in_flight = true;
    repo.due_now = false;
    repo.last_attempt = Some(now);
}

/// Record the statuses one request returned for `chunk`: a branch without one has no pull
/// request. Whether any label changed.
fn apply(repo: &mut Repo, chunk: &[String], statuses: Vec<PrStatus>, finished: Instant) -> bool {
    let mut changed = false;
    for branch in chunk {
        let found = statuses.iter().find(|status| &status.branch == branch);
        match found {
            Some(status) => {
                let same = repo
                    .labels
                    .get(branch)
                    .is_some_and(|(known, _)| known == status);
                repo.labels
                    .insert(branch.clone(), (status.clone(), finished));
                changed |= !same;
            }
            None => changed |= repo.labels.remove(branch).is_some(),
        }
    }
    changed
}

/// Run `future` to its end on a thread of its own, so a caller inside a runtime can wait for
/// it. `None` when the runtime could not start.
fn block_on<F>(future: F) -> Option<F::Output>
where
    F: Future + Send,
    F::Output: Send,
{
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .ok()?;
                Some(runtime.block_on(future))
            })
            .join()
            .ok()
            .flatten()
    })
}

/// A surface that wants pull-request status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrDemand {
    /// The fleet view opened or closed.
    Fleet(bool),
    /// A review opened or closed. The main root it shows.
    Review(PathBuf, bool),
    /// The status line wants the foreground folder's label, or nothing.
    StatusLine(Option<PathBuf>),
}

/// The service has new labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrStatusNotice {
    /// Labels changed.
    Updated,
}
