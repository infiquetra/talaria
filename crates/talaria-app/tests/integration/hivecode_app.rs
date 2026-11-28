use crate::mock_gateway;
use serde_json::{Value, json};
use talaria_app::hivecode::{Action, Foreground, HivecodeService};
use talaria_core::session::ids::SessionId;
use talaria_core::{
    commands::{CatalogResult, CommandCatalog, ErrorView},
    hivecode::{HivecodeCommand, SkillEpoch, follow_up_text, notice},
};
fn session() -> SessionId {
    SessionId::new("fixture-session")
}
fn catalog() -> Value {
    json!({"skills":{"/talaria-workflows":{"usage":0,"origin":"local"}},"pairs":[]})
}
fn parsed(value: Value) -> CommandCatalog {
    let result: CatalogResult = serde_json::from_value(value).unwrap();
    CommandCatalog::from_result(&result)
}
fn foreground(id: &SessionId) -> Option<Foreground<'_>> {
    Some(Foreground {
        session: id,
        profile: "fixture",
    })
}
fn rpc(reason: &str) -> ErrorView {
    ErrorView::Rpc {
        code: -32602,
        message: String::new(),
        reason: Some(reason.into()),
        field: None,
        detail: None,
    }
}
fn on() -> HivecodeService {
    let mut service = HivecodeService::new(false);
    let id = session();
    assert_eq!(
        service.handle_local_command("on", foreground(&id)),
        vec![Action::FetchCatalog {
            session: id.clone(),
            profile: "fixture".into(),
            command: HivecodeCommand::On
        }]
    );
    let _ = service.on_catalog_reply(&id, "fixture", HivecodeCommand::On, Ok(parsed(catalog())));
    assert!(service.status_input(Some(&id)));
    service
}
#[test]
fn on_checks_a_fresh_catalogue() {
    let id = session();
    let mut s = HivecodeService::new(false);
    assert_eq!(
        s.handle_local_command("on", foreground(&id)),
        vec![Action::FetchCatalog {
            session: id.clone(),
            profile: "fixture".into(),
            command: HivecodeCommand::On
        }]
    );
    let actions = s.on_catalog_reply(&id, "fixture", HivecodeCommand::On, Ok(parsed(catalog())));
    assert!(s.status_input(Some(&id)));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::Notice(v) if v.contains("Hivecode on")))
    );
    for value in [
        json!({"skills":{}}),
        json!({"skills":{"/talaria-workflows":{"origin":"local"}},"pairs":[["/talaria-workflows","custom"]],"categories":[{"name":"User commands","pairs":[["/talaria-workflows","custom"]]}]}),
    ] {
        let mut service = HivecodeService::new(false);
        let _ = service.handle_local_command("on", foreground(&id));
        let replies =
            service.on_catalog_reply(&id, "fixture", HivecodeCommand::On, Ok(parsed(value)));
        assert!(!service.status_input(Some(&id)));
        assert!(replies.iter().any(|a| matches!(a, Action::Notice(_))));
    }
    let mut service = HivecodeService::new(false);
    let _ = service.handle_local_command("on", foreground(&id));
    let replies =
        service.on_catalog_reply(&id, "fixture", HivecodeCommand::On, Err("offline".into()));
    assert!(!service.status_input(Some(&id)));
    assert!(
        replies
            .iter()
            .any(|a| matches!(a,Action::Notice(v) if v.contains("offline")))
    );
}
#[tokio::test]
async fn routed_submit_frames() {
    let frames: Vec<Value> = include_str!("../../../../fixtures/frames/p5-09-hivecode-skill.jsonl")
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let first = &frames[1]["frame"];
    let reply = &frames[2]["frame"]["result"];
    let submit = &frames[3]["frame"];
    assert_eq!(first["method"], "slash.exec");
    assert_eq!(
        first["params"]["command"],
        "talaria-workflows What is 2 + 2? Answer in one word."
    );
    assert_eq!(submit["params"]["text"], reply["message"]);
    assert_eq!(
        reply["display"],
        "/talaria-workflows What is 2 + 2? Answer in one word."
    );
    let id = session();
    let service = on();
    let actions = service
        .intercept_submit(&id, "What is 2 + 2? Answer in one word.")
        .unwrap();
    let command_line = actions
        .iter()
        .find_map(|action| match action {
            Action::RunCommand {
                session,
                original_text,
                command_line,
                ..
            } => {
                assert_eq!(session, &id);
                assert_eq!(original_text, "What is 2 + 2? Answer in one word.");
                Some(command_line.as_str())
            }
            _ => None,
        })
        .expect("routed command");
    assert_eq!(
        command_line.strip_prefix('/'),
        first["params"]["command"].as_str()
    );
    let (gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
    let session = mock_gateway::SESSION_ID;
    let sent =
        serde_json::json!({"session_id":session,"command":command_line.strip_prefix('/').unwrap()});
    let result = client
        .call_raw(
            "slash.exec",
            sent.clone(),
            hermes_client::CallOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(result, *reply);
    let live_id = SessionId::new(session);
    let actions =
        service.on_command_outcome(&live_id, "What is 2 + 2? Answer in one word.", Ok(result));
    let (text, display, submission_id) = actions
        .iter()
        .find_map(|a| match a {
            Action::SubmitCommand {
                text,
                display,
                submission_id,
            } => Some((text, display, submission_id)),
            _ => None,
        })
        .expect("skill submission");
    assert_eq!(text, reply["message"].as_str().unwrap());
    assert_eq!(
        display, "What is 2 + 2? Answer in one word.",
        "the cell shows what was typed, never the skill command"
    );
    assert!(!submission_id.is_empty());
    let payload = talaria_app::sessions::submit_params(session, text, submission_id);
    let _ = client
        .call_raw(
            "prompt.submit",
            payload.clone(),
            hermes_client::CallOptions::default(),
        )
        .await
        .unwrap();
    let calls = gateway.requests("slash.exec");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params, sent);
    let calls = gateway.requests("prompt.submit");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params, payload);
    let keys: std::collections::BTreeSet<_> = payload
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        std::collections::BTreeSet::from(["session_id", "submission_id", "text"])
    );
    assert_eq!(payload["text"], submit["params"]["text"]);
    let mut state = talaria_core::session::SessionState::empty(live_id);
    let _ = talaria_core::session::update(
        &mut state,
        talaria_core::session::Event::SubmitCommand {
            text: text.clone(),
            display: display.clone(),
            submission_id: submission_id.clone(),
        },
    );
    assert!(state.cells().iter().any(
        |cell| matches!(cell,talaria_core::session::cell::Cell::User(user) if user.text == *display)
    ));
}
#[test]
fn multiline_message_and_typed_display() {
    let id = session();
    let service = on();
    let original = "a  b\tc\nd";
    let actions = service.intercept_submit(&id, original).unwrap();
    assert!(actions.iter().any(|a| matches!(a,Action::RunCommand{command_line,..} if command_line=="/talaria-workflows a  b\tc\nd")));
    let reply = json!({"type":"skill","name":"talaria-workflows","message":"a  b\tc\nd","display":"/talaria-workflows a b c d"});
    let actions = service.on_command_outcome(&id, original, Ok(reply));
    assert!(actions.iter().any(|a| matches!(a,Action::SubmitCommand{text,display,..} if text==original && display==original)));
}
#[test]
fn failures_restore_composer_and_never_fall_back() {
    let id = session();
    let original = "fix this";
    for failure in [
        Ok(json!({"type":"exec","output":"x"})),
        Ok(json!({"type":"send","message":"x"})),
        Ok(json!({"type":"skill","name":"other","message":"x"})),
        Err(rpc("unsupported_command")),
        Err(rpc("command_unavailable")),
        Err(ErrorView::TimedOut),
    ] {
        let service = on();
        let actions = service.on_command_outcome(&id, original, failure);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a,Action::RestoreComposer(text) if text==original))
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::SubmitCommand { .. }))
        );
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(_))));
    }
}
#[test]
fn a_typed_draft_survives_a_failed_ultracode_command() {
    use hermes_client::replay::Cursor;
    use talaria_app::app::{AppEffect, AppMessage, AppState, update};
    use talaria_app::sessions::{OpenedSession, SessionInfoSummary};
    use talaria_core::session::SessionState;

    let mut state = AppState::new("fixture");
    update(
        &mut state,
        AppMessage::Opened {
            opened: OpenedSession {
                state: SessionState::empty(SessionId::new("fixture-session")),
                session_id: "fixture-session".into(),
                stored_session_id: "fixture-session".into(),
                subscription_id: "subscription-fixture".into(),
                info: SessionInfoSummary {
                    model: None,
                    provider: None,
                    cwd: None,
                    branch: None,
                },
                title: None,
                cursor: Cursor {
                    replay_epoch: "epoch-fixture".into(),
                    last_sequence: 0,
                },
                revision: 0,
                execution_generation: 0,
                launch_notices: vec![],
                effort: None,
            },
            notice: None,
        },
    );
    let failed = || AppMessage::HivecodeDone {
        session: SessionId::new("fixture-session"),
        original_text: "fix this".into(),
        restore_text: None,
        outcome: Err(ErrorView::TimedOut),
    };

    state.draft = "newer line".into();
    let effects = update(&mut state, failed());
    assert_eq!(
        state.draft, "newer line",
        "a draft typed while the command ran is kept"
    );
    assert!(
        effects.iter().any(|effect| matches!(
            effect,
            AppEffect::CommitLines(lines) if lines.iter().any(|line| line.contains(
                "The connection to Hermes was lost before the workflow request answered"
            ))
        )),
        "the failure notice is still shown: {effects:?}"
    );

    state.draft = String::new();
    update(&mut state, failed());
    assert_eq!(
        state.draft, "fix this",
        "an empty composer gets the original prompt back"
    );
}
#[test]
fn slash_shell_and_keyword_paths() {
    let id = session();
    let service = on();
    assert!(service.intercept_submit(&id, "/help").is_none());
    assert!(service.intercept_submit(&id, "!ls").is_none());
    let off = HivecodeService::new(false);
    let actions = off.intercept_submit(&id, "please hivecode this").unwrap();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::RunCommand { .. }))
    );
    assert!(!off.status_input(Some(&id)));
}

