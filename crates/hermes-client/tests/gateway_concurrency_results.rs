//! Offline checks for the gateway concurrency measurement.
//!
//! The committed results file is not part of this change, so the checks that
//! read it fail immediately. The other checks call helpers that are still stubs.

#[path = "concurrency_probe/mod.rs"]
mod concurrency_probe;
mod support;

use std::collections::BTreeSet;
use std::path::PathBuf;

use concurrency_probe::{
    EventTracker, LoopbackProvider, ProviderScript, Recommendations, RecordWriter, TurnClock,
    create_params, descriptor_gate, healthy, highest_healthy, percentile, recommend, submit_params,
    sum_process_tree, wait_for_turn,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MATRIX: &[&str] = &[
    "A-shared-1",
    "A-shared-5",
    "A-shared-10",
    "A-shared-16",
    "A-shared-20",
    "A-per-session-1",
    "A-per-session-5",
    "A-per-session-10",
    "A-per-session-16",
    "A-per-session-20",
    "B-shared-1",
    "B-shared-5",
    "B-shared-10",
    "B-shared-16",
    "C-shared-10",
    "D-shared-10",
    "E-shared-20",
];

const FILE_KEYS: &[&str] = &[
    "execution_mode",
    "hermes_release",
    "recommendations",
    "runs",
];

const RUN_KEYS: &[&str] = &[
    "admissions_unknown",
    "batch_wall_ms",
    "create_ms",
    "create_refused",
    "distinct_sessions",
    "events",
    "evidence",
    "failed_criteria",
    "first_delta_ms",
    "foreground_errors",
    "foreground_rtt_ms",
    "healthy",
    "id",
    "notes",
    "observations",
    "provider",
    "recovery",
    "repetitions",
    "resources",
    "rpc_errors",
    "run",
    "series",
    "start_delay_ms",
    "submit_ms",
    "timeouts",
    "topology",
    "turn_ms",
    "turns_completed",
    "turns_failed",
    "unavailable",
    "width",
];

const PERCENTILE_FIELDS: &[&str] = &[
    "create_ms",
    "first_delta_ms",
    "foreground_rtt_ms",
    "start_delay_ms",
    "submit_ms",
    "turn_ms",
];

const COUNT_FIELDS: &[&str] = &[
    "admissions_unknown",
    "create_refused",
    "foreground_errors",
    "timeouts",
    "turns_completed",
    "turns_failed",
];

/// Launch fields from the pinned gateway session policy, plus `title`.
/// `hidden` is intentionally absent.
const CREATE_PARAMETERS: &[&str] = &[
    "accept_hooks",
    "api_key",
    "base_url",
    "checkpoints",
    "client_env",
    "cwd",
    "cwd_explicit",
    "editor",
    "follow_profile_config",
    "ignore_rules",
    "ignore_user_config",
    "max_turns",
    "model",
    "pass_session_id",
    "provider",
    "reasoning",
    "request_id",
    "row_cwd",
    "run_budget",
    "safe_mode",
    "service_tier",
    "skills",
    "source",
    "title",
    "toolsets",
    "verbose",
    "yolo",
];

const EVENT_KEYS: &[&str] = &[
    "complete_per_admission",
    "epoch_changes",
    "observed",
    "peak_events_per_s",
    "replay_gaps",
    "seq_decreases",
    "seq_gaps",
    "seq_repeats",
    "unresolved_gaps",
];

const RECOVERY_KEYS: &[&str] = &[
    "eviction_evidence",
    "next_turn_ms",
    "receipt_ms",
    "replay_truncated",
    "resume_ms",
    "submit_after_detach_refused",
    "terminal_with_result",
    "terminal_without_result",
    "unknown",
    "unresolved",
];

const OBSERVATION_KEYS: &[&str] = &[
    "admission_statuses",
    "failure_examples",
    "rate_limit_keys",
    "tool_executions",
    "tool_names",
];

#[test]
fn results_parse_and_every_run_is_complete() {
    let doc = load_results();
    assert_keys(&doc, FILE_KEYS, "results file");
    assert_eq!(doc["hermes_release"].as_str(), Some(pinned_tag().as_str()));
    let mode = doc["execution_mode"].as_str().expect("execution mode");
    assert!(matches!(mode, "in-process" | "managed-worker" | "unknown"));
    validate_recommendations(&doc["recommendations"]);
    let runs = doc["runs"].as_array().expect("runs");
    for run in runs {
        validate_run(run);
    }
    assert_tool_name_placement(runs);
}

#[test]
fn results_cover_the_required_matrix() {
    let doc = load_results();
    let runs = doc["runs"].as_array().expect("runs");
    let mut ids: Vec<&str> = runs
        .iter()
        .map(|run| run["id"].as_str().expect("id"))
        .collect();
    ids.sort_unstable();
    let mut expected = MATRIX.to_vec();
    expected.sort_unstable();
    assert_eq!(ids, expected);
}

#[test]
fn results_meet_the_evidence_gate() {
    let doc = load_results();
    assert!(evidence_gate(&doc), "committed results");
    assert!(!evidence_gate(&all_not_run()), "all not-run");
    assert!(!evidence_gate(&missing_d()), "missing D");
    assert!(evidence_gate(&with_note("B-shared-5", "operator waived")));
    assert!(evidence_gate(&with_note(
        "B-shared-5",
        "unsupported_managed_policy"
    )));
    assert!(evidence_gate(&with_note(
        "C-shared-10",
        "toolset refused: missing"
    )));
    assert!(evidence_gate(&with_note(
        "A-shared-5",
        "stopped after an unhealthy width"
    )));
    assert!(evidence_gate(&with_note(
        "A-shared-20",
        "above the operator's width cap"
    )));
    assert!(!evidence_gate(&with_note("A-shared-5", "timebox reached")));
    assert!(!evidence_gate(&with_note(
        "A-shared-5",
        "loopback provider refused: refused"
    )));
    assert!(!evidence_gate(&with_note("A-shared-5", "operator waived")));
    assert!(!evidence_gate(&with_note(
        "A-shared-5",
        "unsupported_managed_policy"
    )));
    assert!(!evidence_gate(&with_note(
        "A-shared-5",
        "toolset refused: missing"
    )));
    assert!(!evidence_gate(&with_note(
        "B-shared-5",
        "toolset refused: missing"
    )));
    assert!(!evidence_gate(&with_note(
        "A-shared-5",
        "stopped: descriptor lacks provider"
    )));
}

#[test]
fn percentile_uses_nearest_rank() {
    assert!(percentile(&[]).is_none());
    let summary = percentile(&[5, 1, 3, 2, 4]).expect("five samples");
    assert_eq!(summary.n, 5);
    assert_eq!(summary.p50, 3);
    assert_eq!(summary.p95, 5);
    assert_eq!(summary.max, 5);
    let one = percentile(&[7]).expect("one sample");
    assert_eq!((one.n, one.p50, one.p95, one.max), (1, 7, 7, 7));
}

#[test]
fn ps_parser_sums_a_process_tree() {
    let text = "\
  100     1   1024 00:01:02\n\
  200   100   2048 1:02.50\n\
  300   200    512 00:00:01\n\
  400     1   8192 00:00:09\n\
not-a-process";
    let sample = sum_process_tree(text, 100);
    assert_eq!(sample.rss_kib, 3_584);
    assert!((sample.cpu_seconds - 125.5).abs() < 1e-6);
    assert_eq!(sample.bad_lines, 1);
}

#[tokio::test]
async fn loopback_provider_streams_the_hermes_test_shape() {
    let provider = LoopbackProvider::bind(ProviderScript::Todo).await;
    provider.reset_peak();
    let addr = provider.addr();
    let ready = http_request("POST", "/v1/chat/completions", Some(&ready_body()));
    let (left, right) = tokio::join!(http_exchange(addr, &ready), http_exchange(addr, &ready));
    assert_eq!(provider.turn_requests_peak(), 2);
    for (_status, body) in [&left, &right] {
        assert!(body.contains("chat.completion.chunk"), "{body}");
        assert!(body.contains("talaria-probe-stub"), "{body}");
        assert!(body.contains("todo_list"), "{body}");
        assert!(has_finish(body, "tool_calls"), "{body}");
        assert!(body.contains("data: [DONE]"), "{body}");
    }
    let follow = http_request("POST", "/v1/chat/completions", Some(&tool_body()));
    let (_status, body) = http_exchange(addr, &follow).await;
    assert!(has_finish(&body, "stop"), "{body}");
    assert!(body.contains("data: [DONE]"), "{body}");
    assert!(
        body.contains("\"content\":\"R\"") || body.contains("\"content\": \"R\""),
        "{body}"
    );
    let peak = provider.turn_requests_peak();
    let aux_before = provider.aux_requests();
    let aux = http_request("POST", "/v1/chat/completions", Some(&aux_body()));
    let (status, body) = http_exchange(addr, &aux).await;
    assert_eq!(status, 200);
    assert!(body.contains("probe"), "{body}");
    assert_eq!(provider.aux_requests(), aux_before + 1);
    assert_eq!(provider.turn_requests_peak(), peak);
    let other = http_request("GET", "/v1/other", None);
    let (status, _body) = http_exchange(addr, &other).await;
    assert_eq!(status, 404);
    assert!(provider.unexpected_routes() >= 1);
}

#[test]
fn outbound_params_never_carry_unattended_or_yolo() {
    let run_id = "ab12cd34";
    let mut request_ids = Vec::new();
    let mut submission_ids = Vec::new();
    for (series, record_id) in [
        ('A', "A-shared-1"),
        ('B', "B-shared-1"),
        ('C', "C-shared-10"),
    ] {
        for session in 0..3 {
            let created = create_params(series, run_id, record_id, session, 9);
            assert_create(series, run_id, record_id, session, 9, &created);
            request_ids.push(created["request_id"].as_str().unwrap().to_string());
            for repetition in 0..3 {
                let session_id = format!("session-{session}");
                let submitted = submit_params(&session_id, run_id, record_id, session, repetition);
                assert_submit(
                    &session_id,
                    run_id,
                    record_id,
                    session,
                    repetition,
                    &submitted,
                );
                submission_ids.push(submitted["submission_id"].as_str().unwrap().to_string());
            }
        }
    }
    assert_eq!(
        request_ids.len(),
        request_ids.iter().collect::<BTreeSet<_>>().len()
    );
    assert_eq!(
        submission_ids.len(),
        submission_ids.iter().collect::<BTreeSet<_>>().len()
    );
}

#[test]
fn descriptor_gate_accepts_the_pinned_descriptor() {
    let described = pinned_descriptor();
    let parameters = described["session_create"]["parameters"]
        .as_array()
        .expect("parameters");
    let mut actual: Vec<&str> = parameters.iter().filter_map(Value::as_str).collect();
    actual.sort_unstable();
    let mut expected = CREATE_PARAMETERS.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected);
    assert!(!actual.contains(&"hidden"));
    assert!(
        described["session_create"]["sources"]
            .as_array()
            .expect("sources")
            .iter()
            .any(|source| source == "tool")
    );
    assert!(descriptor_gate(&described).is_ok());
    let mut stripped = described.clone();
    stripped["session_create"]["parameters"]
        .as_array_mut()
        .expect("parameters")
        .retain(|item| item != "provider");
    let err = descriptor_gate(&stripped).expect_err("provider is required");
    assert_eq!(err, "descriptor lacks provider");
}

