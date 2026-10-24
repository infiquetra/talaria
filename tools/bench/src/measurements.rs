//! The measurements and the run.

use std::path::PathBuf;

use crate::budgets::Budgets;
use crate::budgets::{
    Row, Status, keystroke_schedule, last_t_ms, median, peak_window, percentile_nearest_rank,
    verdict,
};
use crate::error::BenchError;
use crate::report::Report;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

const PROBE: &str = "^kbdprobe-0123456789-abcdefghijklmnopqrs";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Launch,
    Keys,
    Stream(bool),
    MemoryStart,
    MemoryLong,
}

#[derive(Default)]
struct Observation {
    launch_ms: f64,
    error: Option<String>,
    latencies: Vec<f64>,
    cpu: Vec<f64>,
    cpu_mean: Option<f64>,
    idle_cpu: Vec<f64>,
    frame_bytes: Vec<f64>,
    idle_frames: u64,
    idle_bytes: u64,
    rss: u64,
    report: Option<serde_json::Value>,
    counted: u64,
}

fn process_sample(process: &crate::pty::Process) -> Result<crate::Sample, BenchError> {
    process
        .child
        .process_id()
        .ok_or_else(|| BenchError::Io("process id unavailable".into()))
        .and_then(crate::sample)
}

fn observe(
    options: &RunOptions,
    fixture: &Path,
    speed: &str,
    mode: Mode,
    report: &mut Report,
) -> Result<Observation, BenchError> {
    let scratch =
        tempfile::tempdir().map_err(|_| BenchError::Io("measurement directory".into()))?;
    let binary_report = scratch.path().join("child.json");
    let mut args = vec![
        "--bench-replay".into(),
        fixture.to_string_lossy().into_owned(),
        "--bench-report".into(),
        binary_report.to_string_lossy().into_owned(),
    ];
    if speed != "1" {
        args.extend(["--bench-speed".into(), speed.into()]);
    }
    args.extend(options.binary_args.clone());
    let binary = std::fs::canonicalize(&options.binary)
        .map_err(|_| BenchError::BinaryNotFound(options.binary.to_string_lossy().into_owned()))?;
    let factor = if speed == "instant" {
        f64::INFINITY
    } else {
        speed.parse::<f64>().unwrap_or(1.0)
    };
    let expected = Duration::from_secs_f64(last_t_ms(fixture)? as f64 / factor / 1000.0);
    let mut process = crate::pty::Process::spawn(
        &binary,
        &args,
        &[],
        (options.budgets.terminal.cols, options.budgets.terminal.rows),
    )?;
    if let Some(pid) = process.child.process_id() {
        report.child_pids.push(pid);
    }
    let mut emulator =
        crate::Emulator::new(options.budgets.terminal.rows, options.budgets.terminal.cols);
    let mut responder = crate::ResponderState::new();
    let mut splitter = crate::FrameSplitter::new();
    let mut observation = Observation::default();
    let mut started = None;
    let mut report_at = None;
    let schedule = keystroke_schedule(&options.budgets, factor);
    let mut writes: Vec<Instant> = Vec::new();
    let mut echoes = Vec::<Option<f64>>::new();
    let mut samples = Vec::<crate::Sample>::new();
    let mut next_sample = Instant::now();
    let settle = Duration::from_secs(options.budgets.idle.settle_seconds);
    let idle = Duration::from_secs(u64::from(
        options
            .idle_seconds
            .unwrap_or(options.budgets.idle.seconds as u32),
    ));
    let mut cpu_start = None;
    loop {
        let now = Instant::now();
        let pending = process.output.try_recv().ok();
        let queued_before = |cutoff: Instant| pending.as_ref().is_some_and(|(at, _)| *at <= cutoff);
        if binary_report.is_file() && report_at.is_none() {
            if let Ok(bytes) = std::fs::read(&binary_report)
                && let Ok(value) = serde_json::from_slice(&bytes)
            {
                observation.report = Some(value);
                report_at = Some(now);
                if mode != Mode::Launch
                    && let Some(first) = cpu_start
                {
                    match process_sample(&process) {
                        Ok(end) => {
                            observation.cpu_mean = Some(crate::cpu_percent(&first, &end));
                            if let Some(previous) = samples.last() {
                                observation.cpu.push(crate::cpu_percent(previous, &end));
                            }
                        }
                        Err(_) => {
                            observation.error = Some("process sampling failed".into());
                            break;
                        }
                    }
                }
            }
            if mode == Mode::Keys && writes.len() < schedule.len() {
                observation.error = Some("stream ended before the last keystroke".into());
                break;
            }
        }
        if let Ok(Some(status)) = process.child.try_wait()
            && pending.is_none()
        {
            observation.error = Some(if started.is_none() {
                format!(
                    "the binary exited with status {} before its first frame",
                    status.exit_code()
                )
            } else {
                format!("the binary exited with status {}", status.exit_code())
            });
            break;
        }
        if now >= next_sample && mode != Mode::Launch {
            match process_sample(&process) {
                Ok(sample) => {
                    if let Some(prior) = samples.last()
                        && let Some(start) = started
                    {
                        let percent = crate::cpu_percent(prior, &sample);
                        if prior.at >= start && report_at.is_none() {
                            observation.cpu.push(percent);
                        }
                        if report_at.is_some_and(|at| prior.at >= at + settle) {
                            observation.idle_cpu.push(percent);
                        }
                    }
                    observation.rss = observation.rss.max(sample.rss_bytes);
                    samples.push(sample);
                }
                Err(_) => {
                    observation.error = Some("process sampling failed".into());
                    break;
                }
            }
            next_sample = Instant::now() + Duration::from_millis(options.budgets.cpu.sample_ms);
        }
        if let Some(start) = started {
            if mode == Mode::MemoryStart && now >= start + Duration::from_secs(1) {
                match process_sample(&process) {
                    Ok(sample) => observation.rss = sample.rss_bytes,
                    Err(_) => observation.error = Some("process sampling failed".into()),
                }
                break;
            }
            if mode == Mode::Keys {
                while writes.len() < schedule.len() && now >= start + schedule[writes.len()] {
                    if binary_report.exists() {
                        observation.error = Some("stream ended before the last keystroke".into());
                        break;
                    }
                    let index = writes.len();
                    process
                        .writer
                        .write_all(&PROBE.as_bytes()[index..index + 1])
                        .map_err(|_| BenchError::Io("keystroke write".into()))?;
                    process
                        .writer
                        .flush()
                        .map_err(|_| BenchError::Io("keystroke flush".into()))?;
                    let at = Instant::now();
                    report.write_lag_ms = report.write_lag_ms.max(
                        at.saturating_duration_since(start + schedule[index])
                            .as_secs_f64()
                            * 1000.0,
                    );
                    writes.push(at);
                    echoes.push(None);
                }
                if observation.error.is_some() {
                    break;
                }
                if let Some(index) = echoes.iter().enumerate().find_map(|(i, value)| {
                    (value.is_none()
                        && now.saturating_duration_since(writes[i]).as_secs_f64() * 1000.0
                            >= options.budgets.keystroke.visible_within_ms
                        && !queued_before(
                            writes[i]
                                + Duration::from_secs_f64(
                                    options.budgets.keystroke.visible_within_ms / 1000.0,
                                ),
                        ))
                    .then_some(i)
                }) {
                    observation.error = Some(format!("key {} not visible within 1 s", index + 1));
                    break;
                }
                if report_at.is_some()
                    && writes.len() == schedule.len()
                    && echoes.iter().all(Option::is_some)
                    && report_at.is_none_or(|at| !queued_before(at))
                {
                    observation.latencies = echoes.into_iter().flatten().collect();
                    break;
                }
            }
            if report_at.is_none()
                && now
                    >= start
                        + expected
                        + Duration::from_secs(u64::from(options.report_grace_seconds))
                && !queued_before(
                    start + expected + Duration::from_secs(u64::from(options.report_grace_seconds)),
                )
            {
                observation.error = Some("binary report missing".into());
                break;
            }
            if let Some(at) = report_at {
                let watch = match mode {
                    Mode::Stream(true) => idle,
                    _ => Duration::ZERO,
                };
                if mode != Mode::Keys
                    && now >= at + settle + watch
                    && !queued_before(at + settle + watch)
                {
                    break;
                }
            }
        } else if now.saturating_duration_since(process.started) >= Duration::from_secs(5)
            && !queued_before(process.started + Duration::from_secs(5))
        {
            observation.error=Some("no synchronized frames seen; no frame within 5 s; did the binary answer the capability queries?".into());
            break;
        }
        let mut wait = Duration::from_millis(10);
        if let Some(start) = started
            && mode == Mode::Keys
            && writes.len() < schedule.len()
        {
            wait = wait
                .min((start + schedule[writes.len()]).saturating_duration_since(Instant::now()));
        }
        if let Some((at, chunk)) = pending.or_else(|| process.output.recv_timeout(wait).ok()) {
            if report_at.is_some_and(|r| at >= r + settle) {
                observation.idle_bytes += chunk.len() as u64;
            }
            for byte in chunk {
                let answers = crate::answer_queries(&mut responder, &[byte], &mut emulator);
                if !answers.reply.is_empty() && !matches!(process.child.try_wait(), Ok(Some(_))) {
                    process
                        .writer
                        .write_all(&answers.reply)
                        .map_err(|_| BenchError::Io("query reply".into()))?;
                }
                for query in answers.seen {
                    if started.is_some() {
                        report.late_queries.push(match &query {
                            crate::Query::CursorPosition => "cursor position".into(),
                            crate::Query::KeyboardFlags => "keyboard flags".into(),
                            crate::Query::SynchronizedOutput => "synchronized output".into(),
                            crate::Query::DeviceAttributes => "device attributes".into(),
                            crate::Query::Other(kind) => kind.clone(),
                        });
                    }
                    if let crate::Query::Other(kind) = query {
                        report.unanswered_queries.push(kind);
                    }
                }
                for frame in splitter.push(&[byte], at) {
                    if report_at.is_none_or(|r| frame.end <= r + settle) {
                        observation.counted += 1;
                    }
                    if started.is_none() && emulator.has_non_blank_cell() {
                        started = Some(frame.end);
                        observation.launch_ms = frame
                            .end
                            .saturating_duration_since(process.started)
                            .as_secs_f64()
                            * 1000.0;
                        if mode != Mode::Launch {
                            match process_sample(&process) {
                                Ok(sample) => {
                                    cpu_start = Some(sample);
                                    observation.rss = observation.rss.max(sample.rss_bytes);
                                    samples.clear();
                                    samples.push(sample);
                                    next_sample = Instant::now()
                                        + Duration::from_millis(options.budgets.cpu.sample_ms);
                                }
                                Err(_) => {
                                    observation.error = Some("process sampling failed".into())
                                }
                            }
                        }
                        if mode == Mode::Launch {
                            break;
                        }
                    } else if started.is_some() && report_at.is_none_or(|r| frame.end <= r) {
                        observation.frame_bytes.push(frame.bytes as f64);
                    }
                    if report_at.is_some_and(|r| frame.end >= r + settle) {
                        observation.idle_frames += 1;
                    }
                    for (index, value) in echoes.iter_mut().enumerate() {
                        if value.is_none()
                            && crate::typed_prefix_visible(&emulator, PROBE, index + 1)
                        {
                            *value = Some(
                                frame
                                    .end
                                    .saturating_duration_since(writes[index])
                                    .as_secs_f64()
                                    * 1000.0,
                            );
                        }
                    }
                }
            }
            if mode == Mode::Launch && started.is_some() {
                break;
            }
            if observation.error.is_some() {
                break;
            }
        }
    }
    let counts = splitter.counts();
    report.counts.bytes_outside_frames += counts.bytes_outside_frames;
    report.counts.nested_markers += counts.nested_markers;
    report.counts.unterminated_frames += counts.unterminated_frames;
    Ok(observation)
}

