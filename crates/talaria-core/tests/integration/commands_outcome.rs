//! What a slash reply means. Issue #223.

use serde_json::{Value, json};
use talaria_core::commands::{
    CatalogResult, CommandCatalog, CommandOutcome, ErrorView, SlashLine, describe_error,
    interpret_result,
};

fn slash(name: &str, arg: &str) -> SlashLine {
    SlashLine {
        name: name.into(),
        arg: arg.into(),
    }
}

fn message_of(outcome: &CommandOutcome) -> &str {
    match outcome {
        CommandOutcome::Output { text, .. }
        | CommandOutcome::Failed { message: text }
        | CommandOutcome::OutcomeUnknown { message: text }
        | CommandOutcome::Refused { message: text, .. } => text,
        CommandOutcome::Submit { text, .. } => text,
        CommandOutcome::Prefill { text, .. } => text,
        CommandOutcome::Rerun { line } => line,
        CommandOutcome::Nothing => "",
    }
}

#[test]
fn exec_output_and_empty_output() {
    let line = slash("version", "");
    let empty = interpret_result(&line, "/version", &json!({"type": "exec", "output": ""}));
    assert_eq!(
        empty,
        CommandOutcome::Output {
            line: "/version".into(),
            text: "(no output)".into(),
        }
    );
    let missing = interpret_result(&line, "/version", &json!({"type": "exec"}));
    assert_eq!(message_of(&missing), "(no output)");

    let legacy = interpret_result(
        &line,
        "/version",
        &json!({"output": "Hermes Agent v0.22.4", "warning": "watch the path"}),
    );
    let text = message_of(&legacy);
    assert!(text.starts_with("warning: watch the path\n"), "{text}");
    assert!(text.contains("Hermes Agent v0.22.4"));

    let mut output = "a".repeat(262_144);
    output.push_str(&"你".repeat(10));
    let cut = interpret_result(
        &line,
        "/version",
        &json!({"type": "exec", "output": output}),
    );
    let text = message_of(&cut);
    assert!(text.ends_with("(output cut: 30 more bytes)"), "{text}");
    assert!(!text.contains('你'));
}

#[test]
fn send_and_skill_submit_message_and_show_display() {
    let line = slash("plan", "write a haiku");
    let body = "[/plan — plan mode]\nTask to plan:\nwrite a haiku";
    let sent = interpret_result(
        &line,
        "/plan write a haiku",
        &json!({
            "type": "send",
            "message": body,
            "display": "/plan write a haiku",
            "notice": "Planning: write a haiku"
        }),
    );
    assert_eq!(
        sent,
        CommandOutcome::Submit {
            text: body.into(),
            display: "/plan write a haiku".into(),
            notice: Some("Planning: write a haiku".into()),
        }
    );

    let blank = interpret_result(
        &line,
        "/plan write a haiku",
        &json!({"type": "send", "message": "scaffold", "display": "  "}),
    );
    match blank {
        CommandOutcome::Submit { display, .. } => {
            assert_eq!(display, "/plan write a haiku");
        }
        other => panic!("{other:?}"),
    }

    let skill_body = "Fixture skill body. [Skill directory: /work/proj/skills/hidden]";
    let skill = interpret_result(
        &slash("fixture-skill-1", "tidy"),
        "/fixture-skill-1 tidy",
        &json!({
            "type": "skill",
            "name": "fixture-skill-1",
            "message": format!("[IMPORTANT: The user has invoked fixture-skill-1]\n{skill_body}"),
            "display": "/fixture-skill-1 tidy"
        }),
    );
    match &skill {
        CommandOutcome::Submit {
            text,
            display,
            notice,
        } => {
            assert_eq!(display, "/fixture-skill-1 tidy");
            assert!(!display.contains(skill_body));
            if let Some(text) = notice {
                assert!(!text.contains(skill_body));
            }
            assert!(text.contains("fixture-skill-1"));
        }
        other => panic!("{other:?}"),
    }

    let empty_send = interpret_result(
        &line,
        "/plan write a haiku",
        &json!({"type": "send", "message": "", "display": "/plan write a haiku"}),
    );
    assert_eq!(message_of(&empty_send), "/plan returned an empty message.");
    let empty_skill = interpret_result(
        &slash("fixture-skill-1", "tidy"),
        "/fixture-skill-1 tidy",
        &json!({"type": "skill", "name": "fixture-skill-1", "message": "", "display": "/fixture-skill-1 tidy"}),
    );
    assert_eq!(
        message_of(&empty_skill),
        "/fixture-skill-1: the skill sent no instructions."
    );
}