#[test]
fn local_missing_asks_once() {
    let id = session();
    let missing = json!({"skills":{}});

    let mut service = HivecodeService::new(false);
    let _ = service.handle_local_command("on", foreground(&id));
    let replies = service.on_catalog_reply(
        &id,
        "fixture",
        HivecodeCommand::On,
        Ok(parsed(missing.clone())),
    );
    let offer = notice::offer("fixture");
    assert!(
        replies.iter().any(
            |a| matches!(a, Action::Notice(v) if v == &offer && !v.contains("Install it with:"))
        )
    );
    assert!(
        !replies
            .iter()
            .any(|a| matches!(a, Action::InstallSkill { .. }))
    );
    assert!(service.answer_install(&id, "y").is_none());

    service.record_shown(&id);
    let second = service.on_catalog_reply(
        &id,
        "fixture",
        HivecodeCommand::On,
        Ok(parsed(missing.clone())),
    );
    let expected_missing = notice::missing("fixture");
    assert!(second.iter().any(
        |a| matches!(a, Action::Notice(v) if v == &expected_missing && !v.contains("now? [y/N]"))
    ));

    let collision = json!({
        "skills": {"/talaria-workflows": {"origin": "local"}},
        "pairs": [["/talaria-workflows", "custom"]],
        "categories": [{"name": "User commands", "pairs": [["/talaria-workflows", "custom"]]}]
    });
    let mut collision_service = HivecodeService::new(false);
    let _ = collision_service.handle_local_command("on", foreground(&id));
    let collision_replies = collision_service.on_catalog_reply(
        &id,
        "fixture",
        HivecodeCommand::On,
        Ok(parsed(collision)),
    );
    assert!(
        collision_replies
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v.contains("Rename that command") && !v.contains("now? [y/N]")))
    );

    let mut offline_service = HivecodeService::new(false);
    let _ = offline_service.handle_local_command("on", foreground(&id));
    let offline_replies = offline_service.on_catalog_reply(
        &id,
        "fixture",
        HivecodeCommand::On,
        Err("offline".into()),
    );
    assert!(offline_replies.iter().any(
        |a| matches!(a, Action::Notice(v) if v.contains("offline") && !v.contains("now? [y/N]"))
    ));

    let mut status_service = HivecodeService::new(false);
    let _ = status_service.handle_local_command("status", foreground(&id));
    let status_replies = status_service.on_catalog_reply(
        &id,
        "fixture",
        HivecodeCommand::Status,
        Ok(parsed(missing.clone())),
    );
    assert!(
        status_replies
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v.contains("not found in profile fixture") && !v.contains("now? [y/N]")))
    );

    let mut outcome_service = HivecodeService::new(false);
    let _ = outcome_service.handle_local_command("on", foreground(&id));
    let first_outcome =
        outcome_service.on_command_outcome(&id, "hello", Err(rpc("unsupported_command")));
    assert!(
        first_outcome
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v == &offer))
    );
    assert!(
        first_outcome
            .iter()
            .any(|a| matches!(a, Action::RestoreComposer(text) if text == "hello"))
    );

    outcome_service.record_shown(&id);
    let second_outcome =
        outcome_service.on_command_outcome(&id, "hello", Err(rpc("unsupported_command")));
    assert!(
        second_outcome
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v == &expected_missing))
    );
    assert!(
        second_outcome
            .iter()
            .any(|a| matches!(a, Action::RestoreComposer(text) if text == "hello"))
    );
}

