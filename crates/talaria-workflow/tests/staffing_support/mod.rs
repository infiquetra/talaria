#![allow(dead_code)]
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
use talaria_workflow::{
    engine::{AgentRequest, CallId},
    staffing::{Limits, Roster, RunBudget, StaffingResolver, StaffingTable, TierSpec},
};
pub fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-22T16:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
pub fn budget() -> RunBudget {
    RunBudget {
        tokens: None,
        money_usd: None,
    }
}
pub fn table() -> StaffingTable {
    StaffingTable {
        tiers: BTreeMap::from([(
            "survey".into(),
            TierSpec {
                model: "m-small".into(),
                provider: "alpha-cloud".into(),
                effort: None,
                note: None,
            },
        )]),
        default_tier: None,
        limits: Limits { max_in_flight: 10 },
        providers: BTreeMap::new(),
        budget: budget(),
    }
}
pub fn data() -> Value {
    json!({"model":"m-small", "provider":"alpha-cloud", "providers":[{"slug":"alpha-cloud", "name":"Alpha", "authenticated":true, "models":["m-small", "m-large"]}]})
}
pub fn roster(value: &Value) -> Roster {
    Roster::from_model_options(value, "fixture", now()).unwrap()
}
pub fn resolver(t: StaffingTable, v: &Value) -> StaffingResolver {
    StaffingResolver::new(Arc::new(t), Arc::new(roster(v)))
}
pub fn request() -> AgentRequest {
    AgentRequest {
        call_id: CallId {
            digest: "a".repeat(64),
            occurrence: 0,
        },
        label: "test".into(),
        phase: None,
        prompt: "test".into(),
        schema: None,
        tier: None,
        model: None,
        provider: None,
        effort: None,
        profile: None,
        toolsets: None,
        skills: None,
        isolation: None,
        max_turns: None,
        workflow_path: vec![],
    }
}
