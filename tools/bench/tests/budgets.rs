//! The budgets file, the statistics and verdicts, and the keystroke schedule.

mod support;

use std::time::Duration;

use talaria_bench::{
    Budgets, Report, Row, Status, best_of, exit_code, keystroke_schedule, median, peak_window,
    percentile_nearest_rank, replay_duration, verdict, window_means,
};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn within_a_millisecond(actual: Duration, expected_ms: u64) -> bool {
    let expected = Duration::from_millis(expected_ms);
    actual.abs_diff(expected) < Duration::from_millis(1)
}

#[test]
fn default_budgets_match_architecture_table() {
    // The numbers of the architecture's performance budget table: launch to first frame,
    // keystroke 95th percentile and worst, streaming CPU mean and peak, worst single
    // frame, idle frames, memory at start, memory after the long session, and launch to a
    // usable composer with a live gateway. Held here as constants on purpose, so this
    // test reads no other file.
    const LAUNCH_MEDIAN_MS: f64 = 100.0;
    const KEYSTROKE_P95_MS: f64 = 16.0;
    const KEYSTROKE_MAX_MS: f64 = 33.0;
    const CPU_MEAN_PCT: f64 = 5.0;
    const CPU_PEAK_PCT: f64 = 15.0;
    const WORST_FRAME_MS: f64 = 50.0;
    const IDLE_FRAMES: u64 = 0;
    const MEMORY_START_MB: f64 = 40.0;
    const MEMORY_LONG_SESSION_MB: f64 = 100.0;
    const COMPOSER_MS: f64 = 200.0;

    let budgets = Budgets::load(&support::shipped_budgets_path()).expect("shipped budgets load");
    assert!(close(budgets.launch.median_ms, LAUNCH_MEDIAN_MS));
    assert!(close(budgets.keystroke.p95_ms, KEYSTROKE_P95_MS));
    assert!(close(budgets.keystroke.max_ms, KEYSTROKE_MAX_MS));
    assert!(close(budgets.cpu.mean_pct, CPU_MEAN_PCT));
    assert!(close(budgets.cpu.peak_pct, CPU_PEAK_PCT));
    assert!(close(budgets.worst_frame.max_ms, WORST_FRAME_MS));
    assert_eq!(budgets.idle.frames, IDLE_FRAMES);
    assert!(close(budgets.memory.start_max, MEMORY_START_MB));
    assert!(close(
        budgets.memory.long_session_max,
        MEMORY_LONG_SESSION_MB
    ));
    assert!(close(budgets.live_gateway.composer_ms, COMPOSER_MS));
    // Megabytes of 1,000,000 bytes, not mebibytes.
    assert_eq!(budgets.memory.unit, "MB");
}

#[test]
fn statistics_and_verdicts_are_correct() {
    // Median: odd and even counts, input in any order.
    assert!(close(median(&[3.0, 1.0, 2.0]), 2.0));
    assert!(close(median(&[9.0, 1.0, 5.0, 7.0, 3.0]), 5.0));
    assert!(close(median(&[4.0, 1.0, 3.0, 2.0]), 2.5));
    assert!(close(median(&[7.5]), 7.5));

    // Nearest rank: of forty samples the 95th percentile is the 38th smallest value.
    let forty: Vec<f64> = (0..40).map(|i| f64::from((i * 17) % 40 + 1)).collect();
    let mut sorted = forty.clone();
    sorted.sort_by(f64::total_cmp);
    assert!(
        close(sorted[0], 1.0) && close(sorted[39], 40.0),
        "a shuffle of 1 to 40"
    );
    assert!(close(percentile_nearest_rank(&forty, 95.0), 38.0));
    assert!(close(percentile_nearest_rank(&forty, 50.0), 20.0));
    assert!(close(percentile_nearest_rank(&forty, 100.0), 40.0));
    // Of ten samples the 95th percentile is the largest.
    let ten = [8.0, 3.0, 10.0, 1.0, 6.0, 4.0, 9.0, 2.0, 7.0, 5.0];
    assert!(close(percentile_nearest_rank(&ten, 95.0), 10.0));

    // Windows slide by one sample and each is the mean of its samples.
    let means = window_means(&[1.0, 2.0, 3.0, 4.0, 5.0], 3);
    assert_eq!(means.len(), 3);
    assert!(close(means[0], 2.0) && close(means[1], 3.0) && close(means[2], 4.0));

    // Ten-sample peak: one 100 among zeros is a 10 for the window around it, never 100.
    let mut spike = vec![0.0; 25];
    spike[12] = 100.0;
    assert_eq!(window_means(&spike, 10).len(), 16);
    assert!(close(peak_window(&spike, 10), 10.0));
    // A sustained second at 20 is a peak of 20.
    let mut sustained = vec![0.0; 8];
    sustained.extend([20.0; 10]);
    sustained.extend([0.0; 8]);
    assert!(close(peak_window(&sustained, 10), 20.0));

    // Both sides of a limit; the limit itself passes.
    assert_eq!(verdict(14.9, 15.0), Status::Pass);
    assert_eq!(verdict(15.0, 15.0), Status::Pass);
    assert_eq!(verdict(15.1, 15.0), Status::Fail);
    assert_eq!(verdict(0.0, 0.0), Status::Pass);
    assert_eq!(verdict(1.0, 0.0), Status::Fail);

    // Information and skipped rows keep their status and text, and never fail a run.
    let info = Row::info("idle CPU mean (not enforced)", "1.2%");
    assert_eq!(info.status, Status::Info);
    assert_eq!(info.name, "idle CPU mean (not enforced)");
    assert_eq!(info.measured, "1.2%");
    assert!(info.attempts.is_empty());
    let skipped = Row::skipped("composer with a live gateway", "200 ms");
    assert_eq!(skipped.status, Status::Skipped);
    assert_eq!(skipped.name, "composer with a live gateway");
    assert_eq!(skipped.limit, "200 ms");
    assert!(skipped.attempts.is_empty());

    let mut report = Report::default();
    report.rows.push(info);
    report.rows.push(skipped);
    assert_eq!(
        exit_code(&report),
        0,
        "information and skipped rows are not failures"
    );

    let mut failing = Report::default();
    failing.rows.push(Row::info("note", "x"));
    failing.rows.push(Row {
        name: "launch".to_string(),
        measured: "150 ms".to_string(),
        limit: "100 ms".to_string(),
        status: Status::Fail,
        attempts: Vec::new(),
    });
    failing.rows.push(Row::skipped("composer", "200 ms"));
    assert_eq!(exit_code(&failing), 1);
}