fn read_version(options: &RunOptions, report: &mut Report) {
    use std::io::Read;
    use std::process::{Command, Stdio};
    struct Guard(std::process::Child);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let Ok(home) = tempfile::tempdir() else {
        return;
    };
    let mut command = Command::new(&options.binary);
    command
        .arg("--version")
        .env_clear()
        .env("HOME", home.path())
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("LANG", "C.UTF-8");
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    let Ok(child) = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    let mut guard = Guard(child);
    report.child_pids.push(guard.0.id());
    let Some(mut stdout) = guard.0.stdout.take() else {
        return;
    };
    let reader = std::thread::spawn(move || {
        let mut first = Vec::new();
        let mut done = false;
        let mut buf = [0; 4096];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            for &byte in &buf[..n] {
                if byte == b'\n' {
                    done = true;
                }
                if !done && first.len() <= 100 {
                    first.push(byte);
                }
            }
        }
        first
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let success = loop {
        if let Ok(Some(status)) = guard.0.try_wait() {
            break status.success();
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Close the pipe on both normal completion and the timeout before joining.
    drop(guard);
    if let Ok(bytes) = reader.join()
        && success
    {
        let text = String::from_utf8_lossy(&bytes);
        let first = text.trim_end_matches('\r');
        if first.len() < 100 && !first.contains(['/', '\\']) && !first.chars().any(char::is_control)
        {
            report.binary_version = first.into();
        }
    }
}

fn row(name: &str, value: f64, limit: f64, unit: &str, error: Option<&str>) -> Row {
    Row {
        name: name.into(),
        measured: error
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{value:.3} {unit}")),
        limit: format!("{limit} {unit}"),
        status: if error.is_some() {
            Status::Fail
        } else {
            verdict(value, limit)
        },
        attempts: Vec::new(),
    }
}
fn record_attempt(target: &mut Option<Row>, mut attempt: Row, index: u32) {
    let line = format!(
        "attempt {index}: {} {}",
        attempt.measured,
        crate::report::status(attempt.status).to_uppercase()
    );
    match target {
        None => {
            attempt.attempts.push(line);
            *target = Some(attempt);
        }
        Some(old) => {
            old.attempts.push(line);
            let numeric = |text: &str| {
                text.split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(f64::INFINITY)
            };
            if (old.status != Status::Pass && attempt.status == Status::Pass)
                || (old.status == attempt.status
                    && numeric(&attempt.measured) < numeric(&old.measured))
            {
                old.status = attempt.status;
                old.measured = attempt.measured;
            }
        }
    }
}

pub struct RunOptions {
    pub binary: PathBuf,
    pub fixtures: PathBuf,
    pub budgets: Budgets,
    pub report: Option<PathBuf>,
    pub attempts: u32,
    pub idle_seconds: Option<u32>,
    pub report_grace_seconds: u32,
    pub binary_args: Vec<String>,
    pub extra_fixtures: Vec<PathBuf>,
    pub scenario_dir: Option<PathBuf>,
    pub only: Vec<String>,
}

impl RunOptions {
    pub fn new(binary: PathBuf, budgets: Budgets) -> RunOptions {
        RunOptions {
            binary,
            fixtures: PathBuf::from("fixtures/frames"),
            budgets,
            report: None,
            attempts: 3,
            idle_seconds: None,
            report_grace_seconds: 10,
            binary_args: Vec::new(),
            extra_fixtures: Vec::new(),
            scenario_dir: None,
            only: Vec::new(),
        }
    }
}

pub fn run(options: &RunOptions) -> Result<Report, BenchError> {
    if options.attempts == 0
        || options.only.iter().any(|name| {
            !matches!(
                name.as_str(),
                "launch" | "keystroke" | "cpu" | "bytes" | "worst-frame" | "idle" | "memory"
            )
        })
    {
        return Err(BenchError::BudgetsInvalid(
            "invalid run selection or attempt count".into(),
        ));
    }
    if !options.binary.is_file() {
        return Err(BenchError::BinaryNotFound(
            options.binary.to_string_lossy().into_owned(),
        ));
    }
    let scratch = tempfile::tempdir().map_err(|_| BenchError::Io("scenarios".into()))?;
    crate::generate(&options.fixtures, scratch.path(), &options.budgets)?;
    let mut report = Report::default();
    read_version(options, &mut report);
    let selected = |name: &str| options.only.is_empty() || options.only.iter().any(|s| s == name);
    let attempts = options.attempts.max(1);
    if selected("launch") {
        let mut result = None;
        for attempt in 1..=attempts {
            let mut values = Vec::new();
            let mut error = None;
            for index in 0..options.budgets.launch.warmups + options.budgets.launch.runs {
                let sample = observe(
                    options,
                    &scratch.path().join("empty.jsonl"),
                    "1",
                    Mode::Launch,
                    &mut report,
                )?;
                if sample.error.is_some() {
                    error = sample.error;
                    break;
                }
                if index >= options.budgets.launch.warmups {
                    values.push(sample.launch_ms);
                }
            }
            let mut value = row(
                "launch to first frame",
                median(&values),
                options.budgets.launch.median_ms,
                "ms",
                error.as_deref(),
            );
            if error.is_none() && !values.is_empty() {
                value.measured = format!(
                    "{:.3} ms (min {:.3}, max {:.3})",
                    median(&values),
                    values.iter().copied().fold(f64::INFINITY, f64::min),
                    values.iter().copied().fold(0.0, f64::max)
                );
            }
            let passed = value.status == Status::Pass;
            record_attempt(&mut result, value, attempt);
            report.attempts_used = report.attempts_used.max(attempt);
            if passed {
                break;
            }
        }
        if let Some(result) = result {
            report.rows.push(result);
        }
    }
    let mut scenarios = vec![(
        scratch.path().join("reference-stream.jsonl"),
        String::new(),
        true,
    )];
    let mut extra = options.extra_fixtures.clone();
    let scenario_dir = options
        .scenario_dir
        .as_deref()
        .unwrap_or_else(|| Path::new("tools/bench/scenarios"));
    if let Ok(entries) = std::fs::read_dir(scenario_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|s| s == "jsonl") {
                extra.push(entry.path());
            }
        }
    }
    extra.sort();
    extra.dedup();
    for path in extra {
        let label: String = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("scenario")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        scenarios.push((path, format!(" [{label}]"), false));
    }
    for (fixture, label, reference) in scenarios {
        if selected("keystroke") {
            for speed in if reference {
                vec!["1", "10"]
            } else {
                vec!["1"]
            } {
                let mut p95 = None;
                let mut max = None;
                for attempt in 1..=attempts {
                    let sample = observe(options, &fixture, speed, Mode::Keys, &mut report)?;
                    let a = row(
                        &format!("keystroke p95, stream at {speed}x{label}"),
                        percentile_nearest_rank(&sample.latencies, 95.0),
                        options.budgets.keystroke.p95_ms,
                        "ms",
                        sample.error.as_deref(),
                    );
                    let b = row(
                        &format!("keystroke max, stream at {speed}x{label}"),
                        sample.latencies.iter().copied().fold(0.0, f64::max),
                        options.budgets.keystroke.max_ms,
                        "ms",
                        sample.error.as_deref(),
                    );
                    let passed = a.status == Status::Pass && b.status == Status::Pass;
                    record_attempt(&mut p95, a, attempt);
                    record_attempt(&mut max, b, attempt);
                    report.attempts_used = report.attempts_used.max(attempt);
                    if passed {
                        break;
                    }
                }
                report.rows.extend(p95);
                report.rows.extend(max);
            }
        }
        if ["cpu", "bytes", "worst-frame", "idle"]
            .iter()
            .any(|s| selected(s))
        {
            let mut cpu = None;
            let mut peak = None;
            let mut worst = None;
            for attempt in 1..=attempts {
                let sample = observe(
                    options,
                    &fixture,
                    "1",
                    Mode::Stream(attempt == 1 && selected("idle")),
                    &mut report,
                )?;
                let mean = sample.cpu_mean.unwrap_or(f64::NAN);
                let a = row(
                    &format!("CPU mean while streaming{label}"),
                    mean,
                    options.budgets.cpu.mean_pct,
                    "%",
                    sample.error.as_deref(),
                );
                let b = row(
                    &format!("CPU peak (1 s window){label}"),
                    peak_window(&sample.cpu, options.budgets.cpu.window_samples),
                    options.budgets.cpu.peak_pct,
                    "%",
                    sample.error.as_deref(),
                );
                let missing = if sample.report.is_none() {
                    Some("binary report missing")
                } else {
                    sample.error.as_deref()
                };
                let c = row(
                    &format!("worst single frame{label}"),
                    sample
                        .report
                        .as_ref()
                        .and_then(|r| r["worst_pass_us"].as_f64())
                        .unwrap_or(f64::NAN)
                        / 1000.0,
                    options.budgets.worst_frame.max_ms,
                    "ms",
                    missing,
                );
                let passed = (!selected("cpu")
                    || (a.status == Status::Pass && b.status == Status::Pass))
                    && (!selected("worst-frame") || c.status == Status::Pass);
                record_attempt(&mut cpu, a, attempt);
                record_attempt(&mut peak, b, attempt);
                record_attempt(&mut worst, c, attempt);
                report.attempts_used = report.attempts_used.max(attempt);
                if attempt == 1 {
                    if selected("bytes") {
                        let fifth = sample
                            .frame_bytes
                            .len()
                            .div_ceil(5)
                            .max(1)
                            .min(sample.frame_bytes.len());
                        let ratio = if fifth == 0 {
                            f64::NAN
                        } else {
                            median(&sample.frame_bytes[sample.frame_bytes.len() - fifth..])
                                / median(&sample.frame_bytes[..fifth])
                        };
                        report.rows.push(row(
                            &format!("bytes per frame, median (unit choice){label}"),
                            median(&sample.frame_bytes),
                            options.budgets.unit_choice.bytes_median,
                            "B",
                            sample.error.as_deref(),
                        ));
                        report.rows.push(row(
                            &format!("bytes per frame, late over early (unit choice){label}"),
                            ratio,
                            options.budgets.unit_choice.late_over_early,
                            "",
                            sample.error.as_deref(),
                        ));
                    }
                    if selected("idle") {
                        let mut r = row(
                            &format!("idle frames and bytes{label}"),
                            (sample.idle_frames + sample.idle_bytes) as f64,
                            options.budgets.idle.frames as f64,
                            "",
                            sample.error.as_deref(),
                        );
                        if sample.error.is_none() {
                            r.measured = format!(
                                "{} frames / {} bytes",
                                sample.idle_frames, sample.idle_bytes
                            );
                        }
                        report.rows.push(r);
                        if reference {
                            report.rows.push(Row::info(
                                "idle CPU mean (not enforced)",
                                &format!(
                                    "{:.3} %",
                                    if sample.idle_cpu.is_empty() {
                                        0.0
                                    } else {
                                        sample.idle_cpu.iter().sum::<f64>()
                                            / sample.idle_cpu.len() as f64
                                    }
                                ),
                            ));
                        }
                    }
                    let drawn = sample
                        .report
                        .as_ref()
                        .and_then(|r| r["frames_drawn"].as_u64());
                    let allowance = if sample.counted < 100 {
                        0
                    } else {
                        sample.counted.div_ceil(100)
                    };
                    let matches = drawn.is_some_and(|n| sample.counted.abs_diff(n) <= allowance)
                        && sample.counted > 0;
                    report.rows.push(Row {
                        name: format!("check: frame count matches the binary's report{label}"),
                        measured: format!("{} / {}", sample.counted, drawn.unwrap_or(0)),
                        limit: "1 %".into(),
                        status: if matches { Status::Pass } else { Status::Fail },
                        attempts: Vec::new(),
                    });
                }
                if passed {
                    break;
                }
            }
            if selected("cpu") {
                report.rows.extend(cpu);
                report.rows.extend(peak);
            }
            if selected("worst-frame") {
                report.rows.extend(worst);
            }
        }
    }
    if selected("memory") {
        for (fixture, mode, name, limit) in [
            (
                "empty.jsonl",
                Mode::MemoryStart,
                "resident memory at start".to_owned(),
                options.budgets.memory.start_max,
            ),
            (
                "long-session.jsonl",
                Mode::MemoryLong,
                format!(
                    "resident memory after {} cells",
                    options.budgets.scenario.long_session_turns * 2
                ),
                options.budgets.memory.long_session_max,
            ),
        ] {
            let sample = observe(
                options,
                &scratch.path().join(fixture),
                "instant",
                mode,
                &mut report,
            )?;
            report.rows.push(row(
                &name,
                sample.rss as f64 / 1_000_000.0,
                limit,
                "MB",
                sample.error.as_deref(),
            ));
        }
    }
    report.rows.push(Row {
        name: "check: no queries after start-up".into(),
        measured: if report.late_queries.is_empty() {
            "none late".into()
        } else {
            format!("query_after_startup: {}", report.late_queries.join(", "))
        },
        limit: "none".into(),
        status: if report.late_queries.is_empty() {
            Status::Pass
        } else {
            Status::Fail
        },
        attempts: Vec::new(),
    });
    report.rows.push(Row::skipped(
        "launch to usable composer (live gateway)",
        &format!("{} ms", options.budgets.live_gateway.composer_ms),
    ));
    if let Some(path) = &options.report {
        std::fs::write(path, report.to_json())
            .map_err(|_| BenchError::Io("report write".into()))?;
    }
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        use std::fs::OpenOptions;
        let mut summary = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|_| BenchError::Io("summary write".into()))?;
        summary
            .write_all(report.render().as_bytes())
            .map_err(|_| BenchError::Io("summary write".into()))?;
    }
    Ok(report)
}

/// 0 when no row failed, else 1.
pub fn exit_code(report: &Report) -> u8 {
    u8::from(report.rows.iter().any(|r| r.status == Status::Fail))
}
