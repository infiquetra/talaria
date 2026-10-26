//! Live background processes and detached prompt tasks. Ignored unless a person asks for them.
//!
//! A skip is not a pass. With `TALARIA_LIVE_REQUIRE=1` a skip fails the test.

use std::time::{Duration, Instant};

use hermes_client::{
    CallOptions, Client, DiscoverOptions, DiscoveryError, EnsurePolicy, Inbound, InboundStream,
    ProfileLookupError, Purpose, Scope, connect, default_home, discover_with, mint_ticket,
};
use hermes_contract::schemas::{
    ProcessKillParams, ProcessListParams, ProcessStopParams, SideAgentParams,
};
use serde_json::{Value, json};
use talaria_app::launch::{NewSession, SessionRequest};
use talaria_app::sessions::SessionService;
use talaria_core::session::{Effect, SessionId};

const BG_OK: &str = "Reply with exactly: bg-ok";
const TICK_PROMPT: &str = "Run the command sh tick.sh in the background with the terminal tool (background=true, notify=false). Reply with only its process id.";
const APPROVAL_PROMPT: &str = "Run this exact shell command: rm -rf ./talaria-bg-probe";
const TICK_SCRIPT: &str = "for i in $(seq 1 600); do echo \"tick $i\"; sleep 1; done\n";

fn report_skip(reason: &str) {
    println!("LIVE_BACKGROUND_RESULT: SKIPPED ({reason})");
    if std::env::var("TALARIA_LIVE_REQUIRE").ok().as_deref() == Some("1") {
        panic!("live test skipped: {reason}");
    }
}

fn passed() {
    println!("LIVE_BACKGROUND_RESULT: PASSED");
}

struct LiveLink {
    client: Client,
    inbound: InboundStream,
    session_id: String,
    folder: tempfile::TempDir,
}

async fn attach() -> Option<LiveLink> {
    let home = default_home();
    let options = DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    };
    let endpoint = match discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(DiscoveryError::NoGateway) => {
            report_skip("no gateway");
            return None;
        }
        Err(error) => panic!("discovery failed: {error}"),
    };
    let profile = match endpoint.resolve_profile("fixture") {
        Ok(profile) => profile,
        Err(ProfileLookupError::NeverDiscovered { .. }) => {
            report_skip("fixture not served");
            return None;
        }
        Err(error) => panic!("fixture lookup failed: {error}"),
    };
    if let Some(reason) = hermes_contract::pinned::live_skip_reason(endpoint.code_sha()) {
        report_skip(&reason);
        return None;
    }
    let ticket = mint_ticket(
        &home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .unwrap_or_else(|error| panic!("ticket failed: {error}"));
    let connection = connect(&endpoint, ticket)
        .await
        .unwrap_or_else(|error| panic!("connect failed: {error}"));
    let (client, inbound) = Client::start(connection.into_parts());
    let folder = tempfile::tempdir().expect("folder");
    let service = SessionService::with_client("live-background", client.clone());
    let opened = service
        .open(SessionRequest::New(NewSession {
            request_id: format!("live-background-{}", now_ms()),
            source: "tui".to_string(),
            cwd: folder.path().to_path_buf(),
            model: Some("minimax-m3.1-flash".to_string()),
            provider: Some("ollama-cloud".to_string()),
            reasoning: None,
        }))
        .await
        .unwrap_or_else(|error| panic!("open failed: {error:?}"));
    Some(LiveLink {
        client,
        inbound,
        session_id: opened.session_id,
        folder,
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn call_options(seconds: u64) -> CallOptions {
    CallOptions {
        timeout: Some(Duration::from_secs(seconds)),
    }
}

async fn next_inbound(inbound: &mut InboundStream, limit: Duration) -> Option<Inbound> {
    tokio::time::timeout(limit, inbound.next())
        .await
        .ok()
        .flatten()
}

fn event_type(item: &Inbound) -> Option<&str> {
    match item {
        Inbound::Event(event) => event.frame.type_name(),
        _ => None,
    }
}

fn event_session(item: &Inbound) -> Option<&str> {
    match item {
        Inbound::Event(event) => event.frame.session_id.as_deref(),
        _ => None,
    }
}

fn payload<'a>(item: &'a Inbound) -> Option<&'a Value> {
    match item {
        Inbound::Event(event) => event.raw.get("payload"),
        _ => None,
    }
}

async fn deny_approval(client: &Client, item: &Inbound) -> Option<String> {
    let Inbound::Event(event) = item else {
        return None;
    };
    if event.frame.type_name() != Some("approval.request") {
        return None;
    }
    let prompt_id = payload(item)
        .and_then(|payload| payload.get("prompt_id"))
        .and_then(Value::as_str)
        .or_else(|| event.raw.get("prompt_id").and_then(Value::as_str))?;
    let session_id = event.frame.session_id.clone()?;
    match client
        .call_raw(
            "approval.respond",
            json!({
                "session_id": session_id,
                "request_id": prompt_id,
                "choice": "deny",
            }),
            call_options(30),
        )
        .await
    {
        Ok(_) => None,
        Err(error) => Some(error.to_string()),
    }
}

#[tokio::test]
#[ignore]
async fn live_bg_result_arrives() {
    let Some(mut link) = attach().await else {
        return;
    };
    let started = Instant::now();
    let accepted = link
        .client
        .call_with::<hermes_contract::methods::legacy::prompt_background::Marker>(
            SideAgentParams {
                profile: None,
                session_id: link.session_id.clone(),
                text: BG_OK.to_string(),
            },
            call_options(15),
        )
        .await
        .unwrap_or_else(|error| panic!("prompt.background failed: {error}"));
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        let Some(item) = next_inbound(&mut link.inbound, left).await else {
            break;
        };
        let _ = deny_approval(&link.client, &item).await;
        if event_type(&item) == Some("background.complete") {
            let task_id = payload(&item)
                .and_then(|value| value.get("task_id"))
                .and_then(Value::as_str);
            let text = payload(&item)
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if task_id == Some(accepted.task_id.as_str()) && text.contains("bg-ok") {
                println!("bg_latency_ms={}", started.elapsed().as_millis());
                passed();
                return;
            }
        }
    }
    panic!("no background.complete containing bg-ok arrived within 120 s");
}

