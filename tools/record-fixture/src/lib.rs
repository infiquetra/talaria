//! Record the seven live fixtures, replay one, and scan a directory for secrets.
//!
//! The script never starts a gateway, never runs `hermes`, never calls `config.set`, and never
//! sends `unattended`. Discovery uses `EnsurePolicy::Never`. A missing precondition is exit 2
//! with a plain message; nothing is written.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use hermes_client::{
    CallOptions, Client, ConnectOptions, DiscoverOptions, EnsurePolicy, Inbound, InboundStream,
    Purpose, ReplaySource, Scope, connect_with, discover_with, handshake, mint_ticket,
};
use serde_json::{Value, json};

/// The seven scenarios of the fixture set, in the order the script records them.
pub const SCENARIOS: [&str; 7] = [
    "plain-answer",
    "tool-diff",
    "approval",
    "clarification",
    "interrupted",
    "subagent-batch",
    "queued-prompt",
];

/// The shell command the approval scenario asks for. `manual` mode raises an approval for it.
const DELETE_COMMAND: &str = "rm -rf .pytest_cache";

/// What the operator must set when the approval scenario ran its command without a card.
const NO_APPROVAL_CARD: &str =
    "no approval card appeared; the profile `fixture` must set `approvals.mode: manual`";

/// How long each cleanup call may take.
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);

/// A scenario: its file name stem, prompts and what a good recording shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub name: String,
    pub prompts: Vec<String>,
    pub expected_statuses: Vec<String>,
}

impl Scenario {
    /// The scenario named `name`, or `None` when it is not one of [`SCENARIOS`].
    pub fn named(name: &str) -> Option<Scenario> {
        let (prompts, statuses): (&[&str], &[&str]) = match name {
            "plain-answer" => (
                &["Reply with exactly the single word: pong. Do not use any tools."],
                &["complete"],
            ),
            "tool-diff" => (
                &[
                    "In notes.txt change the word beta to gamma. Use your file editing tool, then reply with the single word: done.",
                ],
                &["complete"],
            ),
            "approval" => (
                &[
                    "Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache",
                ],
                &["complete"],
            ),
            "clarification" => (
                &[
                    "Before anything else, use the clarify tool to ask me which colour I prefer, with the choices red and blue. Then tell me the colour I chose.",
                ],
                &["complete"],
            ),
            "interrupted" => (
                &["Count from 1 to 500, one number per line, and do not stop early."],
                &["interrupted"],
            ),
            "subagent-batch" => (
                &[
                    "Use the delegate_task tool to run two subagents in parallel: one lists the files in the current directory, the other reads notes.txt. Then summarise both results in one sentence.",
                ],
                &["complete"],
            ),
            "queued-prompt" => (
                &[
                    "Count from 1 to 30, one number per line.",
                    "Reply with the single word: second.",
                ],
                &["complete", "complete"],
            ),
            _ => return None,
        };
        Some(Scenario {
            name: name.to_owned(),
            prompts: prompts.iter().map(|text| (*text).to_owned()).collect(),
            expected_statuses: statuses.iter().map(|text| (*text).to_owned()).collect(),
        })
    }
}

/// Why the script stops with exit code 2. The message is plain text for the operator.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct PreconditionError {
    pub message: String,
}

impl PreconditionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Everything `preflight` needs, injected so tests control it.
#[derive(Debug, Clone)]
pub struct PreflightInput {
    /// The Hermes home (`--home`), where discovery looks for the gateway.
    pub hermes_home: PathBuf,
    /// `--profile`.
    pub profile: String,
    pub login_name: String,
    pub host_name: String,
}

/// What a passing `preflight` learned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    pub profile_id: String,
    pub api_origin: String,
    pub code_sha: String,
    pub other_profiles: Vec<String>,
}

/// Discovery facts the substitution list is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryFacts {
    pub profile_id: String,
    pub api_origin: String,
}

/// Facts about the machine the script runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvFacts {
    pub home: String,
    pub login_name: String,
}

/// The settings a driver run needs.
#[derive(Debug, Clone)]
pub struct DriverConfig {
    pub model: String,
    pub provider: String,
    pub scratch: PathBuf,
    pub attempt_timeout: std::time::Duration,
}

/// How one attempt at a scenario ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptResult {
    Ok,
    Failed(String),
    PreconditionAbsent(String),
}

