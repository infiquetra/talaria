//! Turn-token sums and the last recorded session cost.
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub cost_status: Option<String>,
}

pub fn call_usage(turn_usages: &[Value]) -> AgentUsage {
    let _ = turn_usages;
    todo!("hermes binding stub")
}
