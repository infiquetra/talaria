//! External agents remain separate from gateway polling and actions.

use talaria_core::fleet::{
    ExternalSession, FleetBackend, FleetInput, FleetRegistry, RowKey, RowState, update,
};
use talaria_core::fleet_view::{
    self, FleetAction, FleetViewEffect, FleetViewInput, FleetViewState,
};

fn external() -> ExternalSession {
    ExternalSession {
        key: RowKey {
            profile: "acp:sample".into(),
            session_id: "s1".into(),
        },
        backend: FleetBackend::Acp,
        agent: "Sample".into(),
        title: "Task".into(),
        model: None,
        activity: None,
        state: RowState::Working,
        outcome: None,
        used: None,
        size: None,
        cost: None,
        needs: vec![],
        at: 1.0,
    }
}

#[test]
fn external_work_never_becomes_a_gateway_poll_target() {
    let mut fleet = FleetRegistry::new();
    let session = external();
    let key = session.key.clone();
    fleet.observe_external(session);
    assert!(fleet.hot_profiles().is_empty());
    update(
        &mut fleet,
        FleetInput::Profiles {
            served: vec!["hermes".into()],
            parked: vec![],
            in_scope: vec!["hermes".into()],
            at: 2.0,
        },
    );
    update(&mut fleet, FleetInput::ConnectionLost { at: 3.0 });
    let row = fleet
        .row(&key)
        .expect("external session survives gateway loss");
    assert_eq!(row.state, RowState::Working);
    assert!(!row.stale);
}

#[test]
fn working_external_turn_refuses_reply_and_confirms_stop() {
    let mut fleet = FleetRegistry::new();
    let session = external();
    let key = session.key.clone();
    fleet.observe_external(session);
    let mut view = FleetViewState::new(None, 1.0);
    fleet_view::update(
        &mut view,
        FleetViewInput::Snapshot {
            version: 1,
            rows: fleet.rows().into_iter().cloned().collect(),
            queue: vec![],
            profiles: vec![],
            scope_note: None,
            link_note: None,
        },
        20,
    );
    let reply = fleet_view::update(&mut view, FleetViewInput::Action(FleetAction::Reply), 20);
    assert!(reply.is_empty());
    assert!(
        view.notice()
            .unwrap_or_default()
            .contains("already working")
    );
    assert!(
        fleet_view::update(&mut view, FleetViewInput::Action(FleetAction::Stop), 20).is_empty()
    );
    let stop = fleet_view::update(&mut view, FleetViewInput::Action(FleetAction::Stop), 20);
    assert!(stop.contains(&FleetViewEffect::Stop(key)));
}
