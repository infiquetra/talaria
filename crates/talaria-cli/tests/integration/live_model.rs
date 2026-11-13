//! Issue 230, scenario "live model switch".
//!
//! Ignored unless someone asks for it. A skip is not a pass: with `TALARIA_LIVE_REQUIRE=1` a
//! skip fails the test. Run it by hand against the gateway of the home named by `HERMES_HOME`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use hermes_client::{CallOptions, DiscoverOptions, EnsurePolicy, default_home, discover_with};
use serde_json::{Map, Value, json};
use talaria_app::launch::{Launch, StartMode, new_request_id};
use talaria_app::mutation::MutationClient;
use talaria_core::mutation::{MutationOutcome, MutationRequest};

const PROFILE: &str = "fixture";
const OPEN_MODEL: &str = "minimax-m3.1-flash";
const PROVIDER: &str = "ollama-cloud";

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn sanitize(reason: &str) -> String {
    let account = std::env::var("USER").unwrap_or_default();
    reason
        .split_whitespace()
        .map(|word| {
            let path = word
                .split(['\'', '"', '(', ')', '=', ':'])
                .any(|piece| std::path::Path::new(piece).is_absolute());
            let named = !account.is_empty() && word.contains(&account);
            if path || named { "<path>" } else { word }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn skip(reason: &str) -> bool {
    let reason = sanitize(reason);
    println!("LIVE_MODEL_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
    false
}

fn gateway_code_sha(home: &std::path::Path) -> Result<Option<String>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let options = DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        };
        discover_with(home, &options)
            .await
            .map(|endpoint| endpoint.code_sha().map(str::to_owned))
            .map_err(|error| sanitize(&error.to_string()))
    })
}

/// True when the pinned gateway is up. A skip prints its reason and returns false.
fn begin() -> bool {
    match gateway_code_sha(&hermes_home()) {
        Err(reason) => skip(&reason),
        Ok(sha) => match hermes_contract::pinned::live_skip_reason(sha.as_deref()) {
            Some(reason) => skip(&reason),
            None => true,
        },
    }
}

fn passed() {
    println!("LIVE_MODEL_RESULT: PASSED");
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().expect("object").clone()
}

fn offers_effort(provider: &Value, model: &str) -> bool {
    provider
        .get("capabilities")
        .and_then(|caps| caps.get(model))
        .and_then(|cap| cap.get("reasoning"))
        .and_then(Value::as_bool)
        != Some(false)
}

fn provider_row<'a>(catalogue: &'a Value, slug: &str) -> Option<&'a Value> {
    catalogue
        .get("providers")
        .and_then(Value::as_array)
        .and_then(|rows| rows.iter().find(|row| row["slug"].as_str() == Some(slug)))
}

async fn catalogue(
    client: &hermes_client::Client,
    session_id: Option<&str>,
) -> Result<Value, String> {
    let mut params = json!({"explicit_only": true});
    if let Some(session_id) = session_id {
        params["session_id"] = json!(session_id);
        params["include_unconfigured"] = json!(true);
    }
    client
        .call_raw("model.options", params, CallOptions::default())
        .await
        .map_err(|error| error.to_string())
}

async fn wait_for_model(
    client: &hermes_client::Client,
    session_id: &str,
    wanted: &str,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let info = client
            .call_raw(
                "session.info",
                json!({"session_id": session_id}),
                CallOptions::default(),
            )
            .await
            .map_err(|error| error.to_string())?;
        if info["model"].as_str() == Some(wanted) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("session info did not report model {wanted}"));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn mutate(
    session_id: &str,
    id: &str,
    revision: i64,
    generation: i64,
    operation: &str,
    body: Value,
) -> MutationRequest {
    MutationRequest {
        session_id: session_id.to_string(),
        request_id: id.to_string(),
        expected_revision: revision,
        expected_generation: Some(generation),
        operation: operation.to_string(),
        payload: object(body),
    }
}

fn committed_field(outcome: MutationOutcome, field: &str) -> String {
    match outcome {
        MutationOutcome::Committed(value) => value[field].as_str().unwrap_or_default().to_string(),
        other => panic!("expected a committed receipt, got {other:?}"),
    }
}

