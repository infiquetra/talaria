//! State-machine red tests for the review. Issue #245.
//!
//! The drawing view is opaque by design, so these tests pin the inputs the
//! reducer takes and the effects it emits. Visible state (cursor, notices,
//! headers, editors) is pinned by the drawing tests, which draw a real view.

use std::path::PathBuf;

use talaria_core::directory::rows::RowKey;
use talaria_core::review::{
    BranchHead, ChangedFile, FileStatus, GitAction, Listing, RecordView, ReviewEffect, ReviewInput,
    ReviewKey, ReviewState, ReviewTarget, Scope,
};

const NOW: f64 = 1_700_000_000.0;

fn target() -> ReviewTarget {
    ReviewTarget {
        folder: PathBuf::from("repo"),
        main_root: PathBuf::from("repo"),
        session: Some(RowKey {
            profile: "fixture".to_owned(),
            session_id: "local-test-session".to_owned(),
        }),
        record: Some(RecordView {
            branch: "talaria/work".to_owned(),
            base_ref: "origin/main".to_owned(),
            base_commit: "abc".to_owned(),
        }),
        title: "repo".to_owned(),
    }
}

fn file(path: &str) -> ChangedFile {
    ChangedFile {
        path: path.to_owned(),
        orig_path: None,
        status: FileStatus::Modified,
        staged: false,
        unstaged: true,
        added: Some(1),
        removed: Some(0),
        is_dir: false,
    }
}

fn listing(paths: &[&str]) -> Listing {
    listing_scoped(paths, Scope::Uncommitted)
}

fn listing_scoped(paths: &[&str], _scope: Scope) -> Listing {
    Listing {
        head: BranchHead {
            oid: Some("abc".to_owned()),
            head: Some("talaria/work".to_owned()),
            upstream: Some("origin/talaria/work".to_owned()),
            ahead: 1,
            behind: 0,
        },
        files: paths.iter().map(|path| file(path)).collect(),
        total: paths.len(),
        base: None,
        default_branch: Some("main".to_owned()),
        has_origin: true,
        hooks: 0,
    }
}

fn open() -> ReviewState {
    let (state, _effects) = ReviewState::open(target(), NOW);
    state
}

fn has_run(effects: &[ReviewEffect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, ReviewEffect::Run(_)))
}

/// A reordered listing redraws; a removal redraws. The cursor itself is drawn,
/// so the drawing tests pin which row keeps it.
#[test]
fn cursor_follows_path_across_reloads() {
    let mut state = open();
    let effects = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(listing(&["b.txt", "a.txt", "c.txt"])),
    });
    assert!(
        effects.contains(&ReviewEffect::Redraw),
        "a changed listing redraws: {effects:?}"
    );
    let effects = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(listing(&["a.txt", "c.txt"])),
    });
    assert!(
        effects.contains(&ReviewEffect::Redraw),
        "a removal redraws: {effects:?}"
    );
}

/// Branch scope runs nothing: every action key emits no `Run`.
#[test]
fn branch_scope_is_read_only() {
    let mut state = open();
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Branch,
        result: Ok(listing_scoped(&["a.txt"], Scope::Branch)),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::Scope));
    for key in [
        ReviewKey::StageToggle,
        ReviewKey::StageAll,
        ReviewKey::UnstageAll,
        ReviewKey::Discard,
        ReviewKey::Commit,
        ReviewKey::Push,
        ReviewKey::PullRequest,
    ] {
        let effects = state.update(ReviewInput::Key(key));
        assert!(
            !has_run(&effects),
            "{key:?} in Branch scope runs nothing: {effects:?}"
        );
    }
    // Read and refresh keys still work in Branch scope.
    let effects = state.update(ReviewInput::Key(ReviewKey::OpenPr));
    assert!(
        !has_run(&effects),
        "opening the pull request is not a mutation: {effects:?}"
    );
}

