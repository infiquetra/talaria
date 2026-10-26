//! The launch state machine. Expected effects are the decision table, written by hand.
//! Step 0 emits nothing, so each sequence compare fails on an empty list.

use std::path::PathBuf;

use talaria_app::bg::{
    CallFailure, CreatedSession, CwdCheck, DescribeSummary, END_REASON_ABANDONED, LaunchEffect,
    LaunchIds, LaunchInput, LaunchMachine, LaunchOrigin, LaunchOutcome, LaunchRequest,
    NewSessionSpec, Step, SubmitReply, WorktreeWanted, compare_cwd,
};
use talaria_app::worktree::WorktreeRecord;

fn request(prompt: &str, worktree: bool) -> LaunchRequest {
    LaunchRequest {
        prompt: prompt.to_owned(),
        profile: "fixture".to_owned(),
        folder: PathBuf::from("repo"),
        model: None,
        provider: None,
        effort: None,
        worktree: worktree.then_some(WorktreeWanted { name: None }),
        origin: LaunchOrigin::CommandLine,
    }
}

fn ids() -> LaunchIds {
    LaunchIds {
        request_id: "req-1".to_owned(),
        submission_id: "sub-1".to_owned(),
    }
}

fn described_ok() -> LaunchInput {
    LaunchInput::Described(DescribeSummary {
        sources: vec!["tui".to_owned()],
        parameters: vec![
            "cwd".to_owned(),
            "model".to_owned(),
            "provider".to_owned(),
            "reasoning".to_owned(),
        ],
    })
}

fn created(cwd: &str) -> CreatedSession {
    CreatedSession {
        session_id: "sess-1".to_owned(),
        stored_session_id: Some("stored-1".to_owned()),
        subscription_id: "subscr-1".to_owned(),
        info_cwd: Some(cwd.to_owned()),
        model: Some("fixture-model".to_owned()),
    }
}

fn record() -> WorktreeRecord {
    WorktreeRecord {
        main_root: PathBuf::from("repo"),
        path: PathBuf::from("repo/.worktrees/talaria-fix-the-flaky-test-in-net-rs"),
        branch: "talaria/fix-the-flaky-test-in-net-rs".to_owned(),
        base_ref: "origin/main".to_owned(),
        base_commit: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        base_fetched: true,
        profile: "fixture".to_owned(),
        session_id: None,
        created_at: 1_700_000_000,
    }
}

fn admitted() -> SubmitReply {
    SubmitReply::Admitted {
        admission_id: "adm-1".to_owned(),
        status: "queued".to_owned(),
    }
}

fn launched(
    prompt: &str,
    cwd: PathBuf,
    tree: Option<WorktreeRecord>,
    warnings: Vec<String>,
) -> LaunchOutcome {
    LaunchOutcome::Launched(talaria_app::bg::Launched {
        session_id: "sess-1".to_owned(),
        stored_session_id: "stored-1".to_owned(),
        profile: "fixture".to_owned(),
        admission_id: "adm-1".to_owned(),
        admission_status: "queued".to_owned(),
        model: Some("fixture-model".to_owned()),
        cwd,
        worktree: tree,
        warnings,
        prompt: prompt.to_owned(),
    })
}

fn not_started(what: &str, next_step: &str, closed: Option<&str>, removed: bool) -> LaunchOutcome {
    LaunchOutcome::NotStarted {
        error: talaria_app::bg::LaunchError {
            what: what.to_owned(),
            next_step: next_step.to_owned(),
        },
        closed_session: closed.map(str::to_owned),
        tree_removed: removed,
        tree_kept: None,
    }
}

fn drive(request: LaunchRequest, inputs: &[LaunchInput]) -> (LaunchMachine, Vec<LaunchEffect>) {
    let (mut machine, mut effects) = LaunchMachine::start(request, ids());
    for input in inputs {
        if effects
            .iter()
            .any(|effect| matches!(effect, LaunchEffect::Finish(_)))
        {
            break;
        }
        effects.extend(machine.step(input.clone()));
        if effects.len() > 40 {
            break;
        }
    }
    (machine, effects)
}

