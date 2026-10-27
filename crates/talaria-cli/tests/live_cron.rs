//! Live scheduled-job checks. Ignored unless someone asks for them.
//!
//! A skip is not a pass. With `TALARIA_LIVE_REQUIRE=1` a skip fails the test.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hermes_client::{
    Client, DiscoverOptions, DiscoveryError, EnsurePolicy, ProfileLookupError, Purpose, Scope,
    connect, default_home, discover_with, mint_ticket,
    rest::{NativeHttpMinter, RequestOpts, RestClient, RestOptions},
};
use hermes_contract::schemas::{CronAction, CronManageParams};

fn hermes_home() -> PathBuf {
    std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(default_home)
}

fn skip(reason: &str) -> bool {
    println!("LIVE_CRON_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
    true
}

fn finish(result: Result<(), String>) {
    match result {
        Ok(()) => println!("LIVE_CRON_RESULT: PASSED"),
        Err(reason) => {
            println!("LIVE_CRON_RESULT: FAILED ({reason})");
            panic!("{reason}");
        }
    }
}

async fn write_gate() -> tokio::sync::MutexGuard<'static, ()> {
    static GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    GATE.lock().await
}

struct VerdictJob {
    last_status: Option<String>,
    last_run_at: Option<String>,
}

#[derive(Clone)]
struct VerdictRun {
    source: String,
    started_at: f64,
    ended_at: Option<f64>,
    input_tokens: u64,
    output_tokens: u64,
    model: Option<String>,
    end_reason: Option<String>,
}

fn run_verdict(
    triggered_at: f64,
    before_last_run_at: Option<&str>,
    job: &VerdictJob,
    runs: &[VerdictRun],
) -> Result<(), String> {
    if job.last_status.as_deref() != Some("ok") {
        return Err("the job did not finish successfully".into());
    }
    if job.last_run_at.as_deref().is_none() || job.last_run_at.as_deref() == before_last_run_at {
        return Err("the job has no fresh last-run time".into());
    }
    if !runs.iter().any(|run| {
        run.source == "cron"
            && run.started_at >= triggered_at
            && run.ended_at.is_some_and(|end| end >= run.started_at)
            && run.input_tokens.saturating_add(run.output_tokens) > 0
            && run.model.as_deref() == Some("minimax-m3.1-flash")
            && run.end_reason.as_deref() == Some("completed")
    }) {
        return Err("no fresh completed agent run used the expected model and tokens".into());
    }
    Ok(())
}

fn pass_case() -> (VerdictJob, Vec<VerdictRun>) {
    (
        VerdictJob {
            last_status: Some("ok".into()),
            last_run_at: Some("2026-10-23T09:05:00Z".into()),
        },
        vec![VerdictRun {
            source: "cron".into(),
            started_at: 200.0,
            ended_at: Some(240.0),
            input_tokens: 10,
            output_tokens: 2,
            model: Some("minimax-m3.1-flash".into()),
            end_reason: Some("completed".into()),
        }],
    )
}

#[test]
fn run_verdict_rejects_blocked_and_failed() {
    let (job, runs) = pass_case();
    assert!(run_verdict(100.0, None, &job, &runs).is_ok());
    let blocked = VerdictJob {
        last_status: Some("blocked_config".into()),
        last_run_at: Some("2026-10-23T09:05:00Z".into()),
    };
    assert!(run_verdict(100.0, None, &blocked, &runs).is_err());
    let failed = VerdictJob {
        last_status: Some("error".into()),
        last_run_at: Some("2026-10-23T09:05:00Z".into()),
    };
    assert!(run_verdict(100.0, None, &failed, &runs).is_err());
    let delivered = VerdictJob {
        last_status: Some("delivery_failed".into()),
        last_run_at: Some("2026-10-23T09:05:00Z".into()),
    };
    let output_only = vec![VerdictRun {
        source: "cron_output".into(),
        started_at: 200.0,
        ended_at: Some(200.0),
        input_tokens: 0,
        output_tokens: 0,
        model: None,
        end_reason: None,
    }];
    assert!(
        run_verdict(
            100.0,
            Some("2026-10-22T09:00:00Z"),
            &delivered,
            &output_only
        )
        .is_err()
    );
    let mut quiet = runs.clone();
    quiet[0].input_tokens = 0;
    quiet[0].output_tokens = 0;
    assert!(run_verdict(100.0, None, &job, &quiet).is_err());
    let mut other = runs;
    other[0].model = Some("some-other-model".into());
    assert!(run_verdict(100.0, None, &job, &other).is_err());
}

