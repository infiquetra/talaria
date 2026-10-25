//! Red-run tests 8 to 14 (#234): the usage report.

use hermes_contract::schemas::{SessionStats, UsageModel};
use serde_json::Value;
use talaria_core::context::text::ReadError;
use talaria_core::context::usage::{CostLine, UsageReport, UsageSource};
use talaria_core::session::Usage;

fn stats_value() -> Value {
    serde_json::from_str(include_str!("data/context/stats.json")).expect("test data parses")
}

fn stats(value: &Value) -> SessionStats {
    serde_json::from_value(value.clone()).expect("test data fits the stats type")
}

fn stored() -> Usage {
    Usage::default()
}

#[test]
fn test_08_gateway_report_reads_stats() {
    let value = stats_value();
    let live = stats(&value);
    let report = UsageReport::build(Ok(&live), &stored(), None, false);

    assert_eq!(report.source, UsageSource::Gateway);
    assert_eq!(report.input, Some(30_210));
    assert_eq!(report.cache_hit_pct, Some(60));
    let context = report.context.expect("context is known");
    assert_eq!(context.used, 15_400);
    assert_eq!(context.max, 200_000);
    assert_eq!(context.percent, 8);
    assert!(!context.estimated);
    assert_eq!(report.rate_limits.len(), 2);
    assert_eq!(report.rate_limits[0].name, "requests per minute");
    assert_eq!(report.rate_limits[0].remaining, 58);
    assert_eq!(report.rate_limits[0].limit, 60);
    assert_eq!(report.rate_limits[1].name, "tokens per minute");
    assert_eq!(report.rate_limits[1].remaining, 84_590);
    assert_eq!(report.rate_limits[1].limit, 100_000);
}

#[test]
fn test_09_empty_stats_fall_back_to_stored_reading() {
    let mut value = stats_value();
    value["api_calls"] = Value::from(0);
    value["compression_count"] = Value::from(1);
    let live = stats(&value);
    let kept = Usage {
        calls: Some(3),
        compressions: Some(0),
        input: Some(4_100),
        ..Usage::default()
    };
    let report = UsageReport::build(Ok(&live), &kept, None, false);

    assert_eq!(report.source, UsageSource::LastReading);
    assert_eq!(report.calls, 3);
    assert_eq!(report.compressions, Some(1));
    assert_eq!(report.input, Some(4_100));
    assert_eq!(report.output, None);
}

#[test]
fn test_10_failed_stats_keep_the_error_on_any_source() {
    let value = stats_value();
    let live = stats(&value);
    let error = ReadError::TimedOut { secs: 15 };
    let _ = &live;

    let none = UsageReport::build(Err(&error), &stored(), None, false);
    assert_eq!(none.source, UsageSource::Nothing);
    assert_eq!(none.stats_error, Some(ReadError::TimedOut { secs: 15 }));

    let kept = Usage {
        calls: Some(2),
        ..Usage::default()
    };
    let last = UsageReport::build(Err(&error), &kept, None, false);
    assert_eq!(last.source, UsageSource::LastReading);
    assert_eq!(last.stats_error, Some(ReadError::TimedOut { secs: 15 }));
    assert_eq!(error.short(), "no answer within 15 s");
}

#[test]
fn test_11_zero_prompt_and_null_length_hide_cache_and_context() {
    let mut value = stats_value();
    value["prompt_tokens"] = Value::from(0);
    value["cache_read_tokens"] = Value::from(50);
    value["context_length"] = Value::Null;
    let live = stats(&value);
    let report = UsageReport::build(Ok(&live), &stored(), None, false);

    assert_eq!(report.cache_hit_pct, None);
    assert_eq!(report.context, None);
}

#[test]
fn test_12_broken_rate_limits_give_no_rows() {
    let base = stats_value();
    let mut null = base.clone();
    null["rate_limit"] = Value::Null;
    let mut zero = base.clone();
    zero["rate_limit"]["captured_at"] = Value::from(0.0);
    let mut wrong_type = base.clone();
    wrong_type["rate_limit"]["requests_min"]["limit"] = Value::from("60");

    for value in [&null, &zero, &wrong_type] {
        let live = stats(value);
        let report = UsageReport::build(Ok(&live), &stored(), None, false);
        assert!(report.rate_limits.is_empty());
    }
}

#[test]
fn test_13_cost_follows_amount_and_status() {
    let value = stats_value();
    let live = stats(&value);
    let cases: [(Option<f64>, Option<&str>, CostLine); 8] = [
        (
            Some(0.421_3),
            Some("estimated"),
            CostLine::Amount {
                usd: 0.421_3,
                status: Some("estimated".to_owned()),
            },
        ),
        (
            Some(0.004),
            Some("actual"),
            CostLine::Amount {
                usd: 0.004,
                status: Some("actual".to_owned()),
            },
        ),
        (None, Some("included"), CostLine::Included),
        (Some(1.0), Some("unknown"), CostLine::NotReported),
        (Some(f64::NAN), None, CostLine::NotReported),
        (Some(-1.0), Some("actual"), CostLine::NotReported),
        (
            Some(2.5),
            None,
            CostLine::Amount {
                usd: 2.5,
                status: None,
            },
        ),
        (
            Some(0.0),
            Some("actual"),
            CostLine::Amount {
                usd: 0.0,
                status: Some("actual".to_owned()),
            },
        ),
    ];
    for (usd, status, expected) in cases {
        let kept = Usage {
            cost_usd: usd,
            cost_status: status.map(str::to_owned),
            ..Usage::default()
        };
        let report = UsageReport::build(Ok(&live), &kept, None, false);
        assert_eq!(report.cost, expected, "cost for ({usd:?}, {status:?})");
    }
}

#[test]
fn test_14_plan_needs_an_available_reply_and_clamps_fill() {
    let value = stats_value();
    let live = stats(&value);

    let unavailable: UsageModel =
        serde_json::from_str(include_str!("data/context/bars_unavailable.json"))
            .expect("test data parses");
    let without = UsageReport::build(Ok(&live), &stored(), Some(&unavailable), false);
    assert_eq!(without.plan, None);

    let healthy: UsageModel = serde_json::from_str(include_str!("data/context/bars_healthy.json"))
        .expect("test data parses");
    let with = UsageReport::build(Ok(&live), &stored(), Some(&healthy), false);
    let plan = with.plan.expect("healthy plan is kept");
    assert_eq!(plan.plan_bar.as_ref().expect("plan bar").fill, 0.7);

    let mut over = serde_json::from_str::<Value>(include_str!("data/context/bars_healthy.json"))
        .expect("test data parses");
    over["topup_bar"] = serde_json::json!({
        "kind": "topup",
        "remaining_display": "$1.00",
        "total_display": "$1.00",
        "spent_display": "$0.00",
        "fill_fraction": 1.7,
    });
    let topped: UsageModel = serde_json::from_value(over).expect("topup fits the plan type");
    let clamped = UsageReport::build(Ok(&live), &stored(), Some(&topped), false);
    let topup = clamped.plan.expect("plan").topup_bar.expect("topup bar");
    assert_eq!(topup.fill, 1.0);
}