/// The scratch project directory: `notes.txt` and `.pytest_cache`. Removed on drop.
#[derive(Debug)]
pub struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    pub fn create() -> std::io::Result<ScratchDir> {
        let path = tempfile::Builder::new()
            .prefix("record-fixture-")
            .tempdir()?
            .keep();
        let scratch = ScratchDir { path };
        std::fs::write(scratch.path.join("notes.txt"), "alpha\nbeta\n")?;
        std::fs::create_dir(scratch.path.join(".pytest_cache"))?;
        Ok(scratch)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if std::fs::remove_dir_all(&self.path).is_err() && self.path.exists() {
            eprintln!("note: the scratch project could not be removed");
        }
    }
}

/// The minimum length of a login or host name: shorter ones can be neither replaced nor proved
/// absent.
const MIN_NAME: usize = 4;

/// Discovery that never starts a gateway.
pub fn never_ensure() -> DiscoverOptions {
    DiscoverOptions {
        control_timeout: Duration::from_secs(5),
        ensure: EnsurePolicy::Never,
    }
}

/// Check the preconditions without starting anything. Runs discovery with
/// `EnsurePolicy::Never`, so no `hermes` process is ever run.
pub async fn preflight(input: &PreflightInput) -> Result<Preflight, PreconditionError> {
    if input.login_name.chars().count() < MIN_NAME {
        return Err(PreconditionError::new(
            "the login name is unset or under four characters, so a recording cannot be proved \
             free of it; record as a user whose login name has four or more characters",
        ));
    }
    if input.host_name.chars().count() < MIN_NAME {
        return Err(PreconditionError::new(
            "the host name is under four characters, so a recording cannot be proved free of \
             it; record on a machine whose host name has four or more characters",
        ));
    }
    let endpoint = discover_with(&input.hermes_home, &never_ensure())
        .await
        .map_err(|error| {
            PreconditionError::new(format!(
                "{error}. Set up the gateway first: {}",
                error.next_step()
            ))
        })?;
    let pinned = hermes_contract::pinned::COMMIT;
    let code_sha = endpoint.code_sha().unwrap_or_default().to_lowercase();
    if code_sha.is_empty() {
        return Err(PreconditionError::new(format!(
            "the gateway did not report its code commit; run Hermes at the pinned commit {} \
             and restart its gateway",
            &pinned[..12]
        )));
    }
    if code_sha.len() < 7 || !pinned.starts_with(&code_sha) {
        return Err(PreconditionError::new(format!(
            "the gateway runs a different Hermes commit; update Hermes to the pinned commit {} \
             and restart its gateway",
            &pinned[..12]
        )));
    }
    let profile = endpoint.resolve_profile(&input.profile).map_err(|error| {
        PreconditionError::new(format!(
            "{error}. The gateway must serve the profile `{}`: {}",
            input.profile,
            error.next_step()
        ))
    })?;

    let unreachable = |what: &str, next: &str| {
        PreconditionError::new(format!("the gateway could not be reached ({what}). {next}"))
    };
    let ticket = mint_ticket(
        &input.hermes_home,
        &endpoint,
        &profile,
        Purpose::Interactive,
        Scope::profile(),
    )
    .await
    .map_err(|error| unreachable(&error.to_string(), error.next_step()))?;
    let mut connection = connect_with(&endpoint, ticket, &ConnectOptions::default())
        .await
        .map_err(|error| unreachable(&error.to_string(), error.next_step()))?;
    handshake(&mut connection)
        .await
        .map_err(|error| unreachable(&error.to_string(), error.next_step()))?;
    let (client, _inbound) = Client::start(connection.into_parts());
    // No `profile`: the ticket already binds the connection to the profile.
    let mode = client
        .call_raw(
            "config.get",
            json!({"key": "approvals.mode"}),
            call_options(Duration::from_secs(30)),
        )
        .await;
    client.close().await;
    let mode = mode.map_err(|error| unreachable(&error.to_string(), error.next_step()))?;
    if mode.get("value").and_then(Value::as_str) != Some("manual") {
        return Err(PreconditionError::new(format!(
            "the profile `{}` must set `approvals.mode: manual`; the gateway reports another \
             mode, so a guardian model could approve the scenario's command",
            input.profile
        )));
    }

    let mut other_profiles: Vec<String> = endpoint
        .profiles()
        .iter()
        .map(|served| served.name.clone())
        .chain(endpoint.parked().iter().map(|(name, _)| name.clone()))
        .filter(|name| *name != input.profile && name != "default")
        .collect();
    other_profiles.sort();
    other_profiles.dedup();
    Ok(Preflight {
        profile_id: profile.as_ref().to_owned(),
        api_origin: endpoint.api_origin().to_owned(),
        code_sha,
        other_profiles,
    })
}