#[test]
fn answer_y_installs_and_other_lines_print_the_command() {
    fn asked_service(id: &SessionId) -> HivecodeService {
        let mut service = HivecodeService::new(false);
        let _ = service.handle_local_command("on", foreground(id));
        let _ = service.on_catalog_reply(
            id,
            "fixture",
            HivecodeCommand::On,
            Ok(parsed(json!({"skills":{}}))),
        );
        assert!(service.answer_install(id, "y").is_none());
        service.record_shown(id);
        service
    }

    let id = session();
    let expected_install = vec![Action::InstallSkill {
        session: id.clone(),
        profile: String::from("fixture"),
    }];

    let mut yes_service = asked_service(&id);
    assert_eq!(
        yes_service.answer_install(&id, "y"),
        Some(expected_install.clone())
    );
    assert_eq!(yes_service.answer_install(&id, "y"), None);

    let mut padded_yes_service = asked_service(&id);
    assert_eq!(
        padded_yes_service.answer_install(&id, " y "),
        Some(expected_install)
    );

    let expected_missing = vec![Action::Notice(notice::missing("fixture"))];
    for answer in ["n", "Y", "yes", ""] {
        let mut service = asked_service(&id);
        let reply = service.answer_install(&id, answer);
        assert_eq!(reply, Some(expected_missing.clone()), "answer={answer:?}");
        assert!(
            !reply
                .unwrap_or_default()
                .iter()
                .any(|a| matches!(a, Action::InstallSkill { .. }))
        );
        assert_eq!(service.answer_install(&id, answer), None);
    }

    let mut unasked = HivecodeService::new(false);
    assert_eq!(unasked.answer_install(&id, "y"), None);
}

#[test]
fn remote_missing_does_not_ask() {
    let id = session();
    let missing = json!({"skills":{}});
    let expected = notice::missing_remote("fixture");

    let mut service = HivecodeService::new(false);
    service.set_remote(true);
    let _ = service.handle_local_command("on", foreground(&id));
    let replies = service.on_catalog_reply(
        &id,
        "fixture",
        HivecodeCommand::On,
        Ok(parsed(missing.clone())),
    );
    assert!(
        replies
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v == &expected && !v.contains("now? [y/N]")))
    );
    assert!(
        !replies
            .iter()
            .any(|a| matches!(a, Action::InstallSkill { .. }))
    );
    assert_eq!(service.answer_install(&id, "y"), None);

    let second = service.on_catalog_reply(&id, "fixture", HivecodeCommand::On, Ok(parsed(missing)));
    assert!(
        second
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v == &expected))
    );

    let mut outcome_service = HivecodeService::new(false);
    outcome_service.set_remote(true);
    let _ = outcome_service.handle_local_command("on", foreground(&id));
    let outcome = outcome_service.on_command_outcome(&id, "hello", Err(rpc("unsupported_command")));
    assert!(
        outcome
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v == &expected))
    );
    assert!(
        outcome
            .iter()
            .any(|a| matches!(a, Action::RestoreComposer(text) if text == "hello"))
    );
}

#[test]
fn background_reply_asks_when_shown() {
    let id = session();
    let missing = json!({"skills":{}});
    let offer = notice::offer("fixture");

    let mut service = HivecodeService::new(false);
    let _ = service.handle_local_command("on", foreground(&id));
    let _ = service.on_catalog_reply(
        &id,
        "fixture",
        HivecodeCommand::On,
        Ok(parsed(missing.clone())),
    );
    assert_eq!(service.answer_install(&id, "y"), None);
    assert_eq!(service.show_kept(&id), Some(offer.clone()));
    assert_eq!(service.show_kept(&id), None);

    let later = service.on_catalog_reply(
        &id,
        "fixture",
        HivecodeCommand::On,
        Ok(parsed(missing.clone())),
    );
    assert!(
        !later
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v == &offer))
    );

    let mut twice = HivecodeService::new(false);
    let _ = twice.handle_local_command("on", foreground(&id));
    let _ = twice.on_catalog_reply(
        &id,
        "fixture",
        HivecodeCommand::On,
        Ok(parsed(missing.clone())),
    );
    let _ = twice.on_catalog_reply(&id, "fixture", HivecodeCommand::On, Ok(parsed(missing)));
    assert_eq!(twice.show_kept(&id), Some(offer));
    assert_eq!(twice.show_kept(&id), None);
}

#[test]
fn routed_prompt_prints_no_interim_line() {
    // The building block above the composer says the prompt is handled as a workflow.
    let id = session();
    let service = on();
    let actions = service.intercept_submit(&id, "fix the parser").unwrap();
    assert!(
        matches!(&actions[..], [Action::RunCommand { .. }]),
        "{actions:?}"
    );
    let off = HivecodeService::new(false);
    let keyword = off.intercept_submit(&id, "please hivecode this").unwrap();
    assert!(
        matches!(&keyword[..], [Action::RunCommand { .. }]),
        "{keyword:?}"
    );
    let mut remote = on();
    remote.set_remote(true);
    let actions = remote.intercept_submit(&id, "fix the parser").unwrap();
    assert!(
        matches!(&actions[..], [Action::RunCommand { .. }]),
        "{actions:?}"
    );
}

fn skill_reply(instruction: &str) -> Value {
    json!({"type": "skill", "name": "talaria-workflows",
        "message": format!("fixture skill body\n\nThe user has provided the following instruction alongside the skill invocation: {instruction}"),
        "display": format!("/talaria-workflows {instruction}")})
}

fn epoch(stored: &str, compressions: i64) -> SkillEpoch {
    SkillEpoch {
        stored: stored.into(),
        compressions,
    }
}

/// What a routed prompt sends to the model and shows on screen, when it skips Hermes.
fn follow_up(actions: &[Action]) -> Option<(&str, &str)> {
    actions.iter().find_map(|action| match action {
        Action::SubmitCommand { text, display, .. } => Some((text.as_str(), display.as_str())),
        _ => None,
    })
}

#[test]
fn routed_prompt_cell_shows_typed_text() {
    let id = session();
    let service = on();
    let actions = service.on_command_outcome(&id, "fix it", Ok(skill_reply("fix it")));
    let (text, display) = follow_up(&actions).expect("skill submission");
    assert!(text.starts_with("fixture skill body"), "{text}");
    assert_eq!(display, "fix it");
    assert!(
        actions.iter().all(
            |action| !matches!(action, Action::Notice(text) if text.contains("talaria-workflows"))
        ),
        "{actions:?}"
    );
}

