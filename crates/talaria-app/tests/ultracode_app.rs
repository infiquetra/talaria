#[path = "support/mock_gateway.rs"]
mod mock_gateway;
use serde_json::{Value, json};
use talaria_app::ultracode::{Action, Foreground, UltracodeService};
use talaria_core::session::ids::SessionId;
use talaria_core::{
    commands::{CatalogResult, CommandCatalog, ErrorView},
    ultracode::UltracodeCommand,
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
    let frames: Vec<Value> = include_str!("../../../fixtures/frames/p5-09-ultracode-skill.jsonl")
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
    assert!(actions.iter().any(|a| matches!(a,Action::Notice(v) if v=="This prompt runs as a workflow (keyword ultracode).")));
    assert!(!off.status_input(Some(&id)));
}
