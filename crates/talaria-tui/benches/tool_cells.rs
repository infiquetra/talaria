//! Render budgets for tool cells. Issue 221.
//!
//! A case passes when the median of 101 renders is within its budget.
//! The diff text is stored on the cell. This program does not parse it.

use std::time::Instant;

use serde_json::json;
use talaria_core::session::{CellId, ToolCell, ToolStatus};
use talaria_tui::tool_cell::{Detail, ToolRenderOptions, ToolTheme, tool_lines};

const COMPACT_BUDGET_US: u128 = 1000;
const FULL_BUDGET_US: u128 = 5000;

struct Case {
    name: &'static str,
    cell: ToolCell,
    detail: Detail,
    budget: u128,
}

fn main() {
    let cases = cases();
    let mut rows = Vec::new();
    let mut failed = false;
    for case in &cases {
        let (median, slowest) = measure(&case.cell, case.detail);
        let pass = median <= case.budget;
        if !pass {
            failed = true;
        }
        let status = if pass { "pass" } else { "fail" };
        rows.push(format!(
            "{} median_us={median} slowest_us={slowest} budget_us={} {status}",
            case.name, case.budget
        ));
    }
    print_machine();
    println!("case median_us slowest_us budget_us status");
    for row in &rows {
        println!("{row}");
    }
    if failed {
        std::process::exit(1);
    }
}

fn measure(cell: &ToolCell, detail: Detail) -> (u128, u128) {
    let options = options(detail);
    let _warm = tool_lines(cell, &options);
    let mut samples = Vec::with_capacity(101);
    for _ in 0..101 {
        let started = Instant::now();
        let _lines = tool_lines(cell, &options);
        samples.push(started.elapsed().as_micros());
    }
    samples.sort_unstable();
    (samples[50], samples[100])
}

fn options(detail: Detail) -> ToolRenderOptions {
    ToolRenderOptions {
        width: 100,
        theme: ToolTheme::default(),
        color: true,
        detail,
        expand_hint: None,
        elapsed: None,
    }
}

fn cases() -> Vec<Case> {
    let output = long_output();
    let cut = cut_result(&output);
    let diff = wide_diff();
    vec![
        Case {
            name: "compact-shell",
            cell: shell(&output),
            detail: Detail::Compact,
            budget: COMPACT_BUDGET_US,
        },
        Case {
            name: "compact-shell-cut",
            cell: shell(&cut),
            detail: Detail::Compact,
            budget: COMPACT_BUDGET_US,
        },
        Case {
            name: "compact-diff",
            cell: edit(&diff),
            detail: Detail::Compact,
            budget: COMPACT_BUDGET_US,
        },
        Case {
            name: "full-diff",
            cell: edit(&diff),
            detail: Detail::Full,
            budget: FULL_BUDGET_US,
        },
    ]
}

/// synthetic: 2,000 lines, just under 65,536 characters, so the text is not cut
fn long_output() -> String {
    let line = "x".repeat(31);
    let mut output = String::new();
    for _ in 0..2000 {
        output.push_str(&line);
        output.push('\n');
    }
    assert!(output.len() < 65_536, "the shell output is under the cut");
    assert_eq!(output.lines().count(), 2000);
    output
}

/// synthetic: the same output, kept to 32,768 characters and marked truncated
fn cut_result(output: &str) -> String {
    let kept: String = output.chars().take(32_768).collect();
    json!({"output": kept, "truncated": true}).to_string()
}

/// synthetic: 6 files and 80 diff rows
fn wide_diff() -> String {
    let mut text = String::new();
    let mut remaining = 80usize;
    for file in 0..6 {
        let rows = if file == 5 { remaining } else { 13 };
        remaining -= rows;
        text.push_str(&format!("--- a/f{file}.txt\n+++ b/f{file}.txt\n"));
        text.push_str(&format!("@@ -1,{rows} +1,{rows} @@\n"));
        for number in 0..rows {
            if number % 3 == 1 {
                text.push_str(&format!("-old {file} {number}\n"));
            } else if number % 3 == 2 {
                text.push_str(&format!("+new {file} {number}\n"));
            } else {
                text.push_str(&format!(" same {file} {number}\n"));
            }
        }
    }
    text
}

fn shell(result: &str) -> ToolCell {
    let mut cell = blank("terminal");
    cell.status = ToolStatus::Succeeded;
    cell.result = Some(result.to_owned());
    cell.args = Some(json!({"command": "seq 1 2000"}));
    cell.duration_s = Some(0.1);
    cell
}

fn edit(diff: &str) -> ToolCell {
    let mut cell = blank("patch");
    cell.status = ToolStatus::Succeeded;
    cell.context = Some("notes.txt".to_owned());
    cell.diff = Some(diff.to_owned());
    cell.duration_s = Some(0.1);
    cell
}

fn blank(name: &str) -> ToolCell {
    ToolCell {
        id: CellId::new(1),
        tool_id: "call-1".to_owned(),
        name: name.to_owned(),
        context: None,
        args: None,
        labels: Vec::new(),
        status: ToolStatus::Succeeded,
        progress: None,
        summary: None,
        result: None,
        diff: None,
        duration_s: None,
    }
}

fn print_machine() {
    let optimised = if cfg!(debug_assertions) { "no" } else { "yes" };
    println!(
        "os={} arch={} cpu={} optimised={optimised}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        cpu_model()
    );
}

fn cpu_model() -> String {
    let output = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output();
    let Ok(output) = output else {
        return "unknown".to_owned();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    if text.is_empty() {
        "unknown".to_owned()
    } else {
        text.to_owned()
    }
}
