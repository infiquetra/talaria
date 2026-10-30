//! External transcripts and permissions appear in the fleet overlay.

use std::collections::BTreeMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::fleet::acp::{
    AcpMapping, AcpSessionKey, AcpUpdate, AcpUsage, PermissionOption, PermissionRequest,
};
use talaria_core::fleet::{ExternalSession, FleetBackend, FleetRegistry, RowKey, RowState};
use talaria_core::fleet_view::{self, FleetAction, FleetViewInput, FleetViewState, PeekStatus};
use talaria_tui::fleet_view::{ExternalPeekPanel, FleetTheme, FleetView};

#[test]
fn external_peek_shows_transcript_usage_and_permission_options() {
    let key = AcpSessionKey {
        agent_id: "example".into(),
        session_id: "s1".into(),
    };
    let row = RowKey {
        profile: "acp:example".into(),
        session_id: "s1".into(),
    };
    let mut mapping = AcpMapping::default();
    mapping.start_turn();
    mapping.apply(AcpUpdate::MessageChunk {
        key: key.clone(),
        text: "Agent says hello".into(),
    });
    mapping.apply(AcpUpdate::Usage {
        key: key.clone(),
        usage: AcpUsage {
            used: 24,
            size: 100,
            cost_amount: Some(1.5),
            cost_currency: Some("EUR".into()),
        },
    });
    mapping.apply(AcpUpdate::Mode {
        key: key.clone(),
        name: "review".into(),
    });
    mapping.apply(AcpUpdate::AvailableCommands {
        key: key.clone(),
        names: vec!["explain".into(), "test".into()],
    });
    mapping.request_permission(PermissionRequest {
        key,
        responder: "1:7".into(),
        tool_call_id: "tool".into(),
        tool_title: "Run tests".into(),
        options: vec![
            PermissionOption {
                id: "allow".into(),
                name: "Allow".into(),
                kind: "allow_once".into(),
            },
            PermissionOption {
                id: "reject".into(),
                name: "Reject".into(),
                kind: "reject_once".into(),
            },
        ],
    });
    let mut registry = FleetRegistry::new();
    registry.observe_external(ExternalSession {
        key: row.clone(),
        backend: FleetBackend::Acp,
        agent: "Example".into(),
        title: "Task".into(),
        model: None,
        activity: None,
        state: RowState::Working,
        outcome: None,
        used: Some(24),
        size: Some(100),
        cost: Some((1.5, "EUR".into())),
        needs: vec![],
        at: 1.0,
    });
    let mut view = FleetViewState::new(None, 1.0);
    fleet_view::update(
        &mut view,
        FleetViewInput::Snapshot {
            version: 1,
            rows: registry.rows().into_iter().cloned().collect(),
            queue: vec![],
            profiles: vec![],
            scope_note: None,
            link_note: None,
        },
        20,
    );
    fleet_view::update(&mut view, FleetViewInput::Action(FleetAction::Peek), 20);
    fleet_view::update(
        &mut view,
        FleetViewInput::Peek {
            key: row,
            status: PeekStatus::Open {
                has_open_prompt: true,
                running: true,
            },
        },
        20,
    );
    let area = Rect::new(0, 0, 100, 24);
    let mut buf = Buffer::empty(area);
    FleetView::draw_with_sources(
        &view,
        None,
        Some(ExternalPeekPanel {
            mapping: &mapping,
            selected_option: 1,
            now: 1.0,
        }),
        area,
        &mut buf,
        &FleetTheme::default(),
        &BTreeMap::new(),
    );
    let screen = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(screen.contains("Agent says hello"));
    assert!(screen.contains("ctx 24/100 - cost 1.5 EUR"));
    assert!(screen.contains("mode: review - commands: explain, test"));
    assert!(screen.contains("Permission: Run tests"));
    assert!(screen.contains("> 2. Reject"));
}