#[test]
fn plain_launch_sequence() {
    let folder = PathBuf::from("repo");
    let inputs = [
        described_ok(),
        LaunchInput::Created(created("repo")),
        LaunchInput::CwdChecked(CwdCheck::Matches),
        LaunchInput::Submitted(admitted()),
        LaunchInput::Detached,
    ];
    let (machine, effects) = drive(request("Reply with the single word: ready", false), &inputs);
    let expected = vec![
        LaunchEffect::Describe,
        LaunchEffect::Create {
            new: NewSessionSpec {
                request_id: "req-1".to_owned(),
                cwd: folder.clone(),
                model: None,
                provider: None,
                reasoning: None,
            },
        },
        LaunchEffect::CheckCwd {
            sent: folder.clone(),
            reported: Some("repo".to_owned()),
        },
        LaunchEffect::Submit {
            session_id: "sess-1".to_owned(),
            text: "Reply with the single word: ready".to_owned(),
            submission_id: "sub-1".to_owned(),
        },
        LaunchEffect::FleetRunning {
            session_id: "sess-1".to_owned(),
        },
        LaunchEffect::Detach {
            session_id: "sess-1".to_owned(),
            subscription_id: "subscr-1".to_owned(),
            end_reason: None,
        },
        LaunchEffect::FleetReleased {
            session_id: "sess-1".to_owned(),
        },
        LaunchEffect::Finish(launched(
            "Reply with the single word: ready",
            folder,
            None,
            Vec::new(),
        )),
    ];
    assert_eq!(effects, expected, "expected effect sequence is empty");
    assert!(machine.submit_sent());
}

#[test]
fn worktree_launch_sequence() {
    let prompt = "Fix the flaky test in net.rs";
    let tree = record();
    let warning = "could not record the session id in git config (git missing).".to_owned();
    let inputs = [
        described_ok(),
        LaunchInput::WorktreeReady(tree.clone()),
        LaunchInput::Created(created(tree.path.to_str().unwrap_or("repo"))),
        LaunchInput::CwdChecked(CwdCheck::Matches),
        LaunchInput::SessionRecorded {
            warning: Some(warning.clone()),
        },
        LaunchInput::Submitted(admitted()),
        LaunchInput::Detached,
    ];
    let (machine, effects) = drive(request(prompt, true), &inputs);
    let expected = vec![
        LaunchEffect::Describe,
        LaunchEffect::PrepareWorktree {
            folder: PathBuf::from("repo"),
            slug: "fix-the-flaky-test-in-net-rs".to_owned(),
        },
        LaunchEffect::Create {
            new: NewSessionSpec {
                request_id: "req-1".to_owned(),
                cwd: tree.path.clone(),
                model: None,
                provider: None,
                reasoning: None,
            },
        },
        LaunchEffect::CheckCwd {
            sent: tree.path.clone(),
            reported: Some(tree.path.to_string_lossy().into_owned()),
        },
        LaunchEffect::RecordSession {
            record: tree.clone(),
            session_id: "sess-1".to_owned(),
        },
        LaunchEffect::Submit {
            session_id: "sess-1".to_owned(),
            text: prompt.to_owned(),
            submission_id: "sub-1".to_owned(),
        },
        LaunchEffect::FleetRunning {
            session_id: "sess-1".to_owned(),
        },
        LaunchEffect::Detach {
            session_id: "sess-1".to_owned(),
            subscription_id: "subscr-1".to_owned(),
            end_reason: None,
        },
        LaunchEffect::FleetReleased {
            session_id: "sess-1".to_owned(),
        },
        LaunchEffect::Finish(launched(
            prompt,
            tree.path.clone(),
            Some(tree),
            vec![warning],
        )),
    ];
    assert_eq!(effects, expected, "expected effect sequence is empty");
    assert!(machine.submit_sent());
}

#[test]
fn describe_without_tui_or_cwd_starts_nothing() {
    let missing_source = LaunchInput::Described(DescribeSummary {
        sources: vec!["cli".to_owned()],
        parameters: vec!["cwd".to_owned()],
    });
    let (_, effects) = drive(request("hello", false), &[missing_source]);
    assert_eq!(
        effects,
        vec![
            LaunchEffect::Describe,
            LaunchEffect::Finish(not_started(
                "this Hermes gateway does not offer the tui session source.",
                "Update Hermes to the release pinned for Talaria.",
                None,
                false,
            )),
        ],
        "expected effect sequence is empty"
    );

    let missing_cwd = LaunchInput::Described(DescribeSummary {
        sources: vec!["tui".to_owned()],
        parameters: Vec::new(),
    });
    let (_, effects) = drive(request("hello", false), &[missing_cwd]);
    assert_eq!(
        effects,
        vec![
            LaunchEffect::Describe,
            LaunchEffect::Finish(not_started(
                "this Hermes gateway does not accept a session folder for new sessions.",
                "Update Hermes to the release pinned for Talaria.",
                None,
                false,
            )),
        ]
    );
}

