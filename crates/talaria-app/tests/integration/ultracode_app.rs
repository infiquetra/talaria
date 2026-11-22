use crate::mock_gateway;
use serde_json::{Value, json};
use talaria_app::ultracode::{Action, Foreground, UltracodeService};
use talaria_core::session::ids::SessionId;
use talaria_core::{
    commands::{CatalogResult, CommandCatalog, ErrorView},
    ultracode::{UltracodeCommand, notice},
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
fn on() -> UltracodeService {
    let mut service = UltracodeService::new(false);
    let id = session();
    assert_eq!(
        service.handle_local_command("on", foreground(&id)),
        vec![Action::FetchCatalog {
            session: id.clone(),
            profile: "fixture".into(),
            command: UltracodeCommand::On
        }]
    );
    let _ = service.on_catalog_reply(&id, "fixture", UltracodeCommand::On, Ok(parsed(catalog())));
    assert!(service.status_input(Some(&id)));
    service
}
#[test]
fn on_checks_a_fresh_catalogue() {
    let id = session();
    let mut s = UltracodeService::new(false);
    assert_eq!(
        s.handle_local_command("on", foreground(&id)),
        vec![Action::FetchCatalog {
            session: id.clone(),
            profile: "fixture".into(),
            command: UltracodeCommand::On
        }]
    );
    let actions = s.on_catalog_reply(&id, "fixture", UltracodeCommand::On, Ok(parsed(catalog())));
    assert!(s.status_input(Some(&id)));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::Notice(v) if v.contains("Ultracode on")))
    );
    for value in [
        json!({"skills":{}}),
        json!({"skills":{"/talaria-workflows":{"origin":"local"}},"pairs":[["/talaria-workflows","custom"]],"categories":[{"name":"User commands","pairs":[["/talaria-workflows","custom"]]}]}),
    ] {
        let mut service = UltracodeService::new(false);
        let _ = service.handle_local_command("on", foreground(&id));
        let replies =
            service.on_catalog_reply(&id, "fixture", UltracodeCommand::On, Ok(parsed(value)));
        assert!(!service.status_input(Some(&id)));
        assert!(replies.iter().any(|a| matches!(a, Action::Notice(_))));
    }
    let mut service = UltracodeService::new(false);
    let _ = service.handle_local_command("on", foreground(&id));
    let replies =
        service.on_catalog_reply(&id, "fixture", UltracodeCommand::On, Err("offline".into()));
    assert!(!service.status_input(Some(&id)));
    assert!(
        replies
            .iter()
            .any(|a| matches!(a,Action::Notice(v) if v.contains("offline")))
    );
}
#[tokio::test]
async fn routed_submit_frames() {
    let frames: Vec<Value> =
        include_str!("../../../../fixtures/frames/p5-09-ultracode-skill.jsonl")
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
    assert_eq!(display, reply["display"].as_str().unwrap());
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
fn multiline_message_and_collapsed_display() {
    let id = session();
    let service = on();
    let original = "a  b\tc\nd";
    let actions = service.intercept_submit(&id, original).unwrap();
    assert!(actions.iter().any(|a| matches!(a,Action::RunCommand{command_line,..} if command_line=="/talaria-workflows a  b\tc\nd")));
    let reply = json!({"type":"skill","name":"talaria-workflows","message":"a  b\tc\nd","display":"/talaria-workflows a b c d"});
    let actions = service.on_command_outcome(&id, original, Ok(reply));
    assert!(actions.iter().any(|a| matches!(a,Action::SubmitCommand{text,display,..} if text==original && display=="/talaria-workflows a b c d")));
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
fn slash_shell_and_keyword_paths() {
    let id = session();
    let service = on();
    assert!(service.intercept_submit(&id, "/help").is_none());
    assert!(service.intercept_submit(&id, "!ls").is_none());
    let off = UltracodeService::new(false);
    let actions = off.intercept_submit(&id, "please ultracode this").unwrap();
    assert!(actions.iter().any(|a| matches!(a,Action::Notice(v) if v=="Ultracode: this prompt goes to the talaria-workflows skill (keyword ultracode). The agent proposes a workflow and asks before starting it.")));
    assert!(!off.status_input(Some(&id)));
}

#[test]
fn local_missing_asks_once() {
    let id = session();
    let missing = json!({"skills":{}});

    let mut service = UltracodeService::new(false);
    let _ = service.handle_local_command("on", foreground(&id));
    let replies = service.on_catalog_reply(
        &id,
        "fixture",
        UltracodeCommand::On,
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
        UltracodeCommand::On,
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
    let mut collision_service = UltracodeService::new(false);
    let _ = collision_service.handle_local_command("on", foreground(&id));
    let collision_replies = collision_service.on_catalog_reply(
        &id,
        "fixture",
        UltracodeCommand::On,
        Ok(parsed(collision)),
    );
    assert!(
        collision_replies
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v.contains("Rename that command") && !v.contains("now? [y/N]")))
    );

    let mut offline_service = UltracodeService::new(false);
    let _ = offline_service.handle_local_command("on", foreground(&id));
    let offline_replies = offline_service.on_catalog_reply(
        &id,
        "fixture",
        UltracodeCommand::On,
        Err("offline".into()),
    );
    assert!(offline_replies.iter().any(
        |a| matches!(a, Action::Notice(v) if v.contains("offline") && !v.contains("now? [y/N]"))
    ));

    let mut status_service = UltracodeService::new(false);
    let _ = status_service.handle_local_command("status", foreground(&id));
    let status_replies = status_service.on_catalog_reply(
        &id,
        "fixture",
        UltracodeCommand::Status,
        Ok(parsed(missing.clone())),
    );
    assert!(
        status_replies
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v.contains("not found in profile fixture") && !v.contains("now? [y/N]")))
    );

    let mut outcome_service = UltracodeService::new(false);
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
    fn asked_service(id: &SessionId) -> UltracodeService {
        let mut service = UltracodeService::new(false);
        let _ = service.handle_local_command("on", foreground(id));
        let _ = service.on_catalog_reply(
            id,
            "fixture",
            UltracodeCommand::On,
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

    let mut unasked = UltracodeService::new(false);
    assert_eq!(unasked.answer_install(&id, "y"), None);
}

#[test]
fn remote_missing_does_not_ask() {
    let id = session();
    let missing = json!({"skills":{}});
    let expected = format!(
        "{}. The skill must be installed on the machine that runs the gateway.",
        notice::missing("fixture")
    );

    let mut service = UltracodeService::new(false);
    service.set_remote(true);
    let _ = service.handle_local_command("on", foreground(&id));
    let replies = service.on_catalog_reply(
        &id,
        "fixture",
        UltracodeCommand::On,
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

    let second =
        service.on_catalog_reply(&id, "fixture", UltracodeCommand::On, Ok(parsed(missing)));
    assert!(
        second
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v == &expected))
    );

    let mut outcome_service = UltracodeService::new(false);
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

    let mut service = UltracodeService::new(false);
    let _ = service.handle_local_command("on", foreground(&id));
    let _ = service.on_catalog_reply(
        &id,
        "fixture",
        UltracodeCommand::On,
        Ok(parsed(missing.clone())),
    );
    assert_eq!(service.answer_install(&id, "y"), None);
    assert_eq!(service.show_kept(&id), Some(offer.clone()));
    assert_eq!(service.show_kept(&id), None);

    let later = service.on_catalog_reply(
        &id,
        "fixture",
        UltracodeCommand::On,
        Ok(parsed(missing.clone())),
    );
    assert!(
        !later
            .iter()
            .any(|a| matches!(a, Action::Notice(v) if v == &offer))
    );

    let mut twice = UltracodeService::new(false);
    let _ = twice.handle_local_command("on", foreground(&id));
    let _ = twice.on_catalog_reply(
        &id,
        "fixture",
        UltracodeCommand::On,
        Ok(parsed(missing.clone())),
    );
    let _ = twice.on_catalog_reply(&id, "fixture", UltracodeCommand::On, Ok(parsed(missing)));
    assert_eq!(twice.show_kept(&id), Some(offer));
    assert_eq!(twice.show_kept(&id), None);
}

