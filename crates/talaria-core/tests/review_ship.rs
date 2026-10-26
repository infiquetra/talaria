//! Shipping-gate red tests for the review. Issue #245.

use talaria_core::review::{
    BranchHead, ChangedFile, Checks, FileStatus, GhReady, Listing, PrState, PrStatus, Scope,
    ShipRefusal, can_commit, can_open_pr, can_push, refusal_text,
};

fn head(branch: Option<&str>) -> BranchHead {
    BranchHead {
        oid: Some("abc".to_owned()),
        head: branch.map(str::to_owned),
        upstream: Some("origin/talaria/work".to_owned()),
        ahead: 1,
        behind: 0,
    }
}

fn file(path: &str, staged: bool, status: FileStatus) -> ChangedFile {
    ChangedFile {
        path: path.to_owned(),
        orig_path: None,
        status,
        staged,
        unstaged: !staged,
        added: Some(1),
        removed: Some(0),
        is_dir: false,
    }
}

fn listing() -> Listing {
    Listing {
        head: head(Some("talaria/work")),
        files: vec![file("a.txt", true, FileStatus::Modified)],
        total: 1,
        base: None,
        default_branch: Some("main".to_owned()),
        has_origin: true,
        hooks: 0,
    }
}

fn status(number: u64, state: PrState) -> PrStatus {
    PrStatus {
        number,
        state,
        draft: true,
        checks: Checks::Pending,
        review: None,
        url: format!("https://github.com/fixture-owner/fixture-repo/pull/{number}"),
        branch: "talaria/work".to_owned(),
    }
}