#[test]
fn later_routed_prompts_send_a_reminder_with_the_typed_text() {
    let id = session();
    let mut service = on();
    let first = service.intercept_submit(&id, "fix the parser").unwrap();
    assert!(
        first
            .iter()
            .any(|action| matches!(action, Action::RunCommand { .. })),
        "the first routed prompt loads the whole skill: {first:?}"
    );
    service.skill_sent(&id, epoch("stored", 0));
    for typed in ["go ahead", "now add a test for the lexer", "keep going"] {
        service.note_epoch(&id, &epoch("stored", 0));
        let actions = service.intercept_submit(&id, typed).unwrap();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Notice(_))),
            "{actions:?}"
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::RunCommand { .. })),
            "{actions:?}"
        );
        let (text, display) = follow_up(&actions).expect("a direct submit");
        assert_eq!(text, follow_up_text(typed));
        assert_eq!(display, typed);
    }
    service.note_epoch(&id, &epoch("stored", 1));
    let compacted = service.intercept_submit(&id, "go ahead").unwrap();
    assert!(
        compacted
            .iter()
            .any(|action| matches!(action, Action::RunCommand { .. })),
        "after a compaction the whole skill goes again: {compacted:?}"
    );
}

#[test]
fn hivecode_on_again_sends_the_whole_skill_next() {
    let id = session();
    let mut service = on();
    service.skill_sent(&id, epoch("stored", 0));
    let _ = service.handle_local_command("on", foreground(&id));
    let _ = service.on_catalog_reply(&id, "fixture", HivecodeCommand::On, Ok(parsed(catalog())));
    let actions = service.intercept_submit(&id, "go ahead").unwrap();
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::RunCommand { .. })),
        "{actions:?}"
    );
}

#[test]
fn keyword_prompt_always_loads_the_whole_skill() {
    let id = session();
    let mut off = HivecodeService::new(false);
    off.skill_sent(&id, epoch("stored", 0));
    let actions = off.intercept_submit(&id, "please hivecode this").unwrap();
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::RunCommand { .. })),
        "{actions:?}"
    );
}

#[test]
fn a_failed_dispatch_never_names_the_skill_command() {
    let id = session();
    for failure in [
        Err(ErrorView::TimedOut),
        Err(ErrorView::NotConnected),
        Err(ErrorView::LocalBug),
        Err(rpc("unknown_reason")),
    ] {
        let actions = on().on_command_outcome(&id, "fix it", failure);
        for action in &actions {
            if let Action::Notice(text) = action {
                assert!(!text.contains("/talaria-workflows"), "{text}");
            }
        }
    }
}

#[test]
fn remote_on_notice_says_where_runs_appear() {
    let id = session();
    let mut local = HivecodeService::new(false);
    let _ = local.handle_local_command("on", foreground(&id));
    assert_eq!(
        local.on_catalog_reply(&id, "fixture", HivecodeCommand::On, Ok(parsed(catalog()))),
        vec![Action::Notice(notice::ON.into()), Action::Changed]
    );
    let mut remote = HivecodeService::new(false);
    remote.set_remote(true);
    let _ = remote.handle_local_command("on", foreground(&id));
    assert_eq!(
        remote.on_catalog_reply(&id, "fixture", HivecodeCommand::On, Ok(parsed(catalog()))),
        vec![Action::Notice(notice::ON_REMOTE.into()), Action::Changed]
    );
    assert!(remote.status_input(Some(&id)));
}

fn run_row(
    run_id: &str,
    profile: &str,
    started_at_ms: i64,
    status: talaria_workflow::runner::RunStatus,
    finished: u32,
) -> talaria_workflow::runner::RunSummary {
    talaria_workflow::runner::RunSummary {
        run_id: run_id.into(),
        name: "review-branch".into(),
        profile: profile.into(),
        started_at_ms,
        status,
        agents_finished: finished,
        agents_total_known: 10,
        parked: 0,
        tokens_spent: None,
        cache_read_tokens: None,
        cache_unknown_turns: 0,
        money_spent_usd: None,
        run_dir: format!("fixture-runs/{run_id}"),
        warning: None,
        attention: Default::default(),
    }
}