#[test]
fn event_tracker_handles_epochs_gaps_and_rate() {
    let clean = track(&[
        (1, 1, "message.start", 0),
        (1, 2, "message.delta", 10),
        (1, 3, "message.complete", 20),
    ]);
    assert_eq!(
        (
            clean.seq_repeats,
            clean.seq_decreases,
            clean.seq_gaps,
            clean.epoch_changes
        ),
        (0, 0, 0, 0)
    );

    let repeat = track(&[
        (1, 1, "message.delta", 0),
        (1, 2, "message.delta", 10),
        (1, 2, "message.delta", 20),
    ]);
    assert_eq!(repeat.seq_repeats, 1);
    assert_eq!((repeat.seq_decreases, repeat.seq_gaps), (0, 0));

    let decrease = track(&[
        (1, 1, "message.delta", 0),
        (1, 2, "message.delta", 10),
        (1, 1, "message.delta", 20),
    ]);
    assert_eq!(decrease.seq_decreases, 1);
    assert_eq!((decrease.seq_repeats, decrease.seq_gaps), (0, 0));

    let gap = track(&[
        (1, 1, "message.delta", 0),
        (1, 2, "message.delta", 10),
        (1, 5, "message.delta", 20),
    ]);
    assert_eq!(gap.seq_gaps, 1);
    assert_eq!((gap.seq_repeats, gap.seq_decreases), (0, 0));

    let epoch = track(&[
        (1, 1, "message.delta", 0),
        (1, 2, "message.delta", 10),
        (2, 1, "message.delta", 20),
    ]);
    assert_eq!(epoch.epoch_changes, 1);
    assert_eq!(
        (epoch.seq_repeats, epoch.seq_decreases, epoch.seq_gaps),
        (0, 0, 0)
    );

    let replay = track(&[(1, 1, "session.replay_gap", 0)]);
    assert_eq!(replay.replay_gaps, 1);
    assert_eq!(
        (replay.seq_repeats, replay.seq_decreases, replay.seq_gaps),
        (0, 0, 0)
    );

    let rate = track(&[
        (1, 1, "message.delta", 0),
        (1, 2, "message.delta", 100),
        (1, 3, "message.delta", 200),
        (1, 4, "message.delta", 300),
        (1, 5, "message.delta", 400),
        (1, 6, "message.delta", 1_500),
    ]);
    assert_eq!(rate.peak_events_per_s, 5);
    assert_eq!(
        (rate.seq_repeats, rate.seq_decreases, rate.seq_gaps),
        (0, 0, 0)
    );
}

