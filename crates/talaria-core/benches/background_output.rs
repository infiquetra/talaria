//! Chunk ingest and visible-tail time for one background process.
//!
//! Prints `background_output chunk_median_us=<n> tail_p95_us=<n>`. Exits non-zero when the
//! median chunk ingest is over 20 microseconds or the visible-tail 95th percentile is over
//! 2 milliseconds. The timed runs are not retried.

use std::time::Instant;

use talaria_core::background::{
    BackgroundEvent, BackgroundWork, OUTPUT_CAP_CHARS, ProcessId, ProcessOutput, update,
};

fn main() {
    let chunk_median_us = time_chunks();
    let tail_p95_us = time_tail();
    println!("background_output chunk_median_us={chunk_median_us} tail_p95_us={tail_p95_us}");
    if chunk_median_us > 20 || tail_p95_us > 2_000 {
        std::process::exit(1);
    }
}

fn time_chunks() -> u128 {
    let mut work = running();
    let sample = chunk(0);
    for index in 0..1_000 {
        let _ = update(
            &mut work,
            BackgroundEvent::TerminalOutput {
                process_id: "proc_bench".to_string(),
                chunk: chunk(index),
                now_ms: index,
            },
        );
    }
    let mut times = Vec::with_capacity(10_000);
    for index in 0..10_000 {
        let started = Instant::now();
        let _ = update(
            &mut work,
            BackgroundEvent::TerminalOutput {
                process_id: "proc_bench".to_string(),
                chunk: sample.clone(),
                now_ms: 1_000 + index,
            },
        );
        times.push(started.elapsed().as_micros());
    }
    median(&mut times)
}

fn time_tail() -> u128 {
    let mut output = ProcessOutput::default();
    let mut raw = String::new();
    while raw.chars().count() < OUTPUT_CAP_CHARS {
        raw.push_str("tick\r\u{1b}[31mX\u{1b}[0m\n");
    }
    output.push_chunk(&raw, 0);
    for _ in 0..20 {
        let _ = output.visible_tail(65_536);
    }
    let mut times = Vec::with_capacity(200);
    for _ in 0..200 {
        let started = Instant::now();
        let _ = output.visible_tail(65_536);
        times.push(started.elapsed().as_micros());
    }
    times.sort_unstable();
    times[189]
}

fn running() -> BackgroundWork {
    let mut work = BackgroundWork::default();
    let _ = update(&mut work, BackgroundEvent::Attached { now_ms: 0 });
    let _ = update(
        &mut work,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 0,
        },
    );
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request: talaria_core::side::RequestId(1),
            processes: vec![talaria_core::background::ListedProcess {
                id: ProcessId("proc_bench".to_string()),
                command: "sh tick.sh".to_string(),
                status_raw: "running".to_string(),
                exit_code: None,
                started_epoch_s: Some(0.0),
                exited_at_s: None,
                completion_reason: None,
                output_tail: None,
            }],
            now_ms: 0,
        },
    );
    work
}

fn chunk(index: u64) -> String {
    format!("tick {index:06} pad\n")
}

fn median(times: &mut [u128]) -> u128 {
    times.sort_unstable();
    times[times.len() / 2]
}