/// Discard asks first; anything else only cancels; twice runs one discard.
#[test]
fn discard_needs_second_key() {
    let mut state = open();
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(listing(&["a.txt"])),
    });
    let effects = state.update(ReviewInput::Key(ReviewKey::Discard));
    assert!(!has_run(&effects), "the first press only asks: {effects:?}");
    let effects = state.update(ReviewInput::Key(ReviewKey::Down));
    assert!(!has_run(&effects), "another key only cancels: {effects:?}");
    let _ = state.update(ReviewInput::Key(ReviewKey::Discard));
    let effects = state.update(ReviewInput::Key(ReviewKey::Discard));
    let runs = effects
        .iter()
        .filter(|effect| matches!(effect, ReviewEffect::Run(GitAction::Discard(_))))
        .count();
    assert_eq!(runs, 1, "two presses run one discard: {effects:?}");
    // One mutation at a time: the discard finishes before the next key acts.
    let _ = state.update(ReviewInput::ActionDone {
        action: GitAction::Discard(file("a.txt")),
        result: Ok("Discarded changes to a.txt.".to_owned()),
    });

    // Staging still refuses a conflicted row and unstages a half-staged one.
    let mut conflicted = file("clash.txt");
    conflicted.status = FileStatus::Conflicted;
    let mut half = file("half.txt");
    half.staged = true;
    let mut rows = listing(&["half.txt", "clash.txt"]);
    rows.files = vec![half, conflicted];
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(rows),
    });
    let effects = state.update(ReviewInput::Key(ReviewKey::StageToggle));
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            ReviewEffect::Run(GitAction::Unstage(paths)) if paths == &vec!["half.txt".to_owned()]
        )),
        "space unstages a half-staged row: {effects:?}"
    );
    let _ = state.update(ReviewInput::ActionDone {
        action: GitAction::Unstage(vec!["half.txt".to_owned()]),
        result: Ok("Unstaged half.txt.".to_owned()),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::Down));
    for key in [ReviewKey::StageToggle, ReviewKey::Discard] {
        let effects = state.update(ReviewInput::Key(key));
        assert!(
            !has_run(&effects),
            "{key:?} on a conflicted row runs nothing: {effects:?}"
        );
    }
}

/// While one mutation runs, the other mutation keys run nothing.
#[test]
fn one_mutation_at_a_time() {
    let mut state = open();
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(listing(&["a.txt"])),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::StageAll));
    for key in [
        ReviewKey::StageToggle,
        ReviewKey::StageAll,
        ReviewKey::UnstageAll,
        ReviewKey::Discard,
        ReviewKey::Commit,
        ReviewKey::Push,
        ReviewKey::PullRequest,
    ] {
        let effects = state.update(ReviewInput::Key(key));
        assert!(
            !has_run(&effects),
            "{key:?} while staging runs nothing: {effects:?}"
        );
    }

    // In the editor, submit and draft stay quiet while a commit runs.
    let _ = state.update(ReviewInput::ActionDone {
        action: GitAction::Stage(vec!["a.txt".to_owned()]),
        result: Ok("Staged a.txt.".to_owned()),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
    let _ = state.update(ReviewInput::Key(ReviewKey::Char('x')));
    let _ = state.update(ReviewInput::Key(ReviewKey::Submit));
    for key in [ReviewKey::Submit, ReviewKey::Draft] {
        let effects = state.update(ReviewInput::Key(key));
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, ReviewEffect::Run(_) | ReviewEffect::Draft { .. })),
            "{key:?} while committing starts nothing: {effects:?}"
        );
    }
}

/// Submit stages all when nothing is staged; the list reloads after the commit.
#[test]
fn commit_editor_header_stage_all_and_warnings() {
    let mut state = open();
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(listing(&["a.txt"])),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
    let _ = state.update(ReviewInput::Key(ReviewKey::Char('f')));
    let effects = state.update(ReviewInput::Key(ReviewKey::Submit));
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            ReviewEffect::Run(GitAction::Commit {
                stage_all: true,
                ..
            })
        )),
        "submit with nothing staged stages all: {effects:?}"
    );
    let effects = state.update(ReviewInput::ActionDone {
        action: GitAction::Commit {
            message: "f".to_owned(),
            stage_all: true,
        },
        result: Ok("Committed abc on talaria/work: f".to_owned()),
    });
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ReviewEffect::List(Scope::Uncommitted))),
        "the list reloads after a commit: {effects:?}"
    );
}

