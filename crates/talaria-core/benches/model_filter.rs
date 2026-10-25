//! Filter budget for the model list. Issue 230, scenario "model filter".

use std::hint::black_box;
use std::time::Instant;

use talaria_core::models::{FilterRow, filter_row_indexes};

const ROWS: usize = 2_000;
const PROVIDERS: usize = 40;
const RUNS: usize = 200;
const LIMIT_US: u128 = 2_000;

fn rows() -> Vec<FilterRow> {
    (0..ROWS)
        .map(|index| {
            let provider = index % PROVIDERS;
            FilterRow {
                provider_name: format!("Provider {provider}"),
                slug: format!("provider-{provider}"),
                model: format!("model-{index}"),
            }
        })
        .collect()
}

fn p95_us(rows: &[FilterRow], filter: &str) -> u128 {
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let started = Instant::now();
        let matched = filter_row_indexes(rows, filter);
        samples.push(started.elapsed().as_micros());
        black_box(matched);
    }
    samples.sort_unstable();
    let index = (RUNS as f64 * 0.95).ceil() as usize - 1;
    samples[index]
}

fn main() {
    let rows = rows();
    let mut failed = false;
    for filter in ["model-1", "provider 7 model-1"] {
        let p95 = p95_us(&rows, filter);
        println!("p95_us={p95}");
        if p95 > LIMIT_US {
            failed = true;
        }
    }
    if failed {
        std::process::exit(1);
    }
}