async fn attach() -> Result<(PathBuf, Client, hermes_client::InboundStream, RestClient), String> {
    let home = hermes_home();
    let endpoint = discover_with(
        &home,
        &DiscoverOptions {
            control_timeout: Duration::from_secs(5),
            ensure: EnsurePolicy::Never,
        },
    )
    .await
    .map_err(|error| match error {
        DiscoveryError::NoGateway => "no gateway".to_string(),
        other => format!("discovery failed: {other}"),
    })?;
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        return Err(reason);
    }
    let profile = endpoint
        .resolve_profile("fixture")
        .map_err(|error| match error {
            ProfileLookupError::NeverDiscovered { .. } => "fixture not served".to_string(),
            other => format!("fixture lookup failed: {other}"),
        })?;
    let ticket = mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .map_err(|error| format!("ticket failed: {error}"))?;
    let connection = connect(&endpoint, ticket)
        .await
        .map_err(|error| format!("connect failed: {error}"))?;
    let (client, inbound) = Client::start(connection.into_parts());
    let endpoint = Arc::new(endpoint);
    let rest = RestClient::new(
        endpoint.api_origin(),
        Arc::new(NativeHttpMinter::new(
            home.clone(),
            Arc::clone(&endpoint),
            profile,
        )),
        RestOptions::default(),
    )
    .map_err(|error| format!("dashboard client failed: {error}"))?;
    Ok((home, client, inbound, rest))
}

fn list_params() -> CronManageParams {
    CronManageParams {
        action: CronAction::List,
        include_disabled: true.into(),
        profile: None,
        ..CronManageParams::default()
    }
}

fn names_only(value: &serde_json::Value, into: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                into.insert(key.clone());
                names_only(child, into);
            }
        }
        serde_json::Value::Array(items) => {
            if let Some(first) = items.first() {
                names_only(first, into);
            }
        }
        _ => {}
    }
}

#[tokio::test]
#[ignore]
async fn live_reads_on_fixture() {
    let (_home, client, _inbound, rest) = match attach().await {
        Ok(open) => open,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    let listed = client
        .call::<hermes_contract::methods::legacy::cron_manage::Marker>(list_params())
        .await
        .map_err(|error| format!("list failed: {error}"));
    let listed = match listed {
        Ok(value) => value,
        Err(reason) => {
            finish(Err(reason));
            return;
        }
    };
    if listed.success != Some(true) || listed.jobs.is_none() {
        finish(Err("list did not return success and a jobs array".into()));
        return;
    }
    let targets = hermes_client::cron_api::delivery_targets(&rest).await;
    let targets = match targets {
        Ok(value) => value,
        Err(error) => {
            finish(Err(format!("delivery targets failed: {error}")));
            return;
        }
    };
    if targets["targets"][0]["id"] != "local" {
        finish(Err("the first delivery target was not local".into()));
        return;
    }
    if let Some(job_id) = listed
        .jobs
        .as_ref()
        .and_then(|jobs| jobs.first())
        .map(|job| job.job_id.clone())
    {
        let runs = hermes_client::cron_api::job_runs(&rest, "fixture", &job_id, 1).await;
        if let Err(error) = runs {
            finish(Err(format!("runs failed: {error}")));
            return;
        }
    }
    let mut fields = BTreeSet::new();
    if let Ok(value) = serde_json::to_value(&listed) {
        names_only(&value, &mut fields);
    }
    names_only(&targets, &mut fields);
    let path = std::env::temp_dir().join(format!("cron-fields-{}.json", std::process::id()));
    let recorded: Vec<_> = fields.into_iter().collect();
    if let Err(error) = std::fs::write(&path, serde_json::to_vec(&recorded).unwrap_or_default()) {
        finish(Err(format!("could not record field names: {error}")));
        return;
    }
    finish(Ok(()));
}

struct ProbeGuard {
    client: Option<Client>,
    job_id: Option<String>,
}

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        let (Some(client), Some(job_id)) = (self.client.take(), self.job_id.clone()) else {
            return;
        };
        let _ = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok();
            if let Some(runtime) = runtime {
                runtime.block_on(async move {
                    let _ = client
                        .call::<hermes_contract::methods::legacy::cron_manage::Marker>(
                            CronManageParams {
                                action: CronAction::Remove,
                                name: Some(job_id),
                                profile: None,
                                ..CronManageParams::default()
                            },
                        )
                        .await;
                });
            }
        })
        .join();
    }
}

