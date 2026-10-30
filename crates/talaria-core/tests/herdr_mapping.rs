//! Terminal agent discovery and pure fleet projection.

use serde_json::{Value, json};
use talaria_core::fleet::herdr::{HerdrPane, decode_agent, decode_list, project};
use talaria_core::fleet::{FleetBackend, FleetRegistry, NeedSlot, Outcome, RowState};
use talaria_core::fleet_view::row_detail;

fn pane(status: &str) -> HerdrPane {
    HerdrPane {
        pane_id: "wfixture:p1".into(),
        workspace_id: "wfixture".into(),
        tab_id: "wfixture:t1".into(),
        agent: "codex".into(),
        name: "fixture".into(),
        status: status.into(),
        title: "Task".into(),
    }
}
fn agent_value(status: &str) -> Value {
    json!({"pane_id":"wfixture:p1","workspace_id":"wfixture","tab_id":"wfixture:t1","agent":"codex","name":"fixture","agent_status":status,"terminal_title_stripped":"Task"})
}

#[test]
fn list_and_get_decode_real_envelopes() {
    let agent = agent_value("working");
    let list =
        json!({"id":"cli:agent:list","result":{"type":"agent_list","agents":[agent.clone()]}});
    let get = json!({"id":"cli:agent:get","result":{"type":"agent_info","agent":agent}});
    let entries = decode_list(&list).expect("agent list");
    assert_eq!(entries, vec![pane("working")]);
    assert_eq!(decode_agent(&get).expect("agent get"), entries[0]);
    assert!(
        decode_list(&json!({"result":{"type":"agent_list","agents":[{"agent_status":"working"}]}}))
            .is_err()
    );
    assert!(decode_agent(&json!({"result":{"type":"agent_info","agent":{}}})).is_err());
    let mut hostile = agent_value("working");
    hostile["pane_id"] = json!("--help");
    assert!(decode_agent(&json!({"result":{"type":"agent_info","agent":hostile}})).is_err());
}

#[test]
fn lifecycle_states_and_unknown_guard() {
    for (status, state, outcome, needs) in [
        ("working", RowState::Working, None, 0),
        ("idle", RowState::Idle, None, 0),
        ("done", RowState::Done, Some(Outcome::Completed), 0),
        ("blocked", RowState::NeedsYou, None, 1),
        ("unknown", RowState::Idle, Some(Outcome::Unknown), 0),
    ] {
        let view = project(&pane(status), "output", 1.0);
        assert_eq!(view.session.state, state, "{status}");
        assert_eq!(view.session.outcome, outcome, "{status}");
        assert_eq!(view.session.needs.len(), needs, "{status}");
        assert_ne!(
            (status, view.session.outcome),
            ("unknown", Some(Outcome::Completed))
        );
    }
}

#[test]
fn row_identity_badge_and_location_are_external() {
    let view = project(&pane("working"), "output", 1.0);
    assert_eq!(view.session.backend, FleetBackend::Herdr);
    assert_eq!(view.session.agent, "codex");
    assert_eq!(view.session.key.profile, "herdr:wfixture:p1");
    assert_eq!(view.session.key.session_id, "wfixture:p1");
    assert_eq!(
        (
            view.workspace.as_str(),
            view.tab.as_str(),
            view.pane.as_str()
        ),
        ("wfixture", "wfixture:t1", "wfixture:p1")
    );
    let mut registry = FleetRegistry::new();
    let key = view.session.key.clone();
    registry.observe_herdr(view);
    let row = registry.row(&key).expect("external row");
    assert_eq!(row.backend(), FleetBackend::Herdr);
    assert_eq!(row.agent(), Some("codex"));
    let detail = row_detail(row, &[]);
    assert!(detail.contains("[herdr codex]"));
    for part in ["wfixture", "wfixture:t1", "wfixture:p1"] {
        assert!(detail.contains(part));
    }
}

#[test]
fn blocked_pane_has_one_responder_slot_and_clears_on_idle() {
    let mut registry = FleetRegistry::new();
    let view = project(&pane("blocked"), "waiting", 1.0);
    let key = view.session.key.clone();
    assert_eq!(view.session.needs.len(), 1);
    assert_eq!(
        view.session.needs[0].slot,
        NeedSlot::External("wfixture:p1".into())
    );
    registry.observe_herdr(view);
    assert_eq!(registry.row(&key).expect("row").state, RowState::NeedsYou);
    assert_eq!(registry.queue().len(), 1);
    registry.observe_herdr(project(&pane("idle"), "done", 2.0));
    assert_eq!(registry.row(&key).expect("row").state, RowState::Idle);
    assert!(registry.queue().is_empty());
}

#[test]
fn pane_text_and_metadata_are_sanitized_before_storage() {
    let mut source = pane("working");
    source.name = "task\u{1b}[31m".into();
    source.title = "title\u{202e}end".into();
    source.agent = "codex\u{7}".into();
    let view = project(&source, "out\u{1b}[31m\u{202e}x", 1.0);
    assert_eq!(view.visible, "out␛[31m‹U+202E›x");
    assert!(view.session.agent.contains('␇'));
    assert!(view.session.title.contains("‹U+202E›"));
    for text in [
        &view.visible,
        &view.session.title,
        &view.session.agent,
        &view.workspace,
        &view.tab,
    ] {
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{202e}'));
    }
}
