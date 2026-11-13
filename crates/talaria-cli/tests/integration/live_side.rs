//! Live shell and side questions against the operator's gateway.
//!
//! Skips naming the missing precondition; under `TALARIA_LIVE_REQUIRE=1` a
//! skip panics.

use talaria_app::connection::ConnectionService;
use talaria_app::launch::{Launch, StartMode, new_launch_id, new_request_id, plan};
use talaria_app::sessions::SessionService;
use talaria_app::side::SideService;
use talaria_core::session::{Effect, Event, SessionId};
use talaria_core::side::{RequestId, ShellOutcome};

fn live_require() -> bool {
    std::env::var("TALARIA_LIVE_REQUIRE").as_deref() == Ok("1")
}

fn skip(reason: &str) {
    println!("LIVE_SIDE_RESULT: SKIPPED ({reason})");
    if live_require() {
        panic!("a live run was required but skipped: {reason}");
    }
}

#[tokio::test]
#[ignore]
async fn live_shell_and_btw() {
    let work = tempfile::tempdir().expect("work dir");
    let launch = Launch {
        mode: StartMode::New,
        model: Some("minimax-m3.1-flash".to_string()),
        provider: Some("ollama-cloud".to_string()),
        effort: None,
        profile: "fixture".to_string(),
        folder: work.path().to_path_buf(),
        record: None,
        bench_replay: None,
        no_ensure: true,
    };
    let home = hermes_client::default_home();
    let connection = match ConnectionService::open(&launch, &home, None).await {
        Ok(connection) => connection,
        Err(error) => {
            skip(&format!("the gateway did not open: {}", error.what));
            return;
        }
    };
    let launch_id = new_launch_id();
    let service = SessionService::with_client(&launch_id, connection.link.client());
    let request = match plan(&launch, new_request_id(), &connection.describe) {
        Ok(request) => request,
        Err(_) => {
            skip("the gateway did not offer a session");
            return;
        }
    };
    let opened = match service.open(request).await {
        Ok(opened) => opened,
        Err(_) => {
            skip("no session opened on the fixture profile");
            return;
        }
    };
    let side = SideService::new(connection.link.client());
    let events = side
        .execute(
            Effect::RunShell {
                session_id: SessionId::new(opened.session_id.clone()),
                request_id: RequestId(1),
                command: "echo talaria-shell-ok".to_string(),
            },
            || 0,
        )
        .await;
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::ShellFinished {
            outcome: ShellOutcome::Ok { stdout, code, .. },
            ..
        } => {
            assert_eq!(*code, 0);
            assert!(stdout.contains("talaria-shell-ok"), "stdout was {stdout:?}");
        }
        other => panic!("the shell command did not run: {other:?}"),
    }
    let events = side
        .execute(
            Effect::AskSide {
                session_id: SessionId::new(opened.session_id.clone()),
                request_id: RequestId(2),
                question: "Reply with the single word ready.".to_string(),
            },
            || 0,
        )
        .await;
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], Event::SideAccepted { task_id, .. } if !task_id.is_empty()),
        "the side question was not accepted: {:?}",
        events[0]
    );
    println!("LIVE_SIDE_RESULT: PASSED");
}
