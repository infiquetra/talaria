//! The rescan control verb. Issue #253.

mod support;

use std::time::Duration;

use hermes_client::control_verbs::{
    ControlVerbError, RescanOutcome, rescan_profiles, scrub_reason, served_and_parked,
};
use hermes_client::discovery::Endpoint;
use serde_json::{Value, json};
use support::{ControlReply, Home, WsPlan, bind_stale, serve};

fn rescan_reply(result: Value) -> String {
    json!({"ok": true, "protocol": 1, "id": 1, "result": result}).to_string()
}

#[tokio::test]
async fn rescan_request_and_answers() {
    // The request line and a finished reconcile.
    let home = Home::new();
    let socket = home.path().join("gateway.sock");
    let full = rescan_reply(json!({
        "multiplex": true,
        "served_profiles": ["default", "fixture"],
        "added": ["fixture-new"],
        "removed": [],
        "rescanned": ["fixture"],
        "parked": ["fixture-b"],
        "reason": "control-socket",
    }));
    let mock = serve(&socket, |_| vec![ControlReply::Line(full)], WsPlan::Idle).await;
    let outcome = rescan_profiles(home.path(), Duration::from_secs(10))
        .await
        .expect("a finished rescan");
    assert_eq!(
        outcome,
        RescanOutcome {
            multiplex: true,
            added: vec!["fixture-new".to_owned()],
            removed: vec![],
            parked: vec!["fixture-b".to_owned()],
            served: vec!["default".to_owned(), "fixture".to_owned()],
            pending: false,
        }
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&requests[0]).expect("the request is JSON"),
        json!({"protocol": 1, "verb": "rescan-profiles", "id": 1})
    );

    // A pending answer carries three keys; absent lists read as empty.
    let home = Home::new();
    let socket = home.path().join("gateway.sock");
    let pending = rescan_reply(json!({
        "multiplex": true,
        "pending": true,
        "served_profiles": ["default"],
    }));
    let _mock = serve(&socket, |_| vec![ControlReply::Line(pending)], WsPlan::Idle).await;
    let outcome = rescan_profiles(home.path(), Duration::from_secs(10))
        .await
        .expect("a pending rescan");
    assert!(outcome.pending);
    assert!(outcome.added.is_empty());
    assert_eq!(outcome.served, vec!["default".to_owned()]);

    // A gateway that serves one profile answers `multiplex: false`.
    let home = Home::new();
    let socket = home.path().join("gateway.sock");
    let single = rescan_reply(json!({
        "multiplex": false,
        "served_profiles": ["default"],
        "added": [],
        "removed": [],
        "parked": [],
    }));
    let _mock = serve(&socket, |_| vec![ControlReply::Line(single)], WsPlan::Idle).await;
    let outcome = rescan_profiles(home.path(), Duration::from_secs(10))
        .await
        .expect("a single-profile rescan");
    assert!(!outcome.multiplex);
    assert!(!outcome.pending);

    // An old gateway answers `unknown verb`.
    let home = Home::new();
    let socket = home.path().join("gateway.sock");
    let old = json!({
        "ok": false,
        "protocol": 1,
        "id": 1,
        "error": "unknown verb: 'rescan-profiles'",
        "supported_verbs": ["identify"],
    })
    .to_string();
    let _mock = serve(&socket, |_| vec![ControlReply::Line(old)], WsPlan::Idle).await;
    assert!(
        matches!(
            rescan_profiles(home.path(), Duration::from_secs(10)).await,
            Err(ControlVerbError::UnknownVerb)
        ),
        "an unknown verb maps exactly"
    );

    // A silent gateway times out.
    let home = Home::new();
    let socket = home.path().join("gateway.sock");
    let _mock = serve(&socket, |_| vec![ControlReply::Silence], WsPlan::Idle).await;
    assert!(matches!(
        rescan_profiles(home.path(), Duration::from_millis(100)).await,
        Err(ControlVerbError::Timeout)
    ));

    // A socket that fails the safety check is reported, never used.
    let home = Home::new();
    bind_stale(&home.path().join("gateway.sock"), 0o644);
    assert!(matches!(
        rescan_profiles(home.path(), Duration::from_secs(10)).await,
        Err(ControlVerbError::UnsafeNode { .. })
    ));

    // Served names and scrubbed parked pairs from one endpoint.
    let home = Home::new();
    let fixture = home.path().join("profiles").join("fixture");
    std::fs::create_dir_all(&fixture).expect("a fixture profile home");
    let root = home.path().to_string_lossy().into_owned();
    let endpoint = Endpoint::from_identify(
        &json!({
            "runtime_protocol": 1,
            "state": "ready",
            "capabilities": ["session-authority-v1"],
            "authority_epoch": 3,
            "instance_id": "0123456789abcdef0123456789abcdef",
            "parked_profiles": {
                "fixture-b": format!("session store unusable: {root}/profiles/fixture-b/state.db"),
            },
            "api_origin": "http://127.0.0.1:9",
            "supervisor": "none",
            "served_profiles": [
                {"profile_id": root, "home": root},
                {
                    "profile_id": fixture.to_string_lossy(),
                    "home": fixture.to_string_lossy(),
                },
            ],
        }),
        home.path(),
    )
    .expect("the identify result decodes");
    let (served, parked) = served_and_parked(&endpoint, home.path());
    assert_eq!(served, ["default", "fixture"]);
    assert_eq!(
        parked,
        [(
            "fixture-b".to_owned(),
            "session store unusable: <home>/profiles/fixture-b/state.db".to_owned()
        )]
    );

    // Reasons scrub a fake home, cap at 500 characters and hide other paths.
    let long = scrub_reason(
        &format!("{} broke /etc/hostname {}", root, "x".repeat(600)),
        home.path(),
    );
    assert!(long.starts_with("<home> broke <path> "), "{long:?}");
    assert_eq!(long.chars().count(), 500);
    assert_eq!(scrub_reason("plain words", home.path()), "plain words");
}
