//! Tests 15 to 17 (#237): the `/cd` reducer checks, moves and outcomes.

use std::path::PathBuf;

use serde_json::json;
use talaria_core::mutation::{MutationLedger, MutationOutcome, MutationRequest};
use talaria_core::session::NoticeKind;
use talaria_core::workspace::{
    CdCommand, CdContext, CdEffect, CdInput, CdState, FolderCheck, SessionFolder,
};

const S1: &str = "s-1";
const S2: &str = "s-2";
const HOME: &str = "/home-fixture";
const FOLDER_A: &str = "/work/proj-a";
const FOLDER_B: &str = "/work/proj-b";
const LAUNCH: &str = "testlaunch-cd";

fn folder(cwd: Option<&str>, branch: Option<&str>) -> SessionFolder {
    SessionFolder {
        cwd: cwd.map(str::to_owned),
        branch: branch.map(str::to_owned),
    }
}

fn ctx<'a>(
    session: Option<&'a str>,
    folder: &'a SessionFolder,
    home: Option<&'a std::path::Path>,
) -> CdContext<'a> {
    CdContext {
        session_id: session,
        turn_running: false,
        folder,
        home,
        interrupt_label: Some("Esc"),
    }
}

fn update(
    state: &mut CdState,
    context: &CdContext<'_>,
    ledger: &mut MutationLedger,
    input: CdInput,
) -> Vec<CdEffect> {
    state.update(context, ledger, input)
}

fn notices(effects: &[CdEffect]) -> Vec<(NoticeKind, String)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            CdEffect::Notice { kind, text } => Some((*kind, text.clone())),
            _ => None,
        })
        .collect()
}

fn checks(effects: &[CdEffect]) -> Vec<(String, PathBuf)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            CdEffect::CheckFolder { session_id, target } => {
                Some((session_id.clone(), target.clone()))
            }
            _ => None,
        })
        .collect()
}

fn sends(effects: &[CdEffect]) -> Vec<MutationRequest> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            CdEffect::Send(request) => Some(request.clone()),
            _ => None,
        })
        .collect()
}

fn updates(effects: &[CdEffect]) -> Vec<(String, String, Option<String>)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            CdEffect::UpdateFolder {
                session_id,
                cwd,
                branch,
            } => Some((session_id.clone(), cwd.clone(), branch.clone())),
            _ => None,
        })
        .collect()
}

fn refused(reason: &str, rpc_code: i64) -> MutationOutcome {
    MutationOutcome::Refused {
        reason: reason.to_owned(),
        field: None,
        detail: None,
        rpc_code,
    }
}

/// Move to `target` and answer the check as a directory; returns the send effects.
fn move_to_send(
    state: &mut CdState,
    ledger: &mut MutationLedger,
    context: &CdContext<'_>,
    target: &str,
) -> Vec<CdEffect> {
    let effects = update(
        state,
        context,
        ledger,
        CdInput::Command(CdCommand::Move(target.to_owned())),
    );
    let pending = checks(&effects);
    assert_eq!(pending.len(), 1);
    update(
        state,
        context,
        ledger,
        CdInput::Checked {
            session_id: pending[0].0.clone(),
            target: pending[0].1.clone(),
            check: FolderCheck::Directory,
        },
    )
}

