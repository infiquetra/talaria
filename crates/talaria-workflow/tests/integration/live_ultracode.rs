#[test]
#[ignore = "requires operator-provisioned fixture gateway"]
fn live_roster_envelope_fixture() {
    let output = std::process::Command::new("talaria")
        .args(["workflow", "roster", "--profile", "fixture", "--json"])
        .output();
    let Ok(output) = output else {
        eprintln!("SKIPPED (talaria unavailable)");
        return;
    };
    if output.status.code() == Some(1) {
        eprintln!("SKIPPED (gateway not running)");
        return;
    }
    assert!([Some(0), Some(2)].contains(&output.status.code()));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["profile"], "fixture");
    assert!(
        value["report"]["providers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["slug"] == "ollama-cloud")
    );
}
#[tokio::test]
#[ignore = "requires operator-provisioned fixture gateway"]
async fn live_skill_directive_fixture() {
    use hermes_client::{
        CallOptions, ConnectOptions, DiscoverOptions, EnsurePolicy, Purpose, Scope, connect_with,
        default_home, discover_with, handshake, mint_ticket,
    };
    use serde_json::json;
    use std::{path::PathBuf, time::Duration};
    let home = std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home);
    let opts = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let Ok(endpoint) = discover_with(&home, &opts).await else {
        eprintln!("SKIPPED (gateway not running)");
        return;
    };
    let Ok(profile) = endpoint.resolve_profile("fixture") else {
        eprintln!("SKIPPED (fixture profile not served)");
        return;
    };
    let ticket = mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap();
    let mut connection = connect_with(&endpoint, ticket, &ConnectOptions::default())
        .await
        .unwrap();
    handshake(&mut connection).await.unwrap();
    let (client, _inbound) = hermes_client::Client::start(connection.into_parts());
    let dir = tempfile::tempdir().unwrap();
    let options = || CallOptions {
        timeout: Some(Duration::from_secs(30)),
    };
    let created=match client.call_raw("session.create",json!({"source":"cli","cwd":dir.path(),"model":"minimax-m3.1-flash","provider":"ollama-cloud"}),options()).await { Ok(v)=>v,Err(e)=>{eprintln!("SKIPPED (session could not be created: {e})");return;} };
    let id = created["session_id"].as_str().unwrap();
    let catalog = client
        .call_raw("commands.catalog", json!({"session_id":id}), options())
        .await
        .unwrap();
    let Some(skill) = catalog["skills"].get("/talaria-workflows") else {
        eprintln!("SKIPPED (talaria-workflows not installed in fixture)");
        return;
    };
    assert_eq!(skill["origin"], "local");
    let response=client.call_raw("slash.exec",json!({"session_id":id,"command":"talaria-workflows What is 2 + 2? Answer in one word."}),options()).await.unwrap();
    assert_eq!(response["type"], "skill");
    assert_eq!(response["name"], "talaria-workflows");
    assert!(
        response["display"]
            .as_str()
            .unwrap()
            .starts_with("/talaria-workflows")
    );
}
