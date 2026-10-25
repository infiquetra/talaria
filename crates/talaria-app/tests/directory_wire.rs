//! Red-run tests 11 to 13 (#231): opening and listing over the side connection.

#[path = "support/mock_gateway.rs"]
mod mock_gateway;

use mock_gateway::{StandaloneGateway, list_row};
use serde_json::{Value, json};
use talaria_app::app::AppMessage;
use talaria_app::directory::{DirectoryService, ExportTarget};
use talaria_app::side_connection::SideScope;
use talaria_core::directory::export::ExportFormat;
use talaria_core::directory::rows::{RowKey, RowOrigin};
use talaria_core::mutation::MutationLedger;

fn rows_for(profile: &str) -> Vec<Value> {
    vec![
        list_row(
            &format!("{profile}-live-1"),
            &format!("{profile} live one"),
            Some(1_792_700_000.0),
            Some("minimax-m3.1-flash"),
            4,
            false,
        ),
        list_row(
            &format!("{profile}-live-2"),
            &format!("{profile} live two"),
            Some(1_792_600_000.0),
            None,
            30,
            true,
        ),
    ]
}

fn stored_for(profile: &str) -> Vec<Value> {
    vec![list_row(
        &format!("{profile}-stored-1"),
        &format!("{profile} earlier one"),
        Some(1_792_000_000.0),
        Some("minimax-m3.1-flash"),
        12,
        false,
    )]
}

fn seed_lists(gateway: &StandaloneGateway) {
    for profile in ["default", "fixture"] {
        gateway.set_list_rows_for(profile, "live", rows_for(profile));
        gateway.set_list_rows_for(profile, "stored", stored_for(profile));
    }
}

fn sorted_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

#[tokio::test]
async fn host_scope_lists_every_profile_in_parallel() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    seed_lists(&gateway);
    gateway.set_behavior(
        "session.list",
        mock_gateway::Behavior::Delay(std::time::Duration::from_millis(500)),
    );

    let mut service = DirectoryService::new();
    let scope = service
        .open(home.path(), "fixture")
        .await
        .expect("the side connection opens");
    assert_eq!(
        scope,
        SideScope::Host {
            profiles: vec!["default".to_owned(), "fixture".to_owned()],
        }
    );

    let tickets = gateway.ticket_records();
    assert_eq!(tickets.len(), 1);
    assert_eq!(tickets[0].scope, "host");
    assert!(
        tickets[0].profile_id.ends_with("profiles/fixture"),
        "the ticket names the foreground profile: {}",
        tickets[0].profile_id
    );

    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let generation = service.start_list(tx);
    assert_eq!(generation, 1);

    // A server request on the side connection is refused at once, before the
    // delayed lists complete.
    let kinds = mock_gateway::contract_server_request_kinds();
    let id = gateway.server_request(&kinds[0]).await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while gateway.answers_for(&id).is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "the refusal arrives before the lists complete"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        gateway.answers_for(&id)[0].is_error,
        "a side request is refused"
    );

    let mut listed = 0;
    while listed < 4 {
        let message = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .expect("lists arrive")
            .expect("the channel stays open");
        assert!(
            matches!(message, AppMessage::Picker(_)),
            "lists arrive as picker messages"
        );
        listed += 1;
    }

    let frames = gateway.requests("session.list");
    assert_eq!(frames.len(), 4);
    let mut seen = Vec::new();
    for frame in &frames {
        assert_eq!(sorted_keys(&frame.params), ["limit", "profile", "scope"]);
        assert_eq!(frame.params["limit"], json!(100));
        seen.push((
            frame.params["profile"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            frame.params["scope"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        ));
    }
    seen.sort();
    assert_eq!(
        seen,
        [
            ("default".to_owned(), "live".to_owned()),
            ("default".to_owned(), "stored".to_owned()),
            ("fixture".to_owned(), "live".to_owned()),
            ("fixture".to_owned(), "stored".to_owned()),
        ]
    );
}

#[tokio::test]
async fn refused_host_scope_falls_back_to_the_foreground_profile() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    gateway.set_refuse_host_scope(true);
    seed_lists(&gateway);

    let mut service = DirectoryService::new();
    let scope = service
        .open(home.path(), "fixture")
        .await
        .expect("the fallback opens");
    assert_eq!(
        scope,
        SideScope::ProfileOnly {
            profile: "fixture".to_owned(),
            why: "ticket refused".to_owned(),
        }
    );
    let tickets = gateway.ticket_records();
    assert_eq!(tickets.len(), 2);
    assert_eq!(tickets[0].scope, "host");
    assert_eq!(tickets[1].scope, "profile");

    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    service.start_list(tx);
    for _ in 0..2 {
        tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .expect("lists arrive");
    }
    let frames = gateway.requests("session.list");
    assert_eq!(frames.len(), 2);
    for frame in &frames {
        assert_eq!(sorted_keys(&frame.params), ["limit", "scope"]);
        assert!(frame.params.get("profile").is_none());
    }

    let mut ledger = MutationLedger::new("launch-1");
    let renamed = service
        .rename(
            &mut ledger,
            &RowKey {
                profile: "fixture".to_owned(),
                session_id: "fixture-live-1".to_owned(),
            },
            "New title",
        )
        .await;
    let _ = renamed;
    let mutate = gateway.requests("session.mutate");
    assert_eq!(mutate.len(), 1);
    assert!(mutate[0].params.get("profile").is_none());

    let folder = tempfile::tempdir().expect("an export folder");
    let exported = service
        .export(
            &RowKey {
                profile: "fixture".to_owned(),
                session_id: "fixture-live-1".to_owned(),
            },
            ExportFormat::Markdown,
            true,
            ExportTarget::Auto {
                folder: folder.path().to_owned(),
            },
        )
        .await;
    let _ = exported;
    let exports = gateway.requests("session.export");
    assert_eq!(exports.len(), 1);
    assert!(exports[0].params.get("profile").is_none());
}

#[tokio::test]
async fn one_failing_profile_keeps_the_others() {
    let home = tempfile::tempdir().expect("a home");
    let gateway = StandaloneGateway::start(home.path());
    seed_lists(&gateway);
    gateway.set_list_error_for("fixture", "live", Some("profile_mismatch"));

    let mut service = DirectoryService::new();
    service
        .open(home.path(), "default")
        .await
        .expect("the side connection opens");
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    service.start_list(tx.clone());

    let mut listed = 0;
    let mut failed = 0;
    while listed + failed < 4 {
        let message = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .expect("replies arrive")
            .expect("the channel stays open");
        match message {
            AppMessage::Picker(talaria_core::directory::state::PickerInput::ListFailed {
                profile,
                origin,
                reason,
                ..
            }) => {
                assert_eq!(profile, "fixture");
                assert_eq!(origin, RowOrigin::Live);
                assert_eq!(reason, "profile_mismatch");
                failed += 1;
            }
            AppMessage::Picker(_) => listed += 1,
            _ => panic!("a list reply or failure arrives, not another message"),
        }
    }
    assert_eq!((listed, failed), (3, 1));

    // A refresh lists all four again under a new generation.
    service.start_list(tx);
    let mut second = 0;
    while second < 4 {
        tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
            .await
            .expect("replies arrive");
        second += 1;
    }
    assert_eq!(gateway.requests("session.list").len(), 8);
}
