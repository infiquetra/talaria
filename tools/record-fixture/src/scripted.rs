//! Scenarios that are a fixed list of raw requests rather than one prompt.
//!
//! Each scenario here sends JSON-RPC requests through the same client the prompt scenarios
//! use, waits for the events its steps expect, and leaves every wire frame to the recorder.
//! None of them calls Talaria's own reducers, none sends `unattended`, none writes
//! `approvals.mode`, and every one undoes the state it changed before it detaches.

use std::time::Duration;

use hermes_client::{
    CallError, CallOptions, Client, Inbound, InboundStream, ReplaySource, RpcError,
};
use hermes_contract::{methods::legacy as native, native::NativeMethod};
use serde_json::{Value, json};
use tokio::time::Instant;

use crate::{AttemptResult, DriverConfig, Scenario, call_options, check_info, submit_params};

/// The scenarios of this module, in the order they are recorded.
pub const SCRIPTED_SCENARIOS: [&str; 10] = [
    "queue-edit-remove",
    "queue-steer",
    "queue-move-send-now",
    "model-switch",
    "permissions-cd",
    "shell-and-btw",
    "background-work",
    "bg-launch",
    "review-session-info",
    "review-oneshot",
];

/// Whether `name` is one of [`SCRIPTED_SCENARIOS`].
pub fn handles(name: &str) -> bool {
    SCRIPTED_SCENARIOS.contains(&name)
}

const SLEEP_PROMPT: &str =
    "Run the shell command sleep 20, then reply with the word done in capital letters.";
const STEER_PROMPT: &str = "Run sleep 15 and then sleep 15 as two separate shell commands. Then reply with the code word I gave you followed by the digits 42 with no space, or NONE.";
const REWIND_FIRST: &str = "Create a file named notes.txt containing exactly the line alpha. Use your file writing tool, not the terminal. Reply with the word done.";
const REWIND_SECOND: &str = "In notes.txt replace alpha with beta using your patch or file tool, not the terminal. Reply with the word done.";
const PICKER_PROMPT: &str = "Reply with exactly the single word: pong. Do not use any tools.";
/// The prompt of the shell scenario. The side question asks which word it answered with.
const SHELL_PROMPT: &str = "Reply with the single word ready.";
/// The side question of the shell scenario, asked while the turn is over.
const BTW_QUESTION: &str = "What single word did you just reply with?";
const BACKGROUND_PROMPT: &str = "Run the command sh tick.sh in the background with the terminal tool (background=true, notify=false). Reply with only its process id.";
const NUMBER_WORDS: [&str; 16] = [
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve", "thirteen", "fourteen", "fifteen", "sixteen",
];

fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

/// The prompts and expected final statuses of a scripted scenario.
pub(crate) fn scenario(name: &str) -> Option<Scenario> {
    if !handles(name) {
        return None;
    }
    let (prompts, statuses): (Vec<String>, Vec<String>) = match name {
        "queue-edit-remove" => (
            owned(&[SLEEP_PROMPT]),
            owned(&["cancelled", "cancelled", "complete", "complete", "complete"]),
        ),
        "queue-steer" => (owned(&[STEER_PROMPT]), owned(&["complete", "steered"])),
        "queue-move-send-now" => (
            owned(&[SLEEP_PROMPT]),
            owned(&[
                "cancelled",
                "interrupted",
                "complete",
                "complete",
                "complete",
            ]),
        ),
        "model-switch" | "permissions-cd" => (Vec::new(), Vec::new()),
        "shell-and-btw" => (owned(&[SHELL_PROMPT]), owned(&["complete"])),
        "background-work" => (owned(&[BACKGROUND_PROMPT]), owned(&["complete"])),
        // Detach without waiting, so a recording is not required to contain a completed turn.
        "bg-launch" => (Vec::new(), Vec::new()),
        // No turn runs: the review only reads the session and asks for one message.
        "review-session-info" | "review-oneshot" => (Vec::new(), Vec::new()),
        _ => return None,
    };
    Some(Scenario {
        name: name.to_owned(),
        prompts,
        expected_statuses: statuses,
    })
}

/// Why a scripted attempt stopped.
enum Stop {
    Fail(String),
    Absent(String),
}

impl From<String> for Stop {
    fn from(reason: String) -> Self {
        Stop::Fail(reason)
    }
}

impl From<&str> for Stop {
    fn from(reason: &str) -> Self {
        Stop::Fail(reason.to_owned())
    }
}

type Step<T = ()> = Result<T, Stop>;

/// How long one wait for an event may take unless a step says otherwise.
const EVENT_WAIT: Duration = Duration::from_secs(30);
/// How long a model turn may take.
const TURN_WAIT: Duration = Duration::from_secs(180);
/// How long a state change announced by an event may take after its command.
const UPDATE_WAIT: Duration = Duration::from_secs(5);

/// One running scripted attempt: the connection, the session and what has been seen.
struct Run<'a> {
    client: &'a Client,
    inbound: &'a mut InboundStream,
    session_id: String,
    subscription_id: Value,
    revision: i64,
    generation: i64,
    events: Vec<(String, Value)>,
    statuses: Vec<String>,
    running: bool,
    pending: Option<usize>,
    closed: bool,
    unexpected: Option<String>,
    pending_approvals: Vec<(Value, Value)>,
    /// Requests that put state back, run newest first by `cleanup`.
    undo: Vec<(&'static str, Value)>,
    expected_tick_approval: bool,
    tick_approval_answered: bool,
    background_cleanup: bool,
}

fn number(value: &Value) -> Option<i64> {
    value.as_i64()
}

