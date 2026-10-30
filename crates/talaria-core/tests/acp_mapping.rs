//! Pure agent-session mapping.

use talaria_core::fleet::acp::{
    AcpCell, AcpMapping, AcpSessionKey, AcpUpdate, AcpUsage, PermissionOption, PermissionRequest,
};
use talaria_core::fleet::{Outcome, RowState};

fn key() -> AcpSessionKey {
    AcpSessionKey {
        agent_id: "fixture".into(),
        session_id: "session-1".into(),
    }
}

fn message(text: &str) -> AcpUpdate {
    AcpUpdate::MessageChunk {
        key: key(),
        text: text.into(),
    }
}

fn tool() -> AcpUpdate {
    AcpUpdate::ToolCall {
        key: key(),
        id: "tool-1".into(),
        title: "Run".into(),
    }
}

fn request(responder: &str) -> PermissionRequest {
    PermissionRequest {
        key: key(),
        responder: responder.into(),
        tool_call_id: "tool-1".into(),
        tool_title: "Run".into(),
        options: vec![
            PermissionOption {
                id: "once".into(),
                name: "Allow once".into(),
                kind: "allow_once".into(),
            },
            PermissionOption {
                id: "always".into(),
                name: "Allow always".into(),
                kind: "allow_always".into(),
            },
            PermissionOption {
                id: "reject".into(),
                name: "Reject".into(),
                kind: "reject_once".into(),
            },
        ],
    }
}

#[test]
fn message_chunks_append_to_transcript() {
    let mut mapping = AcpMapping::default();
    mapping.start_turn();
    mapping.apply(message("hello"));
    mapping.apply(message(" world"));
    assert_eq!(
        mapping.transcript,
        vec![AcpCell::Text("hello world".into())]
    );
    mapping.apply(tool());
    mapping.apply(message("again"));
    assert_eq!(mapping.transcript.len(), 3);
    assert_eq!(mapping.transcript[2], AcpCell::Text("again".into()));
}

#[test]
fn tool_call_and_update_render_as_one_cell() {
    let mut mapping = AcpMapping::default();
    mapping.start_turn();
    mapping.apply(tool());
    mapping.apply(AcpUpdate::ToolCallPatch {
        key: key(),
        id: "tool-1".into(),
        title: None,
        body: Some("partial".into()),
        status: None,
    });
    mapping.apply(AcpUpdate::ToolCallPatch {
        key: key(),
        id: "tool-1".into(),
        title: Some("Compile".into()),
        body: None,
        status: None,
    });
    mapping.apply(AcpUpdate::ToolCallPatch {
        key: key(),
        id: "tool-1".into(),
        title: None,
        body: None,
        status: Some("completed".into()),
    });
    assert_eq!(
        mapping.transcript,
        vec![AcpCell::Tool {
            id: "tool-1".into(),
            title: "Compile".into(),
            body: "partial".into(),
            status: Some("completed".into())
        }]
    );
}

#[test]
fn update_kinds_map_to_row_states() {
    for (reason, outcome) in [
        ("end_turn", Outcome::Completed),
        ("max_tokens", Outcome::Completed),
        ("max_turn_requests", Outcome::Completed),
        ("refusal", Outcome::Unknown),
        ("cancelled", Outcome::Interrupted),
        ("future_reason", Outcome::Unknown),
    ] {
        let mut mapping = AcpMapping::default();
        mapping.start_turn();
        mapping.apply(message("working"));
        mapping.apply(AcpUpdate::ThoughtChunk {
            key: key(),
            text: "thinking".into(),
        });
        mapping.apply(tool());
        mapping.apply(AcpUpdate::ToolCallPatch {
            key: key(),
            id: "tool-1".into(),
            title: None,
            body: None,
            status: Some("failed".into()),
        });
        for update in [
            AcpUpdate::Plan {
                key: key(),
                summary: "retry".into(),
            },
            AcpUpdate::AvailableCommands {
                key: key(),
                names: vec!["check".into()],
            },
            AcpUpdate::Mode {
                key: key(),
                name: "default".into(),
            },
            AcpUpdate::ConfigOption {
                key: key(),
                name: "model".into(),
                value: "fixture".into(),
            },
            AcpUpdate::SessionInfo {
                key: key(),
                title: "Task".into(),
            },
            AcpUpdate::Notice {
                key: key(),
                text: "notice".into(),
            },
            AcpUpdate::Compaction {
                key: key(),
                text: "compacting".into(),
            },
            AcpUpdate::Usage {
                key: key(),
                usage: AcpUsage {
                    used: 1,
                    size: 10,
                    cost_amount: None,
                    cost_currency: None,
                },
            },
        ] {
            mapping.apply(update);
            assert_eq!(mapping.state, RowState::Working, "{reason}");
        }
        mapping.finish(reason);
        assert_eq!(mapping.state, RowState::Done, "{reason}");
        assert_eq!(mapping.outcome, Some(outcome), "{reason}");
        if reason == "future_reason" {
            assert_eq!(mapping.diagnostics.len(), 1);
        }
    }
}