fn notices(actions: &[(SessionId, Action)]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|(_, action)| match action {
            Action::Notice(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn runs_scan_announces_start_once_and_finish_once() {
    use talaria_workflow::runner::RunStatus;
    let id = session();
    let mut service = on();
    assert_eq!(service.next_scan_ms(), None, "nothing is watched yet");
    assert_eq!(service.scan_due(0), None);
    service.arm(&id, "fixture", 1_000, 50);
    assert_eq!(service.scan_due(50), Some(1_000));
    assert_eq!(service.scan_due(60), None, "one scan at a time");
    let running = [run_row("run-0001", "fixture", 2_000, RunStatus::Running, 3)];
    let first = service.on_runs(&running, 100, Some("Alt+W"));
    assert_eq!(
        notices(&first),
        [
            "Workflow run started: review-branch (run-0001). Press Alt+W or type /workflows to see its agents."
        ]
    );
    assert!(first.iter().all(|(owner, _)| owner == &id));
    assert_eq!(service.active_run(&id).map(|r| r.agents_finished), Some(3));
    assert_eq!(service.next_scan_ms(), Some(2_100));
    assert_eq!(service.scan_due(2_000), None, "not due yet");
    assert_eq!(service.scan_due(2_100), Some(1_000));
    assert!(notices(&service.on_runs(&running, 2_200, Some("Alt+W"))).is_empty());
    let done = [run_row(
        "run-0001",
        "fixture",
        2_000,
        RunStatus::Completed,
        10,
    )];
    let _ = service.scan_due(4_200);
    let last = service.on_runs(&done, 4_300, Some("Alt+W"));
    assert_eq!(
        notices(&last),
        [
            "Workflow run completed: review-branch (run-0001). Press Alt+W or type /workflows to see the result."
        ]
    );
    assert!(
        service.active_run(&id).is_none(),
        "a finished run is not active"
    );
    assert_eq!(
        service.tracked(&id).map(|r| r.run_dir.as_str()),
        Some("fixture-runs/run-0001"),
        "the finished run stays the one the view key opens"
    );
    assert_eq!(
        service.next_scan_ms(),
        Some(6_300),
        "the routed prompt may still start another run, so the watch keeps reading"
    );
}

#[test]
fn a_run_its_budget_stopped_is_announced_as_such() {
    use talaria_workflow::runner::RunStatus;
    let id = session();
    let mut service = on();
    service.arm(&id, "fixture", 1_000, 50);
    let _ = service.scan_due(50);
    let running = [run_row("run-0002", "fixture", 2_000, RunStatus::Running, 1)];
    let _ = service.on_runs(&running, 100, Some("Alt+W"));
    let _ = service.scan_due(2_100);
    let ended = [run_row(
        "run-0002",
        "fixture",
        2_000,
        RunStatus::BudgetExhausted,
        3,
    )];
    assert_eq!(
        notices(&service.on_runs(&ended, 2_200, Some("Alt+W"))),
        [
            "Workflow run stopped by its budget: review-branch (run-0002). Press Alt+W or type /workflows to see the result."
        ]
    );
}

#[test]
fn a_run_that_completed_with_failures_is_announced_as_finished_and_ends_the_tracking() {
    use talaria_workflow::runner::RunStatus;
    let id = session();
    let mut service = on();
    service.arm(&id, "fixture", 1_000, 50);
    let _ = service.scan_due(50);
    let running = [run_row("run-0002", "fixture", 2_000, RunStatus::Running, 1)];
    let _ = service.on_runs(&running, 100, Some("Alt+W"));
    let _ = service.scan_due(2_100);
    let ended = [run_row(
        "run-0002",
        "fixture",
        2_000,
        RunStatus::CompletedWithFailures,
        3,
    )];
    assert_eq!(
        notices(&service.on_runs(&ended, 2_200, Some("Alt+W"))),
        [
            "Workflow run completed with failures: review-branch (run-0002). Press Alt+W or type /workflows to see the result."
        ]
    );
}

/// How long a watch lives after the routed prompt that last armed it, as the service states.
const WATCH_WINDOW_MS: u64 = 60 * 60 * 1_000;

#[test]
fn another_run_ending_first_does_not_end_the_watch() {
    use talaria_workflow::runner::RunStatus;
    let id = session();
    let mut service = on();
    service.arm(&id, "fixture", 1_000, 0);
    let _ = service.scan_due(0);
    // A run on the same profile, started by another window, starts and ends first.
    let other = [run_row(
        "run-other",
        "fixture",
        2_000,
        RunStatus::Running,
        0,
    )];
    let _ = service.on_runs(&other, 100, Some("Alt+W"));
    let _ = service.scan_due(2_100);
    let other = [run_row(
        "run-other",
        "fixture",
        2_000,
        RunStatus::Completed,
        1,
    )];
    let _ = service.on_runs(&other, 2_200, Some("Alt+W"));
    assert!(
        service.next_scan_ms().is_some(),
        "this session's own run has not started yet"
    );
    assert_eq!(service.scan_due(4_200), Some(1_000));
    let rows = [
        run_row("run-other", "fixture", 2_000, RunStatus::Completed, 1),
        run_row("run-0002", "fixture", 3_000, RunStatus::Running, 0),
    ];
    assert_eq!(
        notices(&service.on_runs(&rows, 4_300, Some("Alt+W"))),
        [
            "Workflow run started: review-branch (run-0002). Press Alt+W or type /workflows to see its agents."
        ]
    );
    assert_eq!(
        service.active_run(&id).map(|run| run.run_id.as_str()),
        Some("run-0002")
    );
}

#[test]
fn arming_again_opens_a_fresh_window_and_keeps_active_runs() {
    use talaria_workflow::runner::RunStatus;
    let id = session();
    let mut service = on();
    service.arm(&id, "fixture", 1_000, 0);
    let _ = service.scan_due(0);
    let rows = [
        run_row("run-long", "fixture", 2_000, RunStatus::Running, 0),
        run_row("run-short", "fixture", 2_500, RunStatus::Running, 0),
    ];
    let _ = service.on_runs(&rows, 100, Some("Alt+W"));
    let _ = service.scan_due(2_100);
    let rows = [
        run_row("run-long", "fixture", 2_000, RunStatus::Running, 1),
        run_row("run-short", "fixture", 2_500, RunStatus::Completed, 1),
    ];
    let _ = service.on_runs(&rows, 2_200, Some("Alt+W"));
    // A second routed prompt arms the watch again.
    service.arm(&id, "fixture", 10_000, 3_000);
    assert_eq!(
        service.scan_due(4_200),
        Some(2_000),
        "the read still reaches the run that is still active"
    );
    let rows = [
        run_row("run-long", "fixture", 2_000, RunStatus::Completed, 2),
        run_row("run-short", "fixture", 2_500, RunStatus::Completed, 1),
        // Started and ended between the two routed prompts, never seen running.
        run_row("run-between", "fixture", 5_000, RunStatus::Completed, 1),
        run_row("run-new", "fixture", 11_000, RunStatus::Running, 0),
    ];
    assert_eq!(
        notices(&service.on_runs(&rows, 4_300, Some("Alt+W"))),
        [
            "Workflow run completed: review-branch (run-long). Press Alt+W or type /workflows to see the result.",
            "Workflow run started: review-branch (run-new). Press Alt+W or type /workflows to see its agents."
        ]
    );
}

/// docs/workflows/hivecode.md describes this: inside the watch's hour, a run started by hand
/// or in another window on the same profile is announced here and becomes Alt+W's target.
#[test]
fn run_started_by_hand_inside_the_hour_is_announced_and_tracked() {
    use talaria_workflow::runner::RunStatus;
    let id = session();
    let mut service = on();
    service.arm(&id, "fixture", 1_000, 0);
    let _ = service.scan_due(0);
    let mine = [run_row("run-mine", "fixture", 2_000, RunStatus::Running, 0)];
    let _ = service.on_runs(&mine, 100, Some("Alt+W"));
    let _ = service.scan_due(2_100);
    let mine = [run_row(
        "run-mine",
        "fixture",
        2_000,
        RunStatus::Completed,
        1,
    )];
    let _ = service.on_runs(&mine, 2_200, Some("Alt+W"));
    // Thirty minutes later a run is started by hand on the same profile.
    let half_hour = 30 * 60 * 1_000;
    assert!(
        service.scan_due(half_hour).is_some(),
        "the watch is still alive"
    );
    let rows = [
        run_row("run-mine", "fixture", 2_000, RunStatus::Completed, 1),
        run_row(
            "run-by-hand",
            "fixture",
            i64::try_from(half_hour).unwrap(),
            RunStatus::Running,
            0,
        ),
    ];
    assert_eq!(
        notices(&service.on_runs(&rows, half_hour + 100, Some("Alt+W"))),
        [
            "Workflow run started: review-branch (run-by-hand). Press Alt+W or type /workflows to see its agents."
        ]
    );
    assert_eq!(
        service.tracked(&id).map(|run| run.run_id.as_str()),
        Some("run-by-hand"),
        "Alt+W opens the run started by hand"
    );
}

#[test]
fn hivecode_guide_says_a_run_started_elsewhere_can_be_announced() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/workflows/hivecode.md");
    let guide = std::fs::read_to_string(path).unwrap();
    let guide = guide.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        !guide.contains(
            "A run started some other way, or in another window, is reached through the run list"
        ),
        "the guide must not say a run started elsewhere is only reached through the run list"
    );
    assert!(
        guide.contains(
            "a run started by hand or in another window on the same profile is also announced here and can become Alt+W's target"
        ),
        "the guide must say a run started elsewhere inside the hour can become Alt+W's target"
    );
}

#[test]
fn hivecode_guide_quotes_the_notices_as_printed() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/workflows/hivecode.md");
    let guide = std::fs::read_to_string(path).unwrap();
    let guide = guide.split_whitespace().collect::<Vec<_>>().join(" ");
    let changes = [
        "changed:SKILL.md".to_owned(),
        "added:references/extra.md".to_owned(),
    ];
    for quoted in [
        notice::ON.to_owned(),
        notice::OPENED_ON.to_owned(),
        notice::STILL_ON.to_owned(),
        notice::BRANCH_CARRIED.to_owned(),
        notice::REMOTE_RUN_TAIL.to_owned(),
        notice::skill_changed("<name>", &changes),
        notice::skill_differs("<name>"),
        notice::offer("<name>"),
        "/hivecode reinstall".to_owned(),
        "No workflow started. Hivecode is still on.".to_owned(),
        "Step 1 of 5: planning the workflow · the agent is writing project files".to_owned(),
        "stopped by its budget (5 agents).".to_owned(),
        "differs from the one last sent in the conversation".to_owned(),
    ] {
        assert!(guide.contains(&quoted), "the guide must quote: {quoted}");
    }
    for gone in [
        "[y/N]`. An install from an older",
        "one-shot notice",
        "routing notice says so",
    ] {
        assert!(!guide.contains(gone), "the guide still says: {gone}");
    }
}

