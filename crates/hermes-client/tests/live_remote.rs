//! Operator-provided remote gateway round trip.

#[path = "remote_round_trip.rs"]
mod remote_round_trip;

use hermes_client::remote::{
    RemoteConfig, RemoteName, TokenStore, connect, native_auth, profile_injector, status, ws_ticket,
};
use hermes_client::{Client, Inbound, Recorder, RecorderOptions, rpc::CallOptions};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use url::Url;

fn skip(reason: &str) {
    println!("LIVE_REMOTE_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").as_deref() == Ok("1") {
        panic!("a live remote was required");
    }
}
fn config_dir() -> std::path::PathBuf {
    std::env::var_os("TALARIA_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".talaria")))
        .expect("configuration folder")
}
fn read_config(home: &std::path::Path, name: &str) -> RemoteConfig {
    let content = std::fs::read_to_string(home.join("remotes.toml")).expect("remote configuration");
    let document: toml::Value = toml::from_str(&content).expect("configuration syntax");
    let row = &document["remotes"][name];
    let url = Url::parse(row["url"].as_str().expect("remote URL")).expect("URL");
    RemoteConfig {
        name: RemoteName::parse(name).expect("valid remote name"),
        base_url: url,
        provider: row
            .get("provider")
            .and_then(toml::Value::as_str)
            .map(str::to_owned),
    }
}

#[tokio::test]
#[ignore]
async fn live_remote_attach_and_round_trip() {
    let Some(name) = std::env::var_os("TALARIA_LIVE_REMOTE").filter(|v| !v.is_empty()) else {
        skip("TALARIA_LIVE_REMOTE not set; operator precondition missing");
        return;
    };
    let name = name.to_string_lossy();
    let home = config_dir();
    let cfg = read_config(&home, &name);
    let store = TokenStore::new(&home, cfg.name.clone());
    let started = Instant::now();
    let gate = status::probe_status(&cfg.base_url)
        .await
        .expect("gated status");
    assert!(gate.auth_required && gate.auth_flows.iter().any(|f| f == "native_pkce"));
    println!("probe_ms={}", started.elapsed().as_millis());
    let began = Instant::now();
    let tokens = store.load().expect("token store").expect("signed in");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    if now >= tokens.expires_at - 60 {
        native_auth::refresh(&cfg, &store).await.expect("refresh");
    }
    println!("refresh_ms={}", began.elapsed().as_millis());
    let began = Instant::now();
    let ticket = ws_ticket::mint_ws_ticket(&cfg, &store)
        .await
        .expect("ticket");
    println!("ticket_ms={}", began.elapsed().as_millis());
    let began = Instant::now();
    let mut connection = connect::connect_remote(&cfg, ticket)
        .await
        .expect("upgrade");
    println!("upgrade_ms={}", began.elapsed().as_millis());
    let output = tempfile::tempdir().expect("recording dir");
    let record = output.path().join("remote.jsonl");
    let recorder = Recorder::create(
        &record,
        RecorderOptions {
            endpoint: cfg.base_url.to_string(),
            substitutions: vec![(
                cfg.base_url.host_str().unwrap().to_owned(),
                "gateway.example.com".into(),
            )],
            ..Default::default()
        },
    )
    .expect("recorder");
    connection.set_recorder(recorder.clone(), 1);
    let began = Instant::now();
    let handshake = hermes_client::handshake(&mut connection)
        .await
        .expect("handshake");
    assert_eq!(handshake.ready.session_authority, Some(true));
    println!("handshake_ms={}", began.elapsed().as_millis());
    let (client, mut inbound) = Client::start(connection.into_parts());
    client.set_param_injector(Some(profile_injector("fixture".into())));
    let call = CallOptions {
        timeout: Some(Duration::from_secs(120)),
    };
    let created = client
        .call_raw(
            "session.create",
            json!({"request_id":"live-remote-check","source":"oneshot"}),
            call,
        )
        .await
        .expect("create fixture session");
    let session = created["session_id"].as_str().expect("session").to_owned();
    let began = Instant::now();
    let admitted=client.call_raw("prompt.submit",json!({"session_id":session,"text":"Reply with only the word ok.","submission_id":"talaria-live-remote-1","finite":true}),call).await.expect("submit");
    let admission = admitted["admission_id"]
        .as_str()
        .expect("admission")
        .to_owned();
    tokio::time::timeout(Duration::from_secs(120), async {
        let mut first = false;
        loop {
            match inbound.next().await {
                Some(Inbound::Event(event)) => {
                    let frame = event.raw;
                    if !first && frame["type"] == "message.delta" {
                        first = true;
                        println!("first_token_ms={}", began.elapsed().as_millis());
                    }
                    if frame["type"] == "message.complete" && frame["admission_id"] == admission {
                        assert!(remote_round_trip::matches_complete(
                            &frame, &session, &admission, "ok"
                        ));
                        assert!(first);
                        break;
                    }
                }
                Some(_) => {}
                None => panic!("remote closed before completion"),
            }
        }
    })
    .await
    .expect("finite round trip");
    client.close().await;
    recorder.finish().expect("finish recording");
    let log = std::fs::read_to_string(record).expect("recording");
    assert!(hermes_client::scan_for_secrets(&log).is_empty());
    let submits: Vec<Value> = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| row["dir"] == "out" && row["frame"]["method"] == "prompt.submit")
        .collect();
    assert_eq!(submits.len(), 1);
    assert!(submits[0]["frame"]["params"].get("unattended").is_none());
    println!("LIVE_REMOTE_RESULT: PASSED");
}
