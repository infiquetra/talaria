//! Tests 18 and 19 (#237): the permissions service wire shapes and failures.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use std::sync::Arc;
use std::time::Duration;

use hermes_client::{CallError, CallOptions, RpcError};
use hermes_contract::methods::canonical::config_get;
use hermes_contract::schemas::ConfigGetParams;
use mock_gateway::{Behavior, MockGateway};
use serde_json::json;
use talaria_app::permissions::PermissionsService;
use talaria_core::permissions::{ApprovalMode, CallFailure, PermissionsEffect, PermissionsInput};
use talaria_core::session::NoticeKind;

const SID: &str = "s-1";

fn get_params(key: &str, session: Option<&str>) -> ConfigGetParams {
    ConfigGetParams {
        cwd: None,
        key: key.to_owned(),
        profile: None,
        session_id: session.map(str::to_owned),
    }
}

#[tokio::test]
async fn wire_params_results_and_traps() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let service = PermissionsService::new(Arc::new(client));

    // The profile read sends only the key and decodes the configured mode.
    let input = service
        .execute(PermissionsEffect::ReadProfileMode { generation: 0 })
        .await;
    assert_eq!(
        input,
        Some(PermissionsInput::ProfileModeRead {
            generation: 0,
            result: Ok(ApprovalMode::Manual),
        })
    );
    let frames = mock.requests("config.get");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].params, json!({"key": "approvals.mode"}));
    mock.set_approvals_mode("smart");
    let input = service
        .execute(PermissionsEffect::ReadProfileMode { generation: 1 })
        .await;
    assert_eq!(
        input,
        Some(PermissionsInput::ProfileModeRead {
            generation: 1,
            result: Ok(ApprovalMode::Smart),
        })
    );

    // The bypass read names its session and decodes the stored bypass.
    let input = service
        .execute(PermissionsEffect::ReadSessionOff {
            session_id: SID.to_owned(),
            generation: 2,
        })
        .await;
    assert_eq!(
        input,
        Some(PermissionsInput::SessionOffRead {
            session_id: SID.to_owned(),
            generation: 2,
            result: Ok(false),
        })
    );
    let frames = mock.requests("config.get");
    assert_eq!(frames[2].params, json!({"session_id": SID, "key": "yolo"}));
    mock.set_session_bypass(SID, true);
    let input = service
        .execute(PermissionsEffect::ReadSessionOff {
            session_id: SID.to_owned(),
            generation: 3,
        })
        .await;
    assert_eq!(
        input,
        Some(PermissionsInput::SessionOffRead {
            session_id: SID.to_owned(),
            generation: 3,
            result: Ok(true),
        })
    );

    // The bypass write sends words and stores them.
    let input = service
        .execute(PermissionsEffect::SetSessionOff {
            session_id: SID.to_owned(),
            generation: 4,
            off: false,
        })
        .await;
    assert_eq!(
        input,
        Some(PermissionsInput::SessionOffSet {
            session_id: SID.to_owned(),
            generation: 4,
            sent: false,
            result: Ok(false),
        })
    );
    let frames = mock.requests("config.set");
    assert_eq!(frames.len(), 1);
    assert_eq!(
        frames[0].params,
        json!({"session_id": SID, "key": "yolo", "value": "0"})
    );
    let input = service
        .execute(PermissionsEffect::SetSessionOff {
            session_id: SID.to_owned(),
            generation: 5,
            off: true,
        })
        .await;
    assert_eq!(
        input,
        Some(PermissionsInput::SessionOffSet {
            session_id: SID.to_owned(),
            generation: 5,
            sent: true,
            result: Ok(true),
        })
    );
    let frames = mock.requests("config.set");
    assert_eq!(
        frames[1].params,
        json!({"session_id": SID, "key": "yolo", "value": "1"})
    );
    let input = service
        .execute(PermissionsEffect::ReadSessionOff {
            session_id: SID.to_owned(),
            generation: 6,
        })
        .await;
    assert_eq!(
        input,
        Some(PermissionsInput::SessionOffRead {
            session_id: SID.to_owned(),
            generation: 6,
            result: Ok(true),
        })
    );

    // The profile write sends the word without a session and stores it.
    let input = service
        .execute(PermissionsEffect::SetProfileMode {
            generation: 7,
            mode: ApprovalMode::Manual,
        })
        .await;
    assert_eq!(
        input,
        Some(PermissionsInput::ProfileModeSet {
            generation: 7,
            sent: ApprovalMode::Manual,
            result: Ok(ApprovalMode::Manual),
        })
    );
    let frames = mock.requests("config.set");
    assert_eq!(
        frames[2].params,
        json!({"key": "approvals.mode", "value": "manual"})
    );
    let input = service
        .execute(PermissionsEffect::ReadProfileMode { generation: 8 })
        .await;
    assert_eq!(
        input,
        Some(PermissionsInput::ProfileModeRead {
            generation: 8,
            result: Ok(ApprovalMode::Manual),
        })
    );

    // Notices and redraws need no call.
    let input = service
        .execute(PermissionsEffect::Notice {
            kind: NoticeKind::System,
            text: "shown".to_owned(),
        })
        .await;
    assert_eq!(input, None);
    let input = service.execute(PermissionsEffect::Redraw).await;
    assert_eq!(input, None);

    // No scope was ever sent, and neither trap ever fired.
    for frame in mock.seen() {
        if frame.method.as_deref() == Some("config.set") {
            assert!(
                frame.params.get("scope").is_none(),
                "a scope was sent: {}",
                frame.params
            );
        }
    }
    assert!(!mock.yolo_global_flip());
    assert!(!mock.yolo_toggle());
    mock.assert_no_violations();
}