#[test]
fn watch_lives_one_window_after_the_last_routed_prompt() {
    use talaria_workflow::runner::RunStatus;
    let id = session();
    let mut service = on();
    service.arm(&id, "fixture", 1_000, 0);
    let _ = service.scan_due(0);
    let done = [run_row(
        "run-0001",
        "fixture",
        2_000,
        RunStatus::Completed,
        1,
    )];
    let _ = service.on_runs(&done, WATCH_WINDOW_MS, None);
    assert!(service.next_scan_ms().is_some(), "still inside the window");
    let _ = service.scan_due(WATCH_WINDOW_MS + 2_000);
    let running = [run_row("run-0002", "fixture", 3_000, RunStatus::Running, 0)];
    let _ = service.on_runs(&running, WATCH_WINDOW_MS + 2_000, None);
    assert!(
        service.next_scan_ms().is_some(),
        "an active run keeps the watch past the window so its end is announced"
    );
    let _ = service.scan_due(WATCH_WINDOW_MS + 4_000);
    let done = [run_row("run-0002", "fixture", 3_000, RunStatus::Failed, 0)];
    let _ = service.on_runs(&done, WATCH_WINDOW_MS + 4_000, None);
    assert_eq!(service.next_scan_ms(), None, "the window is over");
}

#[test]
fn runs_scan_ignores_older_runs_and_other_profiles() {
    use talaria_workflow::runner::RunStatus;
    let id = session();
    let mut service = on();
    service.arm(&id, "fixture", 5_000, 0);
    let _ = service.scan_due(0);
    let rows = [
        run_row("run-old", "fixture", 4_000, RunStatus::Running, 0),
        run_row("run-other", "fixture-other", 6_000, RunStatus::Running, 0),
    ];
    assert!(notices(&service.on_runs(&rows, 10, Some("Alt+W"))).is_empty());
    assert!(service.tracked(&id).is_none());
    assert!(service.next_scan_ms().is_some(), "still waiting for a run");
}

#[test]
fn remote_service_never_arms_watch() {
    let id = session();
    let mut service = on();
    service.set_remote(true);
    service.arm(&id, "fixture", 0, 0);
    assert_eq!(service.next_scan_ms(), None);
    assert_eq!(service.scan_due(0), None);
}

fn setup(skill: talaria_app::hivecode::SkillCheck) -> talaria_app::hivecode::SetupFacts {
    talaria_app::hivecode::SetupFacts {
        skill,
        tiers: talaria_app::hivecode::TierCheck::Present,
        missing_builtins: vec![],
    }
}

fn notice_texts(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn setup_is_checked_locally_on_turn_on_reopen_and_routed_prompts() {
    let id = session();
    let service = on();
    assert_eq!(
        service.setup_check(&id),
        Some(Action::CheckSetup {
            session: id.clone(),
            profile: "fixture".into()
        })
    );
    let mut remote = on();
    remote.set_remote(true);
    assert_eq!(
        remote.setup_check(&id),
        None,
        "a remote gateway's files are not here"
    );
}

#[test]
fn a_changed_skill_warns_once_and_names_the_reinstall_command() {
    use talaria_app::hivecode::SkillCheck;
    let id = session();
    let mut service = on();
    let changes: Vec<String> = vec![
        "changed:SKILL.md".into(),
        "added:references/extra.md".into(),
    ];
    let changed = SkillCheck::Modified {
        changes: changes.clone(),
        digest: "digest-1".into(),
    };
    let actions = service.on_setup(&id, "fixture", setup(changed.clone()));
    assert_eq!(
        notice_texts(&actions),
        [
            notice::skill_changed("fixture", &changes),
            notice::skill_lock("fixture"),
        ]
    );
    // The same change is reported once, and no typed line answers it.
    assert!(service.on_setup(&id, "fixture", setup(changed)).is_empty());
    for line in ["y", "yes", "n", "", "fix the parser"] {
        assert_eq!(service.answer_install(&id, line), None, "{line:?}");
    }
    // A further change warns again.
    let again = SkillCheck::Modified {
        changes: vec!["changed:SKILL.md".into()],
        digest: "digest-2".into(),
    };
    assert_eq!(
        notice_texts(&service.on_setup(&id, "fixture", setup(again))).len(),
        2
    );
    // An older install, and an unrecorded one that is not this Talaria's copy, say so once.
    let actions = service.on_setup(
        &id,
        "fixture",
        setup(SkillCheck::Outdated {
            digest: "digest-3".into(),
        }),
    );
    assert_eq!(notice_texts(&actions), [notice::skill_outdated("fixture")]);
    let actions = service.on_setup(
        &id,
        "fixture",
        setup(SkillCheck::Differs {
            digest: "digest-4".into(),
        }),
    );
    assert_eq!(notice_texts(&actions), [notice::skill_differs("fixture")]);
    assert!(
        service
            .on_setup(&id, "fixture", setup(SkillCheck::Unchanged))
            .is_empty()
    );
}

#[test]
fn hivecode_reinstall_asks_for_the_reinstall() {
    let id = session();
    let mut service = on();
    assert_eq!(
        service.handle_local_command("reinstall", foreground(&id)),
        vec![Action::ReinstallSkill {
            session: id.clone(),
            profile: "fixture".into()
        }]
    );
    let mut remote = on();
    remote.set_remote(true);
    assert_eq!(
        remote.handle_local_command("reinstall", foreground(&id)),
        vec![Action::Notice(notice::reinstall_remote("fixture"))]
    );
    assert_eq!(
        service.handle_local_command("reinstall", None),
        vec![Action::Notice(notice::NO_SESSION.into())]
    );
}

#[test]
fn missing_tiers_and_builtins_give_two_short_notices() {
    use talaria_app::hivecode::{SetupFacts, SkillCheck, TierCheck};
    let id = session();
    let mut service = on();
    let facts = SetupFacts {
        skill: SkillCheck::Unchanged,
        tiers: TierCheck::Missing,
        missing_builtins: vec!["review-branch".into(), "research".into()],
    };
    let actions = service.on_setup(&id, "fixture", facts.clone());
    assert_eq!(
        actions,
        vec![
            Action::Notice(notice::setup_tiers("fixture", notice::NO_TIERS)),
            Action::Notice(notice::setup_builtins(&[
                "review-branch".into(),
                "research".into()
            ])),
        ]
    );
    assert!(
        service.on_setup(&id, "fixture", facts).is_empty(),
        "said once"
    );
    let unreadable = SetupFacts {
        skill: SkillCheck::Unchanged,
        tiers: TierCheck::Unreadable("staffing.toml: 3:1: expected a table".into()),
        missing_builtins: vec![],
    };
    let text = notice_texts(&service.on_setup(&id, "fixture", unreadable)).join("\n");
    assert!(text.contains("3:1: expected a table"), "{text}");
}

#[test]
fn an_interrupt_after_a_routed_prompt_says_the_mode_is_still_on() {
    let id = session();
    let mut service = on();
    assert_eq!(service.after_interrupt(&id), None, "nothing was routed yet");
    service.note_routed(&id);
    assert_eq!(
        service.after_interrupt(&id),
        Some(Action::Notice(notice::STILL_ON.into()))
    );
    let _ = service.handle_local_command("off", foreground(&id));
    assert_eq!(service.after_interrupt(&id), None);
}

/// A Hermes home with a default profile and the skill installed the way the installer does it.
fn installed_skill() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    use talaria_workflow::skill::{InstallOptions, SkillTarget, install};
    let temp = tempfile::tempdir().unwrap();
    let hermes = temp.path().join("hermes-home");
    let talaria = temp.path().join("talaria-home");
    std::fs::create_dir_all(&hermes).unwrap();
    std::fs::create_dir_all(&talaria).unwrap();
    let target = SkillTarget::resolve_local("default", &hermes).unwrap();
    install(
        &target,
        &InstallOptions {
            command: "talaria".into(),
            force: false,
            dry_run: false,
            talaria_home: talaria.clone(),
            now: chrono::Utc::now(),
            lock_wait: std::time::Duration::from_secs(5),
        },
    )
    .unwrap();
    (temp, hermes, talaria)
}