#[tokio::test]
#[ignore]
async fn live_bg_session_listed_report() {
    let Some(link) = attach().await else {
        return;
    };
    let accepted = link
        .client
        .call_with::<hermes_contract::methods::legacy::prompt_background::Marker>(
            SideAgentParams {
                profile: None,
                session_id: link.session_id.clone(),
                text: BG_OK.to_string(),
            },
            call_options(15),
        )
        .await
        .unwrap_or_else(|error| panic!("prompt.background failed: {error}"));
    let live = list_scope(&link.client, "live").await;
    let stored = list_scope(&link.client, "stored").await;
    println!(
        "BG_SESSION_LISTED: live={} stored={}",
        scope_word(&live, &accepted.task_id),
        scope_word(&stored, &accepted.task_id)
    );
}

async fn list_scope(client: &Client, scope: &str) -> Result<Value, String> {
    client
        .call_raw(
            "session.list",
            json!({"scope": scope, "limit": 200}),
            call_options(30),
        )
        .await
        .map_err(|error| error.to_string())
}

fn scope_word(result: &Result<Value, String>, task_id: &str) -> String {
    match result {
        Err(reason) => format!("refused({reason})"),
        Ok(value) => {
            let rows = value
                .get("sessions")
                .or_else(|| value.get("items"))
                .and_then(Value::as_array);
            match rows {
                Some(rows) if rows.is_empty() => "empty".to_string(),
                Some(rows) => {
                    let found = rows.iter().any(|row| row.to_string().contains(task_id));
                    if found { "yes" } else { "no" }.to_string()
                }
                None if value.to_string().contains(task_id) => "yes".to_string(),
                None => "empty".to_string(),
            }
        }
    }
}

#[tokio::test]
#[ignore]
async fn live_process_streams_and_is_killed() {
    let Some(mut link) = attach().await else {
        return;
    };
    std::fs::write(link.folder.path().join("tick.sh"), TICK_SCRIPT).expect("tick.sh");
    submit(&link, TICK_PROMPT).await;
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut process_id = None;
    while Instant::now() < deadline && process_id.is_none() {
        if let Some(id) = running_process(&link.client, &link.session_id).await {
            process_id = Some(id);
            break;
        }
        let _ = next_inbound(&mut link.inbound, Duration::from_secs(1)).await;
    }
    let process_id = process_id.expect("no running process within 120 s");
    let read_until = Instant::now() + Duration::from_secs(10);
    let mut saw_output = false;
    while Instant::now() < read_until {
        let left = read_until.saturating_duration_since(Instant::now());
        let Some(item) = next_inbound(&mut link.inbound, left).await else {
            break;
        };
        if event_type(&item) == Some("agent.terminal.output") {
            saw_output = true;
        }
    }
    println!(
        "TERMINAL_OUTPUT_EVENTS: {}",
        if saw_output { "seen" } else { "not_seen" }
    );
    let kill_started = Instant::now();
    let _ = link
        .client
        .call_with::<hermes_contract::methods::legacy::process_kill::Marker>(
            ProcessKillParams {
                profile: None,
                session_id: link.session_id.clone(),
                process_id: process_id.clone(),
            },
            call_options(30),
        )
        .await
        .unwrap_or_else(|error| panic!("process.kill failed: {error}"));
    println!("KILL_LATENCY_MS: {}", kill_started.elapsed().as_millis());
    let listed = link
        .client
        .call_with::<hermes_contract::methods::legacy::process_list::Marker>(
            ProcessListParams {
                profile: None,
                session_id: link.session_id.clone(),
            },
            call_options(10),
        )
        .await
        .unwrap_or_else(|error| panic!("process.list failed: {error}"));
    let killed = listed
        .processes
        .iter()
        .any(|row| row.session_id == process_id && row.status == "killed");
    assert!(killed, "the row did not end killed");
    passed();
}