#[test]
fn cd_checks_before_sending() {
    let home = PathBuf::from(HOME);

    // C2 with a known folder, plain and shortened.
    let known = folder(Some(FOLDER_A), Some("main"));
    let mut state = CdState::new();
    let mut ledger = MutationLedger::new(LAUNCH);
    let effects = update(
        &mut state,
        &ctx(Some(S1), &known, Some(&home)),
        &mut ledger,
        CdInput::Command(CdCommand::Show),
    );
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "This session works in /work/proj-a (main). Use /cd <folder> to move it.".to_owned()
        )]
    );
    let under_home = folder(Some("/home-fixture/work/api"), Some("main"));
    let effects = update(
        &mut state,
        &ctx(Some(S1), &under_home, Some(&home)),
        &mut ledger,
        CdInput::Command(CdCommand::Show),
    );
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "This session works in ~/work/api (main). Use /cd <folder> to move it.".to_owned()
        )]
    );

    // C2 with an unknown folder, and with no session at all.
    let unknown = folder(None, None);
    let effects = update(
        &mut state,
        &ctx(Some(S1), &unknown, Some(&home)),
        &mut ledger,
        CdInput::Command(CdCommand::Show),
    );
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "This session's folder is unknown. Use /cd <full path> to set it.".to_owned()
        )]
    );
    let effects = update(
        &mut state,
        &ctx(None, &unknown, Some(&home)),
        &mut ledger,
        CdInput::Command(CdCommand::Show),
    );
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "This session's folder is unknown. Use /cd <full path> to set it.".to_owned()
        )]
    );

    // C15: a move with no session.
    let effects = update(
        &mut state,
        &ctx(None, &known, Some(&home)),
        &mut ledger,
        CdInput::Command(CdCommand::Move(FOLDER_B.to_owned())),
    );
    assert!(checks(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "No session is open, so there is no folder to change.".to_owned()
        )]
    );

    // C7: a turn is running.
    let running = CdContext {
        session_id: Some(S1),
        turn_running: true,
        folder: &known,
        home: Some(&home),
        interrupt_label: Some("Esc"),
    };
    let effects = update(
        &mut state,
        &running,
        &mut ledger,
        CdInput::Command(CdCommand::Move(FOLDER_B.to_owned())),
    );
    assert!(checks(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Warning,
            "A turn is running. Wait for it to finish, or press Esc to stop it, then run /cd \
             again."
                .to_owned()
        )]
    );

    // C19: a second move while a check is pending, and while a move is in flight.
    let context = ctx(Some(S1), &known, Some(&home));
    let effects = update(
        &mut state,
        &context,
        &mut ledger,
        CdInput::Command(CdCommand::Move(FOLDER_B.to_owned())),
    );
    assert_eq!(checks(&effects).len(), 1);
    let effects = update(
        &mut state,
        &context,
        &mut ledger,
        CdInput::Command(CdCommand::Move("/work/proj-c".to_owned())),
    );
    assert!(checks(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Warning,
            "A /cd is already being checked or sent. Wait for its answer.".to_owned()
        )]
    );
    ledger.fences_mut(S1).adopt(Some(12), Some(4));
    let mut state = CdState::new();
    let effects = move_to_send(&mut state, &mut ledger, &context, FOLDER_B);
    assert_eq!(sends(&effects).len(), 1);
    let effects = update(
        &mut state,
        &context,
        &mut ledger,
        CdInput::Command(CdCommand::Move("/work/proj-c".to_owned())),
    );
    assert!(checks(&effects).is_empty());
    assert!(sends(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Warning,
            "A /cd is already being checked or sent. Wait for its answer.".to_owned()
        )]
    );

    // C3b, C3c, C3d: unresolvable targets.
    let mut state = CdState::new();
    let effects = update(
        &mut state,
        &ctx(Some(S1), &unknown, Some(&home)),
        &mut ledger,
        CdInput::Command(CdCommand::Move("rel".to_owned())),
    );
    assert!(checks(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Error,
            "This session's folder is unknown, so a relative path cannot be used. Give the full \
             path."
                .to_owned()
        )]
    );
    let effects = update(
        &mut state,
        &context,
        &mut ledger,
        CdInput::Command(CdCommand::Move("~name".to_owned())),
    );
    assert!(checks(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Error,
            "Talaria does not expand ~name. Give the full path.".to_owned()
        )]
    );
    let effects = update(
        &mut state,
        &ctx(Some(S1), &known, None),
        &mut ledger,
        CdInput::Command(CdCommand::Move("~".to_owned())),
    );
    assert!(checks(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Error,
            "Talaria does not know the home folder, so ~ cannot be used. Give the full path."
                .to_owned()
        )]
    );

    // C6: the same folder, however it is spelled.
    for target in [FOLDER_A, "/work/./proj-a", "/work/proj-a/"] {
        let effects = update(
            &mut state,
            &context,
            &mut ledger,
            CdInput::Command(CdCommand::Move(target.to_owned())),
        );
        assert!(checks(&effects).is_empty());
        assert_eq!(
            notices(&effects),
            vec![(
                NoticeKind::System,
                "This session already works in /work/proj-a.".to_owned()
            )],
            "target {target}"
        );
    }

    // C3, C4, C5: the check answers; the state is idle again afterwards.
    for (check, text) in [
        (FolderCheck::Missing, "No folder at /work/nope."),
        (
            FolderCheck::NotADirectory,
            "/work/file.txt is a file, not a folder.",
        ),
        (FolderCheck::Unreadable, "Talaria cannot read /work/lost."),
    ] {
        let mut state = CdState::new();
        let target = match check {
            FolderCheck::Missing => "/work/nope",
            FolderCheck::NotADirectory => "/work/file.txt",
            _ => "/work/lost",
        };
        let effects = update(
            &mut state,
            &context,
            &mut ledger,
            CdInput::Command(CdCommand::Move(target.to_owned())),
        );
        let pending = checks(&effects);
        assert_eq!(pending.len(), 1);
        let effects = update(
            &mut state,
            &context,
            &mut ledger,
            CdInput::Checked {
                session_id: S1.to_owned(),
                target: pending[0].1.clone(),
                check,
            },
        );
        assert!(sends(&effects).is_empty());
        assert_eq!(
            notices(&effects),
            vec![(NoticeKind::Error, text.to_owned())],
            "check {check:?}"
        );
        // Idle again: the next move is checked, not refused.
        let effects = update(
            &mut state,
            &context,
            &mut ledger,
            CdInput::Command(CdCommand::Move(FOLDER_B.to_owned())),
        );
        assert_eq!(checks(&effects).len(), 1);
    }

    // C18: the fences are still unknown when the check succeeds.
    let mut state = CdState::new();
    let mut bare = MutationLedger::new(LAUNCH);
    let effects = update(
        &mut state,
        &context,
        &mut bare,
        CdInput::Command(CdCommand::Move(FOLDER_B.to_owned())),
    );
    let pending = checks(&effects);
    assert_eq!(pending.len(), 1);
    let effects = update(
        &mut state,
        &context,
        &mut bare,
        CdInput::Checked {
            session_id: S1.to_owned(),
            target: pending[0].1.clone(),
            check: FolderCheck::Directory,
        },
    );
    assert!(sends(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Warning,
            "Talaria does not know the session's revision yet. Try /cd again in a moment."
                .to_owned()
        )]
    );

    // C20 and the request: a directory sends a workspace move at the fences.
    let mut state = CdState::new();
    let mut ledger = MutationLedger::new(LAUNCH);
    ledger.fences_mut(S1).adopt(Some(12), Some(4));
    let effects = move_to_send(&mut state, &mut ledger, &context, FOLDER_B);
    let sent = sends(&effects);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].session_id, S1);
    assert_eq!(sent[0].operation, "workspace");
    assert_eq!(sent[0].expected_revision, 12);
    assert_eq!(sent[0].expected_generation, Some(4));
    assert!(
        sent[0].request_id.starts_with("testlaunch-cd-m"),
        "request id {}",
        sent[0].request_id
    );
    let mut payload = serde_json::Map::new();
    payload.insert("cwd".to_owned(), json!(FOLDER_B));
    assert_eq!(sent[0].payload, payload);
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "Moving this session to /work/proj-b…".to_owned()
        )]
    );
}

