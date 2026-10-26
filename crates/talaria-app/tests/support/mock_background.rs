// Process listing is an ordinary empty read until rows are configured. Kill, stop, and
// detached prompt tasks require the opt-in script. Included by the mock gateway.

const LIST_KEYS: &[&str] = &["session_id", "profile"];
const KILL_KEYS: &[&str] = &["session_id", "process_id", "profile"];
const STOP_KEYS: &[&str] = &["session_id", "profile"];
const BACKGROUND_KEYS: &[&str] = &["session_id", "profile", "text"];
const TAIL_CHARS: usize = 32_768;
const COMMAND_CHARS: usize = 200;

#[derive(Clone, Debug)]
struct ScriptedProcess {
    id: String,
    session_id: String,
    command: String,
    status: String,
    exit_code: Option<i64>,
    started_epoch: Option<f64>,
    exited_at: Option<f64>,
    completion_reason: Option<String>,
    output: String,
}

#[derive(Clone, Debug)]
struct MethodFailure {
    code: i64,
    reason: String,
    drop_connection: bool,
    delay: Option<Duration>,
}

#[derive(Clone, Debug)]
enum BgCompletion {
    Quiet,
    After {
        delay: Option<Duration>,
        text: String,
    },
    Before {
        text: String,
    },
}

#[derive(Clone, Debug)]
struct BackgroundScript {
    enabled: bool,
    draining: bool,
    processes: Vec<ScriptedProcess>,
    managed: HashSet<String>,
    extra_live: HashSet<String>,
    delegations: i64,
    kill_status: String,
    kill_error: Option<String>,
    kill_scope_stop_failed: Option<String>,
    subscribed: bool,
    bg_counter: u64,
    completion: BgCompletion,
    failures: HashMap<String, VecDeque<MethodFailure>>,
}

impl Default for BackgroundScript {
    fn default() -> Self {
        Self {
            enabled: false,
            draining: false,
            processes: Vec::new(),
            managed: HashSet::new(),
            extra_live: HashSet::new(),
            delegations: 0,
            kill_status: "killed".to_string(),
            kill_error: None,
            kill_scope_stop_failed: None,
            subscribed: true,
            bg_counter: 0,
            completion: BgCompletion::Quiet,
            failures: HashMap::new(),
        }
    }
}

impl MockGateway {
    /// Turn on the process and background-task answers.
    pub fn enable_background(&self) {
        lock(&self.shared).background.enabled = true;
    }

    /// Replace the scripted process rows. Each row belongs to `session_id`.
    pub fn set_background_processes(&self, session_id: &str, rows: Vec<Value>) {
        let processes = rows
            .into_iter()
            .map(|row| ScriptedProcess {
                id: row["session_id"].as_str().unwrap_or("proc").to_string(),
                session_id: session_id.to_string(),
                command: row["command"].as_str().unwrap_or("").to_string(),
                status: row["status"].as_str().unwrap_or("running").to_string(),
                exit_code: row["exit_code"].as_i64(),
                started_epoch: row["started_epoch"].as_f64(),
                exited_at: row["exited_at"].as_f64(),
                completion_reason: row["completion_reason"].as_str().map(str::to_owned),
                output: row["output"].as_str().unwrap_or("").to_string(),
            })
            .collect();
        let mut state = lock(&self.shared);
        state.background.enabled = true;
        state.background.processes = processes;
    }

    /// The named session answers `unsupported_projection` to a list.
    pub fn mark_managed_worker(&self, session_id: &str) {
        let mut state = lock(&self.shared);
        state.background.enabled = true;
        state.background.managed.insert(session_id.to_string());
    }

    /// Count returned by stop for background delegations. The default is 0.
    pub fn set_background_delegations(&self, count: i64) {
        let mut state = lock(&self.shared);
        state.background.enabled = true;
        state.background.delegations = count;
    }

    /// Treat `session_id` as a live session in addition to the mock's own session.
    pub fn add_live_session(&self, session_id: &str) {
        let mut state = lock(&self.shared);
        state.background.enabled = true;
        state.background.extra_live.insert(session_id.to_string());
    }

    /// Queue one scripted failure for `method`. `drop_connection` closes the socket.
    pub fn set_background_method_failure(
        &self,
        method: &str,
        code: i64,
        reason: &str,
        drop_connection: bool,
        delay: Option<Duration>,
    ) {
        let mut state = lock(&self.shared);
        state.background.enabled = true;
        state
            .background
            .failures
            .entry(method.to_string())
            .or_default()
            .push_back(MethodFailure {
                code,
                reason: reason.to_string(),
                drop_connection,
                delay,
            });
    }

    /// Every later call of the four methods answers `runtime_draining`.
    pub fn refuse_background_methods(&self) {
        lock(&self.shared).background.draining = true;
        lock(&self.shared).background.enabled = true;
    }

    /// The next kill of a known process answers this registry status.
    pub fn set_kill_result(&self, status: &str, error: Option<&str>) {
        let mut state = lock(&self.shared);
        state.background.enabled = true;
        state.background.kill_status = status.to_string();
        state.background.kill_error = error.map(str::to_owned);
    }

