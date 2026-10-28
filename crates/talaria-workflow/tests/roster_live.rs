#![cfg(feature = "hermes-live")]
#[tokio::test]
#[ignore = "requires the operator-provisioned fixture gateway"]
async fn model_options_on_the_fixture_profile() {
    use hermes_client::{
        CallOptions, ConnectOptions, DiscoverOptions, EnsurePolicy, Purpose, Recorder,
        RecorderOptions, Scope, connect_with, default_home, discover_with, handshake,
        local_forbidden_literals, mint_ticket, scan_for_secrets, scan_literals,
    };
    use std::{
        io::Write,
        path::PathBuf,
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    use talaria_workflow::staffing::Roster;
    struct Shared(Arc<Mutex<Vec<u8>>>);
    impl Write for Shared {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn skip(reason: &str) {
        println!("P5-04-LIVE SKIPPED roster_live: {reason}");
        assert_ne!(
            std::env::var("TALARIA_REQUIRE_LIVE").ok().as_deref(),
            Some("1")
        );
    }
    let home = std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home);
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let gateway = match discover_with(&home, &options).await {
        Ok(v) => v,
        Err(_) => {
            skip("no Hermes gateway: start one and create the fixture profile (operator)");
            return;
        }
    };
    let profile = match gateway.resolve_profile("fixture") {
        Ok(v) => v,
        Err(_) => {
            skip("profile 'fixture' not found");
            return;
        }
    };
    let ticket = mint_ticket(
        &home,
        &gateway,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap();
    let mut connection = connect_with(&gateway, ticket, &ConnectOptions::default())
        .await
        .unwrap();
    let sink = Arc::new(Mutex::new(Vec::new()));
    let substitutions = local_forbidden_literals()
        .into_iter()
        .filter(|s| s.chars().count() >= 4)
        .map(|s| (s, "<local>".into()))
        .collect();
    let recorder = Recorder::from_writer(
        Shared(sink.clone()),
        RecorderOptions {
            endpoint: "ws://<gateway>/api/ws".into(),
            subprotocols: vec!["hermes-gateway-v1".into()],
            scenario: Some("p5-04-model-options".into()),
            substitutions,
            ..Default::default()
        },
    )
    .unwrap();
    connection.set_recorder(recorder.clone(), 1);
    handshake(&mut connection).await.unwrap();
    let (client, _inbound) = hermes_client::Client::start(connection.into_parts());
    let started = Instant::now();
    let result = client
        .call_raw(
            "model.options",
            serde_json::json!({}),
            CallOptions {
                timeout: Some(Duration::from_secs(15)),
            },
        )
        .await
        .unwrap();
    let elapsed = started.elapsed();
    let roster = Roster::from_model_options(&result, "fixture", chrono::Utc::now()).unwrap();
    assert!(roster.providers().iter().any(|p|p.slug=="ollama-cloud" && p.models.iter().any(|m|m=="minimax-m3.1-flash")));
    let summary = recorder.finish().unwrap();
    assert_eq!(summary.dropped, 0);
    let bytes = sink.lock().unwrap().clone();
    let text = String::from_utf8(bytes).unwrap();
    let mut rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for row in &mut rows {
        if let Some(providers) = row["frame"]["result"]["providers"].as_array_mut() {
            for provider in providers {
                if let Some(accounts) = provider["usage"]["accounts"].as_array_mut() {
                    for (i, account) in accounts.iter_mut().enumerate() {
                        if account.get("id").is_some() {
                            account["id"] = format!("fixture-account-{}", i + 1).into();
                        }
                        if account.get("label").is_some() {
                            account["label"] = format!("Fixture account {}", i + 1).into();
                        }
                    }
                }
            }
        }
    }
    let sanitized = rows
        .iter()
        .map(|row| serde_json::to_string(row).unwrap() + "\n")
        .collect::<String>();
    assert!(scan_for_secrets(&sanitized).is_empty());
    assert!(scan_literals(&sanitized, &local_forbidden_literals()).is_empty());
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/frames/p5-04-model-options.jsonl");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => file.write_all(sanitized.as_bytes()).unwrap(),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => panic!("fixture write failed: {e}"),
    };
    println!("P5-04-LIVE PASSED roster_live");
    eprintln!("elapsed: {} ms", elapsed.as_millis());
}