#[test]
fn session_folder_from_snapshot() {
    let both = SessionFolder::from_snapshot(Some(FOLDER_A), Some("main"));
    assert_eq!(
        both,
        SessionFolder {
            cwd: Some(FOLDER_A.to_owned()),
            branch: Some("main".to_owned()),
        }
    );
    let bare = SessionFolder::from_snapshot(Some(FOLDER_A), None);
    assert_eq!(
        bare,
        SessionFolder {
            cwd: Some(FOLDER_A.to_owned()),
            branch: None,
        }
    );
    let neither = SessionFolder::from_snapshot(None, None);
    assert_eq!(neither, SessionFolder::default());
    assert_eq!(
        SessionFolder::default(),
        SessionFolder {
            cwd: None,
            branch: None,
        }
    );
}

fn committed(cwd: &str, branch: Option<&str>) -> MutationOutcome {
    MutationOutcome::Committed(json!({
        "session_id": S1,
        "revision": 13,
        "execution_generation": 5,
        "operation": "workspace",
        "cwd": cwd,
        "branch": branch,
        "git_repo_root": cwd,
    }))
}

#[test]
fn move_outcomes_and_folder_updates() {
    let home = PathBuf::from(HOME);
    let known = folder(Some(FOLDER_A), Some("main"));
    let context = ctx(Some(S1), &known, Some(&home));

    // C1: the move commits, with and without a branch.
    for branch in [Some("main"), None] {
        let mut state = CdState::new();
        let mut ledger = MutationLedger::new(LAUNCH);
        ledger.fences_mut(S1).adopt(Some(12), Some(4));
        let effects = move_to_send(&mut state, &mut ledger, &context, FOLDER_B);
        let sent = sends(&effects);
        assert_eq!(sent.len(), 1);
        let effects = update(
            &mut state,
            &context,
            &mut ledger,
            CdInput::Outcome {
                request: sent[0].clone(),
                outcome: committed(FOLDER_B, branch),
            },
        );
        assert!(sends(&effects).is_empty());
        assert_eq!(
            updates(&effects),
            vec![(
                S1.to_owned(),
                FOLDER_B.to_owned(),
                branch.map(str::to_owned)
            )]
        );
        let suffix = branch.map(|name| format!(" ({name})")).unwrap_or_default();
        assert_eq!(
            notices(&effects),
            vec![(
                NoticeKind::System,
                format!(
                    "This session now works in /work/proj-b{suffix}. Its next command runs \
                     there; the conversation is kept."
                )
            )],
            "branch {branch:?}"
        );
    }

    // C13: one fence retry, then the retry is used up.
    let mut state = CdState::new();
    let mut ledger = MutationLedger::new(LAUNCH);
    ledger.fences_mut(S1).adopt(Some(12), Some(4));
    let effects = move_to_send(&mut state, &mut ledger, &context, FOLDER_B);
    let sent = sends(&effects);
    assert_eq!(sent.len(), 1);
    let effects = update(
        &mut state,
        &context,
        &mut ledger,
        CdInput::Outcome {
            request: sent[0].clone(),
            outcome: MutationOutcome::FenceRefused {
                reason: "revision_conflict".to_owned(),
                revision: Some(14),
                execution_generation: Some(5),
            },
        },
    );
    let retried = sends(&effects);
    assert_eq!(retried.len(), 1);
    assert_ne!(retried[0].request_id, sent[0].request_id);
    assert_eq!(retried[0].expected_revision, 14);
    assert_eq!(retried[0].expected_generation, Some(5));
    assert!(notices(&effects).is_empty());
    let effects = update(
        &mut state,
        &context,
        &mut ledger,
        CdInput::Outcome {
            request: retried[0].clone(),
            outcome: MutationOutcome::FenceRefused {
                reason: "revision_conflict".to_owned(),
                revision: Some(15),
                execution_generation: Some(6),
            },
        },
    );
    assert!(sends(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Warning,
            "The session changed while it was being moved. Run /cd again.".to_owned()
        )]
    );

    // The refusals, the lost answer, and the unknown gateway.
    for (outcome, kind, text) in [
        (
            refused("session_busy", 4001),
            NoticeKind::Warning,
            "Hermes is busy with this session (a turn or a context compression is running). Try \
             /cd again when it finishes.",
        ),
        (
            refused("unknown_execution", 4001),
            NoticeKind::Warning,
            "Hermes does not know yet whether the last turn finished. Try /cd again in a moment.",
        ),
        (
            refused("invalid_params", 4001),
            NoticeKind::Error,
            "Hermes could not use /work/proj-b as the session's folder.",
        ),
        (
            refused("runtime_coordination_required", 4001),
            NoticeKind::Error,
            "Hermes cannot move this session to another folder.",
        ),
        (
            MutationOutcome::Unknown,
            NoticeKind::Warning,
            "Talaria could not confirm the move. Run the same /cd again to retry it safely.",
        ),
        (
            refused("unknown_method", -32601),
            NoticeKind::Error,
            "This Hermes gateway cannot move a session to another folder.",
        ),
        (
            refused("permission_denied", 4001),
            NoticeKind::Error,
            "This connection may not move the session.",
        ),
        (
            refused("admission_conflict", 4001),
            NoticeKind::Error,
            "The session was not moved: admission_conflict.",
        ),
        (
            MutationOutcome::Unreadable,
            NoticeKind::Error,
            "The session was not moved: Hermes gave an answer Talaria could not read.",
        ),
        (
            MutationOutcome::NotSent {
                reason: "not connected".to_owned(),
            },
            NoticeKind::Error,
            "The session was not moved: Talaria is not connected to Hermes.",
        ),
        (
            MutationOutcome::Deferred(json!({"status": "deferred"})),
            NoticeKind::Error,
            "The session was not moved: Hermes gave an answer Talaria could not read.",
        ),
    ] {
        let mut state = CdState::new();
        let mut ledger = MutationLedger::new(LAUNCH);
        ledger.fences_mut(S1).adopt(Some(12), Some(4));
        let effects = move_to_send(&mut state, &mut ledger, &context, FOLDER_B);
        let sent = sends(&effects);
        assert_eq!(sent.len(), 1);
        let effects = update(
            &mut state,
            &context,
            &mut ledger,
            CdInput::Outcome {
                request: sent[0].clone(),
                outcome,
            },
        );
        assert!(sends(&effects).is_empty());
        assert!(updates(&effects).is_empty());
        assert_eq!(notices(&effects), vec![(kind, text.to_owned())], "outcome");
    }

    // C8: the check answers after the session changed; nothing is sent.
    let mut state = CdState::new();
    let mut ledger = MutationLedger::new(LAUNCH);
    ledger.fences_mut(S1).adopt(Some(12), Some(4));
    let effects = update(
        &mut state,
        &context,
        &mut ledger,
        CdInput::Command(CdCommand::Move(FOLDER_B.to_owned())),
    );
    let pending = checks(&effects);
    assert_eq!(pending.len(), 1);
    let other = ctx(Some(S2), &known, Some(&home));
    let effects = update(
        &mut state,
        &other,
        &mut ledger,
        CdInput::Checked {
            session_id: S1.to_owned(),
            target: pending[0].1.clone(),
            check: FolderCheck::Directory,
        },
    );
    assert!(sends(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::Warning,
            "The session changed before the move was sent; nothing was moved.".to_owned()
        )]
    );

    // C8b: a move sent from another session finishes; it settles without touching
    // the foreground session and without a fence retry.
    let mut state = CdState::new();
    let mut ledger = MutationLedger::new(LAUNCH);
    ledger.fences_mut(S1).adopt(Some(12), Some(4));
    let effects = move_to_send(&mut state, &mut ledger, &context, FOLDER_B);
    let sent = sends(&effects);
    assert_eq!(sent.len(), 1);
    let effects = update(
        &mut state,
        &other,
        &mut ledger,
        CdInput::Outcome {
            request: sent[0].clone(),
            outcome: committed(FOLDER_B, Some("main")),
        },
    );
    assert!(sends(&effects).is_empty());
    assert!(updates(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        vec![(
            NoticeKind::System,
            "A /cd sent from another session has finished; open that session to see its folder."
                .to_owned()
        )]
    );

    // Exact repeat: after an unknown outcome, the same `/cd` resends the kept
    // request byte for byte, even when the fences moved since.
    let mut state = CdState::new();
    let mut ledger = MutationLedger::new(LAUNCH);
    ledger.fences_mut(S1).adopt(Some(12), Some(4));
    let effects = move_to_send(&mut state, &mut ledger, &context, FOLDER_B);
    let sent = sends(&effects);
    assert_eq!(sent.len(), 1);
    let effects = update(
        &mut state,
        &context,
        &mut ledger,
        CdInput::Outcome {
            request: sent[0].clone(),
            outcome: MutationOutcome::Unknown,
        },
    );
    assert!(sends(&effects).is_empty());
    ledger.fences_mut(S1).adopt(Some(20), Some(9));
    let effects = move_to_send(&mut state, &mut ledger, &context, FOLDER_B);
    let resent = sends(&effects);
    assert_eq!(resent.len(), 1);
    assert_eq!(resent[0], sent[0]);
    assert_eq!(resent[0].expected_revision, 12);
    assert_eq!(resent[0].expected_generation, Some(4));

    // A `session.info` folder replaces the branch only when it differs.
    let mut shown = folder(Some(FOLDER_A), Some("main"));
    shown.apply_info_cwd(FOLDER_A);
    assert_eq!(shown.branch, Some("main".to_owned()));
    shown.apply_info_cwd(FOLDER_B);
    assert_eq!(shown.cwd.as_deref(), Some(FOLDER_B));
    assert_eq!(shown.branch, None);
    // A committed move replaces both; a missing branch clears it.
    shown.apply_move(FOLDER_A, Some("dev"));
    assert_eq!(shown.cwd.as_deref(), Some(FOLDER_A));
    assert_eq!(shown.branch, Some("dev".to_owned()));
    shown.apply_move(FOLDER_B, None);
    assert_eq!(shown.cwd.as_deref(), Some(FOLDER_B));
    assert_eq!(shown.branch, None);
}