    /// Preserve the registry's open scope-stop warning field on a kill reply.
    pub fn set_kill_scope_stop_failed(&self, scope: &str) {
        lock(&self.shared).background.kill_scope_stop_failed = Some(scope.to_owned());
    }

    /// What `prompt.background` publishes, and when. `when_text` is `before`, `after`, `empty`,
    /// or `quiet`.
    pub fn set_bg_completion(&self, when_text: &str, text: &str, delay: Option<Duration>) {
        let completion = match when_text {
            "before" => BgCompletion::Before {
                text: text.to_string(),
            },
            "after" => BgCompletion::After {
                delay,
                text: text.to_string(),
            },
            "empty" => BgCompletion::After {
                delay,
                text: String::new(),
            },
            _ => BgCompletion::Quiet,
        };
        let mut state = lock(&self.shared);
        state.background.enabled = true;
        state.background.completion = completion;
    }

    /// Whether this connection counts as subscribed for a detached task.
    pub fn set_subscribed(&self, subscribed: bool) {
        let mut state = lock(&self.shared);
        state.background.enabled = true;
        state.background.subscribed = subscribed;
    }

    /// Publish one `agent.terminal.output` event.
    pub async fn publish_terminal_output(&self, process_id: &str, chunk: &str) -> u64 {
        self.send_event(
            "agent.terminal.output",
            json!({"process_id": process_id, "chunk": chunk}),
        )
        .await
    }

    /// Publish one `terminal.close` event.
    pub async fn publish_terminal_close(&self, process_id: &str) -> u64 {
        self.send_event("terminal.close", json!({"process_id": process_id}))
            .await
    }

    /// Publish one `tool.complete` event.
    pub async fn publish_tool_complete(&self, tool_id: &str, name: &str) -> u64 {
        self.send_event("tool.complete", json!({"tool_id": tool_id, "name": name}))
            .await
    }
}

fn plan_background(
    state: &mut State,
    method: &str,
    id: &Value,
    params: &Value,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) -> bool {
    if !state.background.enabled && method != "process.list" {
        return false;
    }
    if !matches!(
        method,
        "process.list" | "process.kill" | "process.stop" | "prompt.background"
    ) {
        return false;
    }
    if state.background.draining {
        outgoing.push((
            None,
            refusal_text(id, "runtime_draining", None),
            reply_meta(id, method),
        ));
        return true;
    }
    if let Some(failure) = state
        .background
        .failures
        .get_mut(method)
        .and_then(VecDeque::pop_front)
    {
        if failure.drop_connection {
            state.kill_after = true;
            return true;
        }
        let text = coded_refusal(id, failure.code, &failure.reason);
        outgoing.push((failure.delay, text, reply_meta(id, method)));
        return true;
    }
    match method {
        "process.list" => plan_process_list(state, id, params, outgoing),
        "process.kill" => plan_process_kill(state, id, params, outgoing),
        "process.stop" => plan_process_stop(state, id, params, outgoing),
        "prompt.background" => plan_prompt_background(state, id, params, outgoing),
        _ => return false,
    }
    true
}

fn plan_process_list(
    state: &mut State,
    id: &Value,
    params: &Value,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) {
    if let Some(reason) = bad_keys(params, LIST_KEYS).or_else(|| blank_session(params)) {
        push_refusal(outgoing, id, "process.list", reason);
        return;
    }
    let session = params["session_id"].as_str().unwrap_or("");
    if state.background.managed.contains(session) {
        push_refusal(outgoing, id, "process.list", "unsupported_projection");
        return;
    }
    if !is_live(state, session) {
        push_refusal(outgoing, id, "process.list", "not_found");
        return;
    }
    let processes: Vec<Value> = state
        .background
        .processes
        .iter()
        .filter(|row| row.session_id == session)
        .map(entry_value)
        .collect();
    let text = json!({"jsonrpc": "2.0", "id": id, "result": {"processes": processes}}).to_string();
    outgoing.push((None, text, reply_meta(id, "process.list")));
}

fn plan_process_kill(
    state: &mut State,
    id: &Value,
    params: &Value,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) {
    if let Some(reason) = bad_keys(params, KILL_KEYS)
        .or_else(|| blank_session(params))
        .or_else(|| blank_process(params))
    {
        push_refusal(outgoing, id, "process.kill", reason);
        return;
    }
    let session = params["session_id"].as_str().unwrap_or("");
    let process_id = params["process_id"].as_str().unwrap_or("");
    let owned = state
        .background
        .processes
        .iter()
        .any(|row| row.session_id == session && row.id == process_id);
    if !owned {
        push_refusal(outgoing, id, "process.kill", "process_not_found");
        return;
    }
    let status = state.background.kill_status.clone();
    let error = state.background.kill_error.clone();
    if status == "killed"
        && let Some(row) = state
            .background
            .processes
            .iter_mut()
            .find(|row| row.id == process_id)
    {
        row.status = "exited".to_string();
        row.exit_code = Some(-15);
        row.completion_reason = Some("killed".to_string());
    }
    let mut result = json!({
        "status": status,
        "session_id": process_id,
        "completion_reason": if status == "killed" { Value::String("killed".into()) } else { Value::Null },
        "termination_source": "process.kill",
    });
    if let Some(error) = error {
        result["error"] = Value::String(error);
    }
    if let Some(scope) = &state.background.kill_scope_stop_failed {
        result["scope_stop_failed"] = Value::String(scope.clone());
    }
    let text = json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string();
    outgoing.push((None, text, reply_meta(id, "process.kill")));
}

