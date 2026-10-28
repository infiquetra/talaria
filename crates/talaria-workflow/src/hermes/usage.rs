//! Turn-token sums and the last recorded cumulative session cost.
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
    let mut usage = AgentUsage::default();
    let mut invalid = false;
    for turn in turn_usages {
        for (name, total) in [
            ("input_tokens", &mut usage.input_tokens),
            ("output_tokens", &mut usage.output_tokens),
            ("total_tokens", &mut usage.total_tokens),
        ] {
            if let Some(value) = turn.get(name) {
                if let Some(n) = value.as_u64() {
                    *total = Some(total.unwrap_or(0).saturating_add(n));
                } else {
                    invalid = true;
                }
            }
        }
        if let Some(cost) = turn["session_cost"].as_object() {
            if let Some(v) = cost.get("estimated_cost_usd") {
                if let Some(n) = v.as_f64().filter(|n| n.is_finite()) {
                    usage.cost_usd = Some(n);
                } else {
                    invalid = true;
                }
            }
            if let Some(status) = cost.get("cost_status").and_then(Value::as_str) {
                usage.cost_status = Some(status.into());
            }
        }
    }
    if invalid {
        tracing::warn!("non-numeric agent usage ignored");
    }
    usage
}