#[test]
fn create_refused_lost_or_malformed_rolls_back() {
    let tree = record();
    let refused = LaunchInput::Failed {
        step: Step::Create,
        failure: CallFailure::Refused {
            code: 4001,
            reason: Some("invalid_params".to_owned()),
            fields: vec!["cwd".to_owned()],
        },
    };
    let (_, effects) = drive(
        request("hello", true),
        &[
            described_ok(),
            LaunchInput::WorktreeReady(tree.clone()),
            refused,
            LaunchInput::RolledBack { kept: None },
        ],
    );
    assert_eq!(
        effects.last().cloned(),
        Some(LaunchEffect::Finish(not_started(
            "Hermes refused to create the session: invalid_params (field cwd).",
            "Nothing was started; check the options you passed.",
            None,
            true,
        ))),
        "expected effect sequence is empty"
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, LaunchEffect::Rollback(_)))
    );

    let lost = LaunchInput::Failed {
        step: Step::Create,
        failure: CallFailure::Unknown,
    };
    let (_, effects) = drive(request("hello", false), &[described_ok(), lost]);
    assert_eq!(
        effects.last().cloned(),
        Some(LaunchEffect::Finish(not_started(
            "Hermes did not confirm that the session was created.",
            "Nothing was sent to an agent. An empty, idle session may appear in talaria fleet list; it is safe to ignore.",
            None,
            false,
        )))
    );

    let malformed = LaunchInput::Failed {
        step: Step::Create,
        failure: CallFailure::Malformed("missing subscription_id".to_owned()),
    };
    let (_, effects) = drive(request("hello", false), &[described_ok(), malformed]);
    assert!(matches!(
        effects.last(),
        Some(LaunchEffect::Finish(LaunchOutcome::NotStarted { .. }))
    ));
}

#[test]
fn cwd_mismatch_abandons_before_submit() {
    let tree = record();
    let inputs = [
        described_ok(),
        LaunchInput::WorktreeReady(tree.clone()),
        LaunchInput::Created(created("elsewhere")),
        LaunchInput::CwdChecked(CwdCheck::Differs),
        LaunchInput::Detached,
        LaunchInput::RolledBack { kept: None },
    ];
    let (_, effects) = drive(request("Fix the flaky test in net.rs", true), &inputs);
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            LaunchEffect::Detach {
                end_reason: Some(END_REASON_ABANDONED),
                ..
            }
        )),
        "expected effect sequence is empty"
    );
    assert_eq!(
        effects.last().cloned(),
        Some(LaunchEffect::Finish(not_started(
            "Hermes did not confirm the session's folder, so the prompt was not sent.",
            "Check that the profile's terminal runs on this machine; the empty session was closed and the worktree removed.",
            Some("sess-1"),
            true,
        )))
    );

    let missing = [
        described_ok(),
        LaunchInput::WorktreeReady(tree),
        LaunchInput::Created(created("")),
        LaunchInput::CwdChecked(CwdCheck::Missing),
    ];
    let (_, effects) = drive(request("hello", true), &missing);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        LaunchEffect::Detach {
            end_reason: Some(END_REASON_ABANDONED),
            ..
        }
    )));

    let plain = [
        described_ok(),
        LaunchInput::Created(created("")),
        LaunchInput::CwdChecked(CwdCheck::Missing),
    ];
    let (_, effects) = drive(request("hello", false), &plain);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, LaunchEffect::Submit { .. }))
    );

    let dir = tempfile::tempdir().expect("temp");
    let sent = dir.path().join("sent");
    std::fs::create_dir(&sent).expect("sent");
    assert_eq!(compare_cwd(&sent, None), CwdCheck::Missing);
    assert_eq!(compare_cwd(&sent, Some("")), CwdCheck::Missing);
    assert_eq!(
        compare_cwd(&sent, Some("missing-folder")),
        CwdCheck::Differs
    );
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&sent, &link).expect("link");
    assert_eq!(
        compare_cwd(&sent, Some(&link.to_string_lossy())),
        CwdCheck::Matches
    );
    let other = dir.path().join("other");
    std::fs::create_dir(&other).expect("other");
    assert_eq!(
        compare_cwd(&sent, Some(&other.to_string_lossy())),
        CwdCheck::Differs
    );
}

