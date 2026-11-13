//! Property red tests for the review. Issue #245.
//!
//! The cursor and the Branch scope live inside the reducer where a property
//! cannot see them: the drawing tests pin the cursor, and the read-only test
//! pins Branch scope. This property pins what the effects show: at most one
//! mutation in flight, and no shipping from a head the listing knows is bad.

use proptest::prelude::*;
use std::path::PathBuf;
use talaria_core::review::{
    BranchHead, ChangedFile, FileStatus, GitAction, Listing, ReviewEffect, ReviewInput, ReviewKey,
    ReviewState, ReviewTarget, Scope,
};

fn target() -> ReviewTarget {
    ReviewTarget {
        folder: PathBuf::from("repo"),
        main_root: PathBuf::from("repo"),
        session: None,
        record: None,
        title: "repo".to_owned(),
    }
}

fn file(path: &str, status: FileStatus) -> ChangedFile {
    ChangedFile {
        path: path.to_owned(),
        orig_path: None,
        status,
        staged: false,
        unstaged: true,
        added: Some(1),
        removed: Some(0),
        is_dir: false,
    }
}

fn listing(head: Option<&str>, default: Option<&str>, paths: Vec<ChangedFile>) -> Listing {
    Listing {
        head: BranchHead {
            oid: Some("abc".to_owned()),
            head: head.map(str::to_owned),
            upstream: Some("origin/talaria/work".to_owned()),
            ahead: 1,
            behind: 0,
        },
        total: paths.len(),
        files: paths,
        base: None,
        default_branch: default.map(str::to_owned),
        has_origin: true,
        hooks: 0,
    }
}

fn listings() -> Vec<Listing> {
    let plain = || vec![file("a.txt", FileStatus::Modified)];
    vec![
        listing(Some("talaria/work"), Some("main"), plain()),
        listing(None, Some("main"), plain()),
        listing(Some("main"), Some("main"), plain()),
        listing(Some("master"), Some("main"), plain()),
        listing(Some("trunk"), Some("trunk"), plain()),
        listing(Some("talaria/work"), None, plain()),
        listing(Some("talaria/work"), Some("main"), vec![]),
        listing(
            Some("talaria/work"),
            Some("main"),
            vec![file("clash.txt", FileStatus::Conflicted)],
        ),
    ]
}

fn key_strategy() -> impl Strategy<Value = ReviewKey> {
    prop_oneof![
        Just(ReviewKey::Up),
        Just(ReviewKey::Down),
        Just(ReviewKey::StageToggle),
        Just(ReviewKey::StageAll),
        Just(ReviewKey::UnstageAll),
        Just(ReviewKey::Discard),
        Just(ReviewKey::Commit),
        Just(ReviewKey::Push),
        Just(ReviewKey::PullRequest),
        Just(ReviewKey::OpenPr),
        Just(ReviewKey::Refresh),
        Just(ReviewKey::Submit),
        Just(ReviewKey::Esc),
        Just(ReviewKey::Enter),
    ]
}

fn input_strategy() -> impl Strategy<Value = ReviewInput> {
    let rows = listings();
    let listed = prop_oneof![
        Just(rows[0].clone()),
        Just(rows[1].clone()),
        Just(rows[2].clone()),
        Just(rows[3].clone()),
        Just(rows[4].clone()),
        Just(rows[5].clone()),
        Just(rows[6].clone()),
        Just(rows[7].clone()),
    ]
    .prop_map(|rows| ReviewInput::Listed {
        scope: Scope::Uncommitted,
        result: Ok(rows),
    });
    let done = prop_oneof![
        Just(GitAction::Push),
        Just(GitAction::Commit {
            message: "x".to_owned(),
            stage_all: false,
        }),
    ]
    .prop_flat_map(|action| {
        prop_oneof![Just(true), Just(false)].prop_map(move |ok| ReviewInput::ActionDone {
            action: action.clone(),
            result: if ok {
                Ok("done".to_owned())
            } else {
                Err("failed".to_owned())
            },
        })
    });
    prop_oneof![
        key_strategy().prop_map(ReviewInput::Key),
        listed,
        done,
        Just(ReviewInput::Tick { now: 1.0 }),
    ]
}

/// At most one `Run` in flight; no commit, push or pull request from a head
/// the listing knows is bad. An unknown default is the git runner's case.
#[test]
fn never_two_mutations_and_never_ship_from_bad_head() {
    proptest!(|(inputs in proptest::collection::vec(input_strategy(), 1..50))| {
        let (mut state, _) = ReviewState::open(target(), 1.0);
        let mut in_flight = 0_u32;
        let mut rows: Option<Listing> = None;
        for input in &inputs {
            if let ReviewInput::Listed { result: Ok(next), .. } = input {
                rows = Some(next.clone());
            }
            let effects = state.update(input.clone());
            for effect in &effects {
                if let ReviewEffect::Run(action) = effect {
                    in_flight += 1;
                    prop_assert!(
                        in_flight <= 1,
                        "two mutations in flight: {inputs:?}"
                    );
                    if let Some(known) = &rows {
                        let head = known.head.head.as_deref();
                        let default = known.default_branch.as_deref();
                        let bad_head = head.is_none()
                            || head == Some("main")
                            || head == Some("master")
                            || default.is_some_and(|name| Some(name) == head);
                        match action {
                            GitAction::Push => prop_assert!(
                                !bad_head,
                                "push from a bad head: {inputs:?}"
                            ),
                            GitAction::CreatePr { base, branch } => {
                                prop_assert!(!bad_head, "pull request from a bad head: {inputs:?}");
                                prop_assert!(
                                    branch != base,
                                    "pull request from its base: {inputs:?}"
                                );
                            }
                            GitAction::Commit { .. } => {
                                prop_assert!(
                                    head.is_some(),
                                    "commit on a detached head: {inputs:?}"
                                );
                                prop_assert!(
                                    !known.files.is_empty(),
                                    "commit with nothing to commit: {inputs:?}"
                                );
                                prop_assert!(
                                    !known.files.iter().any(|row| row.status == FileStatus::Conflicted),
                                    "commit with conflicted files: {inputs:?}"
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
            if matches!(input, ReviewInput::ActionDone { .. }) {
                in_flight = 0;
            }
        }
    });
}

/// Malformed bytes never panic a parser: they parse or they fail.
#[test]
fn parsers_never_panic_on_malformed_bytes() {
    use talaria_core::review::{
        count_lines, parse_name_status_z, parse_numstat_z, parse_push_porcelain, parse_status_v2,
    };
    proptest!(|(bytes in proptest::collection::vec(any::<u8>(), 0..256))| {
        let _ = parse_status_v2(&bytes);
        let _ = parse_numstat_z(&bytes);
        let _ = parse_name_status_z(&bytes);
        let _ = count_lines(&bytes);
        if let Ok(text) = std::str::from_utf8(&bytes) {
            let _ = parse_push_porcelain(text);
        }
    });
}