fn rpc(code: i64, message: &str, reason: Option<&str>) -> CallError {
    CallError::Rpc(RpcError {
        code,
        message: message.to_owned(),
        reason: reason.map(str::to_owned),
        data: reason.map(|reason| json!({"reason": reason})),
    })
}

#[tokio::test]
async fn classify_every_call_error() {
    let (mock, client, _inbound) = MockGateway::start().await;
    let service = PermissionsService::new(Arc::new(client));

    // Calls that never left are not sent.
    assert_eq!(
        service.classify(&CallError::NotConnected),
        CallFailure::NotSent
    );
    assert_eq!(service.classify(&CallError::Encode), CallFailure::NotSent);
    assert_eq!(
        service.classify(&CallError::InvalidTimeout),
        CallFailure::NotSent
    );

    // Refusals keep code, reason and message.
    for (error, failure) in [
        (
            rpc(4001, "permission_denied", Some("permission_denied")),
            CallFailure::Refused {
                reason: "permission_denied".to_owned(),
                rpc_code: 4001,
                message: "permission_denied".to_owned(),
            },
        ),
        (
            rpc(4001, "invalid_params", Some("invalid_params")),
            CallFailure::Refused {
                reason: "invalid_params".to_owned(),
                rpc_code: 4001,
                message: "invalid_params".to_owned(),
            },
        ),
        (
            rpc(4001, "storage_unavailable", Some("storage_unavailable")),
            CallFailure::Refused {
                reason: "storage_unavailable".to_owned(),
                rpc_code: 4001,
                message: "storage_unavailable".to_owned(),
            },
        ),
        (
            rpc(5001, "storage_unavailable", Some("storage_unavailable")),
            CallFailure::Refused {
                reason: "storage_unavailable".to_owned(),
                rpc_code: 5001,
                message: "storage_unavailable".to_owned(),
            },
        ),
        (
            rpc(-32601, "unknown method: config.get", Some("unknown_method")),
            CallFailure::Refused {
                reason: "unknown_method".to_owned(),
                rpc_code: -32601,
                message: "unknown method: config.get".to_owned(),
            },
        ),
        (
            rpc(-32602, "boom", None),
            CallFailure::Refused {
                reason: String::new(),
                rpc_code: -32602,
                message: "boom".to_owned(),
            },
        ),
    ] {
        assert_eq!(service.classify(&error), failure);
    }

    // A reply that never arrives, or arrives unreadable, is unknown or unreadable.
    let (quiet, quiet_client, _inbound) = MockGateway::start().await;
    let quiet_service = PermissionsService::new(Arc::new(quiet_client.clone()));
    quiet.set_behavior("config.get", Behavior::Silent);
    let error = quiet_client
        .call_with::<config_get::Marker>(
            get_params("approvals.mode", None),
            CallOptions {
                timeout: Some(Duration::from_millis(50)),
            },
        )
        .await
        .expect_err("a silent gateway times out");
    assert!(matches!(error, CallError::TimedOut { .. }));
    assert_eq!(quiet_service.classify(&error), CallFailure::Unknown);

    let (dropped, dropped_client, _inbound) = MockGateway::start().await;
    let dropped_service = PermissionsService::new(Arc::new(dropped_client.clone()));
    dropped.set_behavior("config.get", Behavior::DropAfterFrame);
    let error = dropped_client
        .call::<config_get::Marker>(get_params("approvals.mode", None))
        .await
        .expect_err("a dropped socket is unknown");
    assert!(matches!(error, CallError::Unknown { .. }));
    assert_eq!(dropped_service.classify(&error), CallFailure::Unknown);

    // A boolean value where a word belongs is unreadable.
    let (mixed, mixed_client, _inbound) = MockGateway::start().await;
    let mixed_service = PermissionsService::new(Arc::new(mixed_client.clone()));
    mixed.queue_config_raw(
        "config.get",
        json!({"key": "yolo", "value": true, "scope": "session"}),
    );
    let error = mixed_client
        .call::<config_get::Marker>(get_params("yolo", Some(SID)))
        .await
        .expect_err("a boolean value does not decode");
    assert!(matches!(error, CallError::BadResult { .. }));
    assert_eq!(mixed_service.classify(&error), CallFailure::Unreadable);

    // A boolean set result decodes but still counts as unreadable.
    mixed.queue_config_raw(
        "config.set",
        json!({"key": "yolo", "value": true, "scope": "session"}),
    );
    let input = mixed_service
        .execute(PermissionsEffect::SetSessionOff {
            session_id: SID.to_owned(),
            generation: 0,
            off: true,
        })
        .await;
    assert_eq!(
        input,
        Some(PermissionsInput::SessionOffSet {
            session_id: SID.to_owned(),
            generation: 0,
            sent: true,
            result: Err(CallFailure::Unreadable),
        })
    );

    mock.assert_no_violations();
    quiet.assert_no_violations();
    mixed.assert_no_violations();
}