#[test]
fn prefill_loads_composer_without_sending() {
    let outcome = interpret_result(
        &slash("goal", ""),
        "/goal",
        &json!({"type": "prefill", "message": "line one", "notice": "Loaded."}),
    );
    assert_eq!(
        outcome,
        CommandOutcome::Prefill {
            text: "line one".into(),
            notice: Some("Loaded.".into()),
        }
    );
}

#[test]
fn alias_reruns_once_then_stops() {
    let outcome = interpret_result(
        &slash("reset", ""),
        "/reset",
        &json!({"type": "alias", "target": "new"}),
    );
    assert_eq!(
        outcome,
        CommandOutcome::Rerun {
            line: "/new".into()
        },
        "a second alias in the same execute is: \
         /reset points to another alias; Talaria stops after one."
    );
}

#[test]
fn unknown_type_or_non_object_is_plain_error() {
    let line = slash("version", "");
    let wanted = "Hermes answered /version in a form this version of Talaria does not understand.";
    for result in [
        json!({"type": "mystery"}),
        json!(1),
        json!("text"),
        Value::Null,
    ] {
        let outcome = interpret_result(&line, "/version", &result);
        assert_eq!(
            outcome,
            CommandOutcome::Failed {
                message: wanted.into()
            }
        );
    }
}

fn rpc(code: i64, reason: &str, field: Option<&str>, detail: Option<&str>) -> ErrorView {
    ErrorView::Rpc {
        code,
        message: reason.into(),
        reason: Some(reason.into()),
        field: field.map(str::to_owned),
        detail: detail.map(str::to_owned),
    }
}

fn refused(outcome: CommandOutcome) -> (String, bool) {
    match outcome {
        CommandOutcome::Refused { message, dim, .. } => (message, dim),
        other => panic!("{other:?}"),
    }
}

fn held_catalog() -> CommandCatalog {
    let result: CatalogResult = serde_json::from_value(json!({
        "pairs": [["/version", "Show the Hermes version"], ["/usage", "Show token usage"]],
        "categories": [{"name": "Info", "pairs": [
            ["/version", "Show the Hermes version"],
            ["/usage", "Show token usage"]
        ]}],
        "commands": {"/version": {}, "/usage": {"argument_mode": "options"}}
    }))
    .expect("catalogue");
    CommandCatalog::from_result(&result)
}

#[test]
fn refusal_wording_table() {
    unsupported_without_a_held_catalogue();
    unsupported_with_a_held_catalogue();
    other_refusal_reasons();
}

fn unsupported_without_a_held_catalogue() {
    let line = slash("version", "");
    let detail = format!("start\n{}", "x".repeat(200));
    let flat: String = detail.replace('\n', " ").chars().take(160).collect();
    let outcome = describe_error(
        &line,
        &rpc(4001, "unsupported_command", None, Some(&detail)),
        None,
    );
    let (message, dim) = refused(outcome);
    assert!(!dim);
    assert_eq!(
        message,
        format!("Hermes refused /version: it is not a command this gateway runs.\n{flat}")
    );
}