#[test]
fn best_of_attempts_passes_when_any_attempt_passes() {
    let (status, best) = best_of(&[90.0, 140.0, 150.0], 100.0);
    assert_eq!(status, Status::Pass);
    assert!(close(best, 90.0));

    let (status, best) = best_of(&[140.0, 150.0, 160.0], 100.0);
    assert_eq!(status, Status::Fail);
    assert!(close(best, 140.0));

    // The best attempt is the lowest, wherever it sits; a late pass still passes.
    let (status, best) = best_of(&[150.0, 140.0, 100.0], 100.0);
    assert_eq!(status, Status::Pass);
    assert!(close(best, 100.0));

    let (status, best) = best_of(&[42.0], 100.0);
    assert_eq!(status, Status::Pass);
    assert!(close(best, 42.0));
}

/// Checks a schedule against hand-written expectations.
fn check_schedule(
    budgets: &Budgets,
    speed: f64,
    keys: usize,
    first_ms: u64,
    step_ms: u64,
    replay_ms: u64,
    what: &str,
) {
    let schedule = keystroke_schedule(budgets, speed);
    assert_eq!(schedule.len(), keys, "{what}: key count");
    assert!(
        within_a_millisecond(schedule[0], first_ms),
        "{what}: first key {:?}",
        schedule[0]
    );
    let last_ms = first_ms + step_ms * (keys as u64 - 1);
    assert!(
        within_a_millisecond(schedule[keys - 1], last_ms),
        "{what}: last key {:?}, expected {last_ms} ms",
        schedule[keys - 1]
    );
    for pair in schedule.windows(2) {
        assert!(pair[0] < pair[1], "{what}: offsets rise");
    }
    for (index, offset) in schedule.iter().enumerate() {
        assert!(
            within_a_millisecond(*offset, first_ms + step_ms * index as u64),
            "{what}: key {index} at {offset:?}"
        );
    }

    let replay = replay_duration(budgets, speed);
    assert!(
        within_a_millisecond(replay, replay_ms),
        "{what}: replay lasts {replay:?}"
    );
    assert!(
        schedule[keys - 1] + Duration::from_millis(500) <= replay,
        "{what}: the last key ({:?}) is less than half a second before the replay ends ({replay:?})",
        schedule[keys - 1]
    );
}

#[test]
fn keystroke_schedule_ends_before_the_replay() {
    let real = Budgets::load(&support::shipped_budgets_path()).expect("shipped budgets load");
    // Forty keys: from 3,000 ms every 150 ms at 1x (last at 8,850 ms, replay 60 s);
    // from 500 ms every 100 ms at 10x (last at 4,400 ms, replay 6 s).
    check_schedule(&real, 1.0, 40, 3000, 150, 60_000, "shipped budgets at 1x");
    check_schedule(&real, 10.0, 40, 500, 100, 6_000, "shipped budgets at 10x");

    // Ten keys: from 500 ms every 100 ms at 1x (last at 1,400 ms, replay 12 s);
    // from 50 ms every 40 ms at 10x (last at 410 ms, replay 1.2 s).
    let test = support::test_budgets();
    check_schedule(&test, 1.0, 10, 500, 100, 12_000, "test budgets at 1x");
    check_schedule(&test, 10.0, 10, 50, 40, 1_200, "test budgets at 10x");
}
