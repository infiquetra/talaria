mod support;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper::{Request, client::conn::http1};
use hyper_util::rt::TokioIo;
use regex::Regex;
use serde_json::json;
use tokio::net::TcpStream;

async fn get_public(origin: &str, path: &str) -> String {
    let url = url::Url::parse(origin).expect("valid origin");
    let host = url.host_str().expect("origin host");
    let port = url.port_or_known_default().expect("origin port");
    let socket = TcpStream::connect((host, port)).await.expect("connect");
    let (mut sender, connection) = http1::handshake(TokioIo::new(socket))
        .await
        .expect("handshake");
    tokio::spawn(async move {
        connection.await.expect("connection");
    });
    let request = Request::builder()
        .uri(path)
        .header("host", format!("{host}:{port}"))
        .header("connection", "close")
        .body(Empty::<Bytes>::new())
        .expect("request");
    let response = sender.send_request(request).await.expect("response");
    assert_eq!(response.status(), 200);
    String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes()
            .to_vec(),
    )
    .expect("utf8")
}

fn replace_options(schema: &str, key: &str, options: serde_json::Value) -> String {
    let parsed: serde_json::Value = serde_json::from_str(schema).expect("schema JSON");
    if parsed["fields"].get(key).is_none() {
        return schema.to_owned();
    }
    let pattern = format!(
        r#"(\"{}\"\s*:\s*\{{[^{{}}]*?\"options\"\s*:\s*)\[(?:\s*\"(?:[^\"\\]|\\.)*\"\s*,?\s*)*\]"#,
        regex::escape(key)
    );
    let re = Regex::new(&pattern).expect("option pattern");
    assert_eq!(
        re.find_iter(schema).count(),
        1,
        "one options list for {key}"
    );
    re.replace(schema, |captures: &regex::Captures<'_>| {
        format!("{}{}", &captures[1], options)
    })
    .into_owned()
}

#[tokio::test]
#[ignore]
async fn record_schema_and_defaults() {
    let env = match support::live_preconditions().await {
        Ok(env) => env,
        Err(reason) => {
            support::report_skip("LIVE_SETTINGS_RESULT", &reason);
            return;
        }
    };
    let origin = env.endpoint.api_origin();
    let mut schema = get_public(origin, "/api/config/schema").await;
    for (key, options) in [
        (
            "tts.provider",
            json!([
                "edge",
                "elevenlabs",
                "openai",
                "xai",
                "minimax",
                "mistral",
                "gemini",
                "neutts",
                "kittentts",
                "piper"
            ]),
        ),
        (
            "stt.provider",
            json!([
                "local",
                "groq",
                "openai",
                "mistral",
                "xai",
                "elevenlabs",
                "deepinfra"
            ]),
        ),
        ("memory.provider", json!(["", "byterover", "retaindb"])),
        (
            "terminal.backend",
            json!([
                "local",
                "docker",
                "ssh",
                "modal",
                "daytona",
                "vercel_sandbox",
                "singularity"
            ]),
        ),
    ] {
        schema = replace_options(&schema, key, options);
    }
    let defaults = get_public(origin, "/api/config/defaults").await;
    let _: serde_json::Value = serde_json::from_str(&schema).expect("schema JSON");
    let _: serde_json::Value = serde_json::from_str(&defaults).expect("defaults JSON");
    let output = std::path::PathBuf::from(
        std::env::var_os("TALARIA_FIXTURE_OUT").expect("fixture destination"),
    );
    std::fs::create_dir_all(&output).expect("fixture directory");
    std::fs::write(output.join("config-schema.json"), schema).expect("schema fixture");
    std::fs::write(output.join("config-defaults.json"), defaults).expect("defaults fixture");
    println!("LIVE_SETTINGS_RESULT: PASSED");
}

fn fixture_client(env: support::LiveEnv) -> hermes_client::rest::RestClient {
    use hermes_client::rest::{NativeHttpMinter, RestClient, RestOptions};
    use std::sync::Arc;

    let origin = env.endpoint.api_origin().to_owned();
    let minter = NativeHttpMinter::new(env.home, Arc::new(env.endpoint), env.fixture);
    RestClient::new(&origin, Arc::new(minter), RestOptions::default()).expect("fixture client")
}

#[tokio::test]
#[ignore]
async fn live_reads_on_fixture() {
    use hermes_client::config_api::{config_defaults, config_read, config_schema};

    let env = match support::live_preconditions().await {
        Ok(env) => env,
        Err(reason) => {
            support::report_skip("LIVE_SETTINGS_RESULT", &reason);
            return;
        }
    };
    let client = fixture_client(env);
    let schema = config_schema(&client).await.expect("schema read");
    assert!(!schema.fields.is_empty(), "schema has fields");
    let defaults = config_defaults(&client).await.expect("defaults read");
    assert!(defaults.is_object(), "defaults are an object");
    let effective = config_read(&client, true).await.expect("effective read");
    assert!(
        effective.is_object(),
        "effective configuration is an object"
    );
    let raw = config_read(&client, false).await.expect("raw read");
    assert!(raw.is_object(), "raw configuration is an object");
    assert_eq!(
        client
            .probe_ticket_reuse("/api/config")
            .await
            .expect("reuse probe"),
        (200, 401)
    );
    println!("LIVE_SETTINGS_RESULT: PASSED");
}

#[tokio::test]
#[ignore]
async fn live_write_round_trip_on_fixture() {
    use hermes_client::config_api::{config_read, config_write};

    if std::env::var("TALARIA_PROBE_WRITES").ok().as_deref() != Some("1") {
        support::report_skip("LIVE_SETTINGS_RESULT", "writes not enabled");
        return;
    }
    let env = match support::live_preconditions().await {
        Ok(env) => env,
        Err(reason) => {
            support::report_skip("LIVE_SETTINGS_RESULT", &reason);
            return;
        }
    };
    let fixture_id = env
        .endpoint
        .resolve_profile("fixture")
        .expect("fixture profile");
    assert!(
        fixture_id.as_ref() == env.fixture.as_ref(),
        "writes require fixture profile"
    );
    let client = fixture_client(env);
    let initial = config_read(&client, false).await.expect("initial raw read");
    if initial.pointer("/display/compact") != Some(&json!(false)) {
        support::report_skip(
            "LIVE_SETTINGS_RESULT",
            "display.compact not explicit false in fixture",
        );
        return;
    }
    let first_write = config_write(&client, &json!({"display": {"compact": true}})).await;
    let first_read = config_read(&client, true).await;
    let restore_write = config_write(&client, &json!({"display": {"compact": false}})).await;
    let restored_read = config_read(&client, true).await;
    let final_raw = config_read(&client, false).await;
    first_write.expect("write true");
    let first = first_read.expect("read true");
    restore_write.expect("restore false");
    let restored = restored_read.expect("read false");
    let raw = final_raw.expect("final raw read");
    assert!(
        first.pointer("/display/compact") == Some(&json!(true)),
        "true read-back"
    );
    assert!(
        restored.pointer("/display/compact") == Some(&json!(false)),
        "false read-back"
    );
    assert!(raw == initial, "write-residue");
    println!("LIVE_SETTINGS_RESULT: PASSED");
}