/// Every refusal trips in check order, and every text is exact.
#[test]
fn refusals_and_texts() {
    // Commit: scope, nothing, conflicts, detached, message.
    let rows = listing();
    assert_eq!(
        can_commit(&rows, Scope::Branch, "x"),
        Err(ShipRefusal::ReadOnlyScope)
    );
    let empty = Listing {
        files: vec![],
        total: 0,
        ..listing()
    };
    assert_eq!(
        can_commit(&empty, Scope::Uncommitted, "x"),
        Err(ShipRefusal::NothingToCommit)
    );
    let mut clash = listing();
    clash.files = vec![
        file("one.txt", false, FileStatus::Conflicted),
        file("two.txt", false, FileStatus::Conflicted),
    ];
    assert_eq!(
        can_commit(&clash, Scope::Uncommitted, "x"),
        Err(ShipRefusal::Conflicted(2))
    );
    let mut detached = listing();
    detached.head = head(None);
    assert_eq!(
        can_commit(&detached, Scope::Uncommitted, ""),
        Err(ShipRefusal::Detached),
        "detached beats an empty message"
    );
    assert_eq!(
        can_commit(&rows, Scope::Uncommitted, "  \n "),
        Err(ShipRefusal::EmptyMessage)
    );
    assert_eq!(can_commit(&rows, Scope::Uncommitted, "feat: x"), Ok(false));
    let mut unstaged = listing();
    unstaged.files = vec![file("a.txt", false, FileStatus::Modified)];
    assert_eq!(
        can_commit(&unstaged, Scope::Uncommitted, "feat: x"),
        Ok(true),
        "nothing staged stages all"
    );

    // Push: scope, detached, origin, default, base. An unknown default passes
    // on; the git runner refuses before spawning.
    assert_eq!(
        can_push(&rows, Scope::Branch, None),
        Err(ShipRefusal::ReadOnlyScope)
    );
    assert_eq!(
        can_push(&detached, Scope::Uncommitted, None),
        Err(ShipRefusal::Detached)
    );
    let mut no_origin = listing();
    no_origin.has_origin = false;
    assert_eq!(
        can_push(&no_origin, Scope::Uncommitted, None),
        Err(ShipRefusal::NoOrigin)
    );
    for branch in ["main", "master"] {
        let mut named = listing();
        named.head = head(Some(branch));
        assert_eq!(
            can_push(&named, Scope::Uncommitted, None),
            Err(ShipRefusal::DefaultBranch(branch.to_owned())),
            "{branch} always counts as the default branch"
        );
    }
    let mut on_default = listing();
    on_default.head = head(Some("trunk"));
    on_default.default_branch = Some("trunk".to_owned());
    assert_eq!(
        can_push(&on_default, Scope::Uncommitted, None),
        Err(ShipRefusal::DefaultBranch("trunk".to_owned()))
    );
    assert_eq!(
        can_push(&rows, Scope::Uncommitted, Some("talaria/work")),
        Err(ShipRefusal::BaseBranch("talaria/work".to_owned()))
    );
    let mut unknown = listing();
    unknown.default_branch = None;
    assert_eq!(can_push(&unknown, Scope::Uncommitted, None), Ok(()));
    assert_eq!(can_push(&rows, Scope::Uncommitted, None), Ok(()));

    // Pull request: the local refusals, then a known open pull request, then
    // readiness, then commits ahead.
    let ready = GhReady::Ready {
        repo: "fixture-owner/fixture-repo".to_owned(),
    };
    assert_eq!(
        can_open_pr(&rows, Scope::Branch, None, &ready, "main", 1),
        Err(ShipRefusal::ReadOnlyScope)
    );
    assert_eq!(
        can_open_pr(&detached, Scope::Uncommitted, None, &ready, "main", 1),
        Err(ShipRefusal::Detached)
    );
    let open = status(12, PrState::Open);
    assert_eq!(
        can_open_pr(
            &rows,
            Scope::Uncommitted,
            Some(&open),
            &GhReady::NotFound,
            "main",
            1
        ),
        Err(ShipRefusal::PrExists {
            number: 12,
            url: open.url.clone(),
        }),
        "a known pull request beats readiness"
    );
    for state in [PrState::Merged, PrState::Closed] {
        let old = status(11, state);
        assert_eq!(
            can_open_pr(&rows, Scope::Uncommitted, Some(&old), &ready, "main", 1),
            Ok(()),
            "{state:?} does not block a new draft"
        );
    }
    assert_eq!(
        can_open_pr(
            &rows,
            Scope::Uncommitted,
            None,
            &GhReady::NotFound,
            "main",
            1
        ),
        Err(ShipRefusal::GhNotReady(GhReady::NotFound))
    );
    assert_eq!(
        can_open_pr(&rows, Scope::Uncommitted, None, &ready, "main", 0),
        Err(ShipRefusal::NoCommitsAhead {
            base: "main".to_owned(),
            branch: "talaria/work".to_owned(),
        })
    );
    assert_eq!(
        can_open_pr(&rows, Scope::Uncommitted, None, &ready, "main", 1),
        Ok(())
    );

    // Every text, exact.
    let texts: Vec<(ShipRefusal, &str)> = vec![
        (
            ShipRefusal::Detached,
            "HEAD is detached: check out a branch first.",
        ),
        (
            ShipRefusal::NoOrigin,
            "This repository has no remote named origin.",
        ),
        (
            ShipRefusal::DefaultBranch("trunk".to_owned()),
            "Talaria does not push or open pull requests from trunk, the default branch.",
        ),
        (
            ShipRefusal::BaseBranch("main".to_owned()),
            "main is the pull request's base: Talaria does not push or open pull requests from it.",
        ),
        (
            ShipRefusal::DefaultUnknown,
            "Talaria cannot tell origin's default branch, so it will not push.",
        ),
        (ShipRefusal::NothingToCommit, "Nothing to commit."),
        (
            ShipRefusal::Conflicted(2),
            "Resolve the 2 conflicted files first.",
        ),
        (ShipRefusal::EmptyMessage, "Write a commit message first."),
        (
            ShipRefusal::NoCommitsAhead {
                base: "main".to_owned(),
                branch: "talaria/work".to_owned(),
            },
            "No commits between origin/main and talaria/work: commit first.",
        ),
        (
            ShipRefusal::PrExists {
                number: 12,
                url: "https://github.com/fixture-owner/fixture-repo/pull/12".to_owned(),
            },
            "Pull request #12 already exists: https://github.com/fixture-owner/fixture-repo/pull/12",
        ),
        (
            ShipRefusal::GhNotReady(GhReady::NotFound),
            "GitHub CLI (gh) not found: install it and run gh auth login to open pull requests.",
        ),
        (
            ShipRefusal::GhNotReady(GhReady::NotSignedIn("none".to_owned())),
            "gh is not signed in: run gh auth login. (none)",
        ),
        (
            ShipRefusal::GhNotReady(GhReady::NotGitHub("nope".to_owned())),
            "origin is not a GitHub repository gh can reach: nope",
        ),
        (
            ShipRefusal::ReadOnlyScope,
            "Branch view is read-only: press tab for uncommitted changes.",
        ),
        (
            ShipRefusal::WorktreesFolder,
            "Talaria does not stage or discard .worktrees/: it holds other checkouts.",
        ),
        (ShipRefusal::Busy("commit"), "Wait for commit to finish."),
    ];
    for (refusal, text) in &texts {
        assert_eq!(&refusal_text(refusal), text, "{refusal:?}");
    }
}
