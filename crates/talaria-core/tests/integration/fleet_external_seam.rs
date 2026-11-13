//! External fleet ownership and queue identity.

use talaria_core::fleet::{
    ExternalNeed, ExternalOption, ExternalSession, FleetBackend, FleetRegistry, NeedSlot, RowKey,
    RowState,
};
use talaria_core::fleet_view::row_detail;

fn session(needs: Vec<ExternalNeed>) -> ExternalSession {
    ExternalSession {
        key: RowKey {
            profile: "acp:fixture".into(),
            session_id: "session-1".into(),
        },
        backend: FleetBackend::Acp,
        agent: "Fixture Agent".into(),
        title: "Task".into(),
        model: None,
        activity: Some("thinking".into()),
        state: RowState::Working,
        outcome: None,
        used: Some(20),
        size: Some(100),
        cost: Some((1.5, "EUR".into())),
        needs,
        at: 1.0,
    }
}

#[test]
fn external_row_has_backend_agent_usage_and_detail_tag() {
    let mut registry = FleetRegistry::new();
    let input = session(vec![]);
    let key = input.key.clone();
    registry.observe_external(input);
    let row = registry.row(&key).expect("external row");
    assert_eq!(row.backend(), FleetBackend::Acp);
    assert_eq!(row.agent(), Some("Fixture Agent"));
    assert_eq!(row.occupancy(), Some((20, 100)));
    assert_eq!(row.cost(), Some((1.5, "EUR")));
    assert!(row_detail(row, &[]).ends_with(" [acp Fixture Agent]"));
}

#[test]
#[allow(clippy::cloned_ref_to_slice_refs)]
fn external_input_keeps_responder_keyed_options_distinct() {
    let mut registry = FleetRegistry::new();
    let option = ExternalOption {
        id: "reject".into(),
        name: "Reject".into(),
        kind: "reject_once".into(),
    };
    let needs = ["1:7", "1:8"]
        .into_iter()
        .map(|id| ExternalNeed {
            slot: NeedSlot::External(id.into()),
            text: "Fixture Agent: Run".into(),
            options: vec![option.clone()],
        })
        .collect();
    let input = session(needs);
    let key = input.key.clone();
    registry.observe_external(input);
    assert_eq!(
        registry.row(&key).expect("external row").state,
        RowState::NeedsYou
    );
    let queue = registry.queue();
    assert_eq!(queue.len(), 2);
    assert_ne!(queue[0].id.slot, queue[1].id.slot);
    assert_eq!(queue[0].external_options(), &[option.clone()]);
    assert_eq!(queue[1].external_options(), &[option]);
}

#[test]
fn hermes_row_keeps_its_existing_detail() {
    use talaria_core::fleet::{FleetInput, SubscribedTurn, SubscribedView, update};
    let mut registry = FleetRegistry::new();
    let key = RowKey {
        profile: "fixture".into(),
        session_id: "session-1".into(),
    };
    update(
        &mut registry,
        FleetInput::Subscribed {
            key: key.clone(),
            view: SubscribedView {
                turn: SubscribedTurn::Idle,
                needs: vec![],
                activity: None,
                cost_usd: None,
                model: None,
                title: Some("Hermes".into()),
            },
            at: 1.0,
        },
    );
    let row = registry.row(&key).expect("Hermes row");
    assert_eq!(row.backend(), FleetBackend::Hermes);
    assert_eq!(row.agent(), None);
    assert_eq!(row_detail(row, &[]), "idle");
}
