//! A terminal agent's visible output is rendered without a Hermes session.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::fleet::FleetRegistry;
use talaria_core::fleet::herdr::{HerdrPane, project};
use talaria_core::fleet_view::{
    FleetAction, FleetViewEffect, FleetViewInput, FleetViewState, Focus, update,
};
use talaria_tui::fleet_view::{FleetTheme, FleetView};

#[test]
fn terminal_peek_renders_sanitized_output_and_opens_reply_editor() {
    let pane = HerdrPane {
        pane_id: "wfixture:p1".into(),
        workspace_id: "wfixture".into(),
        tab_id: "wfixture:t1".into(),
        agent: "codex".into(),
        name: "fixture".into(),
        status: "blocked".into(),
        title: "Task".into(),
    };
    let mut registry = FleetRegistry::new();
    let projection = project(&pane, "hello\u{1b}[31mworld", 1.0);
    let key = projection.session.key.clone();
    let text = "hello\u{1b}[31mworld".to_owned();
    registry.observe_herdr(projection);
    let mut view = FleetViewState::new(None, 1.0);
    update(
        &mut view,
        FleetViewInput::Snapshot {
            version: 1,
            rows: registry.rows().into_iter().cloned().collect(),
            queue: registry.queue().into_iter().cloned().collect(),
            profiles: vec![],
            scope_note: None,
            link_note: None,
        },
        4,
    );
    assert_eq!(
        update(&mut view, FleetViewInput::Action(FleetAction::Peek), 4),
        vec![FleetViewEffect::OpenPeek(key.clone())]
    );
    update(
        &mut view,
        FleetViewInput::ExternalPeek {
            key: key.clone(),
            text,
            has_open_prompt: true,
            running: false,
        },
        4,
    );
    let area = Rect::new(0, 0, 100, 20);
    let mut buf = Buffer::empty(area);
    FleetView::draw(&view, None, area, &mut buf, &FleetTheme::default());
    let output = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(output.contains("hello␛[31mworld"));
    assert!(!output.contains('\u{1b}'));
    update(&mut view, FleetViewInput::Action(FleetAction::Answer), 4);
    assert_eq!(view.focus(), &Focus::Reply);
    update(&mut view, FleetViewInput::Text('y'), 4);
    assert_eq!(
        update(&mut view, FleetViewInput::Enter, 4),
        vec![FleetViewEffect::SendReply {
            key,
            text: "y".into()
        }]
    );
}
