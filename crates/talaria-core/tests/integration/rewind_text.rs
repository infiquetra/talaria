//! Exact rewind texts. Issue 233. Checkpoint rewind and diff.

use talaria_core::rewind::{
    CallFailure, CallKind, DiffDoc, NoMatch, RestoreOutcome, SessionDiffOutcome,
    checkpoint_diff_doc, failure_text, mutate_refusal_text, no_match_text, python_line_count,
    read_refusal_text, relative_age, restore_refusal_text, restore_summary, session_diff_doc,
};

fn refused(reason: &str, field: Option<&str>, detail: Option<&str>) -> CallFailure {
    CallFailure::Refused {
        reason: reason.to_string(),
        field: field.map(str::to_string),
        detail: detail.map(str::to_string),
        revision: None,
        execution_generation: None,
    }
}

#[test]
fn mutate_refusal_texts_exact() {
    let long = "r".repeat(100);
    let cases = [
        (
            "revision_conflict",
            None,
            None,
            "The session changed while you were choosing (a new turn, or another window). Nothing was rewound. Open /rewind again.",
        ),
        (
            "stale_generation",
            None,
            None,
            "The session changed while you were choosing (a new turn, or another window). Nothing was rewound. Open /rewind again.",
        ),
        (
            "invalid_params",
            Some("target_message_id"),
            Some("target user message is no longer in session history"),
            "That prompt is no longer in the conversation (it may have been rewound in another window). Open /rewind again.",
        ),
        (
            "invalid_params",
            Some("target_message_id"),
            Some("compacted: target user message was summarized when the session was compressed"),
            "That prompt was summarized when the conversation was compacted, so Hermes cannot rewind to it. Choose a later prompt.",
        ),
        (
            "session_busy",
            None,
            None,
            "Hermes will not rewind while a reply is running or prompts are queued. Stop the reply or clear the queue, then try again.",
        ),
        (
            "session_busy",
            Some("compression"),
            None,
            "Hermes is compacting this conversation. Wait for it to finish, then rewind.",
        ),
        (
            "unknown_execution",
            None,
            None,
            "Hermes does not know yet how the last turn ended, so it will not rewind. Wait a moment and try again.",
        ),
        (
            "runtime_coordination_required",
            None,
            None,
            "This Hermes gateway cannot rewind this session (runtime_coordination_required).",
        ),
        (
            "runtime_draining",
            None,
            None,
            "Hermes is restarting and refuses changes for now. Try again when it is back.",
        ),
        (
            "not_found",
            None,
            None,
            "This session no longer exists on the gateway.",
        ),
        (
            "permission_denied",
            None,
            None,
            "Hermes refused: this connection may not change the session (permission_denied).",
        ),
        (
            "unknown_method",
            None,
            None,
            "This Hermes gateway does not offer session.mutate. Talaria needs the Hermes release it was built for, or later.",
        ),
        (
            "storage_unavailable",
            None,
            None,
            "Hermes could not use its session store (storage_unavailable). Open /rewind to check what happened.",
        ),
    ];
    for (reason, field, detail, expected) in cases {
        assert_eq!(mutate_refusal_text(reason, field, detail), expected);
    }
    let cut: String = long.chars().take(64).collect();
    let other = mutate_refusal_text(&long, Some("other"), Some("more"));
    assert!(other.starts_with(&format!("Hermes refused the rewind ({cut}).")));
    assert!(other.ends_with("more"));
}

#[test]
fn rollback_texts_exact() {
    assert_eq!(
        read_refusal_text(CallKind::CheckpointDiff, "invalid_checkpoint", Some("99")),
        "There is no checkpoint 99. /rollback lists them; number 1 is the newest."
    );
    let off = session_diff_doc(
        &SessionDiffOutcome {
            enabled: false,
            success: false,
            stat: String::new(),
            diff: String::new(),
            diff_lines: 0,
            empty: false,
            error: None,
        },
        false,
    );
    assert!(matches!(off, DiffDoc::Off));
    assert_eq!(
        restore_refusal_text(true, "session_busy", None),
        "Hermes reported session_busy after the restore started. It may have restored the files without rewinding the conversation. Run /diff to check the files and /rewind to check the conversation."
    );
    assert_eq!(
        restore_refusal_text(true, "not_found", None),
        "This session no longer exists on the gateway."
    );
    assert_eq!(
        restore_refusal_text(false, "session_busy", None),
        "Hermes refused to restore files (session_busy)."
    );
    assert!(
        failure_text(&CallFailure::NotSent("down".into()), CallKind::ReadSession)
            .contains("Not connected")
    );
    assert_eq!(
        failure_text(&CallFailure::Unreadable, CallKind::ReadSession),
        "Hermes's reply could not be read; nothing was changed."
    );
    assert_eq!(
        failure_text(&CallFailure::Unknown, CallKind::ReadSession),
        "Hermes did not answer in time; nothing was changed. Try again."
    );
    assert_eq!(
        failure_text(
            &refused("permission_denied", None, None),
            CallKind::ReadSession
        ),
        "Hermes refused: this connection may not read the session (permission_denied)."
    );
    assert_eq!(
        no_match_text(&NoMatch::Off),
        "Restore files: checkpoints are off"
    );
    assert_eq!(
        no_match_text(&NoMatch::Unknown),
        "Restore files: checkpoints could not be listed"
    );
    assert_eq!(
        no_match_text(&NoMatch::NoTime),
        "Restore files: Hermes stored no time for this prompt"
    );
    assert_eq!(
        no_match_text(&NoMatch::NoChanges),
        "Restore files: no file changes recorded since this prompt"
    );
    assert_eq!(
        no_match_text(&NoMatch::NoCheckpointForEdits),
        "Restore files: Hermes recorded no checkpoint before this prompt's edits"
    );
    let empty = checkpoint_diff_doc("abcdef01", "", "");
    match empty {
        DiffDoc::Empty(text) => assert!(text.contains("Either the files match it")),
        other => panic!("empty diff is C5, got {other:?}"),
    }
}