fn plan_process_stop(
    state: &mut State,
    id: &Value,
    params: &Value,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) {
    if let Some(reason) = bad_keys(params, STOP_KEYS).or_else(|| blank_session(params)) {
        push_refusal(outgoing, id, "process.stop", reason);
        return;
    }
    let session = params["session_id"].as_str().unwrap_or("");
    let mut killed = 0_i64;
    for row in &mut state.background.processes {
        if row.session_id == session && row.status == "running" {
            row.status = "exited".to_string();
            row.exit_code = Some(-15);
            row.completion_reason = Some("killed".to_string());
            killed += 1;
        }
    }
    let delegations = state.background.delegations;
    let text = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {"killed": killed, "delegations": delegations},
    })
    .to_string();
    outgoing.push((None, text, reply_meta(id, "process.stop")));
}

fn plan_prompt_background(
    state: &mut State,
    id: &Value,
    params: &Value,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
) {
    if let Some(reason) = bad_keys(params, BACKGROUND_KEYS)
        .or_else(|| blank_session(params))
        .or_else(|| blank_text(params))
    {
        push_refusal(outgoing, id, "prompt.background", reason);
        return;
    }
    let session = params["session_id"].as_str().unwrap_or("");
    if !is_live(state, session) {
        push_refusal(outgoing, id, "prompt.background", "not_found");
        return;
    }
    if !state.background.subscribed {
        push_refusal(outgoing, id, "prompt.background", "permission_denied");
        return;
    }
    state.background.bg_counter += 1;
    let task_id = format!("bg_{:06x}", state.background.bg_counter);
    let reply = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {"task_id": task_id},
    })
    .to_string();
    let completion = state.background.completion.clone();
    match completion {
        BgCompletion::Before { text } => {
            push_complete(state, outgoing, &task_id, &text, None);
            outgoing.push((None, reply, reply_meta(id, "prompt.background")));
        }
        BgCompletion::After { delay, text } => {
            outgoing.push((None, reply, reply_meta(id, "prompt.background")));
            push_complete(state, outgoing, &task_id, &text, delay);
        }
        BgCompletion::Quiet => {
            outgoing.push((None, reply, reply_meta(id, "prompt.background")));
        }
    }
}

fn push_complete(
    state: &mut State,
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
    task_id: &str,
    text: &str,
    delay: Option<Duration>,
) {
    let body = event_text(
        state,
        "background.complete",
        json!({"task_id": task_id, "text": text}),
    );
    outgoing.push((
        delay,
        body,
        Meta {
            kind: WrittenKind::Event,
            id: None,
            method: "event".to_owned(),
            event_type: Some("background.complete".to_owned()),
        },
    ));
}

fn entry_value(row: &ScriptedProcess) -> Value {
    let command: String = row.command.chars().take(COMMAND_CHARS).collect();
    let output: String = {
        let chars: Vec<char> = row.output.chars().collect();
        let start = chars.len().saturating_sub(TAIL_CHARS);
        chars[start..].iter().collect()
    };
    json!({
        "session_id": row.id,
        "command": command,
        "status": row.status,
        "exit_code": row.exit_code,
        "started_epoch": row.started_epoch,
        "exited_at": row.exited_at,
        "completion_reason": row.completion_reason,
        "output_tail": output,
    })
}

fn is_live(state: &State, session: &str) -> bool {
    session == state.session_id || state.background.extra_live.contains(session)
}

fn bad_keys(params: &Value, allowed: &[&str]) -> Option<&'static str> {
    let object = params.as_object()?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Some("invalid_params");
    }
    None
}

fn blank_session(params: &Value) -> Option<&'static str> {
    match params.get("session_id").and_then(Value::as_str) {
        Some(text) if !text.is_empty() => None,
        _ => Some("invalid_params"),
    }
}

fn blank_process(params: &Value) -> Option<&'static str> {
    match params.get("process_id") {
        Some(Value::String(text)) if !text.is_empty() => None,
        _ => Some("invalid_params"),
    }
}

fn blank_text(params: &Value) -> Option<&'static str> {
    match params.get("text").and_then(Value::as_str) {
        Some(text) if !text.trim().is_empty() => None,
        _ => Some("invalid_params"),
    }
}

fn push_refusal(
    outgoing: &mut Vec<(Option<Duration>, String, Meta)>,
    id: &Value,
    method: &str,
    reason: &str,
) {
    outgoing.push((None, refusal_text(id, reason, None), reply_meta(id, method)));
}

fn coded_refusal(id: &Value, code: i64, reason: &str) -> String {
    let message = if code == -32601 {
        format!("unknown method: {reason}")
    } else {
        reason.to_string()
    };
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message, "data": {"reason": reason}},
    })
    .to_string()
}
