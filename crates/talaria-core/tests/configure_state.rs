#[path = "support/configure_ctx.rs"]
mod configure_ctx;
#[path = "support/events.rs"]
mod events;
#[path = "support/snapshot_json.rs"]
mod snapshot_json;
use configure_ctx::{context, detect};
use serde_json::json;
use std::time::{Duration, Instant};
use talaria_core::configure::*;

fn open(request: &str, connected: bool) -> ConfigureInput {
    ConfigureInput::Open {
        request: request.into(),
        profile: "fixture".into(),
        connected,
        model: None,
    }
}
fn starting(now: Instant) -> ConfigureState {
    let mut state = ConfigureState::new(detect);
    assert_eq!(
        state.update(open("turn on compact display", true), now),
        vec![ConfigureEffect::Gather]
    );
    let effects = state.update(ConfigureInput::Gathered(Box::new(context())), now);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::CreateSession(_)))
    );
    state
}
fn snapshot(
    tools: Option<serde_json::Value>,
    approvals_off: bool,
) -> hermes_contract::schemas::CanonicalSnapshot {
    let mut info = json!({"desktop_protocol":"1", "yolo":approvals_off});
    if let Some(tools) = tools {
        info["tools"] = tools;
    }
    let mut value = snapshot_json::base("session-1");
    value["info"] = info;
    snapshot_json::snapshot(value)
}

#[test]
fn open_refuses_bad_requests_and_gathers() {
    let now = Instant::now();
    let mut s = ConfigureState::new(detect);
    let empty = s.update(open("", true), now);
    assert!(format!("{empty:?}").contains("Usage"));
    let long = s.update(open(&"a".repeat(4_001), true), now);
    assert!(format!("{long:?}").contains("4,000"));
    let secret = s.update(
        open("sk-or-v1-talaria-sentinel-0000000000000000", true),
        now,
    );
    assert!(
        secret
            .iter()
            .any(|e| matches!(e, ConfigureEffect::ForgetHistoryLine(_)))
    );
    assert!(!secret.iter().any(|e| matches!(e, ConfigureEffect::Gather)));
    let disconnected = s.update(open("hello", false), now);
    assert!(format!("{disconnected:?}").contains("Not connected"));
    assert_eq!(
        s.update(open("hello", true), now),
        vec![ConfigureEffect::Gather]
    );
}

#[test]
fn creates_locked_hidden_session_without_retry() {
    let now = Instant::now();
    let mut s = ConfigureState::new(detect);
    s.update(open("hello", true), now);
    let effects = s.update(ConfigureInput::Gathered(Box::new(context())), now);
    let spec = effects
        .iter()
        .find_map(|e| {
            if let ConfigureEffect::CreateSession(spec) = e {
                Some(spec)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(spec.toolsets, ["clarify", "no_mcp"]);
    assert!(spec.provider.is_none() && spec.model.is_none());
    for (field, expected) in [("toolsets", "clarify tool"), ("max_turns", "failed")] {
        let mut s = starting(now);
        let effects = s.update(
            ConfigureInput::CreateRefused {
                field: Some(field.into()),
                detail: Some("not available".into()),
                message: "invalid".into(),
            },
            now,
        );
        assert!(!effects.iter().any(|e| matches!(
            e,
            ConfigureEffect::CreateSession(_) | ConfigureEffect::Detach { .. }
        )));
        assert!(format!("{:?} {:?}", effects, s.view(now).text_lines()).contains(expected));
    }
}

#[test]
fn refuses_a_session_with_unproven_tools() {
    let now = Instant::now();
    for (tools, off) in [
        (
            Some(json!({"terminal":["terminal"],"clarify":["clarify"]})),
            false,
        ),
        (Some(json!({"clarify":["clarify"]})), true),
        (Some(json!({})), false),
        (None, false),
    ] {
        let mut s = starting(now);
        let effects = s.update(ConfigureInput::Created(Box::new(snapshot(tools, off))), now);
        assert_eq!(
            effects
                .iter()
                .filter(|e| matches!(e, ConfigureEffect::Detach { .. }))
                .count(),
            1
        );
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, ConfigureEffect::Submit { .. }))
        );
    }
    for tools in [json!({"clarify":["clarify"]}), json!({"core":["clarify"]})] {
        let mut s = starting(now);
        let effects = s.update(
            ConfigureInput::Created(Box::new(snapshot(Some(tools), false))),
            now,
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, ConfigureEffect::Submit { .. }))
        );
    }
}

#[test]
fn timeout_and_cancel_interrupt() {
    let now = Instant::now();
    let mut s = starting(now);
    s.update(
        ConfigureInput::Created(Box::new(snapshot(
            Some(json!({"clarify":["clarify"]})),
            false,
        ))),
        now,
    );
    let effects = s.update(ConfigureInput::Tick, now + Duration::from_secs(300));
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Interrupt { .. }))
    );
    assert!(format!("{:?}", s.view(now).text_lines()).contains("No reply within 5 minutes"));
    let mut s = starting(now);
    s.update(
        ConfigureInput::Created(Box::new(snapshot(
            Some(json!({"clarify":["clarify"]})),
            false,
        ))),
        now,
    );
    let effects = s.update(
        ConfigureInput::Key(ConfigureKey::Esc),
        now + Duration::from_secs(1),
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Interrupt { .. }))
    );
    assert!(format!("{:?}", s.view(now).text_lines()).contains("Cancelled. Nothing was changed."));
}

