use std::sync::Arc;
use std::time::{Duration, Instant};

use hermes_client::config_api::config_read;
use hermes_client::rest::{NativeHttpMinter, RestClient, RestOptions, TicketMinter};
use hermes_client::{CallOptions, DiscoverOptions, EnsurePolicy, default_home, discover_with};
use serde_json::{Value, json};
use talaria_app::configure::{ConfigureBackends, SiblingBackends, wire};
use talaria_app::launch::{Launch, StartMode, new_request_id};
use talaria_app::settings::SettingsService;
use talaria_core::configure::{
    ConfigureEffect, ConfigureInput, ConfigureKey, ConfigureState, CreateSpec, RowOutcome,
    StepResult,
};

async fn setup() -> Option<(RestClient, Arc<SettingsService>)> {
    let home = default_home();
    let endpoint = match discover_with(
        &home,
        &DiscoverOptions {
            ensure: EnsurePolicy::Never,
            ..DiscoverOptions::default()
        },
    )
    .await
    {
        Ok(endpoint) => Arc::new(endpoint),
        Err(_) => return skip("gateway unavailable"),
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        return skip(&reason);
    }
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(_) => return skip("fixture unavailable"),
    };
    let origin = endpoint.api_origin().to_owned();
    let rest = RestClient::new(
        &origin,
        Arc::new(NativeHttpMinter::new(
            home.clone(),
            endpoint.clone(),
            profile,
        )),
        RestOptions::default(),
    )
    .expect("rest client");
    let factory = Arc::new(move |name: &str| -> Arc<dyn TicketMinter> {
        assert_eq!(name, "fixture");
        Arc::new(NativeHttpMinter::new(
            home.clone(),
            endpoint.clone(),
            endpoint.resolve_profile("fixture").expect("fixture"),
        ))
    });
    let settings = Arc::new(SettingsService::new(
        Some(origin),
        vec!["fixture".into()],
        factory,
    ));
    Some((rest, settings))
}

fn skip<T>(reason: &str) -> Option<T> {
    println!("LIVE_CONFIGURE_RESULT: SKIPPED ({reason})");
    assert_ne!(
        std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref(),
        Some("1"),
        "live gateway required"
    );
    None
}

fn detector(text: &str) -> bool {
    !hermes_client::redact::scan_for_secrets(text).is_empty()
}

#[tokio::test]
#[ignore = "requires a live gateway"]
async fn live_session_is_locked_down() {
    if setup().await.is_none() {
        return;
    }
    let work = tempfile::tempdir().expect("temporary folder");
    let launch = Launch {
        mode: StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: "fixture".into(),
        folder: work.path().to_owned(),
        record: None,
        bench_replay: None,
        no_ensure: true,
    };
    let connection = talaria_app::app::connect(&launch, &default_home())
        .await
        .expect("connect");
    let client = connection.link.client();
    let spec = CreateSpec {
        request_id: new_request_id(),
        toolsets: vec!["clarify".into(), "no_mcp".into()],
        model: None,
        provider: None,
    };
    let created = client
        .call_raw(
            "session.create",
            wire::create_params(&spec),
            CallOptions::default(),
        )
        .await
        .expect("create");
    let session = created["session_id"].as_str().expect("session");
    let subscription = created["subscription_id"].as_str().expect("subscription");
    let tools = created["info"]["tools"].as_object().expect("tool snapshot");
    let names: Vec<&str> = tools
        .values()
        .flat_map(|v| v.as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let guarded = names == ["clarify"] && created["info"]["yolo"] == false;
    client
        .call_raw(
            "session.detach",
            wire::detach_params(session, subscription),
            CallOptions::default(),
        )
        .await
        .expect("detach");
    assert!(guarded, "snapshot must prove exactly the allowed tool");
    println!("LIVE_CONFIGURE_RESULT: PASSED");
}

#[tokio::test]
#[ignore = "requires a live gateway and a model"]
async fn live_dry_run_proposes_without_writing() {
    let Some((rest, _)) = setup().await else {
        return;
    };
    let before = config_read(&rest, false).await.expect("raw configuration");
    let mut ready = false;
    for _ in 0..3 {
        let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_talaria"))
            .args([
                "--profile",
                "fixture",
                "--provider",
                "ollama-cloud",
                "--model",
                "minimax-m3.1-flash",
                "configure",
                "--dry-run",
                "--json",
                "turn on the terminal bell when a reply finishes",
            ])
            .output()
            .await
            .expect("dry run");
        if output.status.code() == Some(0) {
            let value: Value = serde_json::from_slice(&output.stdout).expect("result");
            ready = value["rows"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["status"] == "ready"));
            if ready {
                break;
            }
        }
    }
    let after = config_read(&rest, false).await.expect("raw configuration");
    assert_eq!(before, after, "dry run changed configuration");
    assert!(ready, "no ready row in three attempts");
    println!("LIVE_CONFIGURE_RESULT: PASSED");
}

async fn apply_bell(backends: &SiblingBackends, value: bool) -> Result<(), String> {
    let context = backends.read_context("fixture").await?;
    let reply = format!(
        "```talaria-configure\n{{\"kind\":\"proposal\",\"summary\":\"Bell\",\"changes\":[{{\"op\":\"set\",\"key\":\"display.bell_on_complete\",\"value\":{value}}}]}}\n```"
    );
    let mut state = ConfigureState::from_reply(context, &reply, detector);
    state.update(ConfigureInput::Key(ConfigureKey::Char('a')), Instant::now());
    let effects = state.update(ConfigureInput::Key(ConfigureKey::Char('y')), Instant::now());
    let (index, step) = effects
        .into_iter()
        .find_map(|effect| match effect {
            ConfigureEffect::Apply { index, step } => Some((index, step)),
            _ => None,
        })
        .ok_or("no apply step")?;
    let result = backends.apply(&step).await;
    let success = matches!(&result, StepResult::Rows(rows) if rows.len() == 1 && matches!(rows[0].1, RowOutcome::Applied));
    state.update(ConfigureInput::StepDone { index, result }, Instant::now());
    if success {
        Ok(())
    } else {
        Err("bell was not applied".into())
    }
}

#[tokio::test]
#[ignore = "requires an explicitly enabled fixture write"]
async fn live_apply_round_trip_on_fixture() {
    if std::env::var("TALARIA_PROBE_WRITES").ok().as_deref() != Some("1") {
        skip::<()>("writes not enabled");
        return;
    }
    let Some((rest, settings)) = setup().await else {
        return;
    };
    let before = config_read(&rest, false).await.expect("raw configuration");
    if before.pointer("/display/bell_on_complete") != Some(&json!(false)) {
        skip::<()>("display.bell_on_complete not explicit false in fixture");
        return;
    }
    let backends = SiblingBackends::for_settings(settings);
    let first = apply_bell(&backends, true).await;
    let first_read = config_read(&rest, true).await;
    let restore = tokio::time::timeout(Duration::from_secs(30), apply_bell(&backends, false)).await;
    let restored_read = config_read(&rest, true).await;
    let after = config_read(&rest, false).await;
    first.expect("apply true");
    assert_eq!(
        first_read
            .expect("read true")
            .pointer("/display/bell_on_complete"),
        Some(&json!(true))
    );
    restore.expect("restore timed out").expect("restore false");
    assert_eq!(
        restored_read
            .expect("read false")
            .pointer("/display/bell_on_complete"),
        Some(&json!(false))
    );
    assert_eq!(
        after.expect("final raw configuration"),
        before,
        "write-residue"
    );
    println!("LIVE_CONFIGURE_RESULT: PASSED");
}
