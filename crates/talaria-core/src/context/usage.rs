//! The usage report: tokens, calls, cost and plan (#234).

use hermes_contract::schemas::{SessionStats, UsageModel};
use serde_json::{Map, Value};

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

/// A count that is never negative.
fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

/// A text that is there when it is not empty.
fn text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// A share in whole percent, rounded and clamped.
fn percent(part: u64, whole: u64) -> u8 {
    if whole == 0 {
        return 0;
    }
    rounded_percent(part as f64 / whole as f64 * 100.0)
}

/// A rounded percentage, clamped to a byte.
fn rounded_percent(share: f64) -> u8 {
    if !share.is_finite() {
        return 0;
    }
    share.round().clamp(0.0, 100.0) as u8
}

/// The cost of the session, as the stored reading reports it.
fn cost_line(stored: &Usage) -> CostLine {
    let status = stored.cost_status.as_deref().map(str::trim);
    if status == Some("included") {
        return CostLine::Included;
    }
    let Some(usd) = stored.cost_usd else {
        return CostLine::NotReported;
    };
    if !usd.is_finite() || usd < 0.0 {
        return CostLine::NotReported;
    }
    match status {
        Some("actual") | Some("estimated") => CostLine::Amount {
            usd,
            status: Some(status.unwrap_or_default().to_owned()),
        },
        None => CostLine::Amount { usd, status: None },
        Some(_) => CostLine::NotReported,
    }
}