#[test]
fn submit_refused_or_voice_stop_abandons() {
    let cases = [
        LaunchInput::Failed {
            step: Step::Submit,
            failure: CallFailure::Refused {
                code: 4001,
                reason: Some("invalid_params".to_owned()),
                fields: Vec::new(),
            },
        },
        LaunchInput::Failed {
            step: Step::Submit,
            failure: CallFailure::Refused {
                code: -32601,
                reason: Some("unknown_method".to_owned()),
                fields: Vec::new(),
            },
        },
        LaunchInput::Submitted(SubmitReply::VoiceStopped),
        LaunchInput::Failed {
            step: Step::Submit,
            failure: CallFailure::NotSent("socket closed".to_owned()),
        },
    ];
    let mut saw_one = false;
    for failure in cases {
        let tree = record();
        let inputs = [
            described_ok(),
            LaunchInput::WorktreeReady(tree),
            LaunchInput::Created(created(
                "repo/.worktrees/talaria-fix-the-flaky-test-in-net-rs",
            )),
            LaunchInput::CwdChecked(CwdCheck::Matches),
            LaunchInput::SessionRecorded { warning: None },
            failure,
            LaunchInput::Detached,
            LaunchInput::RolledBack { kept: None },
        ];
        let (_, effects) = drive(request("hello", true), &inputs);
        if !saw_one {
            assert!(
                effects.iter().any(|effect| matches!(
                    effect,
                    LaunchEffect::Detach {
                        end_reason: Some(END_REASON_ABANDONED),
                        ..
                    }
                )),
                "expected effect sequence is empty"
            );
            saw_one = true;
        }
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, LaunchEffect::Rollback(_)))
        );
        assert!(matches!(
            effects.last(),
            Some(LaunchEffect::Finish(LaunchOutcome::NotStarted {
                tree_removed: true,
                ..
            }))
        ));
    }
}

#[test]
fn submit_unknown_keeps_tree_and_never_ends_session() {
    let cases = [
        LaunchInput::Failed {
            step: Step::Submit,
            failure: CallFailure::Unknown,
        },
        LaunchInput::Failed {
            step: Step::Submit,
            failure: CallFailure::Unknown,
        },
        LaunchInput::Failed {
            step: Step::Submit,
            failure: CallFailure::Malformed("neither admission nor voice stop".to_owned()),
        },
    ];
    let mut first = true;
    for failure in cases {
        let tree = record();
        let inputs = [
            described_ok(),
            LaunchInput::WorktreeReady(tree.clone()),
            LaunchInput::Created(created(
                "repo/.worktrees/talaria-fix-the-flaky-test-in-net-rs",
            )),
            LaunchInput::CwdChecked(CwdCheck::Matches),
            LaunchInput::SessionRecorded { warning: None },
            failure,
            LaunchInput::Detached,
        ];
        let (_, effects) = drive(request("hello", true), &inputs);
        if first {
            assert!(
                effects.iter().any(|effect| matches!(
                    effect,
                    LaunchEffect::Detach {
                        end_reason: None,
                        ..
                    }
                )),
                "expected effect sequence is empty"
            );
            first = false;
        }
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, LaunchEffect::Rollback(_)))
        );
        assert!(matches!(
            effects.last(),
            Some(LaunchEffect::Finish(LaunchOutcome::Unknown { .. }))
        ));
        let _ = tree;
    }
}