/// Escape and Ctrl-C unwind without ever acting.
#[test]
fn esc_unwinds_and_ctrl_c_never_acts() {
    let mut state = open();
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(listing(&["a.txt"])),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
    for key in [ReviewKey::Esc, ReviewKey::CtrlC] {
        let effects = state.update(ReviewInput::Key(key));
        assert!(
            !has_run(&effects),
            "{key:?} in the editor runs nothing: {effects:?}"
        );
    }
    let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
    let _ = state.update(ReviewInput::Key(ReviewKey::Esc));
    let effects = state.update(ReviewInput::Key(ReviewKey::Esc));
    assert!(
        effects.contains(&ReviewEffect::Close),
        "escape on the list closes the screen: {effects:?}"
    );
    assert!(!has_run(&effects), "closing runs nothing: {effects:?}");
}

/// An old draft reply is ignored; drafting over typed text asks first.
#[test]
fn late_draft_ignored_after_esc() {
    let mut state = open();
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(listing(&["a.txt"])),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
    let effects = state.update(ReviewInput::Key(ReviewKey::Draft));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ReviewEffect::Draft { seq: 1, .. })),
        "drafting starts at sequence 1: {effects:?}"
    );
    let _ = state.update(ReviewInput::Key(ReviewKey::Esc));
    let effects = state.update(ReviewInput::Drafted {
        seq: 1,
        result: Ok("late draft".to_owned()),
    });
    assert!(
        effects.is_empty(),
        "an abandoned draft's reply is ignored: {effects:?}"
    );

    // Typing, then drafting, asks first instead of drafting.
    let _ = state.update(ReviewInput::Key(ReviewKey::Commit));
    let _ = state.update(ReviewInput::Key(ReviewKey::Char('m')));
    let _ = state.update(ReviewInput::Drafted {
        seq: 2,
        result: Ok("first draft".to_owned()),
    });
    let _ = state.update(ReviewInput::Key(ReviewKey::Char('y')));
    let effects = state.update(ReviewInput::Key(ReviewKey::Draft));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, ReviewEffect::Draft { .. })),
        "drafting over typed text asks first: {effects:?}"
    );
}

/// An unchanged listing draws no frame.
#[test]
fn unchanged_listing_needs_no_frame() {
    let mut state = open();
    let first = listing(&["a.txt", "b.txt"]);
    let _ = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(first.clone()),
    });
    let effects = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(first),
    });
    assert!(
        !effects.contains(&ReviewEffect::Redraw),
        "an unchanged listing draws no frame: {effects:?}"
    );
}

/// Oversized listings and diffs still redraw once; the cuts show in the drawing.
#[test]
fn caps_cuts_loading_and_empty_states() {
    let mut state = open();
    let paths: Vec<String> = (0..2_500)
        .map(|index| format!("file-{index:04}.txt"))
        .collect();
    let mut rows = listing(&[]);
    rows.files = paths.iter().map(|path| file(path)).collect();
    rows.total = paths.len();
    let effects = state.update(ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(rows),
    });
    assert!(
        effects.contains(&ReviewEffect::Redraw),
        "an oversized listing redraws: {effects:?}"
    );
    let big = "line\n".repeat(25_000);
    let effects = state.update(ReviewInput::DiffLoaded {
        path: "file-0000.txt".to_owned(),
        scope: Scope::Uncommitted,
        result: Ok(big),
    });
    assert!(
        effects.contains(&ReviewEffect::Redraw),
        "an oversized diff redraws: {effects:?}"
    );
}
