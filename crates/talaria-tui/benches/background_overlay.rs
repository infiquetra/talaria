//! Overlay draw time for one chatty background process.
//!
//! Prints `background_overlay render_p95_us=<n>`. Exits non-zero when the 95th percentile is
//! over 4 milliseconds. The timed runs are not retried.

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_core::background::{
    BackgroundEvent, BackgroundWork, OUTPUT_CAP_CHARS, ProcessId, ProcessOutput, update,
};
use talaria_tui::background::{BackgroundTheme, ProcessesView};

fn main() {
    let work = full_work();
    let view = ProcessesView::new();
    let theme = BackgroundTheme::default();
    let area = Rect::new(0, 0, 160, 48);
    for _ in 0..20 {
        let mut buf = Buffer::empty(area);
        view.render(&work, 0, 0.0, area, &mut buf, &theme);
    }
    let mut times = Vec::with_capacity(200);
    for _ in 0..200 {
        let mut buf = Buffer::empty(area);
        let started = Instant::now();
        view.render(&work, 0, 0.0, area, &mut buf, &theme);
        times.push(started.elapsed().as_micros());
    }
    times.sort_unstable();
    let render_p95_us = times[189];
    println!("background_overlay render_p95_us={render_p95_us}");
    if render_p95_us > 4_000 {
        std::process::exit(1);
    }
}

fn full_work() -> BackgroundWork {
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
    let mut output = ProcessOutput::default();
    let mut raw = String::new();
    while raw.chars().count() < OUTPUT_CAP_CHARS {
        raw.push_str("tick\r\u{1b}[31mX\u{1b}[0m\n");
    }
    output.push_chunk(&raw, 0);
    let _ = output;
    work
}