fn call_options(timeout: Duration) -> CallOptions {
    CallOptions {
        timeout: Some(timeout),
    }
}

/// `(event type, params)` of every inbound event frame of a recording.
fn recorded_events(source: &ReplaySource) -> Vec<(String, Value)> {
    source
        .inbound()
        .filter(|frame| frame.frame["method"] == "event")
        .filter_map(|frame| {
            let params = &frame.frame["params"];
            Some((params["type"].as_str()?.to_owned(), params.clone()))
        })
        .collect()
}

fn position(events: &[(String, Value)], kind: &str, from: usize) -> Option<usize> {
    events
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, (event, _))| event == kind)
        .map(|(index, _)| index)
}

/// The required evidence and final statuses of section 8.4 over a recorded file.
pub fn check_evidence(scenario: &Scenario, source: &ReplaySource) -> Result<(), String> {
    let events = recorded_events(source);
    let statuses: Vec<String> = events
        .iter()
        .filter(|(kind, _)| kind == "message.complete")
        .map(|(_, params)| {
            params["payload"]["status"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    if statuses != scenario.expected_statuses {
        return Err(format!(
            "the turns ended {statuses:?}, expected {:?}",
            scenario.expected_statuses
        ));
    }
    let header = &source.header().expected_statuses;
    if !header.is_empty() && *header != scenario.expected_statuses {
        return Err("the header lists other expected statuses".to_owned());
    }
    let has = |kind: &str| position(&events, kind, 0).is_some();
    let ordered = |first: &str, then: &str| {
        position(&events, first, 0).is_some_and(|at| position(&events, then, at + 1).is_some())
    };
    match scenario.name.as_str() {
        "plain-answer" => {
            if !has("message.delta") {
                return Err("no message.delta".to_owned());
            }
            if has("tool.start") {
                return Err("a tool was used".to_owned());
            }
        }
        "tool-diff" => {
            let diff = events.iter().any(|(kind, params)| {
                kind == "tool.complete"
                    && params["payload"]["inline_diff"]
                        .as_str()
                        .is_some_and(|diff| !diff.is_empty())
            });
            if !diff {
                return Err("no tool.complete with a non-empty inline_diff".to_owned());
            }
        }
        "approval" => {
            if !ordered("approval.request", "approval.settled") {
                return Err("no approval.request followed by approval.settled".to_owned());
            }
        }
        "clarification" => {
            if !ordered("clarify.request", "clarify.settled") {
                return Err("no clarify.request followed by clarify.settled".to_owned());
            }
        }
        "interrupted" => {}
        "subagent-batch" => {
            if !has("subagent.start") || !has("subagent.complete") {
                return Err("no subagent.start and subagent.complete".to_owned());
            }
        }
        "queued-prompt" => {
            let first_complete = position(&events, "message.complete", 0).unwrap_or(usize::MAX);
            let second_start = first_complete
                .checked_add(1)
                .and_then(|from| position(&events, "message.start", from));
            if second_start.is_none() {
                return Err("the second turn did not start after the first completed".to_owned());
            }
        }
        other => return Err(format!("unknown scenario {other}")),
    }
    Ok(())
}

/// The `(needle, replacement)` pairs of section 4.7.
pub fn build_substitutions(
    facts: &DiscoveryFacts,
    env: &EnvFacts,
    scratch: &Path,
) -> Vec<(String, String)> {
    let authority = facts
        .api_origin
        .split_once("://")
        .map_or(facts.api_origin.as_str(), |(_, rest)| rest);
    let authority = authority.split('/').next().unwrap_or(authority);
    let scratch_text = scratch.to_string_lossy().into_owned();
    let resolved = std::fs::canonicalize(scratch)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| scratch_text.clone());
    let candidates = [
        (facts.profile_id.clone(), "<profile>"),
        (env.home.clone(), "<home>"),
        (scratch_text, "<tmp>"),
        (resolved, "<tmp>"),
        (authority.to_owned(), "<gateway>"),
        (env.login_name.clone(), "<user>"),
    ];
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (needle, replacement) in candidates {
        if needle.chars().count() >= MIN_NAME
            && needle != "fixture"
            && !pairs.iter().any(|(known, _)| *known == needle)
        {
            pairs.push((needle, replacement.to_owned()));
        }
    }
    pairs
}

/// What the driver has seen of the session so far.
#[derive(Debug, Default)]
struct Session {
    session_id: String,
    subscription_id: Value,
    execution_generation: Value,
    /// `(prompt_id, execution_generation)` of approvals not yet settled or cancelled.
    pending_approvals: Vec<(Value, Value)>,
    pending_clarifications: Vec<(Value, Value)>,
    turn_running: bool,
    statuses: Vec<String>,
    approval_seen: bool,
    approval_answered: bool,
    clarify_answered: bool,
    delete_ran: bool,
    interrupt_sent: bool,
    submitted: usize,
    subagent_complete_seen: bool,
}

/// How long the driver keeps listening after the last turn ends for a delegated child that
/// is still running: the gateway returns from the delegation call at once, so the child's
/// completion can arrive after the turn's own.
const SUBAGENT_LINGER: Duration = Duration::from_secs(60);

/// A submission id no other submission of this process shares.
fn submission_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!(
        "record-fixture-{started:x}-{}",
        NEXT.fetch_add(1, Ordering::SeqCst)
    )
}

/// `prompt.submit` with exactly `session_id`, `text` and `submission_id`; never `unattended`.
fn submit_params(session_id: &str, text: &str) -> Result<Value, String> {
    let params =
        hermes_contract::overrides::prompt_submit_params(session_id, text, submission_id());
    hermes_contract::overrides::check_prompt_submit(&params).map_err(|error| error.to_string())?;
    let encoded = serde_json::to_value(&params).map_err(|_| "prompt.submit did not encode")?;
    let mut kept = serde_json::Map::new();
    for key in ["session_id", "text", "submission_id"] {
        if let Some(value) = encoded.get(key) {
            kept.insert(key.to_owned(), value.clone());
        }
    }
    Ok(Value::Object(kept))
}

/// Drive one attempt over a connected client and its inbound stream (the events come
/// only from the stream `Client::start` returned). Never writes the file itself.
pub async fn run_scenario(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    config: &DriverConfig,
) -> AttemptResult {
    let mut session = Session::default();
    let outcome = tokio::time::timeout(
        config.attempt_timeout,
        drive(client, inbound, scenario, config, &mut session),
    )
    .await
    .unwrap_or_else(|_| {
        AttemptResult::Failed(format!(
            "the attempt ran longer than {} s",
            config.attempt_timeout.as_secs()
        ))
    });
    if !session.session_id.is_empty() {
        cleanup(client, &mut session).await;
    }
    outcome
}

/// Every attempt ends alike: deny or skip what is pending, interrupt only a running turn, then
/// detach. Never interrupt while an approval is pending.
async fn cleanup(client: &Client, session: &mut Session) {
    let options = call_options(CLEANUP_TIMEOUT);
    for (prompt_id, generation) in std::mem::take(&mut session.pending_approvals) {
        let _ = client
            .call_raw(
                "approval.respond",
                json!({"session_id": session.session_id, "execution_generation": generation,
                       "prompt_id": prompt_id, "choice": "deny"}),
                options,
            )
            .await;
    }
    for (prompt_id, generation) in std::mem::take(&mut session.pending_clarifications) {
        let _ = client
            .call_raw(
                "clarify.respond",
                json!({"session_id": session.session_id, "execution_generation": generation,
                       "prompt_id": prompt_id, "answer": "skip"}),
                options,
            )
            .await;
    }
    if session.turn_running {
        let _ = client
            .call_raw(
                "session.interrupt",
                json!({"session_id": session.session_id,
                       "execution_generation": session.execution_generation}),
                options,
            )
            .await;
    }
    let _ = client
        .call_raw(
            "session.detach",
            json!({"session_id": session.session_id,
                   "subscription_id": session.subscription_id}),
            options,
        )
        .await;
}

fn same_path(left: &str, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// The provenance check of section 8.4 on the snapshot's `info` block.
fn check_info(info: &Value, config: &DriverConfig) -> Result<(), String> {
    let provider = info["provider"].as_str();
    if provider != Some(config.provider.as_str()) {
        return Err("the session reports another provider than requested".to_owned());
    }
    let model_matches = info["model"].as_str().is_some_and(|model| {
        model == config.model || model.rsplit('/').next() == Some(config.model.as_str())
    });
    if !model_matches {
        return Err("the session reports another model than requested".to_owned());
    }
    if !info["cwd"]
        .as_str()
        .is_some_and(|cwd| same_path(cwd, &config.scratch))
    {
        return Err("the session reports another working directory than requested".to_owned());
    }
    Ok(())
}

async fn drive(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    config: &DriverConfig,
    session: &mut Session,
) -> AttemptResult {
    let failed = |reason: &str| AttemptResult::Failed(reason.to_owned());
    let created = client
        .call_raw(
            "session.create",
            json!({"source": "cli", "cwd": config.scratch.to_string_lossy(),
                   "model": config.model, "provider": config.provider}),
            CallOptions::default(),
        )
        .await;
    let snapshot = match created {
        Ok(snapshot) => snapshot,
        Err(error) => return AttemptResult::Failed(format!("session.create failed: {error}")),
    };
    let Some(session_id) = snapshot["session_id"].as_str() else {
        return failed("the session snapshot has no session_id");
    };
    session.session_id = session_id.to_owned();
    session.subscription_id = snapshot["subscription_id"].clone();
    session.execution_generation = snapshot["execution_generation"].clone();
    if let Err(reason) = check_info(&snapshot["info"], config) {
        return AttemptResult::Failed(reason);
    }

    if let Err(reason) = submit(client, session, &scenario.prompts[0]).await {
        return AttemptResult::Failed(reason);
    }
    while session.statuses.len() < scenario.expected_statuses.len() {
        let Some(item) = inbound.next().await else {
            return failed("the inbound stream ended");
        };
        match item {
            Inbound::Event(event) => {
                if let Some(result) = on_event(client, scenario, session, &event.raw).await {
                    return result;
                }
            }
            Inbound::ServerRequest(request) => {
                let _ = request
                    .responder
                    .fail(-32601, "the fixture script does not answer this request")
                    .await;
            }
            Inbound::Closed(_) => return failed("the connection closed during the attempt"),
            Inbound::Malformed(_) => {}
        }
    }
    if scenario.name == "subagent-batch" {
        let deadline = tokio::time::Instant::now() + SUBAGENT_LINGER;
        while !session.subagent_complete_seen {
            match tokio::time::timeout_at(deadline, inbound.next()).await {
                Ok(Some(Inbound::Event(event))) => {
                    if event.raw["type"] == "subagent.complete" {
                        session.subagent_complete_seen = true;
                    }
                }
                Ok(Some(Inbound::ServerRequest(request))) => {
                    let _ = request
                        .responder
                        .fail(-32601, "the fixture script does not answer this request")
                        .await;
                }
                Ok(Some(Inbound::Malformed(_))) => {}
                Ok(Some(Inbound::Closed(_)) | None) | Err(_) => break,
            }
        }
    }
    if scenario.name == "approval" && !session.approval_seen && session.delete_ran {
        return AttemptResult::PreconditionAbsent(NO_APPROVAL_CARD.to_owned());
    }
    if session.statuses != scenario.expected_statuses {
        return AttemptResult::Failed(format!(
            "the turns ended {:?}, expected {:?}",
            session.statuses, scenario.expected_statuses
        ));
    }
    AttemptResult::Ok
}

async fn submit(client: &Client, session: &mut Session, text: &str) -> Result<(), String> {
    let params = submit_params(&session.session_id, text)?;
    session.submitted += 1;
    // A submitted prompt is a running turn until its `message.complete`, even before
    // `message.start` arrives: a cleanup that runs in between must still interrupt it.
    session.turn_running = true;
    client
        .call_raw("prompt.submit", params, CallOptions::default())
        .await
        .map(|_| ())
        .map_err(|error| format!("prompt.submit failed: {error}"))
}

/// Handle one event of the session. `Some` ends the attempt.
async fn on_event(
    client: &Client,
    scenario: &Scenario,
    session: &mut Session,
    params: &Value,
) -> Option<AttemptResult> {
    if params["session_id"]
        .as_str()
        .is_some_and(|id| id != session.session_id)
    {
        return None;
    }
    if !params["execution_generation"].is_null() {
        session.execution_generation = params["execution_generation"].clone();
    }
    let payload = &params["payload"];
    match params["type"].as_str().unwrap_or_default() {
        "subagent.complete" => session.subagent_complete_seen = true,
        "message.start" => session.turn_running = true,
        "message.delta" => {
            if scenario.name == "interrupted" && !session.interrupt_sent {
                session.interrupt_sent = true;
                let interrupted = client
                    .call_raw(
                        "session.interrupt",
                        json!({"session_id": session.session_id,
                               "execution_generation": session.execution_generation}),
                        CallOptions::default(),
                    )
                    .await;
                if let Err(error) = interrupted {
                    return Some(AttemptResult::Failed(format!(
                        "session.interrupt failed: {error}"
                    )));
                }
            }
            if scenario.prompts.len() > session.submitted
                && let Err(reason) =
                    submit(client, session, &scenario.prompts[session.submitted]).await
            {
                return Some(AttemptResult::Failed(reason));
            }
        }
        "message.complete" => {
            session
                .statuses
                .push(payload["status"].as_str().unwrap_or_default().to_owned());
            session.turn_running = session.statuses.len() < session.submitted;
            if session.submitted < scenario.prompts.len() {
                return Some(AttemptResult::Failed(
                    "a turn completed before the next prompt was queued".to_owned(),
                ));
            }
        }
        "tool.complete" => {
            let mentions = |value: &Value| {
                value
                    .as_str()
                    .is_some_and(|text| text.contains(DELETE_COMMAND))
            };
            if mentions(&payload["args"]["command"])
                || mentions(&payload["summary"])
                || mentions(&payload["context"])
            {
                session.delete_ran = true;
            }
        }
        "approval.request" => {
            session.approval_seen = true;
            let prompt = (
                payload["prompt_id"].clone(),
                payload_generation(payload, session),
            );
            session.pending_approvals.push(prompt.clone());
            let expected = scenario.name == "approval" && !session.approval_answered;
            let choice = if expected { "once" } else { "deny" };
            session.approval_answered |= expected;
            let answered = client
                .call_raw(
                    "approval.respond",
                    json!({"session_id": session.session_id, "execution_generation": prompt.1,
                           "prompt_id": prompt.0, "choice": choice}),
                    CallOptions::default(),
                )
                .await;
            if !expected {
                return Some(AttemptResult::Failed(
                    "an approval nobody expected appeared; it was denied".to_owned(),
                ));
            }
            if let Err(error) = answered {
                return Some(AttemptResult::Failed(format!(
                    "approval.respond failed: {error}"
                )));
            }
        }
        "approval.settled" => {
            let prompt_id = &payload["prompt_id"];
            session
                .pending_approvals
                .retain(|(pending, _)| pending != prompt_id);
        }
        "approval.cancelled" => session.pending_approvals.clear(),
        "clarify.request" => {
            let prompt = (
                payload["prompt_id"].clone(),
                payload_generation(payload, session),
            );
            session.pending_clarifications.push(prompt.clone());
            let expected = scenario.name == "clarification" && !session.clarify_answered;
            let answer = if expected { "blue" } else { "skip" };
            session.clarify_answered |= expected;
            let answered = client
                .call_raw(
                    "clarify.respond",
                    json!({"session_id": session.session_id, "execution_generation": prompt.1,
                           "prompt_id": prompt.0, "answer": answer}),
                    CallOptions::default(),
                )
                .await;
            if !expected {
                return Some(AttemptResult::Failed(
                    "a clarification nobody expected appeared; it was skipped".to_owned(),
                ));
            }
            if let Err(error) = answered {
                return Some(AttemptResult::Failed(format!(
                    "clarify.respond failed: {error}"
                )));
            }
        }
        "clarify.settled" => {
            let prompt_id = &payload["prompt_id"];
            session
                .pending_clarifications
                .retain(|(pending, _)| pending != prompt_id);
        }
        _ => {}
    }
    None
}

fn payload_generation(payload: &Value, session: &Session) -> Value {
    if payload["execution_generation"].is_null() {
        session.execution_generation.clone()
    } else {
        payload["execution_generation"].clone()
    }
}
