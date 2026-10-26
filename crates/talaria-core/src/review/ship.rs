//! The fail-closed shipping gates. Pure. Issue #245.

use crate::review::model::FileStatus;
use crate::review::pr::{PrState, PrStatus};
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

/// Whether a path is `.worktrees/` or under it. Other checkouts live there.
pub(crate) fn in_worktrees(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    path == ".worktrees" || path.starts_with(".worktrees/")
}

/// The `main` and `master` names always count as the default branch.
fn is_default_name(name: &str) -> bool {
    name == "main" || name == "master"
}

/// Whether a commit may run. `Ok(true)` stages all first.
pub fn can_commit(listing: &Listing, scope: Scope, message: &str) -> Result<bool, ShipRefusal> {
    if scope == Scope::Branch {
        return Err(ShipRefusal::ReadOnlyScope);
    }
    let rows = || {
        listing
            .files
            .iter()
            .filter(|file| !in_worktrees(&file.path))
    };
    if rows().next().is_none() {
        return Err(ShipRefusal::NothingToCommit);
    }
    let conflicted = listing
        .files
        .iter()
        .filter(|file| file.status == FileStatus::Conflicted)
        .count();
    if conflicted > 0 {
        return Err(ShipRefusal::Conflicted(
            u32::try_from(conflicted).unwrap_or(u32::MAX),
        ));
    }
    if listing.head.head.is_none() {
        return Err(ShipRefusal::Detached);
    }
    if message.trim().is_empty() {
        return Err(ShipRefusal::EmptyMessage);
    }
    Ok(!rows().any(|file| file.staged))
}

/// Whether a push may run. An unknown default (`None`) passes on; the runner
/// resolves it in full and refuses before spawning `git push`.
pub fn can_push(listing: &Listing, scope: Scope, base: Option<&str>) -> Result<(), ShipRefusal> {
    if scope == Scope::Branch {
        return Err(ShipRefusal::ReadOnlyScope);
    }
    let Some(branch) = listing.head.head.as_deref() else {
        return Err(ShipRefusal::Detached);
    };
    if !listing.has_origin {
        return Err(ShipRefusal::NoOrigin);
    }
    if is_default_name(branch) || listing.default_branch.as_deref() == Some(branch) {
        return Err(ShipRefusal::DefaultBranch(branch.to_owned()));
    }
    if base == Some(branch) {
        return Err(ShipRefusal::BaseBranch(branch.to_owned()));
    }
    Ok(())
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
    can_push(listing, scope, Some(base))?;
    if let Some(pr) = open_pr.filter(|pr| pr.state == PrState::Open) {
        return Err(ShipRefusal::PrExists {
            number: pr.number,
            url: pr.url.clone(),
        });
    }
    if !matches!(gh, GhReady::Ready { .. }) {
        return Err(ShipRefusal::GhNotReady(gh.clone()));
    }
    if ahead_of_base == 0 {
        return Err(ShipRefusal::NoCommitsAhead {
            base: base.to_owned(),
            branch: listing.head.head.clone().unwrap_or_default(),
        });
    }
    Ok(())
}

/// A count with its noun, singular for one.
pub(crate) fn counted(count: usize, singular: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {singular}s")
    }
}

/// The exact refusal text for the notice line.
pub fn refusal_text(r: &ShipRefusal) -> String {
    match r {
        ShipRefusal::Detached => "HEAD is detached: check out a branch first.".to_owned(),
        ShipRefusal::NoOrigin => "This repository has no remote named origin.".to_owned(),
        ShipRefusal::DefaultBranch(name) => {
            format!("Talaria does not push or open pull requests from {name}, the default branch.")
        }
        ShipRefusal::BaseBranch(name) => format!(
            "{name} is the pull request's base: Talaria does not push or open pull requests from it."
        ),
        ShipRefusal::DefaultUnknown => {
            "Talaria cannot tell origin's default branch, so it will not push.".to_owned()
        }
        ShipRefusal::NothingToCommit => "Nothing to commit.".to_owned(),
        ShipRefusal::Conflicted(count) => format!(
            "Resolve the {} first.",
            counted(*count as usize, "conflicted file")
        ),
        ShipRefusal::EmptyMessage => "Write a commit message first.".to_owned(),
        ShipRefusal::NoCommitsAhead { base, branch } => {
            format!("No commits between origin/{base} and {branch}: commit first.")
        }
        ShipRefusal::PrExists { number, url } => {
            format!("Pull request #{number} already exists: {url}")
        }
        ShipRefusal::GhNotReady(GhReady::NotFound) => {
            "GitHub CLI (gh) not found: install it and run gh auth login to open pull requests."
                .to_owned()
        }
        ShipRefusal::GhNotReady(GhReady::NotSignedIn(reason)) => {
            format!("gh is not signed in: run gh auth login. ({reason})")
        }
        ShipRefusal::GhNotReady(GhReady::NotGitHub(reason)) => {
            format!("origin is not a GitHub repository gh can reach: {reason}")
        }
        ShipRefusal::GhNotReady(GhReady::Ready { .. }) => "gh is ready.".to_owned(),
        ShipRefusal::ReadOnlyScope => {
            "Branch view is read-only: press tab for uncommitted changes.".to_owned()
        }
        ShipRefusal::WorktreesFolder => {
            "Talaria does not stage or discard .worktrees/: it holds other checkouts.".to_owned()
        }
        ShipRefusal::Busy(action) => format!("Wait for {action} to finish."),
    }
}
