//! Check a workflow script before a run: no gateway, no agent, no spend.
//!
//! What it checks, in order, stopping at the first stage that cannot continue:
//!
//! 1. the `meta` block and the script's imports, by the same parser a run uses;
//! 2. JavaScript syntax, by compiling the body in the same wrapper a run uses without calling it;
//! 3. a probe: the script runs against stand-in agents that answer instantly with made-up
//!    values shaped like the call's `schema`. The probe finds options the engine refuses, a
//!    provider named without a model, a tier the staffing table lacks, nested workflows that do
//!    not resolve or compile, and use of `Date.now()` or `Math.random()`.
//!
//! What it cannot check: anything that depends on real agent answers, models and providers
//! (they need the gateway's roster), whether agents will succeed, or code the probe never
//! reaches because a stand-in answer sent the script down another branch. A refusal raised
//! inside `parallel()` or `pipeline()` is turned into a `null` result by those functions, so
//! the probe cannot see it either.

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use futures::future::BoxFuture;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    engine::{
        self, AgentBackend, AgentOutcome, AgentRequest, BudgetSnapshot, EngineConfig, RunOutcome,
        ScriptError,
    },
    runner::FilesystemResolver,
    staffing::{StaffingTable, script_routes},
};

/// Errors a script author caused and a run would hit regardless of what the agents answer.
const DEFINITE_FAILURES: &[&str] = &[
    "WorkflowOptionError",
    "SchemaError",
    "DeterminismError",
    "ItemLimitError",
    "NestedWorkflowError",
    "WorkflowScriptError",
    "WorkflowNotFoundError",
];

