//! External rows use the existing fleet list formats.

use talaria_app::fleet::{FleetLinkState, FleetSnapshot};
use talaria_app::side_connection::SideScope;
use talaria_core::fleet::{ExternalSession, FleetBackend, FleetRegistry, RowKey, RowState};

#[test]
fn fleet_list_adds_backend_and_agent_for_external_rows() {
    let mut registry = FleetRegistry::new();
    registry.observe_external(ExternalSession {
        key: RowKey {
            profile: "acp:fixture".into(),
            session_id: "session-1".into(),
        },
        backend: FleetBackend::Acp,
        agent: "Fixture Agent".into(),
        title: "Task".into(),
        model: None,
        activity: None,
        state: RowState::Idle,
        outcome: None,
        used: None,
        size: None,
        cost: None,
        needs: vec![],
        at: 1.0,
    });
    let snapshot = FleetSnapshot {
        version: 1,
        scope: SideScope::Host { profiles: vec![] },
        link: FleetLinkState::Connected,
        profiles: vec![],
        rows: registry.rows().into_iter().cloned().collect(),
        queue: vec![],
        counts: registry.counts(),
        diagnostics: vec![],
        as_of: 1.0,
    };
    assert!(
        talaria_cli::fleet_cmd::render_text(&snapshot, None, 1.0).contains("[acp Fixture Agent]")
    );
    let json = talaria_cli::fleet_cmd::snapshot_json(&snapshot);
    let row = &json["rows"][0];
    assert_eq!(row["backend"], "acp");
    assert_eq!(row["agent"], "Fixture Agent");
}