#[test]
fn restore_summary_lines() {
    let one = RestoreOutcome {
        success: true,
        restored_to: Some("abcdef01".into()),
        reason: Some("before write_file".into()),
        file: Some("notes.txt".into()),
        restored_files: None,
        skipped_user_edits: None,
        skipped_oversize: None,
        failed_deletes: None,
        history_removed: None,
        error: None,
    };
    let lines = restore_summary(&one, "1");
    assert_eq!(
        lines,
        vec!["Restored notes.txt from checkpoint abcdef01.".to_string()]
    );
    let many = RestoreOutcome {
        success: true,
        restored_to: Some("abcdef01".into()),
        reason: Some("nothing to restore (all changed files were user-edited)".into()),
        file: None,
        restored_files: Some(vec![]),
        skipped_user_edits: Some(vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
            "f".into(),
            "g".into(),
        ]),
        skipped_oversize: Some(vec!["big".into()]),
        failed_deletes: Some(vec!["old".into()]),
        history_removed: Some(3),
        error: None,
    };
    let lines = restore_summary(&many, "2");
    let joined = lines.join("\n");
    assert!(joined.contains("and 2 more"));
    assert!(joined.contains("Kept 7 files you edited by hand: a, b, c, d, e"));
    assert!(joined.contains("Kept 1 file larger than the checkpoint size limit: big"));
    assert!(joined.contains("Could not delete 1 file: old"));
    assert!(joined.contains("Hermes also removed the last exchange (3 messages)"));
    assert!(!joined.contains("directory"));
    // The only slash is the /rollback command the hand-edited line names; no path appears.
    assert!(!joined.replace("/rollback", "").contains('/'));
}

#[test]
fn diff_cut_notices() {
    let received = "line\n".repeat(300);
    let doc = session_diff_doc(
        &SessionDiffOutcome {
            enabled: true,
            success: true,
            stat: " notes.txt | 1 +".into(),
            diff: received,
            diff_lines: 900,
            empty: false,
            error: None,
        },
        false,
    );
    match doc {
        DiffDoc::Text {
            cut_note, subtitle, ..
        } => {
            assert_eq!(
                cut_note.as_deref(),
                Some("Hermes cut this diff to fit one message: showing 300 of 900 lines.")
            );
            assert_eq!(
                subtitle.as_deref(),
                Some(
                    "Includes every change in this folder since then: the agent's, yours and other sessions'."
                )
            );
        }
        other => panic!("session diff text, got {other:?}"),
    }
    let equal = "line\n".repeat(4);
    let doc = session_diff_doc(
        &SessionDiffOutcome {
            enabled: true,
            success: true,
            stat: String::new(),
            diff: equal,
            diff_lines: 4,
            empty: false,
            error: None,
        },
        false,
    );
    match doc {
        DiffDoc::Text { cut_note: None, .. } => {}
        other => panic!("equal counts have no cut note, got {other:?}"),
    }
    let odd = "a\u{000c}b\r";
    assert_eq!(python_line_count(odd), 2);
    let doc = session_diff_doc(
        &SessionDiffOutcome {
            enabled: true,
            success: true,
            stat: String::new(),
            diff: odd.into(),
            diff_lines: 2,
            empty: false,
            error: None,
        },
        false,
    );
    match doc {
        DiffDoc::Text { cut_note: None, .. } => {}
        other => panic!("python line count matches, got {other:?}"),
    }
    let almost = "é".repeat(3999);
    let exact = "é".repeat(4000);
    match checkpoint_diff_doc("abcdef01", "stat", &almost) {
        DiffDoc::Text { cut_note: None, .. } => {}
        other => panic!("3999 code points have no note, got {other:?}"),
    }
    match checkpoint_diff_doc("abcdef01", "stat", &exact) {
        DiffDoc::Text {
            cut_note: Some(note),
            ..
        } => {
            assert!(note.contains("4,000"));
        }
        other => panic!("4000 code points carry the cut note, got {other:?}"),
    }
}

#[test]
fn relative_age_and_clock_text() {
    let now_ms = 1_700_000_000_000_u64;
    let now_s = now_ms as f64 / 1000.0;
    assert_eq!(relative_age(now_s, now_ms), "just now");
    assert_eq!(relative_age(now_s - 59.0, now_ms), "just now");
    assert_eq!(relative_age(now_s - 60.0, now_ms), "1m ago");
    assert_eq!(relative_age(now_s - 3_599.0, now_ms), "59m ago");
    assert_eq!(relative_age(now_s - 3_600.0, now_ms), "1h ago");
    assert_eq!(relative_age(now_s - 86_399.0, now_ms), "23h ago");
    assert_eq!(relative_age(now_s - 86_400.0, now_ms), "1d ago");
    assert_eq!(relative_age(now_s + 10.0, now_ms), "just now");
}