#[derive(Clone, Debug)]
pub struct CheckOptions {
    pub args: Option<Value>,
    /// Where `workflow("name")` finds named workflows.
    pub workflows_dir: PathBuf,
    /// Where `workflow({scriptPath})` looks first.
    pub script_dir: PathBuf,
    /// Where `workflow({scriptPath})` looks second.
    pub cwd: PathBuf,
    pub staffing: StaffingTable,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Finding {
    /// `meta`, `import`, `syntax`, `option`, `route` or `probe`.
    pub kind: &'static str,
    pub message: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// A model and provider a call names, as the script spells them. Not checked against any roster.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NamedRoute {
    pub line: u32,
    pub model: String,
    pub provider: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Probe {
    pub ran: bool,
    /// `completed`, `failed`, `aborted` or `skipped`.
    pub outcome: &'static str,
    /// How many agent calls the probe reached; none of them was run.
    pub agent_calls: u32,
    /// The distinct tiers the reached calls named.
    pub tiers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CheckReport {
    pub version: u32,
    pub ok: bool,
    pub name: Option<String>,
    pub description: Option<String>,
    pub phases: Vec<String>,
    pub errors: Vec<Finding>,
    pub warnings: Vec<Finding>,
    pub named_routes: Vec<NamedRoute>,
    pub probe: Probe,
}

impl CheckReport {
    fn new() -> Self {
        Self {
            version: 1,
            ok: false,
            name: None,
            description: None,
            phases: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            named_routes: Vec::new(),
            probe: Probe {
                ran: false,
                outcome: "skipped",
                agent_calls: 0,
                tiers: Vec::new(),
            },
        }
    }
    fn finish(mut self) -> Self {
        self.ok = self.errors.is_empty();
        self
    }
}

/// A stand-in for an agent's answer: a value shaped like `schema`, or a short sentence.
fn stand_in(schema: Option<&Value>) -> Value {
    let Some(schema) = schema else {
        return json!("stand-in answer");
    };
    if let Some(first) = schema["enum"].as_array().and_then(|values| values.first()) {
        return first.clone();
    }
    match schema["type"].as_str() {
        Some("object") => Value::Object(
            schema["properties"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(key, child)| (key.clone(), stand_in(Some(child))))
                .collect(),
        ),
        Some("array") => json!([stand_in(Some(&schema["items"]))]),
        Some("number" | "integer") => json!(1),
        Some("boolean") => json!(true),
        Some("null") => Value::Null,
        _ => json!("stand-in answer"),
    }
}

struct StandInBackend {
    calls: Arc<Mutex<Vec<AgentRequest>>>,
}
impl AgentBackend for StandInBackend {
    fn run_agent(&self, request: AgentRequest) -> BoxFuture<'static, AgentOutcome> {
        let value = stand_in(request.schema.as_ref());
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(request);
        }
        Box::pin(async move {
            tokio::task::yield_now().await;
            AgentOutcome::Completed(value)
        })
    }
    fn budget(&self) -> BudgetSnapshot {
        BudgetSnapshot::default()
    }
}

fn finding(
    kind: &'static str,
    message: impl Into<String>,
    at: (Option<u32>, Option<u32>),
) -> Finding {
    Finding {
        kind,
        message: message.into(),
        line: at.0,
        column: at.1,
    }
}

/// Problems the staffing table alone settles for a call the probe reached.
fn route_problems(call: &AgentRequest, table: &StaffingTable) -> Option<String> {
    if call.model.is_some() {
        return None;
    }
    if let Some(provider) = &call.provider {
        return Some(format!(
            "agent \"{}\": provider '{provider}' needs a model; to move a tier to another provider, edit staffing.toml",
            call.label
        ));
    }
    let tier = call.tier.as_ref()?;
    if table.tiers.contains_key(tier) {
        return None;
    }
    Some(if table.tiers.is_empty() {
        format!(
            "agent \"{}\": tier '{tier}' needs a staffing table, and none defines any tier",
            call.label
        )
    } else {
        format!(
            "agent \"{}\": tier '{tier}' is not in the staffing table; defined tiers: {}",
            call.label,
            table.tiers.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    })
}

pub async fn check_script(source: &str, options: &CheckOptions) -> CheckReport {
    let mut report = CheckReport::new();
    let parsed = match engine::parse_script(source) {
        Ok(parsed) => parsed,
        Err(error) => {
            let (kind, at) = match &error {
                ScriptError::MetaMissing { line } => ("meta", (Some(*line), None)),
                ScriptError::MetaNotLiteral { line, column, .. } => {
                    ("meta", (Some(*line), Some(*column)))
                }
                ScriptError::MetaField { .. } => ("meta", (None, None)),
                ScriptError::ImportNotAllowed { line, column } => {
                    ("import", (Some(*line), Some(*column)))
                }
            };
            report.errors.push(finding(kind, error.to_string(), at));
            return report.finish();
        }
    };
    report.name = Some(parsed.meta.name.clone());
    report.description = Some(parsed.meta.description.clone());
    report.phases = parsed.meta.phases.iter().map(|p| p.title.clone()).collect();
    for warning in &parsed.warnings {
        report
            .warnings
            .push(finding("meta", warning.clone(), (None, None)));
    }
    report.named_routes = script_routes(&parsed.body)
        .into_iter()
        .map(|r| NamedRoute {
            line: r.line,
            model: r.model,
            provider: r.provider,
        })
        .collect();
    if let Err(issue) = engine::check_syntax(&parsed).await {
        report
            .errors
            .push(finding("syntax", issue.message, (issue.line, issue.column)));
        return report.finish();
    }

    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut config = EngineConfig::new("check", 0, 0);
    config.args = options.args.clone();
    let resolver = Arc::new(
        FilesystemResolver::new(options.workflows_dir.clone(), options.script_dir.clone())
            .with_cwd(options.cwd.clone()),
    );
    let handle = engine::spawn(
        parsed,
        config,
        Arc::new(StandInBackend {
            calls: calls.clone(),
        }),
        resolver,
    );
    let outcome = handle.outcome.await;
    drop(handle.events);
    report.probe.ran = true;
    match outcome {
        Ok(RunOutcome::Completed { .. }) => report.probe.outcome = "completed",
        Ok(RunOutcome::Failed { error }) => {
            report.probe.outcome = "failed";
            let definite = DEFINITE_FAILURES.contains(&error.name.as_str());
            let at = (error.line, error.column);
            if definite {
                report.errors.push(finding(
                    "option",
                    format!("{}: {}", error.name, error.message),
                    at,
                ));
            } else {
                report.warnings.push(finding(
                    "probe",
                    format!(
                        "the probe run stopped with {}: {}. Stand-in agent answers may have caused this; real answers can differ",
                        error.name, error.message
                    ),
                    at,
                ));
            }
        }
        Ok(RunOutcome::Aborted { reason }) => {
            report.probe.outcome = "aborted";
            report.warnings.push(finding(
                "probe",
                format!("the probe run was aborted: {reason}"),
                (None, None),
            ));
        }
        Err(_) => {
            report.probe.outcome = "aborted";
            report.warnings.push(finding(
                "probe",
                "the probe worker stopped before it finished",
                (None, None),
            ));
        }
    }
    let calls = calls.lock().map(|c| c.clone()).unwrap_or_default();
    report.probe.agent_calls = u32::try_from(calls.len()).unwrap_or(u32::MAX);
    report.probe.tiers = calls
        .iter()
        .filter_map(|c| c.tier.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut seen = BTreeSet::new();
    for call in &calls {
        if let Some(problem) = route_problems(call, &options.staffing)
            && seen.insert(problem.clone())
        {
            report.errors.push(finding("route", problem, (None, None)));
        }
    }
    report.finish()
}
