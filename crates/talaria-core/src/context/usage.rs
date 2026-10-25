//! The usage report: tokens, calls, cost and plan (#234).

use hermes_contract::schemas::{SessionStats, UsageModel};

use super::text::ReadError;
use crate::session::Usage;

/// Where the report's figures came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageSource {
    Gateway,
    LastReading,
    Nothing,
}

/// A context figure: used and maximum tokens with its percentage.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextFigure {
    pub used: u64,
    pub max: u64,
    pub percent: u8,
    pub estimated: bool,
}

/// One rate-limit bucket row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateLimitRow {
    pub name: &'static str,
    pub remaining: u64,
    pub limit: u64,
}

/// What the session cost.
#[derive(Clone, Debug, PartialEq)]
pub enum CostLine {
    Amount { usd: f64, status: Option<String> },
    Included,
    NotReported,
}

/// One subscription bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub kind: String,
    pub remaining: String,
    pub total: String,
    pub spent: String,
    pub pct_used: Option<u8>,
    pub fill: f64,
}

/// The subscription plan, when Hermes reports one.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub name: Option<String>,
    pub status: Option<String>,
    pub spendable: Option<String>,
    pub renews: Option<String>,
    pub plan_bar: Option<Bar>,
    pub topup_bar: Option<Bar>,
}

/// Everything `/usage` shows.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct UsageReport {
    pub source: UsageSource,
    pub stats_error: Option<ReadError>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub fallback_model: Option<String>,
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub reasoning: Option<u64>,
    pub total: Option<u64>,
    pub calls: u64,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
    pub cache_hit_pct: Option<u8>,
    pub context: Option<ContextFigure>,
    pub compressions: Option<u64>,
    pub avg_latency_s: Option<f64>,
    pub tokens_per_s: Option<f64>,
    pub active_subagents: Option<u64>,
    pub rate_limits: Vec<RateLimitRow>,
    pub cost: CostLine,
    pub plan: Option<Plan>,
}

impl UsageReport {
    /// Build the report from live stats (or their error), the stored reading,
    /// the plan reply and whether the tracker hides the context figure.
    pub fn build(
        _stats: Result<&SessionStats, &ReadError>,
        _stored: &Usage,
        _plan: Option<&UsageModel>,
        _context_stale: bool,
    ) -> UsageReport {
        unimplemented!("#234")
    }
}
