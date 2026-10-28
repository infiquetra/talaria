use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Instant,
};

use futures::future::{BoxFuture, poll_fn};
use rquickjs::{
    AsyncContext, AsyncRuntime, Ctx, Function, Object, Persistent, Value as JsValue,
    context::EvalOptions,
    loader::{BuiltinLoader, ImportAttributes, Resolver},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::{
    types::{Backend, failure},
    *,
};

const ROOT_PREFIX: &str = "(async function () {\"use strict\";";
const CHILD_PREFIX: &str = "(async function (agent, parallel, pipeline, phase, log, args, budget, workflow, now, random) {\"use strict\";";

struct NoModules(Arc<AtomicBool>);
impl Resolver for NoModules {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        self.0.store(true, Ordering::Relaxed);
        Err(rquickjs::Error::new_resolving(base, name))
    }
}

fn compile<'js>(
    ctx: Ctx<'js>,
    body: String,
    filename: String,
    child: bool,
) -> rquickjs::Result<JsValue<'js>> {
    let source = if child {
        format!("{CHILD_PREFIX}{body}\n}})")
    } else {
        format!("{ROOT_PREFIX}{body}\n}})()")
    };
    let mut options = EvalOptions::default();
    options.filename = Some(filename);
    ctx.eval_with_options(source, options)
}

struct Bridge {
    messages: mpsc::UnboundedSender<Value>,
    backend: Backend,
    resolver: Arc<dyn WorkflowResolver>,
    seed: u64,
    stopped: Arc<AtomicBool>,
    turn: Arc<Mutex<Instant>>,
}
impl Bridge {
    fn call(&self, op: &str, text: &str) -> String {
        let value: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(_) => return "null".into(),
        };
        match op {
            "send" => {
                let _ = self.messages.send(value);
                "null".into()
            }
            "budget" => {
                let b = self.backend.budget();
                if !valid_budget(b) {
                    self.stopped.store(true, Ordering::Relaxed);
                    let _ = self.messages.send(
                        json!({"kind":"fatal", "message":"backend budget snapshot is invalid"}),
                    );
                }
                json!({"tokens_total":b.tokens_total,"tokens_spent":b.tokens_spent,"money_total":b.money_total,"money_spent":b.money_spent}).to_string()
            }
            "random" => {
                let path = strings(&value["path"]).join("/");
                let key = value["key"].as_str().unwrap_or_default();
                let mut hash = Sha256::new();
                hash.update(self.seed.to_be_bytes());
                hash.update(path.as_bytes());
                hash.update([0]);
                hash.update(key.as_bytes());
                let digest = hash.finalize();
                let mut bytes = [0_u8; 8];
                bytes.copy_from_slice(&digest[..8]);
                let bits = u64::from_be_bytes(bytes) >> 11;
                json!(bits as f64 / (1_u64 << 53) as f64).to_string()
            }
            "resolve" => {
                let started = Instant::now();
                let result = self.resolve(&value);
                if let Ok(mut turn) = self.turn.lock() {
                    *turn += started.elapsed();
                }
                result.to_string()
            }
            _ => "null".into(),
        }
    }
    fn resolve(&self, value: &Value) -> Value {
        let reference = &value["reference"];
        let (reference, prefix, display) = if let Some(name) = reference["name"].as_str() {
            (WorkflowRef::Name(name.into()), "name", name)
        } else {
            let path = reference["script_path"].as_str().unwrap_or_default();
            (WorkflowRef::ScriptPath(path.into()), "path", path)
        };
        let source = match self.resolver.resolve(&reference) {
            Ok(source) => source,
            Err(e) => {
                return json!({"error":{"name":"WorkflowNotFoundError","message":format!("workflow `{display}` not found: {e}")}});
            }
        };
        match parse_script(&source) {
            Err(e) => {
                json!({"error":{"name":"WorkflowScriptError","message":format!("workflow `{display}`: {e}")}})
            }
            Ok(parsed) => {
                let digest = hex::encode(Sha256::digest(canonical(&value["args"]).as_bytes()));
                json!({"body":parsed.body,"filename":format!("{}.js",parsed.meta.name),"meta":parsed.meta,"warnings":parsed.warnings,
                    "path":[format!("{prefix}:{display}@{}", &digest[..12])], "display":display})
            }
        }
    }
}

