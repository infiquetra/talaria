//! Red-run tests 1 to 6 (#234): parsing the context breakdown.

use hermes_contract::schemas::SessionContextBreakdownResult;
use serde_json::Value;
use talaria_core::context::breakdown::{Breakdown, ContextSource, FileStatus, GridCell, glyph_for};

fn breakdown_value() -> Value {
    serde_json::from_str(include_str!("data/context/breakdown.json")).expect("test data parses")
}

fn breakdown_result(value: &Value) -> SessionContextBreakdownResult {
    serde_json::from_value(value.clone()).expect("test data fits the result type")
}

#[test]
fn test_01_breakdown_parses_with_categories_files_and_source() {
    let value = breakdown_value();
    let result = breakdown_result(&value);
    let parsed = Breakdown::from_result(&result);

    let ids: Vec<&str> = parsed
        .categories
        .iter()
        .map(|category| category.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "system_prompt",
            "tool_definitions",
            "rules",
            "skills",
            "memory",
            "conversation"
        ]
    );
    assert_eq!(parsed.used, 34_120);
    assert_eq!(parsed.max, 200_000);
    assert_eq!(parsed.percent, 17);
    assert_eq!(parsed.source, ContextSource::ProviderUsage);
    assert_eq!(parsed.files.len(), 2);
    assert_eq!(parsed.files[0].status, FileStatus::Loaded);
    assert_eq!(parsed.files[1].status, FileStatus::Shadowed);
}

#[test]
fn test_02_prebuild_has_no_grid_and_no_rows() {
    let value: Value =
        serde_json::from_str(include_str!("data/context/prebuild.json")).expect("test data parses");
    let result = breakdown_result(&value);
    let parsed = Breakdown::from_result(&result);

    assert!(parsed.is_prebuild());
    assert!(parsed.grid().is_empty());
    assert!(parsed.rows().is_empty());
}

#[test]
fn test_03_negatives_clamp_and_percent_caps_at_100() {
    let mut value = breakdown_value();
    value["context_percent"] = Value::from(140);
    value["context_used"] = Value::from(250_000);
    value["categories"][0]["tokens"] = Value::from(-5);
    let result = breakdown_result(&value);
    let parsed = Breakdown::from_result(&result);

    assert_eq!(parsed.percent, 100);
    assert_eq!(parsed.used, 200_000);
    assert_eq!(parsed.categories[0].tokens, 0);
}

#[test]
fn test_04_unknown_id_gets_other_glyph_and_keeps_label() {
    assert_eq!(glyph_for("plugins"), '▪');

    let mut value = breakdown_value();
    value["categories"] = serde_json::json!([
        {"color": "x", "id": "plugins", "label": "Plug-ins", "tokens": 500},
    ]);
    let result = breakdown_result(&value);
    let parsed = Breakdown::from_result(&result);

    assert_eq!(parsed.categories.len(), 1);
    assert_eq!(parsed.categories[0].label, "Plug-ins");
    let rows = parsed.rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].glyph, '▪');
}

#[test]
fn test_05_grid_cells_follow_rounded_shares() {
    let value = breakdown_value();
    let parsed = Breakdown::from_result(&breakdown_result(&value));
    let grid = parsed.grid();

    assert_eq!(grid.len(), 100);
    let runs: [(usize, GridCell); 7] = [
        (2, GridCell::Category(0)),
        (6, GridCell::Category(1)),
        (1, GridCell::Category(2)),
        (1, GridCell::Category(3)),
        (1, GridCell::Category(4)),
        (8, GridCell::Category(5)),
        (81, GridCell::Free),
    ];
    let mut at = 0;
    for (count, cell) in runs {
        assert!(
            grid[at..at + count].iter().all(|seen| *seen == cell),
            "cells {at}..{} are not all {cell:?}",
            at + count
        );
        at += count;
    }
}

#[test]
fn test_06_row_tenths_over_max_or_estimate() {
    let value = breakdown_value();
    let parsed = Breakdown::from_result(&breakdown_result(&value));
    let rows = parsed.rows();

    let tenths: Vec<u32> = rows.iter().map(|row| row.tenths_of_percent).collect();
    assert_eq!(tenths, [16, 59, 6, 12, 2, 76, 830]);
    assert_eq!(rows[6].label, "Free space");
    assert_eq!(rows[6].tokens, 165_910);

    let mut old = breakdown_value();
    old["context_max"] = Value::from(0);
    old["estimated_total"] = Value::from(34_090);
    let past = Breakdown::from_result(&breakdown_result(&old));
    let past_rows = past.rows();
    assert_eq!(past_rows.len(), 6);
    assert_eq!(past_rows[0].tenths_of_percent, 94);
}