#[test]
fn setup_read_compares_the_installed_skill_with_its_install_record() {
    use talaria_app::hivecode::{SkillCheck, TierCheck};
    use talaria_app::hivecode_files::read_setup;
    let (_temp, hermes, talaria) = installed_skill();
    let facts = read_setup("default", &hermes, &talaria);
    assert_eq!(facts.skill, SkillCheck::Unchanged);
    assert_eq!(facts.tiers, TierCheck::Missing);
    let mut missing = facts.missing_builtins.clone();
    missing.sort();
    assert_eq!(
        missing,
        ["fix-failing-tests", "hivecode", "research", "review-branch"]
    );

    // Another program edits the skill and adds a reference file.
    let dir = hermes.join("skills/talaria-workflows");
    let skill = dir.join("SKILL.md");
    let mut text = std::fs::read_to_string(&skill).unwrap();
    text.push_str("\nAn extra rule added by a review.\n");
    std::fs::write(&skill, text).unwrap();
    std::fs::write(dir.join("references/extra.md"), "extra\n").unwrap();
    let changed = read_setup("default", &hermes, &talaria);
    let SkillCheck::Modified { changes, digest } = &changed.skill else {
        panic!("{:?}", changed.skill);
    };
    assert_eq!(changes, &["changed:SKILL.md", "added:references/extra.md"]);
    // A further edit is a new change.
    std::fs::write(dir.join("references/extra.md"), "extra, edited\n").unwrap();
    let SkillCheck::Modified { digest: later, .. } = read_setup("default", &hermes, &talaria).skill
    else {
        panic!("still modified");
    };
    assert_ne!(digest, &later);

    // Tiers and built-ins.
    std::fs::write(
        talaria.join("staffing.toml"),
        "[tiers.builder]\nmodel = \"fixture-model\"\nprovider = \"fixture-provider\"\n",
    )
    .unwrap();
    talaria_workflow::builtins::install(
        &talaria.join("workflows"),
        talaria_workflow::builtins::InstallMode::MissingOnly,
    )
    .unwrap();
    let ready = read_setup("default", &hermes, &talaria);
    assert_eq!(ready.tiers, TierCheck::Present);
    assert!(
        ready.missing_builtins.is_empty(),
        "{:?}",
        ready.missing_builtins
    );
    std::fs::write(talaria.join("staffing.toml"), "[tiers\n").unwrap();
    assert!(matches!(
        read_setup("default", &hermes, &talaria).tiers,
        TierCheck::Unreadable(_)
    ));

    // No install record: the files are compared with the copy built into this Talaria.
    let other = tempfile::tempdir().unwrap();
    let SkillCheck::Differs { digest } = read_setup("default", &hermes, other.path()).skill else {
        panic!("the edited copy differs");
    };
    assert!(!digest.is_empty());
    let (_fresh, fresh_hermes, _) = installed_skill();
    assert_eq!(
        read_setup("default", &fresh_hermes, other.path()).skill,
        SkillCheck::Unchanged,
        "the same files say nothing"
    );
    // No skill at all is the catalogue check's to report.
    let empty = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(empty.path().join("hermes-home")).unwrap();
    assert_eq!(
        read_setup("default", &empty.path().join("hermes-home"), other.path()).skill,
        SkillCheck::Unknown
    );
}

#[test]
fn a_corrupt_empty_or_huge_mode_file_starts_off_and_is_rewritten_cleanly() {
    use talaria_app::hivecode_files::{MODE_FILE_LIMIT, ModeFile};
    use talaria_core::hivecode::{HivecodeState, SavedModes};
    let huge = {
        let mut text = String::from("{\"version\": 1, \"sessions\": {");
        text.push_str(&" ".repeat(MODE_FILE_LIMIT as usize));
        text.push_str("}}");
        text
    };
    for (what, content) in [
        (
            "corrupt",
            "{\"version\": 1, \"sessions\": {\"stored-s\": tru".to_owned(),
        ),
        ("empty", String::new()),
        ("binary", "\u{0}\u{1b}[31m\u{feff}".to_owned()),
        ("huge", huge),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("workflow-mode.json");
        std::fs::write(&path, &content).unwrap();
        assert!(ModeFile::read(&path).is_err(), "{what} is unreadable");
        // Startup keeps an empty state: the session opens with the mode off.
        let mut state = HivecodeState::new(false);
        if let Ok(saved) = ModeFile::read(&path) {
            state.restore(saved);
        }
        let id = SessionId::new("runtime-1");
        assert!(!state.adopt(&id, "stored-s"), "{what}: the mode starts off");
        // The next change rewrites the file cleanly.
        state.set(&id, true);
        ModeFile::new(path.clone())
            .write(1, &state.take_saved(1_000).unwrap())
            .unwrap();
        let saved = ModeFile::read(&path).unwrap();
        assert_ne!(saved, SavedModes::default(), "{what}");
        let mut restarted = HivecodeState::new(false);
        restarted.restore(saved);
        assert!(
            restarted.adopt(&SessionId::new("runtime-2"), "stored-s"),
            "{what}"
        );
        assert!(std::fs::metadata(&path).unwrap().len() < 1_000, "{what}");
    }
}

