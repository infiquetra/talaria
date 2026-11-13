//! Cleaning untrusted text and wording shell and side outcomes.

use talaria_core::side::{
    ShellOutcome, SideOutcome, clean_output, format_duration, shell_outcome_text,
    shell_refusal_text, side_outcome_text, side_refusal_text,
};

#[test]
fn clean_output_rules() {
    for (dirty, cleaned) in [
        ("\x1b[31mred\x1b[0m", "red"),
        ("a\x1b]0;t\x07b", "ab"),
        ("a\x1b]0;t\x1b\\b", "ab"),
        ("x\x1b[31", "x"),
        ("\x1bé", ""),
        ("a\r\nb", "a\nb"),
        ("abc\rxy", "xy"),
        ("a\n\n\n", "a"),
        ("a\n   \n", "a"),
    ] {
        assert_eq!(clean_output(dirty), cleaned, "{dirty:?}");
    }
}

#[test]
fn shell_refusal_texts_exact() {
    for (reason, detail, want) in [
        (
            "dangerous_command",
            None,
            "Hermes refused to run this: it looks dangerous, and ! runs only safe commands. Ask the agent instead, so the command goes through approval.",
        ),
        (
            "unsupported_terminal_backend",
            Some("docker"),
            "! commands run on the gateway host, but this session's terminal backend is docker, so the command was not run. Ask the agent to run it instead.",
        ),
        (
            "unsupported_terminal_backend",
            None,
            "! commands run on the gateway host, but this session's terminal backend is not local, so the command was not run. Ask the agent to run it instead.",
        ),
        (
            "command_timeout",
            None,
            "The command ran for more than 30 seconds, so Hermes stopped waiting for it. Run long commands in your own terminal.",
        ),
        (
            "cwd_unavailable",
            None,
            "This session's working folder no longer exists, so the command was not run.",
        ),
        (
            "command_failed",
            None,
            "Hermes could not start a shell for this command.",
        ),
        (
            "permission_denied",
            None,
            "Hermes runs ! commands only for local terminal sessions it lets this window control. This session is not one, so the command was not run.",
        ),
        (
            "not_found",
            None,
            "The gateway no longer knows this session, so the command was not run.",
        ),
        (
            "runtime_draining",
            None,
            "The gateway is restarting, so the command was not run. Try again in a moment.",
        ),
        (
            "invalid_params",
            None,
            "Hermes rejected the command as malformed.",
        ),
        (
            "profile_mismatch",
            None,
            "This session belongs to a different profile than this window, so the command was not run.",
        ),
        (
            "unknown_method",
            None,
            "This Hermes gateway does not offer ! commands (shell.exec). Update Hermes to the release Talaria is pinned to.",
        ),
    ] {
        assert_eq!(shell_refusal_text(reason, detail), want, "{reason}");
    }
    let long = "x".repeat(100);
    assert_eq!(
        shell_refusal_text(&long, None),
        format!("Hermes refused the command ({}).", "x".repeat(64))
    );
    assert_eq!(
        shell_refusal_text("storage_unavailable", None),
        "Hermes refused the command (storage_unavailable)."
    );
    let refused = ShellOutcome::Refused {
        reason: "dangerous_command".to_string(),
        detail: None,
        rpc_code: 4001,
    };
    assert_eq!(
        shell_outcome_text(&refused).as_deref(),
        Some(
            "Hermes refused to run this: it looks dangerous, and ! runs only safe commands. Ask the agent instead, so the command goes through approval."
        )
    );
    assert_eq!(
        shell_outcome_text(&ShellOutcome::Unknown).as_deref(),
        Some(
            "The connection dropped before Hermes answered, so the command may or may not have run. Talaria does not retry it."
        )
    );
    assert_eq!(
        shell_outcome_text(&ShellOutcome::Unreadable).as_deref(),
        Some("Hermes ran the command but Talaria could not read its reply.")
    );
    assert_eq!(
        shell_outcome_text(&ShellOutcome::NotSent {
            not_connected: true
        })
        .as_deref(),
        Some("Not connected to Hermes, so the command was not sent.")
    );
    assert_eq!(
        shell_outcome_text(&ShellOutcome::NotSent {
            not_connected: false
        })
        .as_deref(),
        Some("Talaria could not send the command (an internal error), so it was not run.")
    );
    assert_eq!(
        shell_outcome_text(&ShellOutcome::Ok {
            stdout: String::new(),
            stderr: String::new(),
            code: 0,
        }),
        None
    );
}

#[test]
fn side_refusal_texts_exact() {
    for (reason, want) in [
        (
            "permission_denied",
            "Hermes did not accept the side question: this window is not subscribed to the session.",
        ),
        (
            "not_found",
            "The gateway no longer knows this session, so the side question was not asked.",
        ),
        (
            "runtime_draining",
            "The gateway is restarting, so the side question was not asked. Try again in a moment.",
        ),
        (
            "profile_mismatch",
            "This session belongs to a different profile than this window, so the side question was not asked.",
        ),
        (
            "invalid_params",
            "Hermes rejected the side question as malformed.",
        ),
        (
            "unknown_method",
            "This Hermes gateway does not offer /btw (prompt.btw). Update Hermes to the release Talaria is pinned to.",
        ),
    ] {
        assert_eq!(side_refusal_text(reason, None), want, "{reason}");
    }
    let long = "y".repeat(100);
    assert_eq!(
        side_refusal_text(&long, None),
        format!("Hermes refused the side question ({}).", "y".repeat(64))
    );
    assert_eq!(
        side_outcome_text(&SideOutcome::Failed {
            text: "boom".to_string()
        })
        .as_deref(),
        Some("The side question failed: boom")
    );
    assert_eq!(
        side_outcome_text(&SideOutcome::Refused {
            reason: "not_found".to_string(),
            detail: None,
        })
        .as_deref(),
        Some("The gateway no longer knows this session, so the side question was not asked.")
    );
    assert_eq!(
        side_outcome_text(&SideOutcome::NotSent {
            not_connected: true
        })
        .as_deref(),
        Some("Not connected to Hermes, so the side question was not asked.")
    );
    assert_eq!(
        side_outcome_text(&SideOutcome::NotSent {
            not_connected: false
        })
        .as_deref(),
        Some("Talaria could not send the side question (an internal error), so it was not asked.")
    );
    assert_eq!(
        side_outcome_text(&SideOutcome::Answered {
            text: "hi".to_string()
        }),
        None
    );
}

#[test]
fn duration_format() {
    for (ms, want) in [
        (0, "0.0s"),
        (49, "0.0s"),
        (50, "0.1s"),
        (9_949, "9.9s"),
        (9_950, "10s"),
        (12_400, "12s"),
        (12_500, "13s"),
    ] {
        assert_eq!(format_duration(ms), want, "{ms} ms");
    }
}