#[test]
fn turn_clock_handles_event_before_reply() {
    let mut clock = TurnClock::new();
    clock.submit_written("session-1", 1_000);
    clock.event("session-1", "message.start", 1_005);
    clock.submit_admitted("session-1", "admission-1", 1_050);
    let duration = clock
        .duration_ms("admission-1", "message.start")
        .expect("matched to the admission");
    assert!(duration > 0, "duration is measured from the submit frame");
    assert_eq!(duration, 5);
}

#[tokio::test(start_paused = true)]
async fn turn_wait_times_out_without_completion() {
    let started = tokio::time::Instant::now();
    let result = wait_for_turn(std::time::Duration::from_secs(60)).await;
    assert!(started.elapsed() >= std::time::Duration::from_secs(60));
    assert!(
        result.is_err(),
        "a turn that never completes adds no sample"
    );
}

#[test]
fn health_rule_applies_each_criterion() {
    let control = passing("A", "shared", 1);
    let (ok, failed) = healthy(&control, Some(&control));
    assert!(ok && failed.is_empty(), "{failed:?}");

    for criterion in 1..=5 {
        let broken = break_criterion(&control, criterion);
        let (ok, failed) = healthy(&broken, Some(&control));
        assert!(!ok, "criterion {criterion}");
        assert_eq!(failed, vec![criterion], "criterion {criterion}");
    }

    let mut edge = control.clone();
    edge["resources"]["rss_mb"]["after"] = json!(120.0);
    let (ok, failed) = healthy(&edge, Some(&control));
    assert!(
        ok && failed.is_empty(),
        "20 percent is inside the band: {failed:?}"
    );
    let mut over = control.clone();
    over["resources"]["rss_mb"]["after"] = json!(121.0);
    let (ok, failed) = healthy(&over, Some(&control));
    assert!(!ok);
    assert_eq!(failed, vec![4]);

    let (ok, failed) = healthy(&control, None);
    assert!(!ok);
    assert_eq!(failed, vec![2]);

    let free = passing("B", "shared", 1);
    assert!(free["provider"].is_null());
    let (ok, failed) = healthy(&free, Some(&free));
    assert!(ok && failed.is_empty(), "{failed:?}");

    let dropped = passing("D", "shared", 10);
    let (ok, failed) = healthy(&dropped, Some(&control));
    assert!(ok && failed.is_empty(), "{failed:?}");
    let mut bad_receipt = dropped.clone();
    bad_receipt["recovery"]["unknown"] = json!(1);
    let (ok, failed) = healthy(&bad_receipt, Some(&control));
    assert!(!ok);
    assert_eq!(failed, vec![6]);

    let retained = passing("E", "shared", 20);
    let (ok, failed) = healthy(&retained, Some(&control));
    assert!(ok && failed.is_empty(), "{failed:?}");
    let mut refused = retained.clone();
    refused["recovery"]["submit_after_detach_refused"] = json!(false);
    let (ok, failed) = healthy(&refused, Some(&control));
    assert!(!ok);
    assert_eq!(failed, vec![6]);

    let workload = passing("C", "shared", 10);
    let (ok, failed) = healthy(&workload, Some(&control));
    assert!(ok && failed.is_empty(), "{failed:?}");
    let mut thin = workload.clone();
    thin["observations"]["tool_executions"] = json!(9);
    let (ok, failed) = healthy(&thin, Some(&control));
    assert!(!ok);
    assert_eq!(failed, vec![7]);

    let mut widths = vec![
        passing("A", "shared", 1),
        passing("A", "shared", 5),
        passing("A", "shared", 10),
        passing("A", "shared", 16),
    ];
    widths[2]["turns_failed"] = json!(1);
    assert_eq!(highest_healthy(&widths), Some(5));

    let mut width_one = vec![passing("A", "shared", 1), passing("A", "shared", 5)];
    width_one[0]["turns_failed"] = json!(1);
    assert_eq!(highest_healthy(&width_one), None);
}

#[test]
fn recommendations_follow_from_results() {
    let doc = load_results();
    assert_same_recommendations(&recommend(&doc), &recommendations_of(&doc));

    let switched = recommend(&layout_switch());
    assert_eq!(switched.topology.as_deref(), Some("per-session"));
    assert_eq!(switched.default_max_in_flight, Some(10));
    let memory = switched.memory_mb_per_session.expect("memory per session");
    assert!((memory - 20.0).abs() < 0.05, "{memory}");
    let mut cited = switched.cited_runs.expect("cited runs");
    cited.sort();
    assert_eq!(
        cited,
        vec![
            "A-per-session-10".to_string(),
            "A-per-session-20".to_string(),
            "A-shared-5".to_string(),
        ]
    );

    let none = recommend(&no_healthy_width());
    assert_eq!(none.default_max_in_flight, None);
    assert_eq!(none.topology, None);
    assert_eq!(none.memory_mb_per_session, None);
    assert_eq!(none.cited_runs, None);
}

#[test]
fn record_writer_merges_by_id() {
    let dir = tempfile::tempdir().expect("work directory");
    let mut writer = RecordWriter::open(dir.path());
    writer
        .merge("v0.22.4", &json!({"id": "A-shared-1", "notes": "first"}))
        .expect("first write");
    writer
        .merge("v0.22.4", &json!({"id": "A-shared-5", "notes": "five"}))
        .expect("second id");
    writer
        .merge("v0.22.4", &json!({"id": "A-shared-1", "notes": "replaced"}))
        .expect("replacement");
    let records = writer.records();
    assert_eq!(records.len(), 2);
    assert_eq!(notes_for(&records, "A-shared-1"), "replaced");
    assert_eq!(notes_for(&records, "A-shared-5"), "five");
    assert!(dir.path().join("results.json.lock").is_file());
    let refused = writer.merge("v0.22.5", &json!({"id": "A-shared-10", "notes": "other"}));
    assert!(refused.is_err(), "a different release is refused");
    let records = writer.records();
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|record| record["id"] != "A-shared-10"));
}

