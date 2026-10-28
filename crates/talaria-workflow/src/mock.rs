//! Rule-driven backend for reproducible workflow tests and local dry runs.

use std::sync::{Arc, Mutex, MutexGuard};

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::Value;

use crate::engine::{AgentBackend, AgentOutcome, AgentRequest, BudgetSnapshot};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MockMatch {
    pub prompt_contains: Option<String>,
    pub prompt_equals: Option<String>,
    pub label: Option<String>,
    pub phase: Option<String>,
    pub tier: Option<String>,
    pub model: Option<String>,
    pub occurrence: Option<u32>,
}
impl MockMatch {
    fn empty(&self) -> bool {
        self.prompt_contains.is_none()
            && self.prompt_equals.is_none()
            && self.label.is_none()
            && self.phase.is_none()
            && self.tier.is_none()
            && self.model.is_none()
            && self.occurrence.is_none()
    }
    fn matches(&self, request: &AgentRequest) -> bool {
        self.prompt_contains
            .as_ref()
            .is_none_or(|v| request.prompt.contains(v))
            && self
                .prompt_equals
                .as_ref()
                .is_none_or(|v| request.prompt == *v)
            && self.label.as_ref().is_none_or(|v| request.label == *v)
            && self
                .phase
                .as_ref()
                .is_none_or(|v| request.phase.as_ref() == Some(v))
            && self
                .tier
                .as_ref()
                .is_none_or(|v| request.tier.as_ref() == Some(v))
            && self
                .model
                .as_ref()
                .is_none_or(|v| request.model.as_ref() == Some(v))
            && self
                .occurrence
                .is_none_or(|v| request.call_id.occurrence == v)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MockOutcome {
    Text(String),
    Object(Value),
    Failed(String),
    Skipped,
    Abort(String),
}
impl MockOutcome {
    fn outcome(&self) -> AgentOutcome {
        match self {
            Self::Text(text) => AgentOutcome::Completed(Value::String(text.clone())),
            Self::Object(value) => AgentOutcome::Completed(value.clone()),
            Self::Failed(reason) => AgentOutcome::Failed {
                reason: reason.clone(),
            },
            Self::Skipped => AgentOutcome::Skipped,
            Self::Abort(reason) => AgentOutcome::Abort {
                reason: reason.clone(),
            },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MockBudget {
    pub tokens_total: Option<u64>,
    pub tokens_per_agent: u64,
    pub money_total: Option<f64>,
    pub money_per_agent: f64,
}
#[derive(Clone, Debug)]
pub struct MockRule {
    matcher: MockMatch,
    outcome: MockOutcome,
    delay: u64,
}
impl MockRule {
    pub fn new(matcher: MockMatch, outcome: MockOutcome) -> Self {
        Self {
            matcher,
            outcome,
            delay: 0,
        }
    }
    pub fn delay_ms(mut self, delay: u64) -> Self {
        self.delay = delay;
        self
    }
}
#[derive(Clone, Debug, Default)]
pub struct MockBuilder {
    default: Option<MockOutcome>,
    rules: Vec<MockRule>,
    budget: MockBudget,
}
impl MockBuilder {
    pub fn default_outcome(mut self, outcome: MockOutcome) -> Self {
        self.default = Some(outcome);
        self
    }
    pub fn rule(mut self, rule: MockRule) -> Self {
        self.rules.push(rule);
        self
    }
    pub fn budget(mut self, budget: MockBudget) -> Self {
        self.budget = budget;
        self
    }
    pub fn build(self) -> MockBackend {
        if let Err(error) = self.validate() {
            panic!("{error}");
        }
        self.finish()
    }
    fn validate(&self) -> Result<(), MockRulesError> {
        if !self.budget.money_per_agent.is_finite() || self.budget.money_per_agent < 0.0 {
            return Err(rules_error(
                0,
                "budget.money_per_agent must be finite and non-negative",
            ));
        }
        if self
            .budget
            .money_total
            .is_some_and(|v| !v.is_finite() || v < 0.0)
        {
            return Err(rules_error(
                0,
                "budget.money_total must be finite and non-negative",
            ));
        }
        for (i, rule) in self.rules.iter().enumerate() {
            if rule.matcher.empty() {
                return Err(rules_error(i, "match must not be empty"));
            }
        }
        Ok(())
    }
    fn finish(self) -> MockBackend {
        MockBackend {
            config: Arc::new(self),
            state: Arc::new(Mutex::new(Recorded::default())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct MockRulesError {
    message: String,
}
fn rules_error(index: usize, problem: impl std::fmt::Display) -> MockRulesError {
    MockRulesError {
        message: format!("mock rule {index}: {problem}"),
    }
}

#[derive(Clone, Debug)]
pub struct MockBackend {
    config: Arc<MockBuilder>,
    state: Arc<Mutex<Recorded>>,
}
#[derive(Debug, Default)]
struct Recorded {
    requests: Vec<AgentRequest>,
    calls: Vec<(AgentRequest, AgentOutcome)>,
    in_flight: usize,
    peak: usize,
    tokens_spent: u64,
    money_spent: f64,
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
struct Flight(Arc<Mutex<Recorded>>);
impl Drop for Flight {
    fn drop(&mut self) {
        lock(&self.0).in_flight -= 1;
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesFile {
    default: Option<Value>,
    #[serde(default)]
    rules: Vec<Value>,
    #[serde(default)]
    budget: MockBudget,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    #[serde(rename = "match")]
    matcher: MockMatch,
    outcome: Value,
    #[serde(default)]
    delay_ms: u64,
}

fn parse_outcome(value: Value, index: usize) -> Result<MockOutcome, MockRulesError> {
    if value == Value::String("skipped".into()) {
        return Ok(MockOutcome::Skipped);
    }
    let object = value
        .as_object()
        .filter(|o| o.len() == 1)
        .ok_or_else(|| rules_error(index, "outcome must have exactly one form"))?;
    let (key, value) = object
        .iter()
        .next()
        .ok_or_else(|| rules_error(index, "missing outcome"))?;
    if key == "object" {
        return Ok(MockOutcome::Object(value.clone()));
    }
    let text = value
        .as_str()
        .ok_or_else(|| rules_error(index, format!("outcome.{key} must be a string")))?
        .to_owned();
    match key.as_str() {
        "text" => Ok(MockOutcome::Text(text)),
        "failed" => Ok(MockOutcome::Failed(text)),
        "abort" => Ok(MockOutcome::Abort(text)),
        _ => Err(rules_error(index, format!("unknown outcome field {key}"))),
    }
}

impl MockBackend {
    pub fn builder() -> MockBuilder {
        MockBuilder::default()
    }
    pub fn from_json(source: &str) -> Result<Self, MockRulesError> {
        let file: RulesFile = serde_json::from_str(source).map_err(|e| rules_error(0, e))?;
        let mut builder = Self::builder().budget(file.budget);
        if let Some(value) = file.default {
            builder = builder.default_outcome(parse_outcome(value, 0)?);
        }
        for (index, value) in file.rules.into_iter().enumerate() {
            if let Some(delay) = value.get("delay_ms")
                && delay.as_u64().is_none()
            {
                return Err(rules_error(
                    index,
                    "delay_ms must be a non-negative integer",
                ));
            }
            let rule: RuleFile =
                serde_json::from_value(value).map_err(|e| rules_error(index, e))?;
            builder = builder.rule(
                MockRule::new(rule.matcher, parse_outcome(rule.outcome, index)?)
                    .delay_ms(rule.delay_ms),
            );
        }
        builder.validate()?;
        Ok(builder.finish())
    }
    pub fn requests(&self) -> Vec<AgentRequest> {
        lock(&self.state).requests.clone()
    }
    pub fn calls(&self) -> Vec<(AgentRequest, AgentOutcome)> {
        lock(&self.state).calls.clone()
    }
    pub fn max_in_flight(&self) -> usize {
        lock(&self.state).peak
    }
    pub fn in_flight(&self) -> usize {
        lock(&self.state).in_flight
    }
}
impl AgentBackend for MockBackend {
    fn run_agent(&self, request: AgentRequest) -> BoxFuture<'static, AgentOutcome> {
        let matched = self
            .config
            .rules
            .iter()
            .find(|rule| rule.matcher.matches(&request));
        let (outcome, delay) = match matched {
            Some(rule) => (rule.outcome.outcome(), rule.delay),
            None => (
                self.config
                    .default
                    .as_ref()
                    .map(MockOutcome::outcome)
                    .unwrap_or_else(|| AgentOutcome::Failed {
                        reason: "mock: no rule matched".into(),
                    }),
                0,
            ),
        };
        {
            let mut state = lock(&self.state);
            state.requests.push(request.clone());
            state.in_flight += 1;
            state.peak = state.peak.max(state.in_flight);
        }
        let flight = Flight(self.state.clone());
        let state = self.state.clone();
        let budget = self.config.budget;
        Box::pin(async move {
            let _flight = flight;
            if delay > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            let mut state = lock(&state);
            if matches!(outcome, AgentOutcome::Completed(_)) {
                state.tokens_spent = state.tokens_spent.saturating_add(budget.tokens_per_agent);
                state.money_spent += budget.money_per_agent;
            }
            state.calls.push((request, outcome.clone()));
            outcome
        })
    }
    fn budget(&self) -> BudgetSnapshot {
        let state = lock(&self.state);
        BudgetSnapshot {
            tokens_total: self.config.budget.tokens_total,
            tokens_spent: state.tokens_spent,
            money_total: self.config.budget.money_total,
            money_spent: state.money_spent,
        }
    }
}
