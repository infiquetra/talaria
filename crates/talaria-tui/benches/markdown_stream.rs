//! Times one push of a streaming Markdown document.
//!
//! A measured run fails when its slowest push is slower than the budget.

#[path = "support/doc.rs"]
mod doc;

use std::time::{Duration, Instant};

use talaria_tui::markdown::highlight::is_loaded;
use talaria_tui::markdown::{MarkdownOptions, MarkdownStream, warm_up};

const MARKDOWN_FRAME_BUDGET_MS: f64 = 16.0;

fn main() {
    print_machine();
    let mut documents = Vec::new();
    for name in doc::document_names() {
        let Some(text) = doc::document(name) else {
            eprintln!("missing document {name}");
            std::process::exit(1);
        };
        if text.len() != doc::DOCUMENT_BYTES {
            eprintln!("{name} is {} bytes", text.len());
            std::process::exit(1);
        }
        documents.push((name, text));
    }
    if let Err(message) = wait_ready() {
        eprintln!("{message}");
        std::process::exit(1);
    }
    println!(
        "scenario bytes deltas committed_lines p50_ms p95_ms p99_ms max_ms median_ms maxima status"
    );
    let mut failed = false;
    for (name, text) in &documents {
        if !run_scenario(name, text) {
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}

fn run_scenario(name: &str, text: &str) -> bool {
    let deltas = doc::bench_deltas(text);
    let _cold = measure(&deltas);
    let mut maxima = Vec::new();
    let mut frames = Vec::new();
    let mut committed_lines = 0usize;
    for _ in 0..3 {
        let run = measure(&deltas);
        maxima.push(run.max_ms);
        frames.extend(run.frames_ms);
        committed_lines = run.committed_lines;
    }
    let mut ordered = maxima.clone();
    ordered.sort_by(|left, right| left.total_cmp(right));
    let median = ordered[1];
    let slowest = ordered[2];
    let pass = maxima
        .iter()
        .all(|sample| *sample <= MARKDOWN_FRAME_BUDGET_MS);
    let status = if pass { "pass" } else { "fail" };
    println!(
        "{name} bytes={} deltas={} committed_lines={committed_lines} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} max_ms={slowest:.3} median_ms={median:.3} maxima={:.3},{:.3},{:.3} {status}",
        text.len(),
        deltas.len(),
        percentile(&frames, 50),
        percentile(&frames, 95),
        percentile(&frames, 99),
        maxima[0],
        maxima[1],
        maxima[2],
    );
    pass
}

struct Run {
    max_ms: f64,
    frames_ms: Vec<f64>,
    committed_lines: usize,
}

fn measure(deltas: &[String]) -> Run {
    let mut markdown = MarkdownStream::new(MarkdownOptions::default());
    let mut frames_ms = Vec::with_capacity(deltas.len());
    let mut committed_lines = 0usize;
    for delta in deltas {
        let started = Instant::now();
        let update = markdown.push(delta);
        frames_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        committed_lines += update.committed.len();
    }
    let done = markdown.finish();
    committed_lines += done.committed.len();
    let max_ms = frames_ms.iter().copied().fold(0.0, f64::max);
    Run {
        max_ms,
        frames_ms,
        committed_lines,
    }
}

fn percentile(samples: &[f64], pct: usize) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut ordered = samples.to_vec();
    ordered.sort_by(|left, right| left.total_cmp(right));
    let last = ordered.len() - 1;
    let rank = last.saturating_mul(pct) / 100;
    ordered[rank.min(last)]
}

fn wait_ready() -> Result<(), String> {
    warm_up();
    let started = Instant::now();
    while !is_loaded() {
        if started.elapsed() >= Duration::from_secs(10) {
            return Err("highlighter was not ready within 10 seconds".to_string());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn print_machine() {
    let release = if cfg!(debug_assertions) { "no" } else { "yes" };
    println!(
        "os={} arch={} release={release} budget_ms={MARKDOWN_FRAME_BUDGET_MS}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("cpu={}", cpu_model());
}

fn cpu_model() -> String {
    if let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("model name") {
                return rest.trim_start_matches([' ', ':']).to_string();
            }
        }
    }
    if let Ok(output) = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        && output.status.success()
    {
        let model = String::from_utf8_lossy(&output.stdout);
        let model = model.trim();
        if !model.is_empty() {
            return model.to_string();
        }
    }
    "unknown".to_string()
}