fn probe_name() -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or(0);
    format!("talaria-probe-{:08x}", nonce)
}

async fn manage(
    client: &Client,
    action: CronAction,
    name: Option<String>,
    schedule: Option<String>,
    prompt: Option<String>,
) -> Result<hermes_contract::schemas::CronManageResult, String> {
    client
        .call::<hermes_contract::methods::legacy::cron_manage::Marker>(CronManageParams {
            action,
            name,
            schedule,
            prompt,
            deliver: Some("local".into()),
            profile: None,
            ..CronManageParams::default()
        })
        .await
        .map_err(|error| format!("{action} failed: {error}"))
}

#[tokio::test]
#[ignore]
async fn live_add_pause_resume_remove_on_fixture() {
    let _gate = write_gate().await;
    if std::env::var("TALARIA_PROBE_WRITES").ok().as_deref() != Some("1") {
        skip("TALARIA_PROBE_WRITES is not 1");
        return;
    }
    let (_home, client, mut inbound, _rest) = match attach().await {
        Ok(open) => open,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    let started = client
        .call::<hermes_contract::methods::legacy::cron_manage::Marker>(list_params())
        .await
        .map_err(|error| format!("opening list failed: {error}"));
    let started = match started {
        Ok(value) => value,
        Err(reason) => {
            finish(Err(reason));
            return;
        }
    };
    let before: BTreeSet<_> = started
        .jobs
        .unwrap_or_default()
        .iter()
        .map(|job| job.job_id.clone())
        .collect();
    let name = probe_name();
    let created = manage(
        &client,
        CronAction::Add,
        Some(name.clone()),
        Some("0 3 1 1 *".into()),
        Some("Reply with the single word ok.".into()),
    )
    .await;
    let created = match created {
        Ok(value) => value,
        Err(reason) => {
            finish(Err(reason));
            return;
        }
    };
    let mut guard = ProbeGuard {
        client: Some(client),
        job_id: created.job_id.clone(),
    };
    let client = guard.client.as_ref().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut seen = false;
    while std::time::Instant::now() < deadline {
        let next = tokio::time::timeout(Duration::from_millis(200), inbound.next()).await;
        if let Ok(Some(hermes_client::Inbound::Event(event))) = next
            && event.raw["type"] == "cron.changed"
        {
            seen = true;
            break;
        }
    }
    println!(
        "LIVE_CRON_CHANGED: {}",
        if seen { "seen" } else { "not seen" }
    );
    let listed = match client
        .call::<hermes_contract::methods::legacy::cron_manage::Marker>(list_params())
        .await
    {
        Ok(value) => value,
        Err(error) => {
            finish(Err(format!("list after add failed: {error}")));
            return;
        }
    };
    let jobs = listed.jobs.unwrap_or_default();
    let probe = jobs.iter().find(|job| job.name == name);
    if probe.and_then(|job| job.state.as_deref()) != Some("scheduled") {
        finish(Err("the probe job was not scheduled".into()));
        return;
    }
    let job_id = probe.unwrap().job_id.clone();
    guard.job_id = Some(job_id.clone());
    if let Err(error) = manage(client, CronAction::Pause, Some(job_id.clone()), None, None).await {
        finish(Err(error));
        return;
    }
    if let Err(error) = manage(client, CronAction::Resume, Some(job_id.clone()), None, None).await {
        finish(Err(error));
        return;
    }
    if let Err(error) = manage(client, CronAction::Remove, Some(job_id), None, None).await {
        finish(Err(error));
        return;
    }
    guard.job_id = None;
    let ended = match client
        .call::<hermes_contract::methods::legacy::cron_manage::Marker>(list_params())
        .await
    {
        Ok(value) => value,
        Err(error) => {
            finish(Err(format!("closing list failed: {error}")));
            return;
        }
    };
    let after: BTreeSet<_> = ended
        .jobs
        .unwrap_or_default()
        .iter()
        .map(|job| job.job_id.clone())
        .collect();
    if before != after {
        finish(Err("the set of job ids changed".into()));
        return;
    }
    finish(Ok(()));
}

#[tokio::test]
#[ignore]
async fn live_run_now_and_history_on_fixture() {
    let _gate = write_gate().await;
    if std::env::var("TALARIA_PROBE_WRITES").ok().as_deref() != Some("1") {
        skip("TALARIA_PROBE_WRITES is not 1");
        return;
    }
    if std::env::var("TALARIA_CRON_FREE_ROUTE").ok().as_deref() != Some("1") {
        skip("TALARIA_CRON_FREE_ROUTE is not 1");
        return;
    }
    let (_home, client, _inbound, rest) = match attach().await {
        Ok(open) => open,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };
    let name = probe_name();
    let created = match manage(
        &client,
        CronAction::Add,
        Some(name),
        Some("0 3 1 1 *".into()),
        Some("Reply with the single word ok.".into()),
    )
    .await
    {
        Ok(value) => value,
        Err(reason) => {
            finish(Err(reason));
            return;
        }
    };
    let Some(job_id) = created.job_id.clone() else {
        finish(Err("add returned no job id".into()));
        return;
    };
    let mut guard = ProbeGuard {
        client: Some(client),
        job_id: Some(job_id.clone()),
    };
    let before_last_run_at = created.job.as_ref().and_then(|job| job.last_run_at.clone());
    let triggered_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs_f64();
    let triggered = hermes_client::cron_api::trigger_job(
        &rest,
        "fixture",
        &job_id,
        RequestOpts {
            ticket: true,
            deadline: Some(Duration::from_secs(10 * 60)),
        },
    )
    .await;
    if let Err(error) = triggered {
        finish(Err(format!("trigger failed: {error}")));
        return;
    }
    let client = guard.client.as_ref().unwrap();
    let listed = match client
        .call::<hermes_contract::methods::legacy::cron_manage::Marker>(list_params())
        .await
    {
        Ok(value) => value,
        Err(error) => {
            finish(Err(format!("list after trigger failed: {error}")));
            return;
        }
    };
    let Some(job) = listed
        .jobs
        .unwrap_or_default()
        .into_iter()
        .find(|job| job.job_id == job_id)
    else {
        finish(Err("the probe job disappeared after trigger".into()));
        return;
    };
    let runs_value = match hermes_client::cron_api::job_runs(&rest, "fixture", &job_id, 20).await {
        Ok(value) => value,
        Err(error) => {
            finish(Err(format!("runs failed: {error}")));
            return;
        }
    };
    let runs = runs_value["runs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|row| VerdictRun {
            source: row["source"].as_str().unwrap_or_default().to_owned(),
            started_at: row["started_at"].as_f64().unwrap_or(0.0),
            ended_at: row["ended_at"].as_f64(),
            input_tokens: row["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: row["output_tokens"].as_u64().unwrap_or(0),
            model: row["model"].as_str().map(str::to_owned),
            end_reason: row["end_reason"].as_str().map(str::to_owned),
        })
        .collect::<Vec<_>>();
    let verdict = run_verdict(
        triggered_at,
        before_last_run_at.as_deref(),
        &VerdictJob {
            last_status: job.last_status.clone(),
            last_run_at: job.last_run_at.clone(),
        },
        &runs,
    );
    if let Err(reason) = verdict {
        finish(Err(reason));
        return;
    }
    if let Err(error) = manage(client, CronAction::Remove, Some(job_id), None, None).await {
        finish(Err(error));
        return;
    }
    guard.job_id = None;
    finish(Ok(()));
}