#[tokio::test]
#[ignore]
async fn live_stop_all_reports_count() {
    let Some(mut link) = attach().await else {
        return;
    };
    std::fs::write(link.folder.path().join("tick.sh"), TICK_SCRIPT).expect("tick.sh");
    submit(&link, TICK_PROMPT).await;
    submit(&link, TICK_PROMPT).await;
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut running = 0;
    while Instant::now() < deadline && running < 2 {
        running = running_count(&link.client, &link.session_id).await;
        let _ = next_inbound(&mut link.inbound, Duration::from_secs(1)).await;
    }
    assert!(running >= 2, "two processes did not start");
    let stopped = link
        .client
        .call_with::<hermes_contract::methods::legacy::process_stop::Marker>(
            ProcessStopParams {
                profile: None,
                session_id: Some(link.session_id.clone()),
            },
            call_options(60),
        )
        .await
        .unwrap_or_else(|error| panic!("process.stop failed: {error}"));
    assert!(stopped.killed >= 2, "killed {}", stopped.killed);
    let listed = link
        .client
        .call_with::<hermes_contract::methods::legacy::process_list::Marker>(
            ProcessListParams {
                profile: None,
                session_id: link.session_id.clone(),
            },
            call_options(10),
        )
        .await
        .unwrap_or_else(|error| panic!("process.list failed: {error}"));
    let ended = listed
        .processes
        .iter()
        .filter(|row| row.status == "killed")
        .count();
    assert!(ended >= 2, "killed rows {ended}");
    passed();
}

#[tokio::test]
#[ignore]
async fn live_bg_approval_report() {
    let Some(mut link) = attach().await else {
        return;
    };
    let probe = link.folder.path().join("talaria-bg-probe");
    std::fs::create_dir_all(&probe).expect("probe folder");
    let started = link
        .client
        .call_with::<hermes_contract::methods::legacy::prompt_background::Marker>(
            SideAgentParams {
                profile: None,
                session_id: link.session_id.clone(),
                text: APPROVAL_PROMPT.to_string(),
            },
            call_options(15),
        )
        .await;
    let mut what = match &started {
        Ok(result) => format!("started {}", result.task_id),
        Err(error) => format!("start_failed {error}"),
    };
    let mut refused = None;
    let deadline = Instant::now() + Duration::from_secs(330);
    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        let Some(item) = next_inbound(&mut link.inbound, left.min(Duration::from_secs(5))).await
        else {
            continue;
        };
        if event_type(&item) == Some("approval.request") {
            let session = event_session(&item).unwrap_or("").to_string();
            what = format!("approval.request session={session}");
            if let Some(reason) = deny_approval(&link.client, &item).await {
                refused = Some(reason);
            }
        } else if event_type(&item) == Some("background.complete") {
            let text = payload(&item)
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            what = format!("background.complete {text}");
            break;
        }
    }
    let exists = probe.exists();
    if let Some(reason) = refused {
        println!("BG_APPROVAL: {what} folder_exists={exists} respond_refused={reason}");
    } else {
        println!("BG_APPROVAL: {what} folder_exists={exists}");
    }
}

async fn submit(link: &LiveLink, text: &str) {
    let service = SessionService::with_client("live-background", link.client.clone());
    let _ = service
        .execute(Effect::SendPrompt {
            session_id: SessionId::new(link.session_id.clone()),
            text: text.to_string(),
            submission_id: format!("live-bg-{}", now_ms()),
            attachments: Vec::new(),
        })
        .await;
}

async fn running_process(client: &Client, session_id: &str) -> Option<String> {
    let listed = client
        .call_with::<hermes_contract::methods::legacy::process_list::Marker>(
            ProcessListParams {
                profile: None,
                session_id: session_id.to_string(),
            },
            call_options(10),
        )
        .await
        .ok()?;
    listed
        .processes
        .into_iter()
        .find(|row| row.status == "running")
        .map(|row| row.session_id)
}

async fn running_count(client: &Client, session_id: &str) -> usize {
    let Ok(listed) = client
        .call_with::<hermes_contract::methods::legacy::process_list::Marker>(
            ProcessListParams {
                profile: None,
                session_id: session_id.to_string(),
            },
            call_options(10),
        )
        .await
    else {
        return 0;
    };
    listed
        .processes
        .iter()
        .filter(|row| row.status == "running")
        .count()
}
