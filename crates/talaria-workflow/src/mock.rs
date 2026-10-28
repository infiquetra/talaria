use crate::engine::{AgentBackend, AgentOutcome, AgentRequest, BudgetSnapshot};
use futures_util::future::BoxFuture;
use serde_json::Value;

#[derive(Debug)]
pub struct MockBackend;
#[derive(Debug)]
pub struct MockRulesError;
impl std::fmt::Display for MockRulesError {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}
impl std::error::Error for MockRulesError {}
#[derive(Clone, Debug, Default)]
pub struct MockMatch {
    pub prompt_contains: Option<String>,
    pub prompt_equals: Option<String>,
    pub label: Option<String>,
    pub phase: Option<String>,
    pub tier: Option<String>,
    pub model: Option<String>,
    pub occurrence: Option<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum MockOutcome {
    Text(String),
    Object(Value),
    Failed(String),
    Skipped,
    Abort(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct MockBudget {
    pub tokens_total: Option<u64>,
    pub tokens_per_agent: u64,
    pub money_total: Option<f64>,
    pub money_per_agent: f64,
}
pub struct MockRule;
impl MockRule {
    pub fn new(_matcher: MockMatch, _outcome: MockOutcome) -> Self {
        todo!()
    }
    pub fn delay_ms(self, _delay: u64) -> Self {
        todo!()
    }
}
pub struct MockBuilder;
impl MockBuilder {
    pub fn default_outcome(self, _outcome: MockOutcome) -> Self {
        todo!()
    }
    pub fn rule(self, _rule: MockRule) -> Self {
        todo!()
    }
    pub fn budget(self, _budget: MockBudget) -> Self {
        todo!()
    }
    pub fn build(self) -> MockBackend {
        todo!()
    }
}
impl MockBackend {
    pub fn from_json(_json: &str) -> Result<Self, MockRulesError> {
        todo!()
    }
    pub fn builder() -> MockBuilder {
        todo!()
    }
    pub fn requests(&self) -> Vec<AgentRequest> {
        todo!()
    }
    pub fn calls(&self) -> Vec<(AgentRequest, AgentOutcome)> {
        todo!()
    }
    pub fn max_in_flight(&self) -> usize {
        todo!()
    }
    pub fn in_flight(&self) -> usize {
        todo!()
    }
}
impl AgentBackend for MockBackend {
    fn run_agent(&self, _request: AgentRequest) -> BoxFuture<'static, AgentOutcome> {
        todo!()
    }
    fn budget(&self) -> BudgetSnapshot {
        todo!()
    }
}