#[test]
fn results_contain_no_paths_tickets_or_private_names() {
    let results = read_results_text();
    let mut problems = privacy_problems("results.json", &results, false);
    problems.extend(privacy_problems("finding", &read_finding(), true));
    problems.extend(privacy_problems(
        "live_gateway_concurrency.rs",
        &read_repo_file("crates/hermes-client/tests/live_gateway_concurrency.rs"),
        false,
    ));
    problems.extend(privacy_problems(
        "concurrency_probe/mod.rs",
        &read_repo_file("crates/hermes-client/tests/concurrency_probe/mod.rs"),
        false,
    ));
    assert!(problems.is_empty(), "{problems:?}");
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_results_text() -> String {
    let path = repo_root().join("fixtures/gateway-concurrency/results.json");
    std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!("gateway concurrency results are missing: {err}");
    })
}

fn load_results() -> Value {
    serde_json::from_str(&read_results_text()).unwrap_or_else(|err| {
        panic!("gateway concurrency results are not json: {err}");
    })
}

fn pinned_tag() -> String {
    let path = repo_root().join("crates/hermes-contract/PINNED");
    let text = std::fs::read_to_string(&path).expect("pinned release file");
    text.lines()
        .find_map(|line| line.strip_prefix("tag="))
        .expect("pinned tag")
        .trim()
        .to_string()
}

fn read_repo_file(relative: &str) -> String {
    std::fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|_| panic!("missing {relative}"))
}

fn read_finding() -> String {
    let dir = repo_root().join("docs/analysis");
    let mut hits = Vec::new();
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|_| panic!("analysis directory is missing"));
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with("-hermes-gateway-concurrency.md") {
            hits.push(entry.path());
        }
    }
    assert_eq!(hits.len(), 1, "one gateway concurrency finding");
    std::fs::read_to_string(&hits[0]).expect("finding text")
}

fn assert_keys(value: &Value, expected: &[&str], what: &str) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{what} is not an object"));
    let mut actual: Vec<&str> = object.keys().map(String::as_str).collect();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected, "{what} keys");
}

fn expect_u64(value: &Value, what: &str) -> u64 {
    value
        .as_u64()
        .unwrap_or_else(|| panic!("{what} is not an integer"))
}

fn is_one_decimal(value: &Value) -> bool {
    let Some(number) = value.as_f64() else {
        return false;
    };
    if !number.is_finite() {
        return false;
    }
    let scaled = number * 10.0;
    (scaled - scaled.round()).abs() < 1e-6
}

fn expect_percentile(value: &Value, what: &str) {
    assert_keys(value, &["max", "n", "p50", "p95"], what);
    let n = expect_u64(&value["n"], what);
    let p50 = expect_u64(&value["p50"], what);
    let p95 = expect_u64(&value["p95"], what);
    let max = expect_u64(&value["max"], what);
    assert!(n >= 1, "{what} has no samples");
    assert!(p50 <= p95 && p95 <= max, "{what} is not ordered");
}

fn lowercase_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let Ok(year) = text[0..4].parse::<u16>() else {
        return false;
    };
    let Ok(month) = text[5..7].parse::<u8>() else {
        return false;
    };
    let Ok(day) = text[8..10].parse::<u8>() else {
        return false;
    };
    year >= 2000 && (1..=12).contains(&month) && (1..=31).contains(&day)
}

fn reason<'a>(run: &'a Value, field: &str) -> Option<&'a str> {
    run["unavailable"]
        .as_array()?
        .iter()
        .find(|entry| entry["field"] == field)
        .and_then(|entry| entry["reason"].as_str())
}

fn validate_recommendations(value: &Value) {
    assert_keys(
        value,
        &[
            "cited_runs",
            "default_max_in_flight",
            "memory_mb_per_session",
            "topology",
        ],
        "recommendations",
    );
    let nulls = [
        value["default_max_in_flight"].is_null(),
        value["topology"].is_null(),
        value["memory_mb_per_session"].is_null(),
        value["cited_runs"].is_null(),
    ];
    assert!(
        nulls.iter().all(|flag| *flag) || nulls.iter().all(|flag| !*flag),
        "recommendations are all null or all set"
    );
    if value["default_max_in_flight"].is_null() {
        return;
    }
    expect_u64(&value["default_max_in_flight"], "default_max_in_flight");
    let topology = value["topology"].as_str().expect("topology");
    assert!(matches!(topology, "shared" | "per-session"));
    assert!(value["memory_mb_per_session"].as_f64().is_some(), "memory");
    let cited = value["cited_runs"].as_array().expect("cited runs");
    assert!(!cited.is_empty(), "cited runs");
    for item in cited {
        assert!(item.as_str().is_some(), "cited run id");
    }
}

fn validate_run(run: &Value) {
    assert_keys(run, RUN_KEYS, "run record");
    let id = run["id"].as_str().expect("id");
    let series = run["series"].as_str().expect("series");
    let topology = run["topology"].as_str().expect("topology");
    let width = expect_u64(&run["width"], "width");
    assert!(matches!(series, "A" | "B" | "C" | "D" | "E"), "{id} series");
    assert!(
        matches!(topology, "shared" | "per-session"),
        "{id} topology"
    );
    assert_eq!(id, format!("{series}-{topology}-{width}"), "{id}");
    let repetitions = expect_u64(&run["repetitions"], "repetitions");
    assert_eq!(
        repetitions,
        if series == "A" { 3 } else { 1 },
        "{id} repetitions"
    );
    assert_keys(&run["run"], &["date", "run_id", "talaria_commit"], "run");
    let run_id = run["run"]["run_id"].as_str().expect("run id");
    assert!(lowercase_hex(run_id, 8), "{id} run id");
    let commit = run["run"]["talaria_commit"].as_str().expect("commit");
    assert!(lowercase_hex(commit, 12), "{id} commit");
    let date = run["run"]["date"].as_str().expect("date");
    assert!(is_date(date), "{id} date");
    match run["evidence"].as_str() {
        Some("live") => validate_live(run),
        Some("not-run") => validate_not_run(run),
        _ => panic!("{id} evidence"),
    }
}