impl<'a> Run<'a> {
    fn new(client: &'a Client, inbound: &'a mut InboundStream) -> Run<'a> {
        Run {
            client,
            inbound,
            session_id: String::new(),
            subscription_id: Value::Null,
            revision: 0,
            generation: 0,
            events: Vec::new(),
            statuses: Vec::new(),
            running: false,
            pending: None,
            closed: false,
            unexpected: None,
            pending_approvals: Vec::new(),
            undo: Vec::new(),
            expected_tick_approval: false,
            tick_approval_answered: false,
            background_cleanup: false,
        }
    }

    fn progress(&self, label: &str) {
        eprintln!("  {label}");
    }

    fn adopt(&mut self, revision: Option<i64>, generation: Option<i64>) {
        if let Some(revision) = revision {
            self.revision = self.revision.max(revision);
        }
        if let Some(generation) = generation {
            self.generation = self.generation.max(generation);
        }
    }

    async fn observe(&mut self, item: Inbound) {
        match item {
            Inbound::Event(event) => self.on_event(event.raw).await,
            Inbound::ServerRequest(request) => {
                let _ = request
                    .responder
                    .fail(-32601, "the fixture script does not answer this request")
                    .await;
            }
            Inbound::Closed(_) => self.closed = true,
            Inbound::Malformed(_) => {}
        }
    }

    async fn on_event(&mut self, params: Value) {
        if params["session_id"]
            .as_str()
            .is_some_and(|id| id != self.session_id)
            && !(self.expected_tick_approval && params["type"] == "approval.request")
        {
            return;
        }
        let kind = params["type"].as_str().unwrap_or_default().to_owned();
        let payload = params["payload"].clone();
        self.adopt(None, number(&params["execution_generation"]));
        match kind.as_str() {
            "session.info" | "session.updated" => {
                self.adopt(
                    number(&payload["revision"]),
                    number(&payload["execution_generation"]),
                );
                if kind == "session.info" {
                    if let Some(running) = payload["running"].as_bool() {
                        self.running = running;
                    }
                    if let Some(pending) = payload["pending"].as_array() {
                        self.pending = Some(pending.len());
                    }
                }
            }
            "message.start" => self.running = true,
            "message.complete" => {
                self.running = false;
                self.statuses.push(crate::turn_status(&payload));
            }
            "approval.request" => {
                if self.expected_tick_approval {
                    let expected = !self.tick_approval_answered
                        && params["session_id"] == self.session_id
                        && payload["command"].as_str() == Some("sh tick.sh");
                    let choice = if expected { "once" } else { "deny" };
                    let result = self
                        .client
                        .call_raw(
                            "approval.respond",
                            json!({"session_id": params["session_id"],
                               "request_id": payload["prompt_id"], "choice": choice}),
                            call_options(Duration::from_secs(30)),
                        )
                        .await;
                    if expected && result.is_ok() {
                        self.tick_approval_answered = true;
                    } else {
                        self.unexpected = Some(if expected {
                            "the expected tick approval could not be answered".into()
                        } else {
                            "an unexpected approval appeared; it was denied".into()
                        });
                    }
                    self.events.push((kind, params));
                    return;
                }
                let prompt = (
                    payload["prompt_id"].clone(),
                    self.event_generation(&payload),
                );
                self.pending_approvals.push(prompt.clone());
                let _ = self
                    .client
                    .call_raw(
                        "approval.respond",
                        json!({"session_id": self.session_id, "execution_generation": prompt.1,
                               "prompt_id": prompt.0, "choice": "deny"}),
                        CallOptions::default(),
                    )
                    .await;
                self.unexpected =
                    Some("an approval nobody expected appeared; it was denied".into());
            }
            "approval.settled" => {
                let prompt_id = &payload["prompt_id"];
                self.pending_approvals
                    .retain(|(pending, _)| pending != prompt_id);
            }
            "clarify.request" => {
                let _ = self
                    .client
                    .call_raw(
                        "clarify.respond",
                        json!({"session_id": self.session_id,
                               "execution_generation": self.event_generation(&payload),
                               "prompt_id": payload["prompt_id"], "answer": "skip"}),
                        CallOptions::default(),
                    )
                    .await;
                self.unexpected =
                    Some("a clarification nobody expected appeared; it was skipped".into());
            }
            _ => {}
        }
        self.events.push((kind, params));
    }

    fn event_generation(&self, payload: &Value) -> Value {
        if payload["execution_generation"].is_null() {
            json!(self.generation)
        } else {
            payload["execution_generation"].clone()
        }
    }

    /// Send one request and keep reading events while its reply is awaited.
    async fn call_with(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, CallError> {
        let client: &'a Client = self.client;
        let call = client.call_raw(method, params, call_options(timeout));
        tokio::pin!(call);
        loop {
            tokio::select! {
                result = &mut call => return result,
                item = self.inbound.next() => match item {
                    Some(item) => self.observe(item).await,
                    None => return Err(CallError::Unknown { method: method.to_owned() }),
                },
            }
        }
    }

    /// A request that must succeed.
    async fn call(&mut self, method: &str, params: Value) -> Step<Value> {
        self.call_for(method, params, Duration::from_secs(60)).await
    }

    /// Native calls retain the generated parameter and result types while events are read.
    async fn native<M: NativeMethod>(
        &mut self,
        params: M::Params,
        timeout: Duration,
    ) -> Step<M::Result> {
        let client: &'a Client = self.client;
        let call = client.call_with::<M>(params, call_options(timeout));
        tokio::pin!(call);
        loop {
            tokio::select! {
                result = &mut call => return result.map_err(|error|
                    Stop::Fail(format!("{} failed: {}", M::NAME, describe(&error)))),
                item = self.inbound.next() => match item {
                    Some(item) => self.observe(item).await,
                    None => return Err("the inbound stream ended".into()),
                },
            }
        }
    }

    async fn call_for(&mut self, method: &str, params: Value, timeout: Duration) -> Step<Value> {
        self.call_with(method, params, timeout)
            .await
            .map_err(|error| Stop::Fail(format!("{method} failed: {}", describe(&error))))
    }

    /// A request whose refusal is part of the recording.
    async fn call_maybe(&mut self, method: &str, params: Value) -> Step<Result<Value, RpcError>> {
        match self
            .call_with(method, params, Duration::from_secs(60))
            .await
        {
            Ok(value) => Ok(Ok(value)),
            Err(CallError::Rpc(error)) => Ok(Err(error)),
            Err(error) => Err(Stop::Fail(format!("{method} failed: {}", describe(&error)))),
        }
    }

    async fn wait_until<F>(&mut self, what: &str, timeout: Duration, done: F) -> Step
    where
        F: Fn(&Run<'a>) -> bool,
    {
        let deadline = Instant::now() + timeout;
        while !done(self) {
            if let Some(reason) = self.unexpected.clone() {
                return Err(Stop::Fail(reason));
            }
            if self.closed {
                return Err("the connection closed during the attempt".into());
            }
            match tokio::time::timeout_at(deadline, self.inbound.next()).await {
                Ok(Some(item)) => self.observe(item).await,
                Ok(None) => return Err("the inbound stream ended".into()),
                Err(_) => return Err(Stop::Fail(format!("timed out waiting for {what}"))),
            }
        }
        if let Some(reason) = self.unexpected.clone() {
            return Err(Stop::Fail(reason));
        }
        Ok(())
    }

    /// Keep reading events for `span`.
    async fn linger(&mut self, span: Duration) {
        let deadline = Instant::now() + span;
        while let Ok(Some(item)) = tokio::time::timeout_at(deadline, self.inbound.next()).await {
            self.observe(item).await;
        }
    }

    /// Wait for an event of `kind` seen after the first `mark` events whose params satisfy `test`.
    async fn wait_event<F>(
        &mut self,
        kind: &str,
        mark: usize,
        timeout: Duration,
        test: F,
    ) -> Step<Value>
    where
        F: Fn(&Value) -> bool,
    {
        let found = |run: &Run<'a>| run.find_event(kind, mark, &test).is_some();
        if let Err(stop) = self.wait_until(kind, timeout, found).await {
            if std::env::var_os("RECORD_FIXTURE_DEBUG").is_some()
                && let Some((_, params)) = self.events.iter().skip(mark).rfind(|(k, _)| k == kind)
            {
                let shown: String = params["payload"].to_string().chars().take(900).collect();
                eprintln!("  debug: last {kind}: {shown}");
            }
            return Err(stop);
        }
        Ok(self.find_event(kind, mark, &test).unwrap_or(Value::Null))
    }

    fn find_event<F>(&self, kind: &str, mark: usize, test: &F) -> Option<Value>
    where
        F: Fn(&Value) -> bool,
    {
        self.events
            .iter()
            .skip(mark)
            .find(|(event, params)| event == kind && test(&params["payload"]))
            .map(|(_, params)| params["payload"].clone())
    }

    async fn create(&mut self, config: &DriverConfig) -> Step {
        let snapshot = self
            .call(
                "session.create",
                json!({"source": "cli", "cwd": config.scratch.to_string_lossy(),
                       "model": config.model, "provider": config.provider}),
            )
            .await?;
        self.adopt_snapshot(&snapshot)?;
        check_info(&snapshot["info"], config)?;
        Ok(())
    }

    fn adopt_snapshot(&mut self, snapshot: &Value) -> Step {
        let Some(session_id) = snapshot["session_id"].as_str() else {
            return Err("the session snapshot has no session_id".into());
        };
        self.session_id = session_id.to_owned();
        self.subscription_id = snapshot["subscription_id"].clone();
        self.adopt(
            number(&snapshot["revision"]),
            number(&snapshot["execution_generation"]),
        );
        Ok(())
    }

    /// Submit a prompt and return the admission receipt.
    async fn submit(&mut self, text: &str) -> Step<Value> {
        let params = submit_params(&self.session_id, text)?;
        self.call("prompt.submit", params).await
    }

    /// Submit one prompt and wait until its turn ends `complete`.
    async fn turn(&mut self, text: &str) -> Step {
        let before = self.statuses.len();
        self.submit(text).await?;
        self.wait_until("the turn to end", TURN_WAIT, |run| {
            run.statuses.len() > before
        })
        .await?;
        match self.statuses.get(before).map(String::as_str) {
            Some("complete") => Ok(()),
            other => Err(Stop::Fail(format!(
                "a turn ended {other:?}, expected complete"
            ))),
        }
    }

    async fn slash(&mut self, command: &str) -> Step<Value> {
        self.call(
            "slash.exec",
            json!({"session_id": self.session_id, "command": command}),
        )
        .await
    }

    async fn config_get(&mut self, key: &str) -> Step<Value> {
        let result = self
            .call(
                "config.get",
                json!({"session_id": self.session_id, "key": key}),
            )
            .await?;
        Ok(result["value"].clone())
    }

    async fn resume(&mut self) -> Step<Value> {
        let snapshot = self
            .call("session.resume", json!({"session_id": self.session_id}))
            .await?;
        self.adopt(
            number(&snapshot["revision"]),
            number(&snapshot["execution_generation"]),
        );
        Ok(snapshot)
    }

    /// `session.mutate` with the tracked fences. A fence refusal adopts the numbers it names
    /// and the request is sent once more under a new id.
    async fn mutate(&mut self, operation: &str, payload: Value, timeout: Duration) -> Step<Value> {
        for attempt in 0..2 {
            let params = json!({
                "session_id": self.session_id,
                "request_id": crate::submission_id(),
                "expected_revision": self.revision,
                "expected_generation": self.generation,
                "operation": operation,
                "payload": payload,
            });
            match self.call_with("session.mutate", params, timeout).await {
                Ok(result) => {
                    self.adopt(
                        number(&result["revision"]),
                        number(&result["execution_generation"]),
                    );
                    return Ok(result);
                }
                Err(CallError::Rpc(error))
                    if attempt == 0
                        && matches!(
                            error.reason.as_deref(),
                            Some("revision_conflict" | "stale_generation")
                        ) =>
                {
                    let data = error.data.clone().unwrap_or(Value::Null);
                    self.adopt(
                        number(&data["revision"]),
                        number(&data["execution_generation"]),
                    );
                }
                Err(error) => {
                    return Err(Stop::Fail(format!(
                        "session.mutate {operation} failed: {}",
                        describe(&error)
                    )));
                }
            }
        }
        Err("session.mutate kept hitting a fence".into())
    }

    /// Put back what the attempt changed, deny or skip what is pending, interrupt a running
    /// turn, then detach.
    async fn cleanup(&mut self) {
        if self.session_id.is_empty() {
            return;
        }
        let options = call_options(crate::CLEANUP_TIMEOUT);
        if self.background_cleanup {
            let _ = self
                .client
                .call_with::<native::process_stop::Marker>(
                    native::process_stop::Params {
                        session_id: Some(self.session_id.clone()),
                        profile: None,
                    },
                    options,
                )
                .await;
        }
        for (method, params) in std::mem::take(&mut self.undo).into_iter().rev() {
            let _ = self.client.call_raw(method, params, options).await;
        }
        for (prompt_id, generation) in std::mem::take(&mut self.pending_approvals) {
            let _ = self
                .client
                .call_raw(
                    "approval.respond",
                    json!({"session_id": self.session_id, "execution_generation": generation,
                           "prompt_id": prompt_id, "choice": "deny"}),
                    options,
                )
                .await;
        }
        if self.running {
            let _ = self
                .client
                .call_raw(
                    "session.interrupt",
                    json!({"session_id": self.session_id,
                           "execution_generation": self.generation}),
                    options,
                )
                .await;
        }
        let _ = self
            .client
            .call_raw(
                "session.detach",
                json!({"session_id": self.session_id, "subscription_id": self.subscription_id}),
                options,
            )
            .await;
    }
}

fn describe(error: &CallError) -> String {
    match error {
        CallError::Rpc(rpc) => format!(
            "code {} {}",
            rpc.code,
            rpc.reason.clone().unwrap_or_else(|| rpc.message.clone())
        ),
        other => other.to_string(),
    }
}

/// Drive one scripted attempt.
pub(crate) async fn run(
    client: &Client,
    inbound: &mut InboundStream,
    scenario: &Scenario,
    config: &DriverConfig,
) -> AttemptResult {
    let mut run = Run::new(client, inbound);
    let outcome = tokio::time::timeout(config.attempt_timeout, drive(&mut run, scenario, config))
        .await
        .unwrap_or_else(|_| {
            Err(Stop::Fail(format!(
                "the attempt ran longer than {} s",
                config.attempt_timeout.as_secs()
            )))
        });
    run.cleanup().await;
    match outcome {
        Ok(()) => AttemptResult::Ok,
        Err(Stop::Fail(reason)) => AttemptResult::Failed(reason),
        Err(Stop::Absent(reason)) => AttemptResult::PreconditionAbsent(reason),
    }
}

async fn drive(run: &mut Run<'_>, scenario: &Scenario, config: &DriverConfig) -> Step {
    if scenario.name == "model-switch" {
        return model_switch(run, config).await;
    }
    if scenario.name == "bg-launch" {
        return bg_launch(run, config).await;
    }
    run.create(config).await?;
    match scenario.name.as_str() {
        "hermes-commands" => hermes_commands(run).await,
        "session-picker" => session_picker(run).await,
        "review-session-info" => review_session_info(run).await,
        "review-oneshot" => review_oneshot(run).await,
        "rewind-and-rollback" => rewind_and_rollback(run).await,
        "context-compaction" => context_compaction(run).await,
        "queue-edit-remove" => queue_edit_remove(run).await,
        "queue-steer" => queue_steer(run).await,
        "queue-move-send-now" => queue_move_send_now(run).await,
        "permissions-cd" => permissions_cd(run, config).await,
        "shell-and-btw" => shell_and_btw(run).await,
        "background-work" => background_work(run).await,
        other => Err(Stop::Fail(format!("unknown scripted scenario {other}"))),
    }
}

/// Handshake is already done. Create with source `tui` and a request id, submit, then detach
/// without waiting for the turn. The session id is cleared so cleanup does not detach twice.
async fn bg_launch(run: &mut Run<'_>, config: &DriverConfig) -> Step {
    let snapshot = run
        .call(
            "session.create",
            json!({
                "request_id": crate::request_id(),
                "source": "tui",
                "cwd": config.scratch.to_string_lossy(),
                "model": config.model,
                "provider": config.provider,
            }),
        )
        .await?;
    run.adopt_snapshot(&snapshot)?;
    let params = submit_params(&run.session_id, "Reply with the single word: ready")?;
    run.call("prompt.submit", params).await?;
    let session_id = run.session_id.clone();
    let subscription_id = run.subscription_id.clone();
    run.call(
        "session.detach",
        json!({
            "session_id": session_id,
            "subscription_id": subscription_id,
        }),
    )
    .await?;
    run.session_id.clear();
    Ok(())
}

/// The review's folder lookup: `session.info` for the session just created.
async fn review_session_info(run: &mut Run<'_>) -> Step {
    let session_id = run.session_id.clone();
    run.call("session.info", json!({"session_id": session_id}))
        .await?;
    Ok(())
}

/// The review's message draft: `llm.oneshot` bound to the session just created.
async fn review_oneshot(run: &mut Run<'_>) -> Step {
    let session_id = run.session_id.clone();
    let reply = run
        .call_for(
            "llm.oneshot",
            json!({
                "session_id": session_id,
                "template": "commit_message",
                "temperature": 0.8,
                "variables": {
                    "diff": REVIEW_DIFF,
                    "recent_commits": "",
                    "avoid": "",
                },
            }),
            Duration::from_secs(90),
        )
        .await?;
    if reply["text"].as_str().unwrap_or("").trim().is_empty() {
        return Err("llm.oneshot returned no text".into());
    }
    Ok(())
}

/// A small diff for the drafting scenario.
const REVIEW_DIFF: &str = "diff --git a/hello.txt b/hello.txt\nnew file mode 100644\n--- /dev/null\n+++ b/hello.txt\n@@ -0,0 +1 @@\n+hello\n";

fn text_of(result: &Value) -> String {
    let mut text = String::new();
    for key in ["output", "message", "notice", "display"] {
        if let Some(part) = result[key].as_str() {
            text.push_str(part);
            text.push('\n');
        }
    }
    text
}

fn expect_contains(what: &str, text: &str, needle: &str) -> Step {
    if text.contains(needle) {
        Ok(())
    } else {
        Err(Stop::Fail(format!("{what} did not hold the expected text")))
    }
}

// -------------------------------------------------------------------------------------------
// hermes-commands
// -------------------------------------------------------------------------------------------

fn control_of(payload: &Value) -> &Value {
    &payload["control"]
}

async fn hermes_commands(run: &mut Run<'_>) -> Step {
    run.progress("attach reads");
    let sid = run.session_id.clone();
    let control = run
        .call("session.control.read", json!({"session_id": sid}))
        .await?;
    let yolo = run.config_get("yolo").await?;
    let fast = run.config_get("fast").await?;
    if yolo.as_str() != Some("0") {
        return Err(Stop::Absent("yolo already on".to_owned()));
    }
    let snapshot = &control["control"];
    if !(snapshot["goal"].is_null()
        && snapshot["loop"].is_null()
        && snapshot["heartbeat"].is_null()
        && snapshot["revision"] == "")
    {
        return Err("the new session already holds goal, loop or heartbeat state".into());
    }
    if fast.as_str() != Some("normal") {
        return Err("the session's fast mode is not normal".into());
    }

    run.progress("goal");
    let mark = run.events.len();
    let reply = run.slash("goal reply with the word ok").await?;
    run.undo.push((
        "slash.exec",
        json!({"session_id": sid, "command": "goal clear"}),
    ));
    if reply["type"] != "send" {
        return Err("/goal did not answer with a send result".into());
    }
    run.wait_event("session.control.update", mark, UPDATE_WAIT, |payload| {
        let goal = &control_of(payload)["goal"];
        goal["status"] == "active" && goal["turns_used"] == 0 && goal["max_turns"] == 20
    })
    .await?;

    run.progress("subgoal");
    let mark = run.events.len();
    let reply = run.slash("subgoal keep it short").await?;
    expect_contains("/subgoal", &text_of(&reply), "keep it short")?;
    run.wait_event("session.control.update", mark, UPDATE_WAIT, |payload| {
        control_of(payload)["goal"]["subgoals"]
            .as_array()
            .is_some_and(|list| list.len() == 1)
    })
    .await?;

    run.progress("loop");
    let mark = run.events.len();
    let reply = run.slash("loop 45m talaria fixture check").await?;
    run.undo.push((
        "slash.exec",
        json!({"session_id": sid, "command": "loop stop"}),
    ));
    let text = text_of(&reply);
    expect_contains(
        "/loop",
        &text,
        "Loop set (every 45m): talaria fixture check",
    )?;
    expect_contains("/loop", &text, "First wakeup fires now")?;
    expect_contains(
        "/loop",
        &text,
        "Note: an active /goal is driving this session",
    )?;
    run.wait_event("session.control.update", mark, UPDATE_WAIT, |payload| {
        let looped = &control_of(payload)["loop"];
        looped["status"] == "active"
            && looped["interval_seconds"].as_f64() == Some(2700.0)
            && looped["ticks_fired"] == 0
            && looped["deferred_by_goal"] == true
    })
    .await?;

    run.progress("loop pause, resume, stop");
    let mark = run.events.len();
    run.slash("loop pause").await?;
    run.wait_event("session.control.update", mark, UPDATE_WAIT, |payload| {
        control_of(payload)["loop"]["status"] == "paused"
    })
    .await?;
    let mark = run.events.len();
    run.slash("loop resume").await?;
    run.wait_event("session.control.update", mark, UPDATE_WAIT, |payload| {
        let looped = &control_of(payload)["loop"];
        looped["status"] == "active" && looped["deferred_by_goal"] == true
    })
    .await?;
    let mark = run.events.len();
    run.slash("loop stop").await?;
    run.wait_event("session.control.update", mark, UPDATE_WAIT, |payload| {
        control_of(payload)["loop"].is_null()
    })
    .await?;
    run.undo.pop();

    run.progress("goal pause and clear");
    let mark = run.events.len();
    run.slash("goal pause").await?;
    run.wait_event("session.control.update", mark, UPDATE_WAIT, |payload| {
        control_of(payload)["goal"]["status"] == "paused"
    })
    .await?;
    let mark = run.events.len();
    let reply = run.slash("goal clear").await?;
    expect_contains("/goal clear", &text_of(&reply), "Goal cleared.")?;
    run.wait_event("session.control.update", mark, UPDATE_WAIT, |payload| {
        control_of(payload)["goal"].is_null()
    })
    .await?;
    run.undo.clear();

    run.progress("heartbeat");
    let mark = run.events.len();
    run.slash("heartbeat every 1h talaria fixture ping").await?;
    run.undo.push((
        "slash.exec",
        json!({"session_id": sid, "command": "heartbeat clear"}),
    ));
    run.wait_event("session.control.update", mark, UPDATE_WAIT, |payload| {
        !control_of(payload)["heartbeat"].is_null()
    })
    .await?;
    let mark = run.events.len();
    run.slash("heartbeat clear").await?;
    run.wait_event("session.control.update", mark, UPDATE_WAIT, |payload| {
        control_of(payload)["heartbeat"].is_null()
    })
    .await?;
    run.undo.clear();

    run.progress("loop without a prompt");
    let mark = run.events.len();
    let reply = run.slash("loop 45m").await?;
    expect_contains("/loop 45m", &text_of(&reply), "missing prompt")?;
    run.linger(Duration::from_millis(1500)).await;
    // The owner publishes after every goal, loop and heartbeat command, so a refused /loop may
    // announce the unchanged (empty) state; what it must not do is create a loop.
    let created = run.find_event("session.control.update", mark, &|payload| {
        !control_of(payload)["loop"].is_null()
    });
    if created.is_some() {
        return Err("a refused /loop still created a loop".into());
    }

    run.progress("yolo");
    let reply = run.slash("yolo").await?;
    run.undo.push((
        "config.set",
        json!({"session_id": sid, "key": "yolo", "value": "0"}),
    ));
    expect_contains("/yolo", &text_of(&reply), "YOLO mode")?;
    if run.config_get("yolo").await?.as_str() != Some("1") {
        return Err("config.get yolo did not read 1 after /yolo".into());
    }
    let reply = run.slash("yolo").await?;
    expect_contains("/yolo", &text_of(&reply), "YOLO mode")?;
    if run.config_get("yolo").await?.as_str() != Some("0") {
        return Err("config.get yolo did not read 0 after the second /yolo".into());
    }
    run.undo.clear();

    run.progress("fast");
    let reply = run.slash("fast").await?;
    expect_contains("/fast", &text_of(&reply), "Priority Processing")?;
    if run.config_get("fast").await?.as_str() != Some("normal") {
        return Err("config.get fast did not read normal".into());
    }

    run.progress("plan");
    let reply = run.slash("plan write a haiku about rain").await?;
    if reply["type"] != "send" {
        return Err("/plan did not answer with a send result".into());
    }
    expect_contains(
        "/plan",
        &text_of(&reply),
        "Planning: write a haiku about rain",
    )?;

    run.progress("final read");
    let control = run
        .call("session.control.read", json!({"session_id": sid}))
        .await?;
    let snapshot = &control["control"];
    if !(snapshot["goal"].is_null()
        && snapshot["loop"].is_null()
        && snapshot["heartbeat"].is_null())
    {
        return Err("the session still holds goal, loop or heartbeat state".into());
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// session-picker
// -------------------------------------------------------------------------------------------

async fn session_picker(run: &mut Run<'_>) -> Step {
    let sid = run.session_id.clone();
    run.progress("one prompt");
    run.turn(PICKER_PROMPT).await?;
    run.progress("list live and stored");
    let live = run
        .call(
            "session.list",
            json!({"profile": "fixture", "scope": "live", "limit": 20}),
        )
        .await?;
    if !live["sessions"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["session_id"] == sid.as_str()))
    {
        return Err("the live list does not hold the new session".into());
    }
    run.call(
        "session.list",
        json!({"profile": "fixture", "scope": "stored", "limit": 20}),
    )
    .await?;
    run.progress("rename");
    // Titles are unique per profile, so the number changes from run to run.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |elapsed| elapsed.as_secs() % 100_000);
    let title = format!("fixture picker {stamp}");
    let renamed = run
        .mutate("rename", json!({"title": title}), Duration::from_secs(60))
        .await?;
    if renamed["title"].as_str() != Some(title.as_str()) {
        return Err("the rename receipt names another title".into());
    }
    let live = run
        .call(
            "session.list",
            json!({"profile": "fixture", "scope": "live", "limit": 20}),
        )
        .await?;
    let renamed = live["sessions"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|row| row["session_id"] == sid.as_str() && row["title"] == title.as_str())
    });
    if !renamed {
        return Err("the live list does not show the new title".into());
    }
    run.progress("export");
    let export = run
        .call(
            "session.export",
            json!({"session_id": sid, "profile": "fixture", "format": "md", "redact": true}),
        )
        .await?;
    if export["content"].as_str().is_none_or(str::is_empty) {
        return Err("the export holds no content".into());
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// rewind-and-rollback
// -------------------------------------------------------------------------------------------

/// Whether the events from `from` to `to` hold a `write_file` or `patch` tool call.
fn wrote_a_file(run: &Run<'_>, from: usize, to: usize) -> bool {
    run.events[from..to].iter().any(|(kind, params)| {
        kind == "tool.start"
            && matches!(
                params["payload"]["name"].as_str(),
                Some("write_file" | "patch")
            )
    })
}

async fn rewind_and_rollback(run: &mut Run<'_>) -> Step {
    let sid = run.session_id.clone();
    run.progress("prompt one");
    let first = run.events.len();
    run.turn(REWIND_FIRST).await?;
    let after_first = run.events.len();
    run.progress("prompt two");
    run.turn(REWIND_SECOND).await?;
    let after_second = run.events.len();
    if !wrote_a_file(run, first, after_first) || !wrote_a_file(run, after_first, after_second) {
        return Err("a turn did not call write_file or patch".into());
    }

    run.progress("rollback.list");
    let list = run
        .call("rollback.list", json!({"session_id": sid}))
        .await?;
    let checkpoints = list["checkpoints"].as_array().cloned().unwrap_or_default();
    if list["enabled"] != true || checkpoints.len() < 2 {
        return Err("rollback.list did not answer enabled with at least two rows".into());
    }
    let first_hash = checkpoints[0]["hash"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let last_hash = checkpoints[checkpoints.len() - 1]["hash"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if first_hash.is_empty() || last_hash.is_empty() {
        return Err("a checkpoint row has no hash".into());
    }

    run.progress("rollback.diff and session_diff");
    run.call(
        "rollback.diff",
        json!({"session_id": sid, "hash": first_hash}),
    )
    .await?;
    run.call("rollback.session_diff", json!({"session_id": sid}))
        .await?;

    run.progress("rewind prompt two");
    let snapshot = run.resume().await?;
    let target = last_user_row(&snapshot)?;
    run.call_maybe_mutate_rewind(target).await?;

    run.progress("prompt two again");
    run.turn(REWIND_SECOND).await?;

    run.progress("whole-folder restore");
    run.resume().await?;
    let restored = run
        .call_maybe(
            "rollback.restore",
            json!({"session_id": sid, "hash": last_hash}),
        )
        .await?;
    if let Err(error) = restored {
        run.progress(&format!(
            "rollback.restore was refused ({})",
            error.reason.unwrap_or_default()
        ));
    }
    Ok(())
}

impl Run<'_> {
    /// The rewind of the recording: a refusal is recorded as it came.
    async fn call_maybe_mutate_rewind(&mut self, target: i64) -> Step {
        let mark = self.events.len();
        let params = json!({
            "session_id": self.session_id,
            "request_id": crate::submission_id(),
            "expected_revision": self.revision,
            "expected_generation": self.generation,
            "operation": "rewind",
            "payload": {"target_message_id": target},
        });
        match self
            .call_with("session.mutate", params, Duration::from_secs(60))
            .await
        {
            Ok(result) => {
                self.adopt(
                    number(&result["revision"]),
                    number(&result["execution_generation"]),
                );
                self.wait_event("session.updated", mark, EVENT_WAIT, |payload| {
                    payload["operation"] == "rewind"
                })
                .await?;
                Ok(())
            }
            Err(CallError::Rpc(error)) => {
                self.progress(&format!(
                    "the rewind was refused ({})",
                    error.reason.unwrap_or_default()
                ));
                Ok(())
            }
            Err(error) => Err(Stop::Fail(format!(
                "session.mutate rewind failed: {}",
                describe(&error)
            ))),
        }
    }
}

/// The `row_id` of the newest user message of a resume snapshot.
fn last_user_row(snapshot: &Value) -> Step<i64> {
    let messages = snapshot["messages"].as_array().cloned().unwrap_or_default();
    messages
        .iter()
        .rev()
        .find(|message| message["role"] == "user")
        .and_then(|message| message["row_id"].as_i64())
        .filter(|row| *row > 0)
        .ok_or_else(|| Stop::Fail("the resume snapshot holds no user row with a row_id".into()))
}

// -------------------------------------------------------------------------------------------
// context-compaction
// -------------------------------------------------------------------------------------------

async fn context_compaction(run: &mut Run<'_>) -> Step {
    let sid = run.session_id.clone();
    for (index, word) in NUMBER_WORDS.iter().enumerate() {
        run.progress(&format!("turn {}", index + 1));
        run.turn(&format!("Reply with only the word {word}."))
            .await?;
    }
    run.progress("stats and breakdown");
    let stats = run
        .call("session.stats", json!({"session_id": sid}))
        .await?;
    if stats["api_calls"].as_i64().unwrap_or(0) < 16 {
        return Err("session.stats reports fewer than 16 api calls".into());
    }
    let breakdown = run
        .call("session.context_breakdown", json!({"session_id": sid}))
        .await?;
    if !breakdown["context_max"].as_i64().is_some_and(|max| max > 0) {
        return Err("the breakdown has no context_max".into());
    }

    run.progress("compress preview");
    let preview = run
        .mutate(
            "compress",
            json!({"focus": "--preview"}),
            Duration::from_secs(700),
        )
        .await?;
    if preview["status"] != "preview" {
        return Err("the preview did not answer status preview".into());
    }
    run.progress("compress");
    let mark = run.events.len();
    let compressed = run
        .mutate("compress", json!({}), Duration::from_secs(700))
        .await?;
    if compressed["status"] == "noop" || compressed["summary"]["noop"] == true {
        return Err("the compression changed nothing".into());
    }
    if !compressed["revision"].is_i64() {
        return Err("the compress reply has no integer revision".into());
    }
    run.wait_event("session.updated", mark, EVENT_WAIT, |payload| {
        payload["operation"] == "compress"
    })
    .await?;

    run.progress("stats and breakdown again");
    run.call("session.stats", json!({"session_id": sid}))
        .await?;
    run.call("session.context_breakdown", json!({"session_id": sid}))
        .await?;

    run.progress("one more turn");
    let mark = run.events.len();
    run.turn("Reply with only the word done.").await?;
    run.linger(Duration::from_secs(2)).await;
    let usage = run.find_event("session.usage", mark, &|payload| {
        payload["usage"]["context_percent"].is_number() || payload["context_percent"].is_number()
    });
    if usage.is_none() {
        return Err("no session.usage with context_percent followed the compression".into());
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// queue scenarios
// -------------------------------------------------------------------------------------------

fn admission_of(receipt: &Value) -> Step<String> {
    receipt["admission_id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Stop::Fail("a receipt has no admission_id".into()))
}

fn capital_prompt(word: &str) -> String {
    format!("Reply with the word {word} in capital letters.")
}

/// Wait until nothing runs and nothing is queued.
async fn wait_idle(run: &mut Run<'_>) -> Step {
    run.wait_until("the queue to drain", TURN_WAIT, |run| {
        !run.running && run.pending == Some(0)
    })
    .await
}

async fn queue_edit_remove(run: &mut Run<'_>) -> Step {
    run.progress("first turn");
    run.submit(SLEEP_PROMPT).await?;
    run.linger(Duration::from_secs(3)).await;
    run.progress("queue three prompts");
    let alpha = admission_of(&run.submit(&capital_prompt("alpha")).await?)?;
    let _bravo = admission_of(&run.submit(&capital_prompt("bravo")).await?)?;
    let charlie = admission_of(&run.submit(&capital_prompt("charlie")).await?)?;
    run.progress("edit alpha");
    let sid = run.session_id.clone();
    run.call(
        "prompt.replace_queued",
        json!({"session_id": sid, "admission_id": alpha,
               "submission_id": crate::submission_id(),
               "text": "Reply with the word anchor in capital letters."}),
    )
    .await?;
    run.progress("remove charlie");
    run.call(
        "prompt.cancel",
        json!({"session_id": sid, "admission_id": charlie}),
    )
    .await?;
    run.progress("drain");
    wait_idle(run).await?;
    for (admission, name) in [
        (&alpha, "the retired prompt"),
        (&charlie, "the removed prompt"),
    ] {
        if !settled_with(run, admission, "cancelled") {
            return Err(Stop::Fail(format!("{name} has no cancelled settlement")));
        }
    }
    Ok(())
}

/// Whether a `message.complete` for `admission` ended with `outcome`.
fn settled_with(run: &Run<'_>, admission: &str, outcome: &str) -> bool {
    run.events.iter().any(|(kind, params)| {
        kind == "message.complete"
            && params["admission_id"] == admission
            && params["payload"]["outcome"] == outcome
    })
}

async fn queue_steer(run: &mut Run<'_>) -> Step {
    run.progress("first turn");
    run.submit(STEER_PROMPT).await?;
    run.linger(Duration::from_secs(3)).await;
    run.progress("queue the steer");
    let queued = admission_of(&run.submit("The code word is PELICAN.").await?)?;
    run.linger(Duration::from_secs(2)).await;
    let sid = run.session_id.clone();
    run.call(
        "prompt.steer_queued",
        json!({"session_id": sid, "admission_id": queued}),
    )
    .await?;
    run.progress("drain");
    wait_idle(run).await?;
    run.wait_until("the steer settlement", EVENT_WAIT, |run| {
        settled_with(run, &queued, "steered")
    })
    .await?;
    run.progress("resume");
    let snapshot = run.resume().await?;
    let steered = snapshot["messages"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["display_kind"] == "steer"));
    if !steered {
        return Err("the resumed transcript holds no steer message".into());
    }
    Ok(())
}

async fn queue_move_send_now(run: &mut Run<'_>) -> Step {
    run.progress("first turn");
    run.submit(SLEEP_PROMPT).await?;
    run.linger(Duration::from_secs(3)).await;
    run.progress("queue three prompts");
    let one = admission_of(&run.submit(&capital_prompt("one")).await?)?;
    let two = admission_of(&run.submit(&capital_prompt("two")).await?)?;
    let _three = admission_of(&run.submit(&capital_prompt("three")).await?)?;
    run.progress("move two to the back");
    let sid = run.session_id.clone();
    run.call(
        "prompt.replace_queued",
        json!({"session_id": sid, "admission_id": two,
               "submission_id": crate::submission_id(),
               "text": capital_prompt("two")}),
    )
    .await?;
    run.progress("send now");
    let generation = run.generation;
    let mark = run.events.len();
    run.call(
        "session.interrupt",
        json!({"session_id": sid, "execution_generation": generation}),
    )
    .await?;
    run.progress("drain");
    wait_idle(run).await?;
    let head = run
        .events
        .iter()
        .skip(mark)
        .find(|(kind, _)| kind == "message.start")
        .map(|(_, params)| params["admission_id"].clone());
    if head != Some(json!(one)) {
        return Err("the turn after the interrupt did not start the first queued prompt".into());
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// model-switch
// -------------------------------------------------------------------------------------------

async fn model_switch(run: &mut Run<'_>, config: &DriverConfig) -> Step {
    run.progress("profile default before");
    let before = run
        .call("model.options", json!({"explicit_only": true}))
        .await?;
    run.create(config).await?;
    let sid = run.session_id.clone();
    run.progress("model.options");
    let options = run
        .call("model.options", json!({"session_id": sid}))
        .await?;
    run.progress("config.get reasoning");
    run.call("config.get", json!({"session_id": sid, "key": "reasoning"}))
        .await?;
    let other = other_model(&options, &config.provider, &config.model)
        .ok_or_else(|| Stop::Fail("no second model with effort under the provider".to_owned()))?;
    run.progress("switch model");
    let switched = run
        .mutate(
            "model",
            json!({"model": other, "provider": config.provider}),
            Duration::from_secs(60),
        )
        .await?;
    if switched["model"].as_str() != Some(other.as_str()) {
        return Err("the model switch receipt names another model".into());
    }
    run.progress("reasoning low");
    let effort = run
        .mutate(
            "reasoning",
            json!({"effort": "low"}),
            Duration::from_secs(60),
        )
        .await?;
    if effort["effort"] != "low" {
        return Err("the reasoning receipt is not low".into());
    }
    run.progress("switch back");
    let back = run
        .mutate(
            "model",
            json!({"model": config.model, "provider": config.provider}),
            Duration::from_secs(60),
        )
        .await?;
    if back["model"].as_str() != Some(config.model.as_str()) {
        return Err("the switch back receipt names another model".into());
    }
    run.progress("profile default after");
    let after = run
        .call("model.options", json!({"explicit_only": true}))
        .await?;
    if before["model"] != after["model"] || before["provider"] != after["provider"] {
        return Err("the profile default model changed during the scenario".into());
    }
    Ok(())
}

/// The first model of `provider`, other than `current`, that offers a reasoning effort.
fn other_model(options: &Value, provider: &str, current: &str) -> Option<String> {
    let providers = options["providers"].as_array()?;
    let entry = providers
        .iter()
        .find(|entry| entry["slug"] == provider || entry["id"] == provider)?;
    let capabilities = &entry["capabilities"];
    entry["models"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter(|model| *model != current)
        .find(|model| capabilities[*model]["reasoning"].as_bool().unwrap_or(true))
        .map(str::to_owned)
}

// -------------------------------------------------------------------------------------------
// permissions-cd
// -------------------------------------------------------------------------------------------

async fn permissions_cd(run: &mut Run<'_>, config: &DriverConfig) -> Step {
    let sid = run.session_id.clone();
    run.progress("approvals.mode and yolo");
    let mode = run
        .call("config.get", json!({"key": "approvals.mode"}))
        .await?;
    if mode["value"].as_str() != Some("manual") {
        return Err(Stop::Absent(
            "the profile `fixture` must set `approvals.mode: manual`".to_owned(),
        ));
    }
    if run.config_get("yolo").await?.as_str() != Some("0") {
        return Err("yolo already on".into());
    }
    run.progress("yolo on");
    run.undo.push((
        "config.set",
        json!({"session_id": sid, "key": "yolo", "value": "0"}),
    ));
    let on = run
        .call(
            "config.set",
            json!({"session_id": sid, "key": "yolo", "value": "1"}),
        )
        .await?;
    if on["value"].as_str() != Some("1") {
        return Err("config.set yolo 1 did not answer 1".into());
    }
    run.progress("yolo off");
    let off = run
        .call(
            "config.set",
            json!({"session_id": sid, "key": "yolo", "value": "0"}),
        )
        .await?;
    if off["value"].as_str() != Some("0") {
        return Err("config.set yolo 0 did not answer 0".into());
    }
    run.undo.clear();
    if run.config_get("yolo").await?.as_str() != Some("0") {
        return Err("config.get yolo did not read 0 after the restore".into());
    }
    run.progress("move to the second folder");
    let second = config
        .second
        .as_ref()
        .ok_or_else(|| Stop::Fail("the scenario has no second folder".to_owned()))?;
    let mark = run.events.len();
    let moved = run
        .mutate(
            "workspace",
            json!({"cwd": second.to_string_lossy()}),
            Duration::from_secs(60),
        )
        .await?;
    if moved["cwd"].is_null() {
        return Err("the workspace receipt names no folder".into());
    }
    // The gateway announces the move as session.info or session.updated; either ends the wait,
    // and a quiet gateway is recorded as it was.
    run.linger(Duration::from_secs(3)).await;
    let announced = run
        .events
        .iter()
        .skip(mark)
        .any(|(kind, _)| kind == "session.info" || kind == "session.updated");
    run.progress(if announced {
        "the move was announced"
    } else {
        "the move was not announced by an event"
    });
    Ok(())
}

// -------------------------------------------------------------------------------------------
// shell-and-btw
// -------------------------------------------------------------------------------------------

/// The three shell commands the scenario runs: one that succeeds, one that fails with a
/// non-zero exit code, and one the gateway refuses as a dangerous command.
const SHELL_COMMANDS: [&str; 3] = [
    "echo talaria-shell-ok",
    "echo oops >&2; exit 3",
    "rm -r ./talaria-uat-does-not-exist",
];

async fn shell_and_btw(run: &mut Run<'_>) -> Step {
    run.progress("three shell commands");
    let sid = run.session_id.clone();
    for command in SHELL_COMMANDS {
        // The last command is refused as a dangerous command. The refusal is part of the
        // recording, so the error is kept rather than raised.
        let _outcome = run
            .call_maybe("shell.exec", json!({"session_id": sid, "command": command}))
            .await?;
    }
    run.progress("the reply");
    run.turn(SHELL_PROMPT).await?;
    run.progress("the side question");
    let mark = run.events.len();
    run.call(
        "prompt.btw",
        json!({"session_id": sid, "text": BTW_QUESTION}),
    )
    .await?;
    run.wait_event("btw.complete", mark, TURN_WAIT, |payload| {
        payload["text"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    })
    .await?;
    Ok(())
}

async fn background_work(run: &mut Run<'_>) -> Step {
    let full = run.call("config.get", json!({"key": "full"})).await?;
    let deny = &full["config"]["approvals"]["deny"];
    if !deny.is_null() && !deny.as_array().is_some_and(Vec::is_empty) {
        return Err(Stop::Absent(
            "the fixture profile must have no approvals.deny rules".into(),
        ));
    }
    run.expected_tick_approval = true;
    run.background_cleanup = true;
    run.progress("start the background tick command");
    run.turn(BACKGROUND_PROMPT).await?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let process = loop {
        let listed = run
            .native::<native::process_list::Marker>(
                native::process_list::Params {
                    session_id: run.session_id.clone(),
                    profile: None,
                },
                Duration::from_secs(10),
            )
            .await?;
        if let Some(process) = listed
            .processes
            .into_iter()
            .find(|p| p.status == "running" && p.command == "sh tick.sh")
        {
            break process;
        }
        if Instant::now() >= deadline {
            return Err("no running sh tick.sh process was listed".into());
        }
        run.linger(Duration::from_secs(1)).await;
    };
    run.progress("collect five seconds of output, then kill the process");
    run.linger(Duration::from_secs(5)).await;
    let killed = run
        .native::<native::process_kill::Marker>(
            native::process_kill::Params {
                session_id: run.session_id.clone(),
                process_id: process.session_id.clone(),
                profile: None,
            },
            Duration::from_secs(30),
        )
        .await?;
    if killed.status != hermes_contract::schemas::ProcessKillStatus::Killed {
        return Err("process.kill did not report killed".into());
    }
    let listed = run
        .native::<native::process_list::Marker>(
            native::process_list::Params {
                session_id: run.session_id.clone(),
                profile: None,
            },
            Duration::from_secs(10),
        )
        .await?;
    if !listed.processes.iter().any(|p| {
        p.session_id == process.session_id
            && p.completion_reason.as_deref() == Some("killed")
            && p.output_tail
                .as_deref()
                .is_some_and(|tail| tail.contains("tick 1"))
    }) {
        return Err("the killed process or its tick output was absent".into());
    }
    run.progress("receive the detached background answer");
    let mark = run.events.len();
    let task = run
        .native::<native::prompt_background::Marker>(
            native::prompt_background::Params {
                session_id: run.session_id.clone(),
                text: "Reply with exactly: bg-ok".into(),
                profile: None,
            },
            Duration::from_secs(15),
        )
        .await?;
    run.wait_event(
        "background.complete",
        mark,
        Duration::from_secs(120),
        |payload| {
            payload["task_id"].as_str() == Some(task.task_id.as_str())
                && payload["text"]
                    .as_str()
                    .is_some_and(|text| text.contains("bg-ok"))
        },
    )
    .await?;
    run.native::<native::process_stop::Marker>(
        native::process_stop::Params {
            session_id: Some(run.session_id.clone()),
            profile: None,
        },
        Duration::from_secs(60),
    )
    .await?;
    run.background_cleanup = false;
    if let Some(reason) = run.unexpected.clone() {
        return Err(Stop::Fail(reason));
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// evidence over a recorded file
// -------------------------------------------------------------------------------------------

/// The result of every request of `method`, in order.
fn results_of(source: &ReplaySource, method: &str) -> Vec<Value> {
    source
        .outbound()
        .filter(|sent| sent.frame["method"] == method)
        .filter_map(|sent| {
            let id = &sent.frame["id"];
            source
                .inbound()
                .find(|reply| reply.frame["id"] == *id && reply.frame.get("result").is_some())
                .map(|reply| reply.frame["result"].clone())
        })
        .collect()
}

fn count_events(source: &ReplaySource, kind: &str) -> usize {
    source
        .inbound()
        .filter(|frame| frame.frame["method"] == "event" && frame.frame["params"]["type"] == kind)
        .count()
}

fn inbound_text(source: &ReplaySource) -> String {
    source
        .inbound()
        .map(|frame| frame.frame.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a good recording of a scripted scenario holds, beyond its final statuses.
pub(crate) fn check_evidence(scenario: &Scenario, source: &ReplaySource) -> Result<(), String> {
    let sent = |method: &str| {
        source
            .outbound()
            .filter(|f| f.frame["method"] == method)
            .count()
    };
    let text = inbound_text(source);
    match scenario.name.as_str() {
        "background-work" => {
            for method in [
                "process.list",
                "process.kill",
                "prompt.background",
                "process.stop",
            ] {
                if results_of(source, method).is_empty() {
                    return Err(format!("no successful {method} reply"));
                }
            }
            if !results_of(source, "process.list").iter().any(|result| {
                result["processes"].as_array().is_some_and(|processes| {
                    processes.iter().any(|p| {
                        p["completion_reason"] == "killed"
                            && p["output_tail"]
                                .as_str()
                                .is_some_and(|tail| tail.contains("tick 1"))
                    })
                })
            }) {
                return Err("no killed process with tick output".into());
            }
            let tasks = results_of(source, "prompt.background");
            if !source.inbound().any(|frame| {
                frame.frame["params"]["type"] == "background.complete"
                    && tasks
                        .iter()
                        .any(|task| task["task_id"] == frame.frame["params"]["payload"]["task_id"])
                    && frame.frame["params"]["payload"]["text"]
                        .as_str()
                        .is_some_and(|text| text.contains("bg-ok"))
            }) {
                return Err("no matching background answer".into());
            }
        }
        "hermes-commands" => {
            if count_events(source, "session.control.update") < 10 {
                return Err("fewer than ten session.control.update events".to_owned());
            }
            for needle in [
                "Loop set (every 45m)",
                "YOLO mode",
                "Priority Processing",
                "Planning: ",
            ] {
                if !text.contains(needle) {
                    return Err(format!("no reply held {needle:?}"));
                }
            }
            if sent("prompt.submit") != 0 {
                return Err("the scenario submitted a prompt".to_owned());
            }
        }
        "session-picker" => {
            if results_of(source, "session.list").len() < 2 {
                return Err("fewer than two session.list replies".to_owned());
            }
            if results_of(source, "session.export").is_empty() {
                return Err("no session.export reply".to_owned());
            }
        }
        "rewind-and-rollback" => {
            for method in [
                "rollback.list",
                "rollback.diff",
                "rollback.session_diff",
                "rollback.restore",
            ] {
                if sent(method) == 0 {
                    return Err(format!("no {method} request"));
                }
            }
            if count_events(source, "session.updated") == 0 {
                return Err("no session.updated event".to_owned());
            }
        }
        "context-compaction" => {
            let stats = results_of(source, "session.stats");
            if !stats
                .iter()
                .any(|reply| reply["api_calls"].as_i64().unwrap_or(0) >= 16)
            {
                return Err("no session.stats reply with 16 api calls".to_owned());
            }
            for reply in results_of(source, "session.context_breakdown") {
                let files = reply["context_files"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for file in files {
                    let path = file["path"].as_str().unwrap_or_default();
                    if !["<tmp>", "<home>", "<profile>"]
                        .iter()
                        .any(|root| path.starts_with(root))
                    {
                        return Err("a context file path is not under a placeholder".to_owned());
                    }
                }
            }
            if !text.contains("\"operation\":\"compress\"") {
                return Err("no compress reply".to_owned());
            }
        }
        "queue-edit-remove" | "queue-steer" | "queue-move-send-now" => {
            if sent("prompt.submit") < 2 {
                return Err("fewer than two prompts were submitted".to_owned());
            }
            if !source
                .inbound()
                .any(|frame| frame.frame["params"]["payload"]["pending"].is_array())
            {
                return Err("no session.info with a pending list".to_owned());
            }
            let wanted = match scenario.name.as_str() {
                "queue-edit-remove" => "prompt.cancel",
                "queue-steer" => "prompt.steer_queued",
                _ => "session.interrupt",
            };
            if sent(wanted) == 0 {
                return Err(format!("no {wanted} request"));
            }
        }
        "model-switch" => {
            if sent("session.mutate") < 3 {
                return Err("fewer than three session.mutate requests".to_owned());
            }
        }
        "permissions-cd" => {
            if sent("config.set") < 2 {
                return Err("fewer than two config.set requests".to_owned());
            }
            if sent("session.mutate") == 0 {
                return Err("no session.mutate request".to_owned());
            }
        }
        "review-session-info" | "review-oneshot" => {
            let method = if scenario.name == "review-oneshot" {
                "llm.oneshot"
            } else {
                "session.info"
            };
            if sent(method) != 1 {
                return Err(format!("{} did not send one {method}", scenario.name));
            }
        }
        "bg-launch" => {
            let creates = source
                .outbound()
                .filter(|frame| frame.frame["method"] == "session.create")
                .collect::<Vec<_>>();
            if creates.len() != 1 {
                return Err("bg-launch did not send one session.create".to_owned());
            }
            let params = &creates[0].frame["params"];
            if params["source"] != "tui" {
                return Err("bg-launch create source is not tui".to_owned());
            }
            if params["request_id"].as_str().unwrap_or("").is_empty() {
                return Err("bg-launch create has no request_id".to_owned());
            }
            let submits = source
                .outbound()
                .filter(|frame| frame.frame["method"] == "prompt.submit")
                .collect::<Vec<_>>();
            if submits.len() != 1 {
                return Err("bg-launch did not send one prompt.submit".to_owned());
            }
            if submits[0].frame["params"]["text"] != "Reply with the single word: ready" {
                return Err("bg-launch submit text does not match".to_owned());
            }
            if sent("session.detach") != 1 {
                return Err("bg-launch did not detach once".to_owned());
            }
        }
        "shell-and-btw" => {
            if count_events(source, "btw.complete") == 0 {
                return Err("no btw.complete event".to_owned());
            }
            if sent("shell.exec") < 3 {
                return Err("fewer than three shell.exec requests".to_owned());
            }
            if sent("prompt.btw") == 0 {
                return Err("no prompt.btw request".to_owned());
            }
            let shells = results_of(source, "shell.exec");
            let first = shells.first().ok_or("no shell.exec reply")?;
            if first["stdout"].as_str() != Some("talaria-shell-ok\n") {
                return Err("the first shell command did not print its marker".to_owned());
            }
            let second = shells.get(1).ok_or("no second shell.exec reply")?;
            if second["code"].as_i64() != Some(3) {
                return Err("the second shell command did not exit with 3".to_owned());
            }
            // The third command is refused, so its reply is an error and no result names it.
            let refused = source
                .inbound()
                .any(|frame| frame.frame["error"]["data"]["reason"] == "dangerous_command");
            if !refused {
                return Err("no shell.exec refusal for a dangerous command".to_owned());
            }
        }
        _ => {}
    }
    if text.contains("unattended")
        || source
            .outbound()
            .any(|f| f.frame.to_string().contains("unattended"))
    {
        return Err("a frame holds an unattended key".to_owned());
    }
    Ok(())
}
