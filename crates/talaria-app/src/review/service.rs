//! Runs a review's effects on tasks, one mutation at a time per process.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use talaria_core::review::{
    DraftError, GitAction, ReviewEffect, ReviewInput, ReviewState, ReviewTarget, ReviewView,
};
use tokio::sync::mpsc;

use crate::app::AppMessage;
use crate::bg::{LaunchConn, LaunchLink};
use crate::review::draft::draft_message;
use crate::review::gh::GhRunner;
use crate::review::git_ops;
use crate::review::pr_status::PrStatusService;
use crate::worktree::GitRunner;

/// Where a running effect sends its answer.
pub type AppSender = mpsc::UnboundedSender<AppMessage>;

/// The Hermes link a review drafts on: the home and the foreground profile it is minted for.
#[derive(Clone, Debug)]
struct LinkConfig {
    home: PathBuf,
    foreground_profile: String,
}

/// The review service: the open review's state, and the runners its effects use.
pub struct ReviewService {
    git: GitRunner,
    gh: GhRunner,
    pr: Arc<PrStatusService>,
    link: Option<LinkConfig>,
    state: Option<ReviewState>,
    // The one mutation slot of this process: the running action's name, shared by every
    // review opened meanwhile.
    busy: Arc<Mutex<Option<&'static str>>>,
}

impl std::fmt::Debug for ReviewService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReviewService")
            .field("open", &self.state.is_some())
            .finish_non_exhaustive()
    }
}

impl ReviewService {
    /// Serve reviews with these runners. Tests pass `GhRunner::with_binary`.
    #[must_use]
    pub fn new(git: GitRunner, gh: GhRunner, pr: Arc<PrStatusService>) -> ReviewService {
        ReviewService {
            git,
            gh,
            pr,
            link: None,
            state: None,
            busy: Arc::new(Mutex::new(None)),
        }
    }

    /// Draft messages on links minted in `home` for `foreground_profile`.
    #[must_use]
    pub fn with_link(mut self, home: PathBuf, foreground_profile: String) -> ReviewService {
        self.link = Some(LinkConfig {
            home,
            foreground_profile,
        });
        self
    }

    /// The pull-request status service the review reads its label from.
    #[must_use]
    pub fn pr(&self) -> &Arc<PrStatusService> {
        &self.pr
    }

    /// Open a review. The effects read the first listing.
    pub fn open(&mut self, target: ReviewTarget, now: f64) -> Vec<ReviewEffect> {
        let (state, effects) = ReviewState::open(target, now);
        self.state = Some(state);
        effects
    }