fn validate_not_run(run: &Value) {
    let id = run["id"].as_str().unwrap_or("record");
    for field in PERCENTILE_FIELDS {
        assert!(run[*field].is_null(), "{id} {field}");
    }
    for field in COUNT_FIELDS {
        assert!(run[*field].is_null(), "{id} {field}");
    }
    for field in [
        "batch_wall_ms",
        "distinct_sessions",
        "events",
        "observations",
        "provider",
        "recovery",
        "resources",
        "rpc_errors",
    ] {
        assert!(run[field].is_null(), "{id} {field}");
    }
    assert!(
        run["unavailable"]
            .as_array()
            .is_some_and(|items| items.is_empty())
    );
    assert!(run["healthy"].is_null(), "{id} healthy");
    assert!(
        run["failed_criteria"]
            .as_array()
            .is_some_and(|items| items.is_empty())
    );
    let notes = run["notes"].as_str().expect("notes");
    assert!(is_fixed_not_run_phrase(notes), "{id} notes");
}

fn validate_live(run: &Value) {
    let series = run["series"].as_str().unwrap_or("");
    for field in PERCENTILE_FIELDS {
        if !run[*field].is_null() {
            expect_percentile(&run[*field], field);
        }
    }
    if !run["batch_wall_ms"].is_null() {
        expect_u64(&run["batch_wall_ms"], "batch_wall_ms");
    }
    for field in COUNT_FIELDS {
        if !run[*field].is_null() {
            expect_u64(&run[*field], field);
        }
    }
    expect_u64(&run["distinct_sessions"], "distinct_sessions");
    let errors = run["rpc_errors"].as_object().expect("rpc errors");
    for (key, value) in errors {
        assert!(valid_rpc_key(key), "{key}");
        expect_u64(value, "rpc count");
    }
    validate_events(&run["events"], series);
    if !run["resources"].is_null() {
        validate_resources(&run["resources"]);
    }
    validate_series_shape(run);
    validate_observations(&run["observations"], series);
    validate_unavailable(run);
    assert!(run["healthy"].as_bool().is_some(), "healthy");
    let mut previous = 0u64;
    for item in run["failed_criteria"].as_array().expect("failed criteria") {
        let number = expect_u64(item, "criterion");
        assert!((1..=7).contains(&number) && number > previous, "{number}");
        previous = number;
    }
    assert!(run["notes"].as_str().is_some(), "notes");
}

fn validate_events(value: &Value, series: &str) {
    assert_keys(value, EVENT_KEYS, "events");
    for key in EVENT_KEYS {
        if *key == "complete_per_admission" && series == "D" {
            assert!(value[*key].is_null(), "complete_per_admission");
            continue;
        }
        expect_u64(&value[*key], key);
    }
}

fn validate_resources(value: &Value) {
    assert_keys(
        value,
        &[
            "children_peak",
            "cpu_s_estimate",
            "rss_mb",
            "sampler_bad_lines",
        ],
        "resources",
    );
    assert_keys(&value["rss_mb"], &["after", "before", "peak"], "rss");
    for key in ["after", "before", "peak"] {
        assert!(is_one_decimal(&value["rss_mb"][key]), "{key}");
    }
    assert!(value["cpu_s_estimate"].as_f64().is_some(), "cpu");
    expect_u64(&value["children_peak"], "children_peak");
    expect_u64(&value["sampler_bad_lines"], "sampler_bad_lines");
}

fn validate_series_shape(run: &Value) {
    let series = run["series"].as_str().unwrap_or("");
    let id = run["id"].as_str().unwrap_or("record");
    if series == "B" {
        assert!(run["provider"].is_null(), "{id} provider");
    } else {
        validate_provider(&run["provider"]);
    }
    if series == "D" || series == "E" {
        validate_recovery(&run["recovery"], series);
    } else {
        assert!(run["recovery"].is_null(), "{id} recovery");
    }
    if series == "D" {
        for field in [
            "batch_wall_ms",
            "turn_ms",
            "turns_completed",
            "turns_failed",
        ] {
            assert!(run[field].is_null(), "{id} {field}");
            assert_eq!(
                reason(run, field),
                Some("connection dropped by design"),
                "{id} {field}"
            );
        }
        assert_eq!(
            reason(run, "complete_per_admission"),
            Some("connection dropped by design"),
            "{id} complete_per_admission"
        );
        assert!(run["start_delay_ms"].is_object(), "{id} start delay");
    }
}

fn validate_provider(value: &Value) {
    assert_keys(
        value,
        &[
            "aux_requests",
            "turn_requests",
            "turn_requests_peak_min",
            "unexpected_routes",
        ],
        "provider",
    );
    for key in [
        "aux_requests",
        "turn_requests",
        "turn_requests_peak_min",
        "unexpected_routes",
    ] {
        expect_u64(&value[key], key);
    }
}

fn validate_recovery(value: &Value, series: &str) {
    assert_keys(value, RECOVERY_KEYS, "recovery");
    for key in [
        "replay_truncated",
        "terminal_with_result",
        "terminal_without_result",
        "unknown",
        "unresolved",
    ] {
        expect_u64(&value[key], key);
    }
    expect_percentile(&value["receipt_ms"], "receipt_ms");
    if !value["resume_ms"].is_null() {
        expect_percentile(&value["resume_ms"], "resume_ms");
    }
    if !value["next_turn_ms"].is_null() {
        expect_u64(&value["next_turn_ms"], "next_turn_ms");
    }
    match series {
        "E" => assert!(value["submit_after_detach_refused"].is_boolean()),
        "D" => assert!(value["submit_after_detach_refused"].is_null()),
        _ => unreachable!(),
    }
    if !value["eviction_evidence"].is_null() {
        assert!(
            value["eviction_evidence"].as_str().is_some(),
            "eviction evidence"
        );
    }
}