#[test]
fn routed_prompt_announces_routing() {
    let id = session();
    let service = on();
    let actions = service.intercept_submit(&id, "fix the parser").unwrap();
    assert_eq!(actions[0], Action::Notice(notice::ROUTED.into()));
    assert!(matches!(&actions[1], Action::RunCommand { .. }));
    let off = UltracodeService::new(false);
    let keyword = off.intercept_submit(&id, "please ultracode this").unwrap();
    assert_eq!(keyword[0], Action::Notice(notice::ONE_SHOT.into()));
    assert!(!keyword.contains(&Action::Notice(notice::ROUTED.into())));
    let mut remote = on();
    remote.set_remote(true);
    let actions = remote.intercept_submit(&id, "fix the parser").unwrap();
    assert_eq!(actions[0], Action::Notice(notice::ROUTED_REMOTE.into()));
}

#[test]
fn remote_on_notice_says_where_runs_appear() {
    let id = session();
    let mut local = UltracodeService::new(false);
    let _ = local.handle_local_command("on", foreground(&id));
    assert_eq!(
        local.on_catalog_reply(&id, "fixture", UltracodeCommand::On, Ok(parsed(catalog()))),
        vec![Action::Notice(notice::ON.into()), Action::Changed]
    );
    let mut remote = UltracodeService::new(false);
    remote.set_remote(true);
    let _ = remote.handle_local_command("on", foreground(&id));
    assert_eq!(
        remote.on_catalog_reply(&id, "fixture", UltracodeCommand::On, Ok(parsed(catalog()))),
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
        money_spent_usd: None,
        run_dir: format!("fixture-runs/{run_id}"),
        warning: None,
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

/// docs/workflows/ultracode.md describes this: inside the watch's hour, a run started by hand
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
fn ultracode_guide_says_a_run_started_elsewhere_can_be_announced() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/workflows/ultracode.md");
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