#[test]
fn every_exit_detaches_once() {
    let now = Instant::now();
    let mut s = starting(now);
    let before = s.update(ConfigureInput::Key(ConfigureKey::Esc), now);
    assert!(
        !before
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Detach { .. }))
    );
    let late = s.update(
        ConfigureInput::Created(Box::new(snapshot(
            Some(json!({"clarify":["clarify"]})),
            false,
        ))),
        now,
    );
    assert_eq!(
        late.iter()
            .filter(|e| matches!(e, ConfigureEffect::Detach { .. }))
            .count(),
        1
    );
    assert!(
        !late
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Submit { .. }))
    );
    let next = s.update(ConfigureInput::Quit, now);
    assert!(
        !next
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Detach { .. }))
    );
    let mut refused = starting(now);
    refused.update(ConfigureInput::Key(ConfigureKey::Esc), now);
    let effects = refused.update(
        ConfigureInput::CreateRefused {
            field: None,
            detail: None,
            message: "refused".into(),
        },
        now,
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, ConfigureEffect::Detach { .. }))
    );
    let mut disconnected = starting(now);
    disconnected.update(
        ConfigureInput::Created(Box::new(snapshot(
            Some(json!({"clarify":["clarify"]})),
            false,
        ))),
        now,
    );
    let effects = disconnected.update(ConfigureInput::LinkLost, now);
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, ConfigureEffect::Detach { .. }))
            .count(),
        1
    );
    assert!(
        format!("{:?} {:?}", effects, disconnected.view(now).text_lines())
            .contains("Lost the connection to Hermes")
    );
}

#[test]
fn terminal_replies_and_quit_release_the_created_session() {
    let now = Instant::now();
    for (status, text) in [
        (
            "complete",
            "```talaria-configure\n{\"kind\":\"answer\",\"text\":\"done\"}\n```",
        ),
        (
            "complete",
            "```talaria-configure\n{\"kind\":\"cannot\",\"reason\":\"no\"}\n```",
        ),
        ("complete", "no structured reply"),
        ("error", "failed"),
        ("interrupted", "stopped"),
    ] {
        let mut s = starting(now);
        let mut value = snapshot_json::base(events::SESSION);
        value["info"] =
            json!({"desktop_protocol":"1","tools":{"clarify":["clarify"]},"yolo":false});
        let mut effects = s.update(
            ConfigureInput::Created(Box::new(snapshot_json::snapshot(value))),
            now,
        );
        effects.extend(s.update(
            ConfigureInput::Session(events::ev(
                1,
                "message.complete",
                json!({"status":status,"text":text}),
            )),
            now,
        ));
        effects.extend(s.update(ConfigureInput::Key(ConfigureKey::Esc), now));
        effects.extend(s.update(ConfigureInput::Quit, now));
        assert_eq!(
            effects
                .iter()
                .filter(|effect| matches!(effect, ConfigureEffect::Detach { .. }))
                .count(),
            1,
            "{status} {text}"
        );
    }
    let mut s = starting(now);
    let mut value = snapshot_json::base(events::SESSION);
    value["info"] = json!({"desktop_protocol":"1","tools":{"clarify":["clarify"]},"yolo":false});
    let mut effects = s.update(
        ConfigureInput::Created(Box::new(snapshot_json::snapshot(value))),
        now,
    );
    let proposal = "```talaria-configure\n{\"kind\":\"proposal\",\"summary\":\"change\",\"changes\":[{\"op\":\"set\",\"key\":\"display.compact\",\"value\":true}]}\n```";
    effects.extend(s.update(
        ConfigureInput::Session(events::ev(
            1,
            "message.complete",
            json!({"status":"complete","text":proposal}),
        )),
        now,
    ));
    effects.extend(s.update(ConfigureInput::Key(ConfigureKey::Char('a')), now));
    effects.extend(s.update(ConfigureInput::Key(ConfigureKey::Char('y')), now));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ConfigureEffect::Apply { index: 0, .. }))
    );
    effects.extend(s.update(
        ConfigureInput::StepDone {
            index: 0,
            result: StepResult::Rows(vec![(0, RowOutcome::Applied)]),
        },
        now,
    ));
    effects.extend(s.update(ConfigureInput::Key(ConfigureKey::Enter), now));
    effects.extend(s.update(ConfigureInput::Quit, now));
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, ConfigureEffect::Detach { .. }))
            .count(),
        1
    );
    let mut s = starting(now);
    let mut value = snapshot_json::base(events::SESSION);
    value["info"] = json!({"desktop_protocol":"1","tools":{"clarify":["clarify"]},"yolo":false});
    value["prompts"] = json!([{"kind":"approval","prompt_id":"approval-1","execution_generation":1,"command":"inspect","choices":["once","deny"]}]);
    s.update(
        ConfigureInput::Created(Box::new(snapshot_json::snapshot(value))),
        now,
    );
    s.update(
        ConfigureInput::Session(events::ev(
            1,
            "clarify.request",
            events::clarify_request("prompt-1", 1, "question"),
        )),
        now,
    );
    let effects = s.update(ConfigureInput::Quit, now);
    let deny = effects
        .iter()
        .position(|e| matches!(e, ConfigureEffect::DenyApproval { .. }))
        .expect("deny");
    let interrupt = effects
        .iter()
        .position(|e| matches!(e, ConfigureEffect::Interrupt { .. }))
        .expect("interrupt");
    let detach = effects
        .iter()
        .position(|e| matches!(e, ConfigureEffect::Detach { .. }))
        .expect("detach");
    assert!(deny < interrupt && interrupt < detach);
}
#[path = "support/configure_state_extra.rs"]
mod extra;