fn committed_numbers(outcome: &MutationOutcome) -> (i64, i64) {
    match outcome {
        MutationOutcome::Committed(value) => (
            value["revision"].as_i64().expect("revision on the receipt"),
            value["execution_generation"]
                .as_i64()
                .expect("execution generation on the receipt"),
        ),
        other => panic!("expected a committed receipt, got {other:?}"),
    }
}

async fn switch_models() -> Result<(), String> {
    let work = tempfile::tempdir().map_err(|_| "could not create the test folder")?;
    let launch = Launch {
        mode: StartMode::New,
        model: None,
        provider: None,
        effort: None,
        profile: PROFILE.to_string(),
        folder: work.path().to_path_buf(),
        record: None,
        bench_replay: None,
        no_ensure: true,
    };
    let connection = talaria_app::app::connect(&launch, &hermes_home())
        .await
        .map_err(|error| sanitize(&error.what))?;
    let client = connection.link.client();

    let profile = catalogue(&client, None).await?;
    let profile_model = profile["model"].as_str().unwrap_or_default().to_string();
    let profile_provider = profile["provider"].as_str().unwrap_or_default().to_string();

    let created = client
        .call_raw(
            "session.create",
            json!({
                "request_id": new_request_id(),
                "source": "tui",
                "cwd": work.path(),
                "model": OPEN_MODEL,
                "provider": PROVIDER,
            }),
            CallOptions::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let session_id = created["session_id"]
        .as_str()
        .ok_or("create did not return a session")?
        .to_string();
    let mut revision = created["revision"]
        .as_i64()
        .ok_or("create did not return a revision")?;
    let mut generation = created["execution_generation"]
        .as_i64()
        .ok_or("create did not return a generation")?;

    let listed = catalogue(&client, Some(&session_id)).await?;
    let row = provider_row(&listed, PROVIDER)
        .ok_or("ollama-cloud is absent from the session catalogue")?;
    assert_eq!(
        row["authenticated"].as_bool(),
        Some(true),
        "ollama-cloud is authenticated"
    );
    let models = row["models"]
        .as_array()
        .ok_or("ollama-cloud has no model list")?;
    assert!(models.len() >= 2, "ollama-cloud lists at least two models");
    let target = models
        .iter()
        .filter_map(Value::as_str)
        .find(|model| *model != OPEN_MODEL && offers_effort(row, model))
        .ok_or("no other ollama-cloud model offers effort")?
        .to_string();

    let mutation = MutationClient::new(client.clone());
    let switched = mutation
        .send(&mutate(
            &session_id,
            "live-m1",
            revision,
            generation,
            "model",
            json!({"model": target, "provider": PROVIDER}),
        ))
        .await;
    assert_eq!(committed_field(switched.clone(), "model"), target);
    (revision, generation) = committed_numbers(&switched);
    wait_for_model(&client, &session_id, &target).await?;

    let effort = mutation
        .send(&mutate(
            &session_id,
            "live-m2",
            revision,
            generation,
            "reasoning",
            json!({"effort": "low"}),
        ))
        .await;
    assert_eq!(committed_field(effort.clone(), "effort"), "low");
    (revision, generation) = committed_numbers(&effort);

    let back = mutation
        .send(&mutate(
            &session_id,
            "live-m3",
            revision,
            generation,
            "model",
            json!({"model": OPEN_MODEL, "provider": PROVIDER}),
        ))
        .await;
    assert_eq!(committed_field(back, "model"), OPEN_MODEL);
    wait_for_model(&client, &session_id, OPEN_MODEL).await?;

    let again = catalogue(&client, None).await?;
    assert_eq!(again["model"].as_str().unwrap_or_default(), profile_model);
    assert_eq!(
        again["provider"].as_str().unwrap_or_default(),
        profile_provider
    );

    if let Some(subscription) = created["subscription_id"].as_str() {
        let _ = client
            .call_raw(
                "session.detach",
                json!({
                    "session_id": session_id,
                    "subscription_id": subscription,
                }),
                CallOptions::default(),
            )
            .await;
    }
    Ok(())
}

/// Issue 230, scenario "live model switch".
#[test]
#[ignore = "runs against a live Hermes gateway"]
fn live_model_switch() {
    if !begin() {
        return;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime
        .block_on(switch_models())
        .unwrap_or_else(|error| panic!("{}", sanitize(&error)));
    passed();
}
