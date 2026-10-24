//! A test double for the real binary that writes raw escape sequences and misbehaves
//! on request, so each budget and check can be shown to fail.

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Options {
    args: Vec<String>,
}
impl Options {
    fn value(&self, key: &str) -> Option<&str> {
        self.args
            .iter()
            .position(|a| a == key)
            .and_then(|i| self.args.get(i + 1))
            .map(String::as_str)
    }
    fn number(&self, key: &str) -> u64 {
        self.value(key).and_then(|s| s.parse().ok()).unwrap_or(0)
    }
    fn has(&self, key: &str) -> bool {
        self.args.iter().any(|a| a == key)
    }
}
struct Output {
    out: std::io::Stdout,
    frames: u64,
    bytes: u64,
    options: Options,
}
impl Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.out.write_all(bytes)?;
        self.out.flush()?;
        self.bytes += bytes.len() as u64;
        Ok(())
    }
    fn draw(&mut self, text: &str) -> std::io::Result<()> {
        let mut bytes = Vec::new();
        if !self.options.has("--no-markers") {
            bytes.extend_from_slice(b"\x1b[?2026h");
        }
        bytes.extend_from_slice(text.as_bytes());
        let padding = self.options.number("--frame-bytes").max(
            self.frames
                .saturating_mul(self.options.number("--grow-bytes")),
        );
        let needed = (padding as usize).saturating_sub(bytes.len() + 8);
        bytes.resize(bytes.len() + needed, b' ');
        if !self.options.has("--no-markers") {
            bytes.extend_from_slice(b"\x1b[?2026l");
        }
        self.write(&bytes)?;
        self.frames += 1;
        Ok(())
    }
}
fn report(
    output: &Arc<Mutex<Output>>,
    options: &Options,
    done: &AtomicBool,
    frames: u64,
    cells: u64,
) {
    if options.has("--no-report") || done.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(path) = options.value("--bench-report") else {
        return;
    };
    if let Ok(out) = output.lock() {
        let value = json!({"format":1,"replay_frames":frames,"frames_skipped":0,"cells":cells,
            "loop_passes":out.frames,"frames_drawn":out.frames + out.frames * options.number("--frames-drawn-skew-pct") / 100,
            "bytes_written":out.bytes,"worst_pass_us":options.number("--slow-pass-ms").saturating_mul(1000).max(100)});
        let path = std::path::Path::new(path);
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        if let Ok(mut temp) = tempfile::NamedTempFile::new_in(parent)
            && writeln!(temp, "{value}").is_ok()
        {
            let _ = temp.persist(path);
        }
    }
}
fn ballast(options: &Options) -> Vec<u8> {
    let size = options.number("--ballast-mib").saturating_mul(1024 * 1024) as usize;
    let mut bytes = vec![0; size];
    for byte in bytes.iter_mut().step_by(4096) {
        *byte = 1;
    }
    std::hint::black_box(&bytes);
    bytes
}
fn main() {
    let options = Options {
        args: std::env::args().skip(1).collect(),
    };
    if options.has("--version") {
        println!("fake-tui {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let _ = std::process::Command::new("stty")
        .args(["raw", "-echo"])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if options.has("--spin") {
        std::thread::spawn(|| {
            loop {
                std::hint::black_box(17u64.wrapping_mul(23));
            }
        });
    }
    let mut held = if options.value("--ballast-when") != Some("replay") {
        ballast(&options)
    } else {
        Vec::new()
    };
    let output = Arc::new(Mutex::new(Output {
        out: std::io::stdout(),
        frames: 0,
        bytes: 0,
        options: options.clone(),
    }));
    let (input, keys) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut byte = [0];
        while stdin.read_exact(&mut byte).is_ok() {
            if input.send(byte[0]).is_err() {
                break;
            }
        }
    });
    if let Ok(mut out) = output.lock() {
        let _ = out.write(b"\x1b[6n\x1b[?u\x1b[?2026$p\x1b[c");
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        match keys.recv_timeout(Duration::from_millis(10)) {
            Ok(b'c') => break,
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(_) => {}
        }
    }
    if options.has("--exit-before-first-frame") {
        std::process::exit(3);
    }
    std::thread::sleep(Duration::from_millis(options.number("--startup-delay-ms")));
    if let Ok(mut out) = output.lock() {
        let _ = out.draw("\x1b[48;1H> ");
    }
    let start = Instant::now();
    if options.has("--late-query") {
        std::thread::sleep(Duration::from_millis(20));
        if let Ok(mut out) = output.lock() {
            let _ = out.write(b"\x1b[6n");
        }
    }
    if options.has("--exit-after-ms") {
        let delay = options.number("--exit-after-ms");
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(delay));
            std::process::exit(4);
        });
    }
    let finished = Arc::new(AtomicBool::new(false));
    if options.has("--early-report-ms") {
        let output = output.clone();
        let opts = options.clone();
        let done = finished.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(opts.number("--early-report-ms")));
            report(&output, &opts, &done, 0, 0);
        });
    }
    {
        let output = output.clone();
        let opts = options.clone();
        std::thread::spawn(move || {
            let mut text = String::new();
            let mut count = 0;
            while let Ok(byte) = keys.recv() {
                if byte == 0x1b {
                    continue;
                }
                count += 1;
                if count == opts.number("--drop-key") {
                    continue;
                }
                std::thread::sleep(Duration::from_millis(opts.number("--echo-delay-ms")));
                if (0x20..=0x7e).contains(&byte) {
                    text.push(byte as char);
                }
                if let Ok(mut out) = output.lock() {
                    let _ = out.draw(&format!("\x1b[48;1H\x1b[2K> {text}"));
                }
            }
        });
    }
    let records: Vec<Value> = options
        .value("--bench-replay")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| {
            s.lines()
                .skip(1)
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect()
        })
        .unwrap_or_default();
    let speed = options.value("--bench-speed").unwrap_or("recorded");
    let factor = if speed == "instant" {
        f64::INFINITY
    } else if speed == "recorded" {
        1.0
    } else {
        speed.parse::<f64>().unwrap_or(1.0)
    };
    let mut frames = 0;
    let mut cells = 0;
    let mut i = 0;
    let mut row = 1;
    while i < records.len() {
        let at = records[i]["tMs"].as_u64().unwrap_or(0);
        let due = start + Duration::from_secs_f64(at as f64 / factor / 1000.0);
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
        if held.is_empty() && options.value("--ballast-when") == Some("replay") {
            held = ballast(&options);
        }
        let mut text = String::new();
        while i < records.len() && records[i]["tMs"].as_u64().unwrap_or(0) == at {
            let record = &records[i];
            if record["dir"] == "in" {
                frames += 1;
                let params = &record["frame"]["params"];
                if params["type"] == "message.delta" {
                    text.push_str(params["payload"]["text"].as_str().unwrap_or(""));
                }
                if params["type"] == "message.complete" || params["type"] == "tool.start" {
                    cells += 1;
                }
            }
            i += 1;
        }
        if !text.is_empty() {
            row = row % 46 + 1;
            if let Ok(mut out) = output.lock() {
                let _ = out.draw(&format!("\x1b[{row};1H\x1b[2K{text}"));
            }
        }
    }
    report(&output, &options, &finished, frames, cells);
    std::hint::black_box(&held);
    let idle = options.number("--idle-repaint-ms");
    loop {
        std::thread::sleep(Duration::from_millis(if idle == 0 { 1000 } else { idle }));
        if idle > 0
            && let Ok(mut out) = output.lock()
        {
            let _ = out.draw("\x1b[1;1Hidle");
        }
        std::hint::black_box(&held);
    }
}