fn validate_observations(value: &Value, series: &str) {
    assert_keys(value, OBSERVATION_KEYS, "observations");
    match &value["tool_names"] {
        Value::Null => {}
        Value::Array(names) => {
            assert!(
                names.iter().all(|name| name.as_str().is_some()),
                "tool name"
            );
        }
        _ => panic!("tool names"),
    }
    if series == "C" {
        if !value["tool_executions"].is_null() {
            expect_u64(&value["tool_executions"], "tool_executions");
        }
    } else {
        assert!(value["tool_executions"].is_null(), "tool_executions");
    }
    if series == "B" {
        if let Some(keys) = value["rate_limit_keys"].as_array() {
            assert!(
                keys.iter().all(|key| key.as_str().is_some()),
                "rate limit key"
            );
        } else {
            assert!(value["rate_limit_keys"].is_null(), "rate limit keys");
        }
    } else {
        assert!(value["rate_limit_keys"].is_null(), "rate limit keys");
    }
    let examples = value["failure_examples"]
        .as_array()
        .expect("failure examples");
    assert!(examples.len() <= 3, "failure examples");
    for example in examples {
        assert_keys(example, &["kind", "status"], "failure example");
        assert!(example["status"].as_str().is_some(), "status");
        let kind = example["kind"].as_str().expect("kind");
        assert!(valid_kind(kind), "{kind}");
    }
    assert_keys(
        &value["admission_statuses"],
        &["queued", "started", "terminal", "unknown"],
        "admission statuses",
    );
    for key in ["queued", "started", "terminal", "unknown"] {
        expect_u64(&value["admission_statuses"][key], key);
    }
}

fn validate_unavailable(run: &Value) {
    let entries = run["unavailable"].as_array().expect("unavailable");
    let mut seen = BTreeSet::new();
    for entry in entries {
        assert_keys(entry, &["field", "reason"], "unavailable entry");
        let field = entry["field"].as_str().expect("field");
        let reason = entry["reason"].as_str().expect("reason");
        assert!(!field.is_empty() && !reason.is_empty(), "unavailable entry");
        assert!(
            seen.insert(field.to_string()),
            "duplicate unavailable field"
        );
    }
    let Some(object) = run.as_object() else {
        return;
    };
    for (key, value) in object {
        if value.is_null() {
            assert!(seen.contains(key), "{key} is null and unlisted");
        }
    }
}

fn assert_tool_name_placement(runs: &[Value]) {
    let mut seen = BTreeSet::new();
    for run in runs {
        if run["evidence"] != "live" {
            continue;
        }
        let series = run["series"].as_str().expect("series");
        let names = &run["observations"]["tool_names"];
        if seen.insert(series.to_string()) {
            assert!(names.is_array(), "{series} names its tools once");
        } else {
            assert!(names.is_null(), "{series} names its tools only once");
        }
    }
}

fn valid_rpc_key(key: &str) -> bool {
    let Some((method, reason)) = key.split_once(':') else {
        return false;
    };
    !method.is_empty() && !reason.is_empty() && !method.contains(':')
}

fn valid_kind(kind: &str) -> bool {
    !kind.is_empty()
        && kind.len() <= 40
        && kind.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'.'
        })
}

fn reason_code(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'.'
        })
}

fn is_fixed_not_run_phrase(notes: &str) -> bool {
    matches!(
        notes,
        "above the operator's width cap"
            | "operator waived"
            | "stopped after an unhealthy width"
            | "timebox reached"
            | "unsupported_managed_policy"
    ) || notes
        .strip_prefix("toolset refused: ")
        .is_some_and(reason_code)
        || notes
            .strip_prefix("loopback provider refused: ")
            .is_some_and(reason_code)
        || notes
            .strip_prefix("stopped: ")
            .is_some_and(|rest| !rest.is_empty() && rest == rest.trim())
}

fn allowed_not_run_phrase(series: &str, notes: &str) -> bool {
    match notes {
        "above the operator's width cap" | "stopped after an unhealthy width" => true,
        "operator waived" | "unsupported_managed_policy" => series == "B",
        _ => notes
            .strip_prefix("toolset refused: ")
            .is_some_and(|reason| series == "C" && reason_code(reason)),
    }
}

fn evidence_gate(doc: &Value) -> bool {
    let Some(runs) = doc["runs"].as_array() else {
        return false;
    };
    for id in ["A-shared-1", "D-shared-10", "E-shared-20"] {
        let Some(run) = runs.iter().find(|run| run["id"] == id) else {
            return false;
        };
        if run["evidence"] != "live" {
            return false;
        }
    }
    runs.iter().all(|run| {
        if run["evidence"] != "not-run" {
            return true;
        }
        let series = run["series"].as_str().unwrap_or("");
        let notes = run["notes"].as_str().unwrap_or("");
        allowed_not_run_phrase(series, notes)
    })
}

fn skeleton(id: &str, evidence: &str, notes: &str) -> Value {
    let series = id.split('-').next().unwrap_or("");
    json!({
        "id": id,
        "series": series,
        "evidence": evidence,
        "notes": notes,
    })
}

fn document(runs: Vec<Value>) -> Value {
    json!({
        "hermes_release": "v0.22.4",
        "execution_mode": "in-process",
        "runs": runs,
        "recommendations": {
            "default_max_in_flight": null,
            "topology": null,
            "memory_mb_per_session": null,
            "cited_runs": null,
        },
    })
}

fn live_matrix_doc() -> Value {
    document(MATRIX.iter().map(|id| skeleton(id, "live", "")).collect())
}

fn with_note(id: &str, notes: &str) -> Value {
    let mut doc = live_matrix_doc();
    for run in doc["runs"].as_array_mut().expect("runs") {
        if run["id"] == id {
            run["evidence"] = json!("not-run");
            run["notes"] = json!(notes);
        }
    }
    doc
}

fn all_not_run() -> Value {
    document(
        MATRIX
            .iter()
            .map(|id| skeleton(id, "not-run", "stopped after an unhealthy width"))
            .collect(),
    )
}

fn missing_d() -> Value {
    document(
        MATRIX
            .iter()
            .filter(|id| **id != "D-shared-10")
            .map(|id| skeleton(id, "live", ""))
            .collect(),
    )
}

fn track(frames: &[(u64, u64, &str, u64)]) -> concurrency_probe::EventStats {
    let mut tracker = EventTracker::new();
    for (epoch, seq, kind, at_ms) in frames {
        tracker.push("session-1", *epoch, *seq, kind, *at_ms);
    }
    tracker.stats()
}

fn has_finish(body: &str, reason: &str) -> bool {
    body.contains(&format!("\"finish_reason\":\"{reason}\""))
        || body.contains(&format!("\"finish_reason\": \"{reason}\""))
}

fn ready_body() -> String {
    serde_json::to_string(&json!({
        "model": "talaria-probe-stub",
        "stream": true,
        "messages": [{"role": "user", "content": "Reply with the single word READY."}]
    }))
    .expect("ready body")
}

fn tool_body() -> String {
    serde_json::to_string(&json!({
        "model": "talaria-probe-stub",
        "stream": true,
        "messages": [{
            "role": "tool",
            "content": "{\"todos\":[{\"id\":\"1\",\"content\":\"probe\",\"status\":\"pending\"}]}"
        }]
    }))
    .expect("tool body")
}

