//! Operator-configured real agent acceptance.

use std::path::PathBuf;
use std::time::{Duration, Instant};
use talaria_app::acp::{AcpConfig, AcpService};
use talaria_app::fleet::{FleetConfig, FleetService};
use talaria_core::fleet::RowState;
use talaria_core::fleet::acp::AcpCell;

#[tokio::test]
#[ignore = "gate: operator-configured agent"]
async fn live_acp_session_reaches_the_fleet() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        println!("LIVE_ACP_RESULT: SKIPPED (configuration home is unavailable)");
        return;
    };
    let Ok(text) = std::fs::read_to_string(home.join(".talaria/config.toml")) else {
        println!("LIVE_ACP_RESULT: SKIPPED (no agent configuration)");
        return;
    };
    if !text.contains("[[acp.agents]]") {
        println!("LIVE_ACP_RESULT: SKIPPED (no agent configured)");
        return;
    }
    let config = AcpConfig::parse(&text).expect("agent configuration");
    let Some(entry) = config.agents.first() else {
        println!("LIVE_ACP_RESULT: SKIPPED (no usable agent configured)");
        return;
    };
    let id = entry.id.clone();
    let fleet = FleetService::start(home, "fixture".into(), FleetConfig::default(), |_| {});
    let mut service = AcpService::start(config, fleet);
    let cwd = std::env::current_dir().expect("working directory");
    let key = service
        .new_session(&id, &cwd, "Reply with one short sentence.")
        .await
        .expect("live session");
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let snapshot = service.fleet_snapshot();
        if let Some(row) = snapshot.rows.iter().find(|row| {
            row.key.profile == format!("acp:{id}") && row.key.session_id == key.session_id
        }) {
            if row.state == RowState::Done {
                break;
            }
            assert_ne!(row.state, RowState::Failed, "live session failed");
        }
        assert!(Instant::now() < deadline, "live session timed out");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        service
            .transcript(&key)
            .iter()
            .any(|cell| matches!(cell,AcpCell::Text(text) if !text.is_empty()))
    );
    service.close().await;
    println!("LIVE_ACP_RESULT: PASSED agent={id}");
}
