//! Capture requests without opening a gateway or submitting work.
use super::guard;
use crate::engine::{
    self, AgentBackend, AgentOutcome, AgentRequest, BudgetSnapshot, CallId, EngineConfig,
    MapResolver, ResolveError, RunOutcome, WorkflowRef, WorkflowResolver,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::sync::mpsc;

#[derive(Clone, Debug, PartialEq)]
pub struct CapturedCall {
    pub call_id: CallId,
    pub label: String,
    pub phase: Option<String>,
    pub prompt: String,
    pub schema: Option<Value>,
    pub tier: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub effort: Option<String>,
    pub profile: Option<String>,
    pub toolsets: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub isolation: Option<String>,
    pub max_turns: Option<u32>,
    pub workflow_path: Vec<String>,
    pub outstanding: bool,
}
impl CapturedCall {
    pub fn request(&self) -> Result<AgentRequest, String> {
        let option = |v: &Option<String>| {
            v.as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null)
        };
        Ok(AgentRequest {
            call_id: self.call_id.clone(),
            label: self.label.clone(),
            phase: self.phase.clone(),
            prompt: self.prompt.clone(),
            schema: self.schema.clone(),
            tier: self.tier.clone(),
            model: self.model.clone(),
            provider: self.provider.clone(),
            effort: serde_json::from_value(option(&self.effort))
                .map_err(|_| "invalid reasoning effort")?,
            isolation: serde_json::from_value(option(&self.isolation))
                .map_err(|_| "invalid workspace isolation")?,
            profile: self.profile.clone(),
            toolsets: self.toolsets.clone(),
            skills: self.skills.clone(),
            max_turns: self.max_turns,
            workflow_path: self.workflow_path.clone(),
        })
    }
}
enum Signal {
    Started(Box<AgentRequest>),
    Finished(Vec<String>),
}
struct CapturingBackend {
    sender: mpsc::UnboundedSender<Signal>,
}
impl AgentBackend for CapturingBackend {
    fn run_agent(
        &self,
        request: AgentRequest,
    ) -> futures::future::BoxFuture<'static, AgentOutcome> {
        let sender = self.sender.clone();
        let _ = sender.send(Signal::Started(Box::new(request.clone())));
        Box::pin(async move {
            tokio::task::yield_now().await;
            let value = request.schema.as_ref().map_or_else(
                || json!(sentinel(&request.call_id)),
                |schema| stub(schema, &request.call_id),
            );
            let _ = sender.send(Signal::Finished(request.workflow_path));
            AgentOutcome::Completed(value)
        })
    }
    fn budget(&self) -> BudgetSnapshot {
        BudgetSnapshot::default()
    }
}
pub(super) fn sentinel(id: &CallId) -> String {
    format!(
        "⟨wf:{}:{}⟩",
        id.digest.chars().take(12).collect::<String>(),
        id.occurrence
    )
}
fn stub(schema: &Value, id: &CallId) -> Value {
    match schema["type"].as_str() {
        Some("object") => Value::Object(
            schema["properties"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(key, schema)| (key.clone(), stub(schema, id)))
                .collect(),
        ),
        Some("array") => json!([stub(&schema["items"], id)]),
        Some("number" | "integer") => json!(900_000_000_u64 + u64::from(id.occurrence)),
        Some("boolean") => json!(true),
        Some("null") => Value::Null,
        _ => json!(sentinel(id)),
    }
}
struct Sources {
    base: PathBuf,
    sender: mpsc::UnboundedSender<(String, String)>,
}
impl WorkflowResolver for Sources {
    fn resolve(&self, reference: &WorkflowRef) -> Result<String, ResolveError> {
        let path = match reference {
            WorkflowRef::Name(name) => self.base.join(format!("{name}.js")),
            WorkflowRef::ScriptPath(path) => self.base.join(path),
        };
        let source = std::fs::read_to_string(path).map_err(|_| ResolveError::NotFound)?;
        guard::check(&source).map_err(ResolveError::Unreadable)?;
        let _ = self.sender.send((format!("{reference:?}"), source.clone()));
        Ok(source)
    }
}
pub(super) struct Captures {
    pub calls: Vec<CapturedCall>,
    pub children: Vec<String>,
}
async fn once(
    source: &str,
    args: &Value,
    seed: u64,
    time: i64,
    base: Option<PathBuf>,
) -> Result<Captures, String> {
    let parsed = engine::parse_script(source).map_err(|e| e.to_string())?;
    let mut config = EngineConfig::new("kanban-capture", time, seed);
    config.args = Some(args.clone());
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let backend = Arc::new(CapturingBackend { sender });
    let (sources, mut children) = mpsc::unbounded_channel();
    let resolver: Arc<dyn WorkflowResolver> = match base {
        Some(base) => Arc::new(Sources {
            base,
            sender: sources,
        }),
        None => Arc::new(MapResolver::new()),
    };
    let mut handle = engine::spawn(parsed, config, backend, resolver);
    let outcome = loop {
        tokio::select! {
            outcome = &mut handle.outcome => break outcome.map_err(|_| "capture worker stopped")?,
            event = handle.events.recv() => { if event.is_none() { break handle.outcome.await.map_err(|_| "capture worker stopped")?; } }
        }
    };
    match outcome {
        RunOutcome::Completed { .. } => {}
        RunOutcome::Failed { error } => return Err(format!("{}: {}", error.name, error.message)),
        RunOutcome::Aborted { reason } => return Err(reason),
    }
    let mut calls = Vec::new();
    let mut active: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    while let Ok(signal) = receiver.try_recv() {
        match signal {
            Signal::Started(q) => {
                let outstanding = active.get(&q.workflow_path).copied().unwrap_or(0) > 0;
                *active.entry(q.workflow_path.clone()).or_default() += 1;

                calls.push(CapturedCall {
                    call_id: q.call_id,
                    label: q.label,
                    phase: q.phase,
                    prompt: q.prompt,
                    schema: q.schema,
                    tier: q.tier,
                    model: q.model,
                    provider: q.provider,
                    effort: name(q.effort),
                    profile: q.profile,
                    toolsets: q.toolsets,
                    skills: q.skills,
                    isolation: name(q.isolation),
                    max_turns: q.max_turns,
                    workflow_path: q.workflow_path,
                    outstanding,
                });
            }
            Signal::Finished(path) => {
                if let Some(n) = active.get_mut(&path) {
                    *n = n.saturating_sub(1);
                }
            }
        }
    }
    let mut sources = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while let Ok((reference, source)) = children.try_recv() {
        if seen.insert(reference) {
            sources.push(source);
        }
    }
    Ok(Captures {
        calls,
        children: sources,
    })
}
pub async fn capture_twice(source: &str) -> Result<(Vec<CapturedCall>, Vec<CapturedCall>), String> {
    guard::check(source)?;
    let a = once(source, &json!({}), 11, 0, None).await?;
    let b = once(source, &json!({}), 99, 86_400_000, None).await?;
    compare(&a, &b)?;
    Ok((a.calls, b.calls))
}
pub(super) async fn capture(source: &str, args: &Value, base: PathBuf) -> Result<Captures, String> {
    guard::check(source)?;
    let a = once(source, args, 11, 0, Some(base.clone())).await?;
    let b = once(source, args, 99, 86_400_000, Some(base)).await?;
    compare(&a, &b)?;
    Ok(a)
}
fn normalized(call: &CapturedCall, calls: &[CapturedCall]) -> CapturedCall {
    let mut call = call.clone();
    for parent in calls {
        call.prompt = call
            .prompt
            .replace(&sentinel(&parent.call_id), "⟨wf-result⟩");
    }
    call
}
fn compare(a: &Captures, b: &Captures) -> Result<(), String> {
    for i in 0..a.calls.len().max(b.calls.len()) {
        if a.calls.get(i).map(|q| normalized(q, &a.calls))
            != b.calls.get(i).map(|q| normalized(q, &b.calls))
        {
            let label = a
                .calls
                .get(i)
                .or_else(|| b.calls.get(i))
                .map(|c| c.label.as_str())
                .unwrap_or("unknown");
            return Err(format!(
                "not a static graph: call {label} changed between captures; run it with talaria workflow run instead"
            ));
        }
    }
    if a.children != b.children {
        return Err("not a static graph: call workflow changed between captures; run it with talaria workflow run instead".into());
    }
    Ok(())
}

fn name(v: impl serde::Serialize) -> Option<String> {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
}