#[test]
fn the_mode_file_round_trips_and_ignores_a_stale_write() {
    use talaria_app::hivecode_files::ModeFile;
    use talaria_core::hivecode::SavedModes;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state/workflow-mode.json");
    assert_eq!(ModeFile::read(&path).unwrap(), SavedModes::default());
    let file = ModeFile::new(path.clone());
    let mut state = talaria_core::hivecode::HivecodeState::new(false);
    let id = SessionId::new("runtime-1");
    state.adopt(&id, "stored-1");
    state.set(&id, true);
    let newer = state.take_saved(1_000).unwrap();
    state.set(&id, false);
    let older = state.take_saved(1_000).unwrap();
    file.write(2, &newer).unwrap();
    file.write(1, &older).unwrap();
    let mut expected = SavedModes::default();
    expected.merge(&newer);
    assert_eq!(ModeFile::read(&path).unwrap(), expected);
    // An unreadable file is replaced by the next save.
    std::fs::write(&path, "not json").unwrap();
    assert!(ModeFile::read(&path).is_err());
    file.write(3, &older).unwrap();
    assert!(ModeFile::read(&path).is_ok());
}

#[test]
fn two_windows_saving_to_one_mode_file_keep_each_others_settings() {
    use talaria_app::hivecode_files::ModeFile;
    use talaria_core::hivecode::HivecodeState;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("workflow-mode.json");
    // Two Talaria windows, each with its own state and its own handle on the shared file.
    let (mut first, mut second) = (HivecodeState::new(false), HivecodeState::new(false));
    let (first_file, second_file) = (ModeFile::new(path.clone()), ModeFile::new(path.clone()));
    let (s, t) = (SessionId::new("runtime-s"), SessionId::new("runtime-t"));
    second.adopt(&s, "stored-s");
    second.set(&s, true);
    second_file
        .write(1, &second.take_saved(1_000).unwrap())
        .unwrap();
    first.adopt(&t, "stored-t");
    first.set(&t, true);
    first_file
        .write(1, &first.take_saved(2_000).unwrap())
        .unwrap();
    let mut restarted = HivecodeState::new(false);
    restarted.restore(ModeFile::read(&path).unwrap());
    assert!(restarted.adopt(&SessionId::new("runtime-1"), "stored-s"));
    assert!(restarted.adopt(&SessionId::new("runtime-2"), "stored-t"));
}

#[test]
fn the_reinstall_also_installs_the_missing_builtin_workflows_and_keeps_edited_ones() {
    use talaria_app::hivecode_files::{install_missing_builtins, read_setup};
    let (_temp, hermes, talaria) = installed_skill();
    assert_eq!(
        read_setup("default", &hermes, &talaria)
            .missing_builtins
            .len(),
        4
    );
    let mut created = install_missing_builtins(&talaria).unwrap().created;
    created.sort();
    assert_eq!(
        created,
        ["fix-failing-tests", "hivecode", "research", "review-branch"]
    );
    assert!(
        read_setup("default", &hermes, &talaria)
            .missing_builtins
            .is_empty()
    );
    // A second pass creates nothing, and a copy the user edited is left alone.
    let edited = talaria.join("workflows/research.js");
    std::fs::write(&edited, "// mine\n").unwrap();
    assert!(
        install_missing_builtins(&talaria)
            .unwrap()
            .created
            .is_empty()
    );
    assert_eq!(std::fs::read_to_string(edited).unwrap(), "// mine\n");
    assert_eq!(
        notice::builtins_installed(&["research".to_owned(), "hivecode".to_owned()]),
        "Installed built-in workflows: research, hivecode."
    );
    assert!(
        notice::builtins_failed("disk full").ends_with("Try: talaria workflow builtins install")
    );
}

#[cfg(unix)]
#[test]
fn an_unreadable_builtin_is_named_and_the_rest_still_install() {
    use std::os::unix::fs::PermissionsExt;
    use talaria_app::hivecode_files::install_missing_builtins;
    let (_temp, _hermes, talaria) = installed_skill();
    let workflows = talaria.join("workflows");
    std::fs::create_dir_all(&workflows).unwrap();
    // One built-in already exists but cannot be read.
    let blocked = workflows.join("hivecode.js");
    std::fs::write(&blocked, "// kept\n").unwrap();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&blocked).is_ok() {
        return; // running with rights that ignore permissions
    }
    let done = install_missing_builtins(&talaria).unwrap();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut created = done.created.clone();
    created.sort();
    assert_eq!(created, ["fix-failing-tests", "research", "review-branch"]);
    assert_eq!(done.unreadable.len(), 1, "{done:?}");
    assert_eq!(done.unreadable[0].0, "hivecode");
    assert!(
        done.unreadable[0].1.starts_with("could not be read: "),
        "{done:?}"
    );
    assert_eq!(std::fs::read_to_string(&blocked).unwrap(), "// kept\n");
    let text = notice::builtins_outcome(&done.created, &done.unreadable).unwrap();
    assert!(text.starts_with("Installed built-in workflows: "), "{text}");
    assert!(
        text.contains(
            "Left as they are, because they could not be read: hivecode (could not be read: "
        ),
        "{text}"
    );
    assert_eq!(notice::builtins_outcome(&[], &[]), None);
}

#[test]
fn a_running_runs_start_time_is_the_runs_own_and_follows_its_session_across_a_switch() {
    use talaria_workflow::runner::RunStatus;
    let id = session();
    let other = SessionId::new("other-session");
    let mut service = on();
    service.arm(&id, "fixture", 1_000, 50);
    let _ = service.on_runs(
        &[run_row("run-0001", "fixture", 2_000, RunStatus::Running, 1)],
        100,
        None,
    );
    let started = |service: &HivecodeService, session: &SessionId| {
        service.active_run(session).map(|run| run.started_at_ms)
    };
    assert_eq!(started(&service, &id), Some(2_000));
    // The window shows another session, which has no run: its dock has no start time.
    assert_eq!(started(&service, &other), None);
    // Later scans, and coming back to the first session, keep the start the run's records give.
    let _ = service.scan_due(2_100);
    let _ = service.on_runs(
        &[run_row("run-0001", "fixture", 2_000, RunStatus::Running, 2)],
        2_200,
        None,
    );
    assert_eq!(started(&service, &id), Some(2_000));
}