#[test]
fn cancel_follows_submit_sent() {
    let (_, effects) = drive(request("hello", false), &[LaunchInput::Cancel]);
    assert_eq!(
        effects,
        vec![
            LaunchEffect::Describe,
            LaunchEffect::Finish(LaunchOutcome::Interrupted {
                tree_removed: false,
                tree_kept: None,
            }),
        ],
        "expected effect sequence is empty"
    );

    let created_late = [
        described_ok(),
        LaunchInput::Cancel,
        LaunchInput::Created(created("repo")),
        LaunchInput::Detached,
    ];
    let (machine, effects) = drive(request("hello", false), &created_late);
    assert!(!machine.submit_sent());
    assert!(effects.iter().any(|effect| matches!(
        effect,
        LaunchEffect::Detach {
            end_reason: Some(END_REASON_ABANDONED),
            ..
        }
    )));

    let tree = record();
    let during_record = [
        described_ok(),
        LaunchInput::WorktreeReady(tree),
        LaunchInput::Created(created(
            "repo/.worktrees/talaria-fix-the-flaky-test-in-net-rs",
        )),
        LaunchInput::CwdChecked(CwdCheck::Matches),
        LaunchInput::Cancel,
        LaunchInput::SessionRecorded { warning: None },
        LaunchInput::Detached,
        LaunchInput::RolledBack { kept: None },
    ];
    let (_, effects) = drive(request("hello", true), &during_record);
    assert!(matches!(
        effects.last(),
        Some(LaunchEffect::Finish(LaunchOutcome::Interrupted { .. }))
    ));

    let after_submit = [
        described_ok(),
        LaunchInput::Created(created("repo")),
        LaunchInput::CwdChecked(CwdCheck::Matches),
        LaunchInput::Cancel,
        LaunchInput::Submitted(admitted()),
        LaunchInput::Detached,
    ];
    let (machine, effects) = drive(request("hello", false), &after_submit);
    assert!(machine.submit_sent());
    assert!(matches!(
        effects.last(),
        Some(LaunchEffect::Finish(LaunchOutcome::Launched(_)))
    ));
}

proptest::proptest! {
    #![proptest_config(machine_config())]

    #[test]
    fn machine_invariants(choices in proptest::collection::vec(0u8..12u8, 0..12usize)) {
        let inputs: Vec<LaunchInput> = choices.into_iter().map(choice_input).collect();
        let (mut machine, mut effects) = drive(request("hello", true), &inputs);
        // The generated inputs may stop before the machine finishes; answer what it is still
        // waiting for, as the service would, so every run is taken to its finish.
        for _ in 0..16 {
            if effects
                .iter()
                .any(|effect| matches!(effect, LaunchEffect::Finish(_)))
            {
                break;
            }
            let Some(answer) = effects.iter().rev().find_map(answer_for) else {
                break;
            };
            effects.extend(machine.step(answer));
        }
        let finishes = effects
            .iter()
            .filter(|effect| matches!(effect, LaunchEffect::Finish(_)))
            .count();
        proptest::prop_assert_eq!(finishes, 1, "no Finish");
        let submits = effects
            .iter()
            .filter(|effect| matches!(effect, LaunchEffect::Submit { .. }))
            .count();
        proptest::prop_assert!(submits <= 1);
    }
}

/// The result a service would feed back for an effect that waits for one.
fn answer_for(effect: &LaunchEffect) -> Option<LaunchInput> {
    Some(match effect {
        LaunchEffect::Describe => described_ok(),
        LaunchEffect::PrepareWorktree { .. } => LaunchInput::WorktreeReady(record()),
        LaunchEffect::Create { new } => LaunchInput::Created(created(&new.cwd.to_string_lossy())),
        LaunchEffect::CheckCwd { .. } => LaunchInput::CwdChecked(CwdCheck::Matches),
        LaunchEffect::RecordSession { .. } => LaunchInput::SessionRecorded { warning: None },
        LaunchEffect::Submit { .. } => LaunchInput::Submitted(admitted()),
        LaunchEffect::Detach { .. } => LaunchInput::Detached,
        LaunchEffect::Rollback(_) => LaunchInput::RolledBack { kept: None },
        LaunchEffect::FleetRunning { .. }
        | LaunchEffect::FleetReleased { .. }
        | LaunchEffect::Finish(_) => return None,
    })
}

fn choice_input(choice: u8) -> LaunchInput {
    match choice {
        0 => described_ok(),
        1 => LaunchInput::Cancel,
        2 => LaunchInput::Detached,
        3 => LaunchInput::CwdChecked(CwdCheck::Matches),
        4 => LaunchInput::CwdChecked(CwdCheck::Differs),
        5 => LaunchInput::CwdChecked(CwdCheck::Missing),
        6 => LaunchInput::Submitted(admitted()),
        7 => LaunchInput::Submitted(SubmitReply::VoiceStopped),
        8 => LaunchInput::Failed {
            step: Step::Submit,
            failure: CallFailure::Unknown,
        },
        9 => LaunchInput::Failed {
            step: Step::Submit,
            failure: CallFailure::NotSent("lost".to_owned()),
        },
        10 => LaunchInput::WorktreeReady(record()),
        _ => LaunchInput::RolledBack { kept: None },
    }
}

fn machine_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        cases: 32,
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
        ..proptest::test_runner::Config::default()
    }
}
