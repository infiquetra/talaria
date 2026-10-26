//! The one live test: record `plain-answer` against a real gateway and replay it.
//!
//! Ignored by default. It needs the operator preconditions of the issue (Hermes at the pinned
//! release, profile `fixture` served with `approvals.mode: manual`, the provider key present).
//! It prints exactly one line, `LIVE_RECORD_RESULT: PASSED` or
//! `LIVE_RECORD_RESULT: SKIPPED (<reason>)`, and fails on a skip when `TALARIA_LIVE_REQUIRE=1`.
//! Discovery never starts a gateway (`EnsurePolicy::Never`). The other six scenarios are checked
//! live only during live verification.

use std::path::{Path, PathBuf};
use std::time::Duration;

use hermes_client::{
    ConnectOptions, DiscoverOptions, EnsurePolicy, GatewayIdentity, Purpose, Recorder,
    RecorderOptions, ReplaySource, Scope, Speed, connect_with, default_home, discover_with,
    handshake, local_forbidden_literals, mint_ticket, scan_for_secrets, scan_literals, ws_url,
};
use record_fixture::{
    AttemptResult, DiscoveryFacts, DriverConfig, EnvFacts, PreflightInput, Scenario, ScratchDir,
    build_substitutions, check_evidence, preflight, run_scenario,
};

const MODEL: &str = "minimax-m3.1-flash";
const PROVIDER: &str = "ollama-cloud";
const ATTEMPTS: usize = 3;

/// Print the one skip line; fail the test when the run requires a live gateway.
fn skip(reason: &str) {
    println!("LIVE_RECORD_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("a live gateway is required (TALARIA_LIVE_REQUIRE=1) and the run was skipped");
    }
}

/// How one attempt ended.
enum Attempt {
    Recorded(PathBuf),
    Failed(String),
    Skip(String),
}

async fn attempt(
    home: &Path,
    scenario: &Scenario,
    scratch: &ScratchDir,
    out: &Path,
    number: usize,
    user_home: &str,
    login: &str,
) -> Attempt {
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(home, &options).await {
        Ok(endpoint) => endpoint,
        Err(err) => return Attempt::Skip(err.to_string()),
    };
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(err) => return Attempt::Skip(err.to_string()),
    };
    let ticket = match mint_ticket(
        home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    {
        Ok(ticket) => ticket,
        Err(err) => return Attempt::Skip(err.to_string()),
    };
    let mut connection = match connect_with(&endpoint, ticket, &ConnectOptions::default()).await {
        Ok(connection) => connection,
        Err(err) => return Attempt::Skip(err.to_string()),
    };

    let path = out.join(format!(".{}-{number}.jsonl", scenario.name));
    let facts = DiscoveryFacts {
        profile_id: profile.as_ref().to_owned(),
        api_origin: endpoint.api_origin().to_owned(),
    };
    let env = EnvFacts {
        home: user_home.to_owned(),
        login_name: login.to_owned(),
    };
    let recorder = match Recorder::create(
        &path,
        RecorderOptions {
            endpoint: ws_url(endpoint.api_origin()),
            subprotocols: vec!["hermes-gateway-v1".to_owned()],
            gateway: Some(GatewayIdentity {
                code_version: endpoint.code_version().unwrap_or_default().to_owned(),
                code_sha: endpoint.code_sha().unwrap_or_default().to_owned(),
            }),
            scenario: Some(scenario.name.clone()),
            expected_statuses: scenario.expected_statuses.clone(),
            substitutions: build_substitutions(&facts, &env, scratch.path()),
            queue_capacity: None,
        },
    ) {
        Ok(recorder) => recorder,
        Err(err) => return Attempt::Failed(format!("could not start the recording: {err}")),
    };
    connection.set_recorder(recorder.clone(), 1);
    if let Err(err) = handshake(&mut connection).await {
        return Attempt::Skip(err.to_string());
    }
    let (client, mut inbound) = hermes_client::Client::start(connection.into_parts());
    let config = DriverConfig {
        model: MODEL.to_owned(),
        provider: PROVIDER.to_owned(),
        scratch: scratch.path().to_path_buf(),
        second: None,
        home: scratch.path().to_path_buf(),
        attempt_timeout: Duration::from_secs(120),
    };
    let result = run_scenario(&client, &mut inbound, scenario, &config).await;
    client.close().await;
    if let Err(err) = recorder.finish() {
        return Attempt::Failed(format!("the recording failed: {err}"));
    }
    match result {
        AttemptResult::Ok => Attempt::Recorded(path),
        AttemptResult::Failed(reason) => Attempt::Failed(reason),
        AttemptResult::PreconditionAbsent(reason) => Attempt::Skip(reason),
    }
}

