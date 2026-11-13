use serde_json::json;
use talaria_app::configure::wire::{create_params, detach_params, interrupt_params, submit_params};
use talaria_core::configure::CreateSpec;

#[test]
fn create_params_are_exact() {
    let mut spec = CreateSpec {
        request_id: "request-1".into(),
        toolsets: vec!["clarify".into(), "no_mcp".into()],
        provider: None,
        model: None,
    };
    assert_eq!(
        create_params(&spec),
        json!({
            "request_id": "request-1",
            "toolsets": ["clarify", "no_mcp"],
            "hidden": true,
            "ignore_rules": true,
            "max_turns": 8
        })
    );

    spec.provider = Some("provider".into());
    spec.model = Some("model".into());
    assert_eq!(
        create_params(&spec),
        json!({
            "request_id": "request-1",
            "toolsets": ["clarify", "no_mcp"],
            "hidden": true,
            "ignore_rules": true,
            "max_turns": 8,
            "provider": "provider",
            "model": "model"
        })
    );
}

#[test]
fn submit_interrupt_detach_params_are_exact() {
    assert_eq!(
        submit_params("session-1", "the brief", "submission-1"),
        json!({"session_id": "session-1", "text": "the brief", "submission_id": "submission-1"})
    );
    assert_eq!(
        interrupt_params("session-1", 17),
        json!({"session_id": "session-1", "execution_generation": 17})
    );
    let detach = detach_params("session-1", "subscription-1");
    assert_eq!(
        detach,
        json!({"session_id": "session-1", "subscription_id": "subscription-1", "end_reason": "configure_closed"})
    );
    let reason = detach["end_reason"].as_str().unwrap();
    assert!((1..=32).contains(&reason.len()));
    assert!(reason.bytes().next().unwrap().is_ascii_lowercase());
    assert!(reason.bytes().all(|c| c.is_ascii_lowercase() || c == b'_'));
}