    /// Whether a review is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.state.is_some()
    }

    /// Close the review. A running action still finishes and reports.
    pub fn close(&mut self) {
        self.state = None;
    }

    /// The open review's state.
    #[must_use]
    pub fn state(&self) -> Option<&ReviewState> {
        self.state.as_ref()
    }

    /// What the drawing needs.
    #[must_use]
    pub fn view(&self) -> Option<ReviewView<'_>> {
        self.state.as_ref().map(ReviewState::view)
    }

    /// Apply one input to the open review.
    pub fn update(&mut self, input: ReviewInput) -> Vec<ReviewEffect> {
        match self.state.as_mut() {
            Some(state) => state.update(input),
            None => Vec::new(),
        }
    }

    /// Run one effect. Answers go through `tx`; `Close` and `Redraw` belong to the app.
    pub fn execute(&self, effect: ReviewEffect, tx: AppSender) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let target = state.target().clone();
        let git = self.git.clone();
        match effect {
            ReviewEffect::List(scope) => {
                tokio::spawn(async move {
                    let result = git_ops::list(&git, &target, scope).await;
                    send(&tx, ReviewInput::Listed { scope, result });
                });
            }
            ReviewEffect::LoadDiff { path, scope } => {
                let file = state.file(&path).cloned();
                let base = state.listing().and_then(|listing| listing.base.clone());
                tokio::spawn(async move {
                    let result = match file {
                        Some(file) => {
                            git_ops::file_diff(&git, &target, scope, base.as_deref(), &file).await
                        }
                        None => Err(format!("{path} is no longer changed.")),
                    };
                    send(
                        &tx,
                        ReviewInput::DiffLoaded {
                            path,
                            scope,
                            result,
                        },
                    );
                });
            }
            ReviewEffect::Run(action) => self.run(action, target, tx),
            ReviewEffect::Draft { seq, avoid } => {
                let link = self.link.clone();
                tokio::spawn(async move {
                    let result = draft(&git, &target, link, &avoid).await;
                    send(&tx, ReviewInput::Drafted { seq, result });
                });
            }
            ReviewEffect::RefreshPr => self.pr.refresh_now(&target.main_root),
            ReviewEffect::OpenUrl(url) => {
                let gh = self.gh.clone();
                tokio::spawn(async move {
                    let opened = match pr_number(&url) {
                        Some(number) => gh.open_in_browser(&target.main_root, number).await,
                        None => Err(format!("no pull request number in {url}")),
                    };
                    if let Err(error) = opened {
                        let _ = tx.send(AppMessage::ReviewNotice(format!(
                            "Could not open the browser: {error}"
                        )));
                    }
                });
            }
            ReviewEffect::Close | ReviewEffect::Redraw => {}
        }
    }

    /// One mutation: refused at once while another runs anywhere in this process.
    fn run(&self, action: GitAction, target: ReviewTarget, tx: AppSender) {
        let name = action_name(&action);
        {
            let mut busy = lock(&self.busy);
            if let Some(running) = *busy {
                send(
                    &tx,
                    ReviewInput::ActionDone {
                        action,
                        result: Err(format!("Wait for {running} to finish.")),
                    },
                );
                return;
            }
            *busy = Some(name);
        }
        let git = self.git.clone();
        let gh = self.gh.clone();
        let pr = Arc::clone(&self.pr);
        let busy = Arc::clone(&self.busy);
        tokio::spawn(async move {
            let mut result = git_ops::run_action(&git, &target, &action).await;
            // The push ran (and passed its guard) first; a refused or failed push stops here.
            if let (Ok(_), GitAction::CreatePr { base, branch }) = (&result, &action) {
                result = gh
                    .create_draft(&target.main_root, base, branch)
                    .await
                    .map(|url| format!("Draft pull request opened: {url}"));
            }
            // A shipped branch changes its label; a refused or failed one does not.
            if result.is_ok() && matches!(action, GitAction::Push | GitAction::CreatePr { .. }) {
                pr.refresh_now(&target.main_root);
            }
            *lock(&busy) = None;
            send(&tx, ReviewInput::ActionDone { action, result });
        });
    }
}

/// Draft a commit message on a short-lived link, bound to the reviewed session.
async fn draft(
    git: &GitRunner,
    target: &ReviewTarget,
    link: Option<LinkConfig>,
    avoid: &str,
) -> Result<String, DraftError> {
    let Some(row) = target.session.clone() else {
        return Err(DraftError::NoSession);
    };
    let Some(link) = link else {
        return Err(DraftError::NoSession);
    };
    let (diff, recent) = git_ops::commit_context(git, target)
        .await
        .map_err(DraftError::Failed)?;
    let opened = LaunchLink::open(&link.home, &link.foreground_profile)
        .await
        .map_err(|error| DraftError::Failed(error.what))?;
    let conn = LaunchConn::Link(opened);
    let result = draft_message(&conn, &row, &diff, &recent, avoid).await;
    conn.close().await;
    result
}

fn send(tx: &AppSender, input: ReviewInput) {
    let _ = tx.send(AppMessage::Review(input));
}

/// The name a busy refusal gives a running action.
fn action_name(action: &GitAction) -> &'static str {
    match action {
        GitAction::Stage(_) => "staging",
        GitAction::Unstage(_) => "unstaging",
        GitAction::Discard(_) => "discard",
        GitAction::Commit { .. } => "commit",
        GitAction::Push => "push",
        GitAction::CreatePr { .. } => "the pull request",
    }
}

/// The number at the end of a pull request's URL (`.../pull/<n>`).
fn pr_number(url: &str) -> Option<u64> {
    let (_, tail) = url.trim_end_matches('/').rsplit_once("/pull/")?;
    tail.parse().ok()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