/// The rate-limit buckets of one reading, in Hermes's order.
fn rate_limits(block: Option<&Map<String, Value>>) -> Vec<RateLimitRow> {
    let Some(block) = block else {
        return Vec::new();
    };
    let Some(captured) = block.get("captured_at").and_then(Value::as_f64) else {
        return Vec::new();
    };
    if captured <= 0.0 {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for (key, name) in [
        ("requests_min", "requests per minute"),
        ("requests_hour", "requests per hour"),
        ("tokens_min", "tokens per minute"),
        ("tokens_hour", "tokens per hour"),
    ] {
        let Some(bucket) = block.get(key) else {
            continue;
        };
        let (Some(limit), Some(remaining)) = (
            bucket.get("limit").and_then(Value::as_i64),
            bucket.get("remaining").and_then(Value::as_i64),
        ) else {
            // A bucket that does not read as a bucket voids the whole reading.
            return Vec::new();
        };
        if limit <= 0 {
            continue;
        }
        let limit = count(limit);
        rows.push(RateLimitRow {
            name,
            remaining: count(remaining).min(limit),
            limit,
        });
    }
    rows
}

/// One subscription bar, with its fill clamped.
fn bar(bar: &hermes_contract::schemas::UsageBar) -> Bar {
    Bar {
        kind: bar.kind.to_string(),
        remaining: bar.remaining_display.clone(),
        total: bar.total_display.clone(),
        spent: bar.spent_display.clone(),
        pct_used: bar
            .pct_used
            .filter(|value| (0..=100).contains(value))
            .and_then(|value| u8::try_from(value).ok()),
        fill: if bar.fill_fraction.is_finite() {
            bar.fill_fraction.clamp(0.0, 1.0)
        } else {
            0.0
        },
    }
}

/// The plan, when the gateway has one.
fn plan(plan: &UsageModel) -> Plan {
    Plan {
        name: plan.plan_name.clone(),
        status: plan.status.clone(),
        spendable: plan.total_spendable_display.clone(),
        renews: plan.renews_display.clone(),
        plan_bar: plan.plan_bar.as_ref().map(bar),
        topup_bar: plan.topup_bar.as_ref().map(bar),
    }
}

impl UsageReport {
    /// Build the report from live stats (or their error), the stored reading,
    /// the plan reply and whether the tracker hides the context figure.
    pub fn build(
        stats: Result<&SessionStats, &ReadError>,
        stored: &Usage,
        plan: Option<&UsageModel>,
        context_stale: bool,
    ) -> UsageReport {
        let stats_error = stats.as_ref().err().map(|error| (*error).clone());
        let live = stats.ok();
        let stored_calls = count(stored.calls.unwrap_or(0));
        let source = match live {
            Some(stats) if stats.api_calls > 0 => UsageSource::Gateway,
            _ if stored_calls > 0 => UsageSource::LastReading,
            _ => UsageSource::Nothing,
        };
        let mut report = UsageReport {
            source,
            stats_error,
            model: None,
            provider: None,
            fallback_model: None,
            input: None,
            output: None,
            reasoning: None,
            total: None,
            calls: 0,
            cache_read: None,
            cache_write: None,
            cache_hit_pct: None,
            context: None,
            compressions: None,
            avg_latency_s: None,
            tokens_per_s: None,
            active_subagents: None,
            rate_limits: Vec::new(),
            cost: cost_line(stored),
            plan: plan.filter(|plan| plan.available).map(super::usage::plan),
        };
        match source {
            UsageSource::Gateway => {
                let stats = live.expect("the gateway source has stats");
                report.model = text(&stats.model);
                report.provider = text(&stats.provider);
                report.fallback_model = stats.fallback_model.as_deref().and_then(text);
                let input = if stats.input_tokens > 0 {
                    stats.input_tokens
                } else {
                    stats.prompt_tokens
                };
                report.input = Some(count(input));
                let output = if stats.output_tokens > 0 {
                    stats.output_tokens
                } else {
                    stats.completion_tokens
                };
                report.output = Some(count(output));
                report.reasoning = Some(count(stats.reasoning_tokens));
                report.total = Some(count(stats.total_tokens));
                report.calls = count(stats.api_calls);
                report.cache_read = Some(count(stats.cache_read_tokens));
                report.cache_write = Some(count(stats.cache_write_tokens));
                if stats.prompt_tokens > 0 && stats.cache_read_tokens > 0 {
                    report.cache_hit_pct = Some(percent(
                        count(stats.cache_read_tokens),
                        count(stats.prompt_tokens),
                    ));
                }
                if let Some(max) = stats.context_length.filter(|max| *max > 0) {
                    let max = count(max);
                    let used = count(stats.context_tokens).min(max);
                    if used > 0 {
                        report.context = Some(ContextFigure {
                            used,
                            max,
                            percent: percent(used, max),
                            estimated: stats
                                .context_source
                                .as_deref()
                                .is_some_and(|source| source != "provider_usage"),
                        });
                    }
                }
                report.compressions = Some(count(stats.compression_count));
                report.avg_latency_s = stats
                    .avg_latency_s
                    .filter(|value| value.is_finite() && *value > 0.0);
                report.tokens_per_s = stats
                    .tokens_per_s
                    .filter(|value| value.is_finite() && *value > 0.0);
                report.active_subagents =
                    (stats.active_subagents > 0).then(|| count(stats.active_subagents));
                report.rate_limits = rate_limits(stats.rate_limit.as_ref());
            }
            UsageSource::LastReading => {
                report.model = stored.model.clone().filter(|model| !model.is_empty());
                report.input = stored.input.map(count);
                report.output = stored.output.map(count);
                report.reasoning = stored.reasoning.map(count);
                report.total = stored.total.map(count);
                report.calls = stored_calls;
                report.cache_read = stored.cache_read.map(count);
                report.cache_write = stored.cache_write.map(count);
                report.cache_hit_pct = stored.cache_hit_pct.map(rounded_percent);
                if let (Some(used), Some(max), Some(share)) = (
                    stored.context_used.map(count),
                    stored.context_max.map(count),
                    stored.context_percent,
                ) && max > 0
                {
                    report.context = Some(ContextFigure {
                        used: used.min(max),
                        max,
                        percent: rounded_percent(share),
                        estimated: false,
                    });
                }
                report.compressions = match (stored.compressions, live) {
                    (Some(stored), Some(stats)) => Some(count(stored.max(stats.compression_count))),
                    (Some(stored), None) => Some(count(stored)),
                    (None, Some(stats)) => Some(count(stats.compression_count)),
                    (None, None) => None,
                };
            }
            UsageSource::Nothing => {}
        }
        if context_stale {
            report.context = None;
        }
        report
    }
}