#[tokio::test]
#[ignore = "needs a live Hermes gateway; run with --ignored"]
async fn live_plain_answer_records_and_replays() {
    let login = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();
    let Ok(user_home) = std::env::var("HOME") else {
        skip("HOME is not set");
        return;
    };
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let home = default_home();

    // Preconditions: the same check the script makes, with discovery that never starts
    // a gateway. A missing precondition is a skip, not a failure.
    let input = PreflightInput {
        hermes_home: home.clone(),
        profile: "fixture".to_owned(),
        login_name: login.clone(),
        host_name: host,
    };
    if let Err(err) = preflight(&input).await {
        skip(&err.message);
        return;
    }

    let scenario = Scenario::named("plain-answer").expect("plain-answer is a scenario");
    let scratch = ScratchDir::create().expect("the scratch project could not be created");
    let out = tempfile::tempdir().expect("the output directory could not be created");

    let mut recorded = None;
    let mut last_failure = String::new();
    for number in 1..=ATTEMPTS {
        match attempt(
            &home,
            &scenario,
            &scratch,
            out.path(),
            number,
            &user_home,
            &login,
        )
        .await
        {
            Attempt::Skip(reason) => {
                skip(&reason);
                return;
            }
            Attempt::Failed(reason) => last_failure = reason,
            Attempt::Recorded(path) => {
                let source = match ReplaySource::open(&path) {
                    Ok(source) => source,
                    Err(err) => {
                        last_failure = format!("the recording did not open: {err}");
                        continue;
                    }
                };
                match check_evidence(&scenario, &source) {
                    Ok(()) => {
                        recorded = Some((path, source));
                        break;
                    }
                    Err(reason) => last_failure = reason,
                }
            }
        }
    }
    let Some((path, source)) = recorded else {
        panic!("plain-answer was not recorded in {ATTEMPTS} attempts: {last_failure}");
    };

    // Nothing secret-shaped, and nothing from this machine, reached the file.
    let text = std::fs::read_to_string(&path).expect("the recording could not be read");
    let hits = scan_for_secrets(&text);
    assert!(
        hits.is_empty(),
        "the scan found {} secret-shaped string(s), first rule {} at line {}",
        hits.len(),
        hits[0].rule,
        hits[0].line
    );
    let forbidden = local_forbidden_literals();
    let hits = scan_literals(&text, &forbidden);
    assert!(
        hits.is_empty(),
        "the recording holds a machine specific value, rule {} at line {}",
        hits[0].rule,
        hits[0].line
    );

    // Replay it with no socket and read the final statuses back.
    let (sink, mut frames) = tokio::sync::mpsc::channel::<hermes_client::ReplayItem>(256);
    let collector = tokio::spawn(async move {
        let mut count = 0usize;
        let mut statuses = Vec::new();
        while let Some(item) = frames.recv().await {
            count += 1;
            let value: serde_json::Value =
                serde_json::from_str(&item.text).expect("a replayed frame is JSON");
            if value["params"]["type"] == "message.complete" {
                statuses.push(
                    value["params"]["payload"]["status"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                );
            }
        }
        (count, statuses)
    });
    let sent = source
        .run(Speed::Instant, sink)
        .await
        .expect("the replay failed");
    let (received, statuses) = collector.await.expect("the replay consumer failed");
    assert!(sent > 0, "the replay sent no frames");
    assert_eq!(sent, received, "every replayed frame reached the consumer");
    assert_eq!(statuses, ["complete"], "final statuses");
    assert_eq!(
        sent,
        source.inbound().count(),
        "the replay count matches the file"
    );

    println!("LIVE_RECORD_RESULT: PASSED");
}
