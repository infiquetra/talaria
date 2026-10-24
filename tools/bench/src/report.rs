//! The table and the JSON report.

use crate::budgets::{Row, Status};

#[derive(Debug, Default)]
pub struct Report {
    pub rows: Vec<Row>,
    /// Every child process the run spawned.
    pub child_pids: Vec<u32>,
    pub binary_version: String,
    pub attempts_used: u32,
    pub counts: crate::frames::SplitCounts,
    pub unanswered_queries: Vec<String>,
    pub late_queries: Vec<String>,
    pub write_lag_ms: f64,
}

impl Report {
    pub fn render(&self) -> String {
        let mut out = format!(
            "os: {} arch: {} cpus: {}\nbench wiring: streaming tail and composer only (no status line, no prompt cards)\nbudget | measured | limit | result\n",
            std::env::consts::OS,
            std::env::consts::ARCH,
            cpus()
        );
        for row in &self.rows {
            out.push_str(&format!(
                "{} | {} | {} | {}\n",
                row.name,
                row.measured,
                row.limit,
                status(row.status).to_uppercase()
            ));
            for attempt in &row.attempts {
                out.push_str(&format!("  {attempt}\n"));
            }
        }
        out.push_str(&format!("counts: unanswered queries {}, bytes outside frames {}, nested markers {}, unterminated frames {}, largest write lag {:.3} ms\n",
            self.unanswered_queries.len(), self.counts.bytes_outside_frames, self.counts.nested_markers, self.counts.unterminated_frames, self.write_lag_ms));
        out
    }

    pub fn to_json(&self) -> String {
        serde_json::json!({
            "format": 1,
            "machine": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH, "cpus": cpus()},
            "binary_version": if self.binary_version.is_empty() { "unknown" } else { &self.binary_version },
            "attempts_used": self.attempts_used,
            "results": self.rows.iter().map(|row| serde_json::json!({
                "name": row.name, "measured": if row.measured.contains(" / ") {serde_json::Value::String(row.measured.clone())} else {quantity(&row.measured)}, "unit": unit(row),
                "limit": quantity(&row.limit), "status": status(row.status), "attempts": row.attempts,
            })).collect::<Vec<_>>(),
            "checks": {
                "query_after_startup": self.rows.iter().any(|r| r.name.contains("no queries after start-up") && r.status == Status::Fail),
                "frame_count_mismatch": self.rows.iter().any(|r| r.name.contains("frame count matches") && r.status == Status::Fail),
                "unanswered_queries": self.unanswered_queries,
                "unterminated_frames": self.counts.unterminated_frames,
                "nested_markers": self.counts.nested_markers,
                "bytes_outside_frames": self.counts.bytes_outside_frames,
                "write_lag_ms": self.write_lag_ms,
                "late_queries": self.late_queries,
            }
        }).to_string()
    }
}

fn cpus() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
}

fn quantity(text: &str) -> serde_json::Value {
    text.split_whitespace()
        .next()
        .and_then(|word| word.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .map_or_else(
            || serde_json::Value::String(text.into()),
            serde_json::Value::from,
        )
}
fn unit(row: &Row) -> &str {
    if row.name.contains("late over early") {
        "ratio"
    } else if row.name.contains("idle frames") {
        "frames and bytes"
    } else {
        row.limit
            .split_whitespace()
            .last()
            .filter(|s| matches!(*s, "ms" | "%" | "MB" | "B"))
            .unwrap_or("")
    }
}
pub(crate) fn status(status: Status) -> &'static str {
    match status {
        Status::Pass => "pass",
        Status::Fail => "fail",
        Status::Info => "info",
        Status::Skipped => "skipped",
    }
}