#[test]
fn usage_merges_field_by_field() {
    let mut mapping = AcpMapping::default();
    mapping.apply(AcpUpdate::Usage {
        key: key(),
        usage: AcpUsage {
            used: 30,
            size: 100,
            cost_amount: Some(1.25),
            cost_currency: Some("EUR".into()),
        },
    });
    mapping.apply(AcpUpdate::Usage {
        key: key(),
        usage: AcpUsage {
            used: 40,
            size: 200,
            cost_amount: None,
            cost_currency: None,
        },
    });
    assert_eq!(
        mapping.usage,
        Some(AcpUsage {
            used: 40,
            size: 200,
            cost_amount: Some(1.25),
            cost_currency: Some("EUR".into())
        })
    );
}

#[test]
fn usage_missing_required_field_is_refused() {
    let mut mapping = AcpMapping::default();
    let original = AcpUsage {
        used: 20,
        size: 100,
        cost_amount: None,
        cost_currency: None,
    };
    mapping.apply(AcpUpdate::Usage {
        key: key(),
        usage: original.clone(),
    });
    mapping.apply(AcpUpdate::UsageFields {
        key: key(),
        used: None,
        size: Some(200),
        cost_amount: None,
        cost_currency: None,
    });
    mapping.apply(AcpUpdate::UsageFields {
        key: key(),
        used: Some(50),
        size: None,
        cost_amount: None,
        cost_currency: None,
    });
    assert_eq!(mapping.usage, Some(original));
    assert_eq!(mapping.diagnostics.len(), 1);
}

#[test]
fn unknown_update_kind_is_one_diagnostic() {
    let mut mapping = AcpMapping::default();
    for _ in 0..3 {
        mapping.apply(AcpUpdate::Unknown {
            key: key(),
            kind: "future_update".into(),
        });
    }
    assert_eq!(mapping.diagnostics.len(), 1);
    assert!(mapping.diagnostics[0].contains("future_update"));
}

#[test]
fn permission_request_becomes_one_queue_item() {
    let mut mapping = AcpMapping::default();
    let offered = request("generation-1/request-7");
    mapping.request_permission(offered.clone());
    assert_eq!(mapping.needs, vec![offered]);
    assert_eq!(mapping.needs[0].options.len(), 3);
    assert_eq!(mapping.needs[0].tool_call_id, "tool-1");
    assert_eq!(mapping.needs[0].responder, "generation-1/request-7");
}

#[test]
fn two_requests_for_one_tool_stay_distinct() {
    let mut mapping = AcpMapping::default();
    mapping.request_permission(request("generation-1/request-7"));
    mapping.request_permission(request("generation-1/request-8"));
    assert_eq!(mapping.needs.len(), 2);
    assert_eq!(mapping.needs[0].tool_call_id, mapping.needs[1].tool_call_id);
    assert_ne!(mapping.needs[0].responder, mapping.needs[1].responder);
}

#[test]
fn agent_text_is_sanitised() {
    let mut mapping = AcpMapping::default();
    mapping.apply(message("A\u{1b}B\u{7}C\u{202e}D"));
    assert_eq!(
        mapping.transcript,
        vec![AcpCell::Text("A␛B␇C‹U+202E›D".into())]
    );
}
