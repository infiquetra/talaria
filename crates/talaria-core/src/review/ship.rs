//! The fail-closed shipping gates. Pure. Issue #245.

use crate::review::pr::PrStatus;
use crate::review::state::{Listing, Scope};

/// Why a commit, push or pull request is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShipRefusal {
    /// `HEAD` is detached.
    Detached,
    /// No `origin` remote.
    NoOrigin,
    /// The default branch, or `main` or `master`.
    DefaultBranch(String),
    /// The pull request's base branch.
    BaseBranch(String),
    /// The default branch is unknown.
    DefaultUnknown,
    /// Nothing to commit.
    NothingToCommit,
    /// Conflicted files, with their count.
    Conflicted(u32),
    /// The commit message is empty.
    EmptyMessage,
    /// No commits between the base and the branch.
    NoCommitsAhead {
        /// The base branch.
        base: String,
        /// The head branch.
        branch: String,
    },
    /// An open pull request already exists.
    PrExists {
        /// The pull-request number.
        number: u64,
        /// The pull-request URL.
        url: String,
    },
    /// `gh` is not ready.
    GhNotReady(GhReady),
    /// The Branch scope is read-only.
    ReadOnlyScope,
    /// The row is `.worktrees/` or under it.
    WorktreesFolder,
    /// Another action runs. The action it names.
    Busy(&'static str),
}

/// Whether `gh` can open pull requests here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GhReady {
    /// Ready, in this repository.
    Ready {
        /// `owner/name`.
        repo: String,
    },
    /// No `gh` on the machine.
    NotFound,
    /// `gh` is not signed in.
    NotSignedIn(String),
    /// `origin` is not a GitHub repository `gh` can reach.
    NotGitHub(String),
}

/// Whether a commit may run. `Ok(true)` stages all first.
pub fn can_commit(listing: &Listing, scope: Scope, message: &str) -> Result<bool, ShipRefusal> {
    let _ = (listing, scope, message);
    todo!()
}

/// Whether a push may run. An unknown default (`None`) passes on; the runner
/// resolves it in full and refuses before spawning `git push`.
pub fn can_push(listing: &Listing, scope: Scope, base: Option<&str>) -> Result<(), ShipRefusal> {
    let _ = (listing, scope, base);
    todo!()
}

/// Whether a draft pull request may be opened.
pub fn can_open_pr(
    listing: &Listing,
    scope: Scope,
    open_pr: Option<&PrStatus>,
    gh: &GhReady,
    base: &str,
    ahead_of_base: u32,
) -> Result<(), ShipRefusal> {
    let _ = (listing, scope, open_pr, gh, base, ahead_of_base);
    todo!()
}

/// The exact refusal text for the notice line.
pub fn refusal_text(r: &ShipRefusal) -> String {
    let _ = r;
    todo!()
}