fn aux_body() -> String {
    serde_json::to_string(&json!({
        "model": "talaria-probe-stub",
        "stream": false,
        "messages": [{"role": "user", "content": "ping"}]
    }))
    .expect("aux body")
}

fn http_request(method: &str, path: &str, body: Option<&str>) -> String {
    let body = body.unwrap_or("");
    format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn http_exchange(addr: std::net::SocketAddr, request: &str) -> (u16, String) {
    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.expect("read");
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, text)
}

fn object_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
    keys.sort();
    keys
}

fn assert_absent(value: &Value) {
    for key in ["cwd", "profile", "title", "unattended", "yolo"] {
        assert!(value.get(key).is_none(), "{key}");
    }
}

fn assert_create(
    series: char,
    run_id: &str,
    record_id: &str,
    session: u32,
    port: u16,
    value: &Value,
) {
    assert_eq!(
        value["request_id"],
        format!("talaria-probe:{run_id}:{record_id}:{session}")
    );
    assert_eq!(value["source"], "tool");
    assert_eq!(value["hidden"], true);
    let toolsets = if series == 'C' {
        json!(["todo"])
    } else {
        json!([])
    };
    assert_eq!(value["toolsets"], toolsets);
    if series == 'B' {
        assert_eq!(value["model"], "minimax-m3.1-flash");
        assert_eq!(value["provider"], "ollama-cloud");
        assert!(value.get("base_url").is_none());
        assert!(value.get("api_key").is_none());
        assert_eq!(
            object_keys(value),
            strings(&[
                "hidden",
                "model",
                "provider",
                "request_id",
                "source",
                "toolsets"
            ])
        );
    } else {
        assert_eq!(value["model"], "talaria-probe-stub");
        assert_eq!(value["provider"], "custom");
        assert_eq!(value["base_url"], format!("http://127.0.0.1:{port}/v1"));
        assert_eq!(value["api_key"], "talaria-probe-placeholder");
        assert_eq!(
            object_keys(value),
            strings(&[
                "api_key",
                "base_url",
                "hidden",
                "model",
                "provider",
                "request_id",
                "source",
                "toolsets",
            ])
        );
    }
    assert_absent(value);
}

fn assert_submit(
    session_id: &str,
    run_id: &str,
    record_id: &str,
    session: u32,
    repetition: u32,
    value: &Value,
) {
    assert_eq!(value["session_id"], session_id);
    assert_eq!(
        value["submission_id"],
        format!("talaria-probe:{run_id}:{record_id}:{session}:{repetition}")
    );
    assert_eq!(value["text"], "Reply with the single word READY.");
    assert_eq!(value["finite"], true);
    assert_eq!(
        object_keys(value),
        strings(&["finite", "session_id", "submission_id", "text"])
    );
    assert_absent(value);
}

fn strings(keys: &[&str]) -> Vec<String> {
    keys.iter().map(|key| (*key).to_string()).collect()
}

fn pinned_descriptor() -> Value {
    json!({
        "session_create": {
            "sources": ["acp", "cli", "gui", "oneshot", "tool", "tui"],
            "parameters": CREATE_PARAMETERS,
        }
    })
}

fn percentile_value(p95: u64, max: u64) -> Value {
    json!({"n": 4, "p50": 10, "p95": p95, "max": max})
}

fn passing(series: &str, topology: &str, width: u64) -> Value {
    let repetitions = if series == "A" { 3 } else { 1 };
    let admissions = width * repetitions;
    let mut unavailable = Vec::new();
    let next_turn_ms = if series == "E" {
        json!(80)
    } else {
        Value::Null
    };
    let detach_refused = if series == "E" {
        json!(true)
    } else {
        Value::Null
    };
    let recovery = if series == "D" || series == "E" {
        json!({
            "terminal_with_result": width,
            "terminal_without_result": 0,
            "unknown": 0,
            "unresolved": 0,
            "receipt_ms": {"n": width, "p50": 100, "p95": 200, "max": 250},
            "resume_ms": {"n": width, "p50": 40, "p95": 80, "max": 90},
            "next_turn_ms": next_turn_ms,
            "replay_truncated": 0,
            "submit_after_detach_refused": detach_refused,
            "eviction_evidence": Value::Null,
        })
    } else {
        unavailable.push(json!({"field": "recovery", "reason": "not in this series"}));
        Value::Null
    };
    let provider = if series == "B" {
        unavailable.push(json!({"field": "provider", "reason": "real model"}));
        Value::Null
    } else {
        json!({
            "turn_requests": admissions,
            "turn_requests_peak_min": width,
            "aux_requests": 0,
            "unexpected_routes": 0,
        })
    };
    let (turn_ms, batch_wall_ms, turns_completed, turns_failed, complete) = if series == "D" {
        for field in [
            "batch_wall_ms",
            "complete_per_admission",
            "turn_ms",
            "turns_completed",
            "turns_failed",
        ] {
            unavailable.push(json!({"field": field, "reason": "connection dropped by design"}));
        }
        (
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        )
    } else {
        (
            percentile_value(30, 40),
            json!(1_000),
            json!(admissions),
            json!(0),
            json!(admissions),
        )
    };
    let tool_names = if width == 1 || series != "A" {
        json!(["todo_list"])
    } else {
        Value::Null
    };
    let tool_executions = if series == "C" {
        json!(width)
    } else {
        Value::Null
    };
    json!({
        "id": format!("{series}-{topology}-{width}"),
        "series": series,
        "topology": topology,
        "width": width,
        "repetitions": repetitions,
        "evidence": "live",
        "run": {"run_id": "ab12cd34", "talaria_commit": "abc123def456", "date": "2026-10-27"},
        "create_ms": percentile_value(20, 30),
        "submit_ms": percentile_value(20, 30),
        "start_delay_ms": percentile_value(20, 30),
        "first_delta_ms": percentile_value(20, 30),
        "turn_ms": turn_ms,
        "foreground_rtt_ms": percentile_value(50, 250),
        "batch_wall_ms": batch_wall_ms,
        "turns_completed": turns_completed,
        "turns_failed": turns_failed,
        "admissions_unknown": 0,
        "create_refused": 0,
        "timeouts": 0,
        "foreground_errors": 0,
        "distinct_sessions": width,
        "rpc_errors": {},
        "events": {
            "observed": admissions,
            "seq_repeats": 0,
            "seq_decreases": 0,
            "seq_gaps": 0,
            "epoch_changes": 0,
            "replay_gaps": 0,
            "unresolved_gaps": 0,
            "peak_events_per_s": 1,
            "complete_per_admission": complete,
        },
        "resources": {
            "rss_mb": {"before": 100.0, "peak": 110.0, "after": 100.0},
            "cpu_s_estimate": 1.0,
            "children_peak": 0,
            "sampler_bad_lines": 0,
        },
        "provider": provider,
        "recovery": recovery,
        "observations": {
            "tool_names": tool_names,
            "tool_executions": tool_executions,
            "rate_limit_keys": Value::Null,
            "failure_examples": [],
            "admission_statuses": {"queued": 0, "started": 0, "unknown": 0, "terminal": admissions},
        },
        "unavailable": unavailable,
        "healthy": true,
        "failed_criteria": [],
        "notes": "",
    })
}

