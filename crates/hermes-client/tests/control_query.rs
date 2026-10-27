mod support;

use hermes_client::discovery::{
    DiscoverOptions, DiscoveryError, EnsurePolicy, ReadVerb, control_query,
};
use serde_json::json;
use std::time::Duration;
use support::{ControlReply, Home, WsPlan, serve, set_mode};

#[tokio::test]
async fn sends_only_read_verbs() {
    let home = Home::new();
    let socket = home.path().join("gateway.sock");
    let replies = vec![
        ControlReply::Line(
            json!({"ok": true, "protocol": 1, "id": 1, "result": {"state": "ready"}}).to_string(),
        ),
        ControlReply::Line(
            json!({"ok": true, "protocol": 1, "id": 1, "result": {"active_agents": 0}}).to_string(),
        ),
        ControlReply::Line(
            json!({"ok": false, "protocol": 1, "id": 1, "error": "refused"}).to_string(),
        ),
    ];
    let mock = serve(&socket, |_| replies, WsPlan::Idle).await;
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    assert_eq!(ReadVerb::Identify.as_str(), "identify");
    assert_eq!(ReadVerb::ActiveWork.as_str(), "active-work");
    let identify = control_query(home.path(), ReadVerb::Identify, &options).await;
    assert_eq!(identify.as_ref().ok(), Some(&json!({"state":"ready"})));
    let active = control_query(home.path(), ReadVerb::ActiveWork, &options).await;
    assert_eq!(active.as_ref().ok(), Some(&json!({"active_agents":0})));
    assert_eq!(
        mock.request_lines(),
        vec![
            "{\"protocol\":1,\"verb\":\"identify\",\"id\":1}\n".to_owned(),
            "{\"protocol\":1,\"verb\":\"active-work\",\"id\":1}\n".to_owned(),
        ]
    );
    let refused = control_query(home.path(), ReadVerb::ActiveWork, &options).await;
    assert!(matches!(
        refused,
        Err(DiscoveryError::ControlRefused { .. })
    ));
    assert_eq!(mock.request_lines().len(), 3);
    set_mode(&socket, 0o666);
    let untrusted = control_query(home.path(), ReadVerb::Identify, &options).await;
    assert!(matches!(untrusted, Err(DiscoveryError::UnsafeNode { .. })));
    assert_eq!(
        mock.request_lines().len(),
        3,
        "unsafe socket was never contacted"
    );
}