fn valid_budget(b: BudgetSnapshot) -> bool {
    b.money_spent.is_finite()
        && b.money_spent >= 0.0
        && b.money_total.is_none_or(|n| n.is_finite() && n >= 0.0)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn canonical(value: &Value) -> String {
    // Build sorted maps explicitly: another workspace crate may enable preserve_order.
    match value {
        Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.iter().collect();
            format!(
                "{{{}}}",
                sorted
                    .into_iter()
                    .map(|(k, v)| format!("{}:{}", json!(k), canonical(v)))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(values) => format!(
            "[{}]",
            values.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        _ => value.to_string(),
    }
}

fn identity(request: &AgentRequest) -> String {
    let mut value = json!({"v":1,"workflow_path":request.workflow_path,"prompt":request.prompt});
    let opts = json!({"schema":request.schema,"tier":request.tier,"model":request.model,"provider":request.provider,
        "effort":request.effort,"profile":request.profile,"toolsets":request.toolsets,"skills":request.skills,
        "isolation":request.isolation,"max_turns":request.max_turns});
    if let (Some(dst), Some(src)) = (value.as_object_mut(), opts.as_object()) {
        for (key, value) in src {
            if !value.is_null() {
                dst.insert(key.clone(), value.clone());
            }
        }
    }
    hex::encode(Sha256::digest(canonical(&value).as_bytes()))
}

struct Pending {
    id: u64,
    request: AgentRequest,
    future: Option<BoxFuture<'static, AgentOutcome>>,
    ready: Option<AgentOutcome>,
}
struct State<'a> {
    config: &'a EngineConfig,
    backend: Backend,
    events: &'a mpsc::UnboundedSender<EngineEvent>,
    pending: Vec<Option<Pending>>,
    arrivals: VecDeque<usize>,
    rejections: VecDeque<Value>,
    occurrences: HashMap<String, u32>,
    replay: HashMap<CallId, AgentOutcome>,
    replay_index: usize,
    ordered_replay: bool,
    count: u32,
    completion_seq: u64,
    stall_warned: bool,
    children: Vec<(u64, Vec<String>)>,
    fatal: Option<RunOutcome>,
}
impl<'a> State<'a> {
    fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
    }
    fn reject(&mut self, id: u64, name: &str, message: String) {
        self.rejections
            .push_back(json!({"id":id,"error":{"name":name,"message":message}}));
    }
    fn message(&mut self, value: Value, stopping: bool) {
        let id = value["id"].as_u64().unwrap_or_default();
        let path = strings(&value["workflow_path"]);
        match value["kind"].as_str() {
            Some("agent") if !stopping => self.agent(id, value["request"].clone()),
            Some("phase") => self.emit(EngineEvent::Phase {
                title: value["title"].as_str().unwrap_or_default().into(),
                workflow_path: path,
            }),
            Some("log") => self.emit(EngineEvent::Log {
                message: value["message"].as_str().unwrap_or_default().into(),
                workflow_path: path,
            }),
            Some("child_started") if !stopping => {
                if let Ok(meta) = serde_json::from_value(value["meta"].clone()) {
                    self.children.push((id, path.clone()));
                    self.emit(EngineEvent::ChildStarted {
                        workflow_path: path,
                        meta,
                        warnings: strings(&value["warnings"]),
                    });
                }
            }
            Some("child_finished") => {
                if let Some(index) = self.children.iter().position(|(child, _)| *child == id) {
                    self.children.remove(index);
                    self.emit(EngineEvent::ChildFinished {
                        workflow_path: path,
                        ok: value["ok"].as_bool().unwrap_or(false),
                    });
                }
            }
            Some("fatal") => {
                self.fatal = Some(failure(
                    "Error",
                    value["message"].as_str().unwrap_or_default(),
                ))
            }
            _ => {}
        }
    }
    fn agent(&mut self, id: u64, mut value: Value) {
        let stalled = value.get("stallMs").is_some();
        if let Some(object) = value.as_object_mut() {
            object.remove("stallMs");
            object.insert("call_id".into(), json!({"digest":"","occurrence":0}));
        }
        let mut request: AgentRequest = match serde_json::from_value(value) {
            Ok(request) => request,
            Err(error) => {
                self.reject(id, "WorkflowOptionError", format!("agent(): {error}"));
                return;
            }
        };
        if stalled && !self.stall_warned {
            self.stall_warned = true;
            self.emit(EngineEvent::Log {
                message: "stallMs is ignored: Talaria does not restart stalled agents".into(),
                workflow_path: request.workflow_path.clone(),
            });
        }
        if self.count >= self.config.max_agents {
            self.reject(
                id,
                "AgentLimitError",
                format!(
                    "the run has started {} agents, the limit",
                    self.config.max_agents
                ),
            );
            return;
        }
        let digest = identity(&request);
        let occurrence = self.occurrences.get(&digest).copied().unwrap_or_default();
        request.call_id = CallId {
            digest: digest.clone(),
            occurrence,
        };
        let replayed = self.replay.contains_key(&request.call_id);
        if !replayed {
            let budget = self.backend.budget();
            if !valid_budget(budget) {
                self.fatal = Some(failure("Error", "backend budget snapshot is invalid"));
                return;
            }
            if let Some(total) = budget
                .tokens_total
                .filter(|total| budget.tokens_spent >= *total)
            {
                self.reject(
                    id,
                    "BudgetExhaustedError",
                    format!(
                        "token budget exhausted (spent {} of {total})",
                        budget.tokens_spent
                    ),
                );
                return;
            }
            if let Some(total) = budget
                .money_total
                .filter(|total| budget.money_spent >= *total)
            {
                self.reject(
                    id,
                    "BudgetExhaustedError",
                    format!(
                        "money budget exhausted (spent {} of {total})",
                        budget.money_spent
                    ),
                );
                return;
            }
        }
        self.occurrences.insert(digest, occurrence + 1);
        self.count += 1;
        self.emit(EngineEvent::AgentStarted {
            request: request.clone(),
            replayed,
        });
        let ready = self.replay.remove(&request.call_id);
        let future = if replayed {
            None
        } else {
            Some(self.backend.run_agent(request.clone()))
        };
        let index = self.pending.len();
        self.pending.push(Some(Pending {
            id,
            request,
            future,
            ready,
        }));
        if replayed && !self.ordered_replay {
            self.arrivals.push_back(index);
        }
    }
    fn poll_live(&mut self, cx: &mut Context<'_>) -> bool {
        let mut progress = false;
        for (index, pending) in self.pending.iter_mut().enumerate() {
            let Some(pending) = pending else { continue };
            if let Some(future) = pending.future.as_mut()
                && let Poll::Ready(outcome) = future.as_mut().poll(cx)
            {
                pending.future = None;
                pending.ready = Some(outcome);
                self.arrivals.push_back(index);
                progress = true;
            }
        }
        progress
    }
    fn next(&mut self) -> Option<Result<Value, RunOutcome>> {
        if let Some(value) = self.rejections.pop_front() {
            return Some(Ok(value));
        }
        let mut index = None;
        if self.ordered_replay {
            let entries = self
                .config
                .replay
                .as_ref()
                .map(|r| r.entries.as_slice())
                .unwrap_or_default();
            if let Some(entry) = entries.get(self.replay_index) {
                index = self.pending.iter().position(|p| {
                    p.as_ref()
                        .is_some_and(|p| p.request.call_id == entry.call_id)
                });
                if index.is_some() {
                    self.replay_index += 1;
                } else {
                    self.emit(EngineEvent::ReplayDiverged {
                        at_seq: self.replay_index as u64,
                    });
                    self.ordered_replay = false;
                    for (i, p) in self.pending.iter().enumerate() {
                        if p.as_ref()
                            .is_some_and(|p| p.ready.is_some() && p.future.is_none())
                            && !self.arrivals.contains(&i)
                        {
                            self.arrivals.push_back(i);
                        }
                    }
                }
            } else {
                self.ordered_replay = false;
            }
        }
        let index = index.or_else(|| self.arrivals.pop_front())?;
        let pending = self.pending.get_mut(index)?.take()?;
        let outcome = pending.ready?;
        if let AgentOutcome::Abort { reason } = outcome {
            // Keep this request pending so finalisation pairs its start with an aborted finish.
            self.pending[index] = Some(Pending {
                ready: None,
                ..pending
            });
            return Some(Err(RunOutcome::Aborted { reason }));
        }
        let (kind, value) = match outcome {
            AgentOutcome::Completed(value) => (OutcomeKind::Completed, value),
            AgentOutcome::Failed { reason } => (OutcomeKind::Failed { reason }, Value::Null),
            AgentOutcome::Skipped => (OutcomeKind::Skipped, Value::Null),
            AgentOutcome::Abort { .. } => unreachable!(),
        };
        self.emit(EngineEvent::AgentFinished {
            call_id: pending.request.call_id,
            completion_seq: Some(self.completion_seq),
            outcome: kind,
        });
        self.completion_seq += 1;
        Some(Ok(json!({"id":pending.id,"value":value})))
    }
    fn finish(&mut self, outcome: &RunOutcome, imported: bool) {
        let aborted = matches!(outcome, RunOutcome::Aborted { .. }) || imported;
        for pending in &mut self.pending {
            if let Some(pending) = pending.take() {
                let _ = self.events.send(EngineEvent::AgentFinished {
                    call_id: pending.request.call_id,
                    completion_seq: None,
                    outcome: if aborted {
                        OutcomeKind::Aborted
                    } else {
                        OutcomeKind::Abandoned
                    },
                });
            }
        }
        for (_, path) in self.children.drain(..) {
            let _ = self.events.send(EngineEvent::ChildFinished {
                workflow_path: path,
                ok: false,
            });
        }
        self.emit(EngineEvent::Finished {
            outcome: match outcome {
                RunOutcome::Completed { .. } => RunOutcomeKind::Completed,
                RunOutcome::Failed { .. } => RunOutcomeKind::Failed,
                RunOutcome::Aborted { .. } => RunOutcomeKind::Aborted,
            },
        });
    }
}

fn locate(mut error: ScriptFailure, root: &ParsedScript) -> ScriptFailure {
    if error.message == "Maximum call stack size exceeded" {
        error.message = format!("stack overflow: {}", error.message);
    }
    if let Some(stack) = error.stack.as_deref() {
        // Prefer script frames to prelude and host frames.
        for frame in stack.lines() {
            let Some(js) = frame.rfind(".js:") else {
                continue;
            };
            let rest = &frame[js + 4..];
            let mut parts = rest
                .split(|c: char| !c.is_ascii_digit())
                .filter(|p| !p.is_empty());
            if let Some(line) = parts.next().and_then(|p| p.parse::<u32>().ok()) {
                error.line = Some(line);
                error.column = parts.next().and_then(|p| p.parse::<u32>().ok());
                if let Some(column) = error.column {
                    let prefix = if error.workflow_path.is_empty() {
                        ROOT_PREFIX.len()
                    } else {
                        CHILD_PREFIX.len()
                    };
                    let column = if line == 1 {
                        column.saturating_sub(prefix as u32).max(1)
                    } else {
                        column
                    };
                    error.column = if error.workflow_path.is_empty() {
                        root.body
                            .lines()
                            .nth(line.saturating_sub(1) as usize)
                            .map(|s| {
                                let offset = (column.saturating_sub(1) as usize).min(s.len());
                                let scalar_column =
                                    s.char_indices().take_while(|(i, _)| *i < offset).count()
                                        as u32
                                        + 1;
                                // QuickJS points at the constructor in `throw new Error(...)`.
                                // Anchor that direct throw to its statement in the original source.
                                let prefix: String = s
                                    .chars()
                                    .take(scalar_column.saturating_sub(1) as usize)
                                    .collect();
                                if let Some(at) = prefix.rfind("throw")
                                    && (at == 0
                                        || !prefix[..at].chars().next_back().is_some_and(|c| {
                                            c.is_alphanumeric() || c == '_' || c == '$'
                                        }))
                                    && prefix[at + 5..].trim() == "new"
                                {
                                    prefix[..at].chars().count() as u32 + 1
                                } else {
                                    scalar_column
                                }
                            })
                    } else {
                        Some(column)
                    };
                }
                break;
            }
        }
    }
    error
}

fn interruption(
    cancel: &CancellationToken,
    imported: &AtomicBool,
    expired: &AtomicBool,
    stopped: &AtomicBool,
    slice: std::time::Duration,
) -> Option<RunOutcome> {
    if cancel.is_cancelled() {
        return Some(RunOutcome::Aborted {
            reason: "cancelled".into(),
        });
    }
    if imported.load(Ordering::Relaxed) {
        return Some(failure(
            "ImportNotAllowed",
            "modules are not available in workflow scripts",
        ));
    }
    if stopped.load(Ordering::Relaxed) {
        return Some(failure("Error", "backend budget snapshot is invalid"));
    }
    if expired.load(Ordering::Relaxed) {
        return Some(failure(
            "InternalError",
            format!(
                "the script ran JavaScript for more than {} s without waiting for an agent or a child workflow",
                slice.as_secs_f64()
            ),
        ));
    }
    None
}

fn js_error(ctx: &Ctx<'_>, error: rquickjs::Error) -> RunOutcome {
    if error.is_exception() {
        let caught = ctx.catch();
        if let Some(object) = caught.as_object() {
            return RunOutcome::Failed {
                error: ScriptFailure {
                    name: object.get("name").unwrap_or_else(|_| "Error".into()),
                    message: object
                        .get("message")
                        .unwrap_or_else(|_| "out of memory".into()),
                    line: None,
                    column: None,
                    stack: object.get("stack").ok(),
                    workflow_path: Vec::new(),
                },
            };
        }
    }
    failure(
        "Error",
        if matches!(error, rquickjs::Error::Allocation) {
            "out of memory".into()
        } else {
            error.to_string()
        },
    )
}

pub async fn run(
    parsed: ParsedScript,
    config: EngineConfig,
    backend: Backend,
    resolver: Arc<dyn WorkflowResolver>,
    events: mpsc::UnboundedSender<EngineEvent>,
    cancel: CancellationToken,
) -> RunOutcome {
    let _ = events.send(EngineEvent::Started {
        meta: parsed.meta.clone(),
        warnings: parsed.warnings.clone(),
    });
    let mut state = State {
        config: &config,
        backend: backend.clone(),
        events: &events,
        pending: Vec::new(),
        arrivals: VecDeque::new(),
        rejections: VecDeque::new(),
        occurrences: HashMap::new(),
        replay: config
            .replay
            .as_ref()
            .map(|r| {
                r.entries
                    .iter()
                    .map(|e| (e.call_id.clone(), e.outcome.clone()))
                    .collect()
            })
            .unwrap_or_default(),
        replay_index: 0,
        ordered_replay: config
            .replay
            .as_ref()
            .is_some_and(|r| !r.entries.is_empty()),
        count: 0,
        completion_seq: 0,
        stall_warned: false,
        children: Vec::new(),
        fatal: None,
    };
    let outcome = execute(&parsed, &config, backend, resolver, &cancel, &mut state).await;
    let imported = matches!(&outcome,RunOutcome::Failed {error} if error.name=="ImportNotAllowed");
    state.finish(&outcome, imported);
    outcome
}

async fn execute(
    parsed: &ParsedScript,
    config: &EngineConfig,
    backend: Backend,
    resolver: Arc<dyn WorkflowResolver>,
    cancel: &CancellationToken,
    state: &mut State<'_>,
) -> RunOutcome {
    let runtime = match AsyncRuntime::new() {
        Ok(r) => r,
        Err(e) => return failure("Error", e.to_string()),
    };
    runtime.set_memory_limit(config.memory_limit_bytes).await;
    runtime.set_max_stack_size(config.max_stack_bytes).await;
    let imported = Arc::new(AtomicBool::new(false));
    let expired = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    let turn = Arc::new(Mutex::new(Instant::now()));
    let (interrupt_cancel, interrupt_import, interrupt_expired, interrupt_stop, interrupt_turn) = (
        cancel.clone(),
        imported.clone(),
        expired.clone(),
        stopped.clone(),
        turn.clone(),
    );
    let slice = config.max_sync_slice;
    runtime
        .set_interrupt_handler(Some(Box::new(move || {
            let timeout = interrupt_turn.lock().is_ok_and(|t| t.elapsed() > slice);
            if timeout {
                interrupt_expired.store(true, Ordering::Relaxed);
            }
            timeout
                || interrupt_cancel.is_cancelled()
                || interrupt_import.load(Ordering::Relaxed)
                || interrupt_stop.load(Ordering::Relaxed)
        })))
        .await;
    runtime
        .set_loader(NoModules(imported.clone()), BuiltinLoader::default())
        .await;
    let context = match AsyncContext::full(&runtime).await {
        Ok(c) => c,
        Err(e) => return failure("Error", e.to_string()),
    };
    let (messages, mut receiver) = mpsc::unbounded_channel();
    let bridge = Bridge {
        messages,
        backend,
        resolver,
        seed: config.seed,
        stopped: stopped.clone(),
        turn: turn.clone(),
    };
    let setup=context.with(|ctx| {
        let result=(|| {
            let factory:Function=ctx.eval(include_str!("prelude.js"))?;
            let host=Function::new(ctx.clone(),move |op:String,text:String|bridge.call(&op,&text))?;
            let compiler=Function::new(ctx.clone(),compile)?;
            let config_text=json!({"args":config.args,"has_args":config.args.is_some(),"started_at_ms":config.started_at_ms,"max_items":config.max_items_per_call}).to_string();
            let control:Object=factory.call((host,compiler,config_text))?;
            let start:Function=control.get("start")?;
            start.call::<_,()>((parsed.body.clone(),format!("{}.js",parsed.meta.name)))?;
            Ok(Persistent::save(&ctx,control))
        })();
        result.map_err(|e|js_error(&ctx,e))
    }).await;
    let control = match setup {
        Ok(c) => c,
        Err(e) => {
            // A synchronous body may interrupt before the controller has returned.
            while let Ok(message) = receiver.try_recv() {
                state.message(message, true);
            }
            return interruption(cancel, &imported, &expired, &stopped, slice).unwrap_or(e);
        }
    };
    loop {
        while let Ok(message) = receiver.try_recv() {
            state.message(
                message,
                cancel.is_cancelled()
                    || imported.load(Ordering::Relaxed)
                    || stopped.load(Ordering::Relaxed),
            );
        }
        if cancel.is_cancelled() {
            return RunOutcome::Aborted {
                reason: "cancelled".into(),
            };
        }
        if imported.load(Ordering::Relaxed) {
            return failure(
                "ImportNotAllowed",
                "modules are not available in workflow scripts",
            );
        }
        if let Some(fatal) = state.fatal.take() {
            return fatal;
        }
        if expired.load(Ordering::Relaxed) || turn.lock().is_ok_and(|t| t.elapsed() > slice) {
            return failure(
                "InternalError",
                format!(
                    "the script ran JavaScript for more than {} s without waiting for an agent or a child workflow",
                    slice.as_secs_f64()
                ),
            );
        }
        let snapshot = context
            .with(|ctx| {
                let result = (|| {
                    let object = control.clone().restore(&ctx)?;
                    object.get::<_, Function>("state")?.call::<_, String>(())
                })();
                result.map_err(|e| js_error(&ctx, e))
            })
            .await;
        match snapshot {
            Ok(text) if text != "null" => {
                let value: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => return failure("Error", e.to_string()),
                };
                if let Some(error) = value.get("error") {
                    let error = ScriptFailure {
                        name: error["name"].as_str().unwrap_or("Error").into(),
                        message: error["message"].as_str().unwrap_or_default().into(),
                        line: None,
                        column: None,
                        stack: error["stack"].as_str().map(str::to_owned),
                        workflow_path: strings(&error["workflow_path"]),
                    };
                    return RunOutcome::Failed {
                        error: locate(error, parsed),
                    };
                }
                return RunOutcome::Completed {
                    result: value["result"].clone(),
                };
            }
            Err(e) => {
                return interruption(cancel, &imported, &expired, &stopped, slice).unwrap_or(e);
            }
            _ => {}
        }
        if runtime.is_job_pending().await {
            match runtime.execute_pending_job().await {
                Ok(_) => {}
                Err(e) => {
                    if let Some(outcome) =
                        interruption(cancel, &imported, &expired, &stopped, slice)
                    {
                        return outcome;
                    }
                    return e
                        .0
                        .with(|ctx| js_error(&ctx, rquickjs::Error::Exception))
                        .await;
                }
            }
            continue;
        }
        // Poll every backend in invocation order. Each ready batch therefore has a stable tie order.
        poll_fn(|cx| {
            state.poll_live(cx);
            Poll::Ready(())
        })
        .await;
        if let Some(delivery) = state.next() {
            let delivery = match delivery {
                Ok(d) => d,
                Err(outcome) => return outcome,
            };
            if let Ok(mut start) = turn.lock() {
                *start = Instant::now();
            }
            let result = context
                .with(|ctx| {
                    let result = (|| {
                        let object = control.clone().restore(&ctx)?;
                        object
                            .get::<_, Function>("deliver")?
                            .call::<_, ()>((delivery.to_string(),))
                    })();
                    result.map_err(|e| js_error(&ctx, e))
                })
                .await;
            if let Err(e) = result {
                return e;
            }
            continue;
        }
        if !state.pending.iter().any(Option::is_some) {
            return failure(
                "Error",
                "workflow is waiting on a promise with no host work",
            );
        }
        tokio::select! {
            biased;
            _=cancel.cancelled()=>return RunOutcome::Aborted {reason:"cancelled".into()},
            _=poll_fn(|cx| if state.poll_live(cx) {Poll::Ready(())} else {Poll::Pending})=>{},
        }
        if let Ok(mut start) = turn.lock() {
            *start = Instant::now();
        }
    }
}

pub fn spawn(
    parsed: ParsedScript,
    config: EngineConfig,
    backend: Backend,
    resolver: Arc<dyn WorkflowResolver>,
) -> RunHandle {
    let (events_tx, events) = mpsc::unbounded_channel();
    let (outcome_tx, outcome) = oneshot::channel();
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();
    std::thread::spawn(move || {
        let result = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(run(
                parsed,
                config,
                backend,
                resolver,
                events_tx,
                worker_cancel,
            )),
            Err(error) => failure("Error", error.to_string()),
        };
        let _ = outcome_tx.send(result);
    });
    RunHandle {
        events,
        cancel,
        outcome,
    }
}