fn break_criterion(record: &Value, criterion: u8) -> Value {
    let mut record = record.clone();
    match criterion {
        1 => record["turns_failed"] = json!(1),
        2 => {
            record["start_delay_ms"]["p95"] = json!(1_021);
            record["start_delay_ms"]["max"] = json!(1_021);
        }
        3 => {
            record["foreground_rtt_ms"]["p95"] = json!(51);
            record["foreground_rtt_ms"]["max"] = json!(51);
        }
        4 => record["resources"]["rss_mb"]["after"] = json!(150.0),
        5 => record["events"]["seq_repeats"] = json!(1),
        _ => unreachable!(),
    }
    record
}

fn with_memory(mut record: Value, before: f64, peak: f64) -> Value {
    record["resources"]["rss_mb"]["before"] = json!(before);
    record["resources"]["rss_mb"]["peak"] = json!(peak);
    record["resources"]["rss_mb"]["after"] = json!(before);
    record
}

fn layout_switch() -> Value {
    let mut runs = Vec::new();
    for width in [1, 5, 10, 16, 20] {
        let mut shared = passing("A", "shared", width);
        if width == 10 {
            shared["turns_failed"] = json!(1);
        }
        runs.push(shared);
        let mut per_session = passing("A", "per-session", width);
        if width == 10 {
            per_session = with_memory(per_session, 100.0, 300.0);
        }
        runs.push(per_session);
    }
    document(runs)
}

fn no_healthy_width() -> Value {
    let mut shared_one = passing("A", "shared", 1);
    shared_one["turns_failed"] = json!(1);
    let mut per_one = passing("A", "per-session", 1);
    per_one["turns_failed"] = json!(1);
    document(vec![
        shared_one,
        passing("A", "shared", 5),
        per_one,
        passing("A", "per-session", 5),
    ])
}

fn recommendations_of(doc: &Value) -> Recommendations {
    let value = &doc["recommendations"];
    Recommendations {
        default_max_in_flight: value["default_max_in_flight"]
            .as_u64()
            .map(|number| number as u32),
        topology: value["topology"].as_str().map(str::to_string),
        memory_mb_per_session: value["memory_mb_per_session"].as_f64(),
        cited_runs: value["cited_runs"].as_array().map(|items| {
            items
                .iter()
                .map(|item| item.as_str().expect("cited run").to_string())
                .collect()
        }),
    }
}

fn assert_same_recommendations(actual: &Recommendations, expected: &Recommendations) {
    assert_eq!(actual.default_max_in_flight, expected.default_max_in_flight);
    assert_eq!(actual.topology, expected.topology);
    match (actual.memory_mb_per_session, expected.memory_mb_per_session) {
        (None, None) => {}
        (Some(left), Some(right)) => assert!((left - right).abs() < 0.05, "{left} {right}"),
        _ => panic!("memory per session does not match the file"),
    }
    match (&actual.cited_runs, &expected.cited_runs) {
        (None, None) => {}
        (Some(left), Some(right)) => {
            let mut left = left.clone();
            let mut right = right.clone();
            left.sort();
            right.sort();
            assert_eq!(left, right);
        }
        _ => panic!("cited runs do not match the file"),
    }
}

fn notes_for(records: &[Value], id: &str) -> String {
    records
        .iter()
        .find(|record| record["id"] == id)
        .and_then(|record| record["notes"].as_str())
        .unwrap_or("")
        .to_string()
}

fn path_markers() -> Vec<String> {
    ["Users", "home", "private", "tmp", "var", "root"]
        .into_iter()
        .map(|name| format!("/{name}/"))
        .chain(std::iter::once(format!("{}{}", '~', '/')))
        .collect()
}

fn ticket_prefix() -> String {
    format!("{}-{}.", "hermes-gateway", "ticket")
}

fn privacy_problems(label: &str, text: &str, allow_commit: bool) -> Vec<String> {
    let mut problems = Vec::new();
    for marker in path_markers() {
        if text.contains(&marker) {
            problems.push(format!("{label}: forbidden path"));
            break;
        }
    }
    if has_drive_path(text) {
        problems.push(format!("{label}: forbidden path"));
    }
    if text.contains(&ticket_prefix()) || has_long_token(text, allow_commit) {
        problems.push(format!("{label}: forbidden token"));
    }
    for name in private_names() {
        if contains_word(text, &name) {
            problems.push(format!("{label}: private name"));
            break;
        }
    }
    problems
}

fn has_drive_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    for index in 0..bytes.len().saturating_sub(2) {
        let boundary = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
        if boundary
            && bytes[index].is_ascii_alphabetic()
            && bytes[index + 1] == b':'
            && bytes.get(index + 2) == Some(&b'\\')
        {
            return true;
        }
    }
    false
}

fn has_long_token(text: &str, allow_commit: bool) -> bool {
    let mut run = String::new();
    let mut flagged = false;
    for byte in text.bytes().chain(std::iter::once(b' ')) {
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' {
            run.push(byte as char);
            continue;
        }
        if run.len() >= 40 && !(allow_commit && is_commit_id(&run)) {
            flagged = true;
        }
        run.clear();
    }
    flagged
}

fn is_commit_id(text: &str) -> bool {
    text.len() == 40
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn private_names() -> Vec<String> {
    let mut names = Vec::new();
    for key in ["USER", "LOGNAME"] {
        if let Ok(value) = std::env::var(key)
            && !value.is_empty()
        {
            names.push(value);
        }
    }
    if let Ok(list) = std::env::var("TALARIA_PRIVATE_NAMES") {
        names.extend(
            list.split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string),
        );
    }
    names
}

fn contains_word(text: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    text.match_indices(word).any(|(index, _)| {
        let bytes = text.as_bytes();
        let before = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
        let after = index + word.len();
        let after_ok = after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
        before && after_ok
    })
}