fn unsupported_with_a_held_catalogue() {
    let line = slash("version", "");
    let catalog = held_catalog();
    let unknown = describe_error(
        &slash("nope", ""),
        &rpc(4001, "unsupported_command", None, None),
        Some(&catalog),
    );
    let (message, dim) = refused(unknown);
    assert!(!dim);
    assert_eq!(
        message,
        "Hermes does not know /nope. Type / to see the commands you can use."
    );

    let bare = describe_error(
        &line,
        &rpc(
            4001,
            "unsupported_command",
            None,
            Some("try the classic CLI"),
        ),
        Some(&catalog),
    );
    let (message, dim) = refused(bare);
    assert!(dim);
    assert_eq!(
        message,
        "/version is not available on the shared Hermes gateway.\ntry the classic CLI"
    );

    let argued = describe_error(
        &slash("usage", "reset"),
        &rpc(4001, "unsupported_command", None, Some("reads only")),
        Some(&catalog),
    );
    let (message, dim) = refused(argued);
    assert!(!dim);
    assert_eq!(
        message,
        "/usage reset is not available on the shared Hermes gateway.\nreads only"
    );
}

fn other_refusal_reasons() {
    let line = slash("version", "");
    let catalog = held_catalog();
    let cases = [
        (
            rpc(4001, "permission_denied", None, None),
            "/version was refused: this connection is not allowed to run it.",
        ),
        (
            rpc(4001, "not_found", None, None),
            "/version could not run: Hermes no longer has this session. Quit and start a new one with talaria, or open another with talaria --resume <id>.",
        ),
        (
            rpc(4001, "profile_mismatch", None, None),
            "/version was refused: the session belongs to a different profile.",
        ),
        (
            rpc(4001, "session_superseded", None, None),
            "/version did not run: this conversation has moved on to a newer session and is read-only.",
        ),
        (
            rpc(
                4001,
                "invalid_params",
                Some("arg"),
                Some("No closing quotation"),
            ),
            "/version could not read its arguments: No closing quotation.",
        ),
        (
            rpc(4001, "invalid_params", Some("command"), None),
            "Hermes did not accept /version as sent. If it is one of your quick commands, check its alias target; otherwise this is a Talaria bug, please report it.",
        ),
        (
            rpc(4001, "command_unavailable", None, None),
            "/version could not be loaded. The skill may be empty or missing.",
        ),
        (
            rpc(4001, "cwd_unavailable", None, None),
            "/version could not run: the session's working folder no longer exists.",
        ),
        (
            rpc(4001, "runtime_draining", None, None),
            "Hermes is restarting, so /version did not run. Try again in a moment.",
        ),
        (
            ErrorView::Rpc {
                code: 5001,
                message: "storage_unavailable".into(),
                reason: Some("storage_unavailable".into()),
                field: None,
                detail: None,
            },
            "Hermes could not read its session store, so /version did not run.",
        ),
        (
            ErrorView::Rpc {
                code: -32603,
                message: "boom".into(),
                reason: None,
                field: None,
                detail: None,
            },
            "/version failed: boom",
        ),
        (
            ErrorView::Rpc {
                code: -32603,
                message: String::new(),
                reason: None,
                field: None,
                detail: None,
            },
            "/version failed: code -32603",
        ),
        (
            ErrorView::NotConnected,
            "Talaria is not connected to Hermes, so /version did not run.",
        ),
        (
            ErrorView::BadResult,
            "Hermes answered /version in a form this version of Talaria does not understand.",
        ),
    ];
    for (error, wanted) in cases {
        let outcome = describe_error(&line, &error, Some(&catalog));
        assert_eq!(message_of(&outcome), wanted);
    }
}

#[test]
fn outcome_unknown_is_never_retried() {
    let line = slash("version", "");
    let lost = "The connection to Hermes was lost before /version answered. It may or may not have run; check before running it again.";
    for error in [ErrorView::TimedOut, ErrorView::ConnectionLost] {
        let outcome = describe_error(&line, &error, None);
        assert_eq!(
            outcome,
            CommandOutcome::OutcomeUnknown {
                message: lost.into()
            }
        );
    }
    let bug = describe_error(&line, &ErrorView::LocalBug, None);
    assert_eq!(
        bug,
        CommandOutcome::Failed {
            message: "Talaria could not send /version. This is a Talaria bug; please report it."
                .into(),
        }
    );
}
