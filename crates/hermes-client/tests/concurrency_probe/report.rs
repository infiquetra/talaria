//! Record schema, persistence, evidence gates and deterministic recommendations.
use super::*;
use std::path::Path;

fn num(v: &Value, key: &str) -> Option<u64> {
    v[key].as_u64()
}
fn zero(v: &Value, key: &str) -> bool {
    num(v, key) == Some(0)
}
pub fn healthy(record: &Value, baseline: &Value) -> (bool, Vec<u8>) {
    if record["evidence"] != "live" {
        return (false, vec![1]);
    }
    let mut failed = Vec::new();
    let series = record["series"].as_str().unwrap_or("");
    let width = num(record, "width").unwrap_or(0);
    let turns = width * num(record, "repetitions").unwrap_or(0);
    if !["create_refused", "admissions_unknown", "timeouts"]
        .iter()
        .all(|k| zero(record, k))
        || record["rpc_errors"]
            .as_object()
            .is_none_or(|v| !v.is_empty())
        || num(record, "distinct_sessions") != Some(width)
        || (series != "D"
            && (!zero(record, "turns_failed") || num(record, "turns_completed") != Some(turns)))
    {
        failed.push(1);
    }
    if baseline["evidence"] != "live"
        || record
            .pointer("/start_delay_ms/p95")
            .and_then(Value::as_u64)
            .zip(
                baseline
                    .pointer("/start_delay_ms/p95")
                    .and_then(Value::as_u64),
            )
            .is_none_or(|(v, b)| v > b + 1000)
        || (series != "B"
            && record
                .pointer("/provider/turn_requests_peak_min")
                .and_then(Value::as_u64)
                != Some(width))
    {
        failed.push(2);
    }
    if !zero(record, "foreground_errors")
        || record
            .pointer("/foreground_rtt_ms/p95")
            .and_then(Value::as_u64)
            .is_none_or(|v| v > 50)
        || record
            .pointer("/foreground_rtt_ms/max")
            .and_then(Value::as_u64)
            .is_none_or(|v| v > 250)
    {
        failed.push(3);
    }
    let before = record
        .pointer("/resources/rss_mb/before")
        .and_then(Value::as_f64);
    let after = record
        .pointer("/resources/rss_mb/after")
        .and_then(Value::as_f64);
    if before
        .zip(after)
        .is_none_or(|(b, a)| b <= 0.0 || (a - b).abs() > b * 0.2)
    {
        failed.push(4);
    }
    if series != "D"
        && (!["seq_repeats", "seq_decreases", "unresolved_gaps"]
            .iter()
            .all(|k| zero(&record["events"], k))
            || num(&record["events"], "complete_per_admission") != Some(turns))
    {
        failed.push(5);
    }
    if ["D", "E"].contains(&series) {
        let r = &record["recovery"];
        if num(r, "terminal_with_result") != Some(turns)
            || !["terminal_without_result", "unknown", "unresolved"]
                .iter()
                .all(|k| zero(r, k))
            || (series == "E"
                && (r["submit_after_detach_refused"] != true
                    || r["next_turn_ms"].as_u64().is_none()))
        {
            failed.push(6);
        }
    }
    if series == "C"
        && (num(&record["observations"], "tool_executions") != Some(width)
            || num(record, "turns_completed") != Some(turns)
            || !zero(record, "turns_failed"))
    {
        failed.push(7);
    }
    (failed.is_empty(), failed)
}
pub fn highest_healthy(records: &[Value]) -> Option<u32> {
    let mut records: Vec<_> = records.iter().collect();
    records.sort_by_key(|r| num(r, "width"));
    let mut high = None;
    let baseline = records
        .iter()
        .find(|r| num(r, "width") == Some(1))
        .copied()
        .unwrap_or(&Value::Null);
    for width in WIDTHS {
        let Some(r) = records
            .iter()
            .find(|r| num(r, "width") == Some(u64::from(width)))
        else {
            break;
        };
        if r["evidence"] != "live" || !healthy(r, baseline).0 {
            break;
        }
        high = Some(width);
    }
    high
}
pub fn recommend(results: &Value) -> Value {
    let records = results["runs"].as_array().cloned().unwrap_or_default();
    let group = |topology: &str| {
        records
            .iter()
            .filter(|r| r["series"] == "A" && r["topology"] == topology)
            .cloned()
            .collect::<Vec<_>>()
    };
    let hs = highest_healthy(&group("shared"));
    let hp = highest_healthy(&group("per-session"));
    if hs.is_none() && hp.is_none() {
        return json!({"default_max_in_flight":null,"topology":null,"memory_mb_per_session":null,"cited_runs":null});
    }
    let (topology, high) = if hs >= hp {
        ("shared", hs.unwrap())
    } else {
        ("per-session", hp.unwrap())
    };
    let width = high.min(10);
    let selected = records
        .iter()
        .find(|r| r["series"] == "A" && r["topology"] == topology && r["width"] == width)
        .unwrap();
    let memory = selected
        .pointer("/resources/rss_mb/peak")
        .and_then(Value::as_f64)
        .zip(
            selected
                .pointer("/resources/rss_mb/before")
                .and_then(Value::as_f64),
        )
        .map(|(p, b)| (p - b) / f64::from(width));
    let mut cited = BTreeSet::new();
    for (t, h) in [("shared", hs), ("per-session", hp)] {
        if let Some(h) = h {
            cited.insert(format!("A-{t}-{h}"));
        }
    }
    cited.insert(format!("A-{topology}-{width}"));
    json!({"default_max_in_flight":width,"topology":topology,"memory_mb_per_session":memory,"cited_runs":cited})
}

pub fn matrix() -> Vec<(String, String, u32)> {
    let mut out = Vec::new();
    for t in ["shared", "per-session"] {
        for w in WIDTHS {
            out.push(("A".into(), t.into(), w));
        }
    }
    for w in [1, 5, 10, 16] {
        out.push(("B".into(), "shared".into(), w));
    }
    for (s, w) in [("C", 10), ("D", 10), ("E", 20)] {
        out.push((s.into(), "shared".into(), w));
    }
    out
}
pub fn empty_record(series: &str, topology: &str, width: u32, run: Value) -> Value {
    json!({"id":format!("{series}-{topology}-{width}"),"series":series,"topology":topology,"width":width,"repetitions":if series=="A" {3}else{1},"evidence":"not-run","run":run,
        "create_ms":null,"submit_ms":null,"start_delay_ms":null,"first_delta_ms":null,"turn_ms":null,"foreground_rtt_ms":null,"batch_wall_ms":null,
        "turns_completed":null,"turns_failed":null,"admissions_unknown":null,"create_refused":null,"timeouts":null,"foreground_errors":null,"distinct_sessions":null,"rpc_errors":null,
        "events":null,"resources":null,"provider":null,"recovery":null,"observations":null,"unavailable":[],"healthy":null,"failed_criteria":[],"notes":"timebox reached"})
}
fn valid_percentile(v: &Value) -> bool {
    v.is_null()
        || v.as_object().is_some_and(|o| {
            o.len() == 4
                && ["n", "p50", "p95", "max"]
                    .iter()
                    .all(|k| o.get(*k).is_some_and(Value::is_u64))
                && num(v, "n").unwrap_or(0) > 0
                && num(v, "p50") <= num(v, "p95")
                && num(v, "p95") <= num(v, "max")
        })
}
fn safe_code(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}
pub fn allowed_not_run(series: &str, notes: &str) -> bool {
    [
        "stopped after an unhealthy width",
        "above the operator's width cap",
    ]
    .contains(&notes)
        || series == "B" && ["operator waived", "unsupported_managed_policy"].contains(&notes)
        || series == "C"
            && notes
                .strip_prefix("toolset refused: ")
                .is_some_and(safe_code)
}
fn fixed_notes(s: &str, series: &str) -> bool {
    s.is_empty()
        || s == "cleanup incomplete"
        || s == "timebox reached"
        || s == "stopped: live evidence pending"
        || s == "stopped: descriptor call failed"
        || allowed_not_run(series, s)
        || s.strip_prefix("loopback provider refused: ")
            .is_some_and(safe_code)
        || s.strip_prefix("stopped: ").is_some_and(|s| {
            s == "turn request classification failed"
                || s.strip_prefix("descriptor lacks ")
                    .is_some_and(|s| s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
        })
}
pub fn validate_record(r: &Value) -> Result<(), String> {
    let series = r["series"].as_str().ok_or("series")?;
    let topology = r["topology"].as_str().ok_or("topology")?;
    let width = u32::try_from(r["width"].as_u64().ok_or("width")?).map_err(|_| "width")?;
    if !matrix().contains(&(series.into(), topology.into(), width)) {
        return Err("matrix entry".into());
    }
    let template = empty_record(series, topology, width, Value::Null);
    let object = r.as_object().ok_or("record object")?;
    if object.keys().collect::<BTreeSet<_>>()
        != template
            .as_object()
            .unwrap()
            .keys()
            .collect::<BTreeSet<_>>()
    {
        return Err("record keys".into());
    }
    if r["id"] != format!("{series}-{topology}-{width}")
        || r["repetitions"] != if series == "A" { 3 } else { 1 }
    {
        return Err("record identity".into());
    }
    let run = &r["run"];
    for (key, len) in [("run_id", 8), ("talaria_commit", 12)] {
        if !run[key].as_str().is_some_and(|s| {
            s.len() == len
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) {
            return Err(format!("run.{key}"));
        }
    }
    if run.as_object().is_none_or(|o| o.len() != 3)
        || !run["date"].as_str().is_some_and(|s| {
            s.len() == 10
                && s.bytes().enumerate().all(|(i, b)| {
                    if [4, 7].contains(&i) {
                        b == b'-'
                    } else {
                        b.is_ascii_digit()
                    }
                })
        })
    {
        return Err("run date".into());
    }
    if !r["notes"].as_str().is_some_and(|s| fixed_notes(s, series)) {
        return Err("notes".into());
    }
    let unavailable = r["unavailable"].as_array().ok_or("unavailable")?;
    let failed = r["failed_criteria"].as_array().ok_or("failed criteria")?;
    let mut last = 0;
    for v in failed {
        let n = v.as_u64().ok_or("criterion")?;
        if !(1..=7).contains(&n) || n <= last {
            return Err("criterion order".into());
        }
        last = n;
    }
    if r["evidence"] == "not-run" {
        for (k, v) in template.as_object().unwrap() {
            if v.is_null() && k != "run" && !r[k].is_null() {
                return Err(format!("not-run {k}"));
            }
        }
        if !unavailable.is_empty()
            || !failed.is_empty()
            || r["notes"] == ""
            || r["notes"] == "cleanup incomplete"
        {
            return Err("not-run details".into());
        }
        return Ok(());
    }
    if r["evidence"] != "live" || !r["healthy"].is_boolean() {
        return Err("evidence".into());
    }
    if r["healthy"] != failed.is_empty() {
        return Err("health verdict".into());
    }
    for k in [
        "create_ms",
        "submit_ms",
        "start_delay_ms",
        "first_delta_ms",
        "turn_ms",
        "foreground_rtt_ms",
    ] {
        if !valid_percentile(&r[k]) {
            return Err(k.into());
        }
    }
    for k in [
        "batch_wall_ms",
        "turns_completed",
        "turns_failed",
        "admissions_unknown",
        "create_refused",
        "timeouts",
        "foreground_errors",
    ] {
        if !r[k].is_null() && !r[k].is_u64() {
            return Err(k.into());
        }
    }
    if !r["distinct_sessions"].is_u64()
        || !r["rpc_errors"].as_object().is_some_and(|o| {
            o.iter().all(|(k, v)| {
                k.split_once(':')
                    .is_some_and(|(method, reason)| safe_code(method) && safe_code(reason))
                    && v.is_u64()
            })
        })
    {
        return Err("counts".into());
    }
    for (k, v) in object {
        if v.is_null()
            && !unavailable
                .iter()
                .any(|u| u["field"] == *k && u["reason"].as_str().is_some_and(|s| !s.is_empty()))
        {
            return Err(format!("unavailable {k}"));
        }
    }
    let mut fields = BTreeSet::new();
    for u in unavailable {
        if u.as_object().is_none_or(|o| o.len() != 2)
            || u["field"].as_str().is_none()
            || !u["reason"].as_str().is_some_and(|s| {
                [
                    "no samples",
                    "not applicable",
                    "external provider",
                    "sampler failed",
                    "connection dropped by design",
                    "no baseline",
                ]
                .contains(&s)
            })
            || !fields.insert(u["field"].as_str().unwrap_or(""))
            || !u["field"].as_str().is_some_and(|k| {
                template.get(k).is_some()
                    || [
                        "complete_per_admission",
                        "baseline",
                        "recovery.receipt_ms",
                        "recovery.resume_ms",
                        "recovery.next_turn_ms",
                    ]
                    .contains(&k)
            })
        {
            return Err("unavailable entry".into());
        }
    }
    let event_keys = [
        "observed",
        "seq_repeats",
        "seq_decreases",
        "seq_gaps",
        "epoch_changes",
        "replay_gaps",
        "unresolved_gaps",
        "peak_events_per_s",
        "complete_per_admission",
    ];
    if !exact_numeric(
        &r["events"],
        &event_keys,
        if series == "D" {
            Some("complete_per_admission")
        } else {
            None
        },
    ) {
        return Err("events".into());
    }
    if !r["resources"].is_null() {
        let x = &r["resources"];
        if x.as_object().is_none_or(|o| o.len() != 4)
            || x["rss_mb"].as_object().is_none_or(|o| o.len() != 3)
            || !["before", "peak", "after"].iter().all(|k| {
                x["rss_mb"][k]
                    .as_f64()
                    .is_some_and(|n| n.is_finite() && n >= 0.0)
            })
            || !x["cpu_s_estimate"].is_number()
            || !x["children_peak"].is_u64()
            || !x["sampler_bad_lines"].is_u64()
        {
            return Err("resources".into());
        }
    }
    if series == "B" {
        if !r["provider"].is_null() {
            return Err("provider".into());
        }
    } else if !exact_numeric(
        &r["provider"],
        &[
            "turn_requests",
            "turn_requests_peak_min",
            "aux_requests",
            "unexpected_routes",
        ],
        None,
    ) {
        return Err("provider".into());
    }
    if ["D", "E"].contains(&series) {
        validate_recovery(&r["recovery"], series)?;
    } else if !r["recovery"].is_null() {
        return Err("recovery".into());
    }
    validate_observations(&r["observations"], series)?;
    if series == "D" {
        for k in [
            "turn_ms",
            "batch_wall_ms",
            "turns_completed",
            "turns_failed",
            "complete_per_admission",
        ] {
            let v = if k == "complete_per_admission" {
                &r["events"][k]
            } else {
                &r[k]
            };
            if !v.is_null()
                || !unavailable
                    .iter()
                    .any(|u| u["field"] == k && u["reason"] == "connection dropped by design")
            {
                return Err(format!("drop {k}"));
            }
        }
    }
    Ok(())
}
fn exact_numeric(v: &Value, keys: &[&str], nullable: Option<&str>) -> bool {
    v.as_object().is_some_and(|o| {
        o.len() == keys.len()
            && keys.iter().all(|k| {
                o.get(*k)
                    .is_some_and(|v| v.is_u64() || nullable == Some(k) && v.is_null())
            })
    })
}
pub fn validate_recovery(r: &Value, series: &str) -> Result<(), String> {
    let keys = [
        "terminal_with_result",
        "terminal_without_result",
        "unknown",
        "unresolved",
        "receipt_ms",
        "resume_ms",
        "next_turn_ms",
        "replay_truncated",
        "submit_after_detach_refused",
        "eviction_evidence",
    ];
    if r.as_object()
        .is_none_or(|o| o.len() != keys.len() || keys.iter().any(|k| !o.contains_key(*k)))
    {
        return Err("recovery keys".into());
    }
    if ![
        "terminal_with_result",
        "terminal_without_result",
        "unknown",
        "unresolved",
        "replay_truncated",
    ]
    .iter()
    .all(|k| r[k].is_u64())
        || !valid_percentile(&r["receipt_ms"])
        || !valid_percentile(&r["resume_ms"])
        || !(r["next_turn_ms"].is_null() || r["next_turn_ms"].is_u64())
        || !(r["submit_after_detach_refused"].is_null()
            || r["submit_after_detach_refused"].is_boolean())
        || !(r["eviction_evidence"].is_null()
            || r["eviction_evidence"].as_str().is_some_and(safe_code))
    {
        return Err("recovery types".into());
    }
    if series == "D"
        && (!r["next_turn_ms"].is_null() || !r["submit_after_detach_refused"].is_null())
    {
        return Err("drop fields".into());
    }
    Ok(())
}
pub fn validate_observations(o: &Value, series: &str) -> Result<(), String> {
    if o.as_object().is_none_or(|o| o.len() != 5)
        || ![
            "tool_names",
            "tool_executions",
            "rate_limit_keys",
            "failure_examples",
            "admission_statuses",
        ]
        .iter()
        .all(|k| o.get(k).is_some())
    {
        return Err("observations keys".into());
    }
    for k in ["tool_names", "rate_limit_keys"] {
        if !o[k].is_null()
            && !o[k]
                .as_array()
                .is_some_and(|a| a.iter().all(|v| v.as_str().is_some_and(safe_code)))
        {
            return Err(k.into());
        }
    }
    if !(o["tool_executions"].is_null() || o["tool_executions"].is_u64())
        || series != "C" && !o["tool_executions"].is_null()
        || series != "B" && !o["rate_limit_keys"].is_null()
    {
        return Err("observation applicability".into());
    }
    if !exact_numeric(
        &o["admission_statuses"],
        &["queued", "started", "unknown", "terminal"],
        None,
    ) || !o["failure_examples"].as_array().is_some_and(|a| {
        a.len() <= 3
            && a.iter().all(|e| {
                e.as_object().is_some_and(|o| o.len() == 2)
                    && e["status"].as_str().is_some_and(safe_code)
                    && e["kind"].as_str().is_some_and(safe_code)
            })
    }) {
        return Err("observations values".into());
    }
    Ok(())
}
pub fn evidence_gate(results: &Value) -> Result<(), String> {
    let runs = results["runs"].as_array().ok_or("runs")?;
    let expected = matrix();
    if runs.len() != expected.len() {
        return Err("matrix size".into());
    }
    let mut ids = BTreeSet::new();
    for r in runs {
        validate_record(r)?;
        if !ids.insert(r["id"].as_str().unwrap()) {
            return Err("duplicate record".into());
        }
        if r["evidence"] == "not-run"
            && !allowed_not_run(r["series"].as_str().unwrap(), r["notes"].as_str().unwrap())
        {
            return Err("not-run evidence".into());
        }
    }
    for id in ["A-shared-1", "D-shared-10", "E-shared-20"] {
        if !runs
            .iter()
            .any(|r| r["id"] == id && r["evidence"] == "live")
        {
            return Err(format!("missing live {id}"));
        }
    }
    Ok(())
}
pub fn merge_record(
    path: &Path,
    release: &str,
    mode: &str,
    record: &Value,
) -> Result<Value, String> {
    validate_record(record)?;
    merge_raw_record(path, release, mode, record)
}
/// Lock and replace an already validated record, independently of its schema.
pub fn merge_raw_record(
    path: &Path,
    release: &str,
    mode: &str,
    record: &Value,
) -> Result<Value, String> {
    if record["id"].as_str().is_none() {
        return Err("record id".into());
    }
    std::fs::create_dir_all(path.parent().ok_or("results directory")?)
        .map_err(|_| "results directory")?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("json.lock"))
        .map_err(|_| "results lock")?;
    lock.lock().map_err(|_| "results lock")?;
    let mut results = if path.exists() {
        serde_json::from_slice::<Value>(&std::fs::read(path).map_err(|_| "results read")?)
            .map_err(|_| "results decode")?
    } else {
        json!({"hermes_release":release,"execution_mode":mode,"runs":[],"recommendations":null})
    };
    if results["hermes_release"] != release {
        return Err("Hermes release differs".into());
    }
    let runs = results["runs"].as_array_mut().ok_or("runs")?;
    runs.retain(|r| r["id"] != record["id"]);
    runs.push(record.clone());
    runs.sort_by_key(|r| r["id"].as_str().unwrap_or("").to_owned());
    results["recommendations"] = recommend(&results);
    let temporary = path.with_extension(format!("json.{}.new", fastrand::u64(..)));
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(&results).map_err(|_| "results encode")?,
    )
    .map_err(|_| "results write")?;
    std::fs::rename(temporary, path).map_err(|_| "results replace")?;
    Ok(results)
}
pub fn import_records(
    path: &Path,
    input: &str,
    release: &str,
    mode: &str,
) -> Result<usize, String> {
    let records: Result<Vec<_>, _> = input
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let raw = line.strip_prefix("RECORD ").ok_or("record prefix")?;
            let record: Value = serde_json::from_str(raw).map_err(|_| "record decode")?;
            validate_record(&record).map_err(|_| "record validation")?;
            if record["series"] != "B" {
                return Err("operator series");
            }
            Ok(record)
        })
        .collect();
    let records = records?;
    for r in &records {
        merge_record(path, release, mode, r)?;
    }
    Ok(records.len())
}

/// Structural validation remains useful before live measurements are available.
pub fn validate_results(results: &Value) -> Result<(), String> {
    let object = results.as_object().ok_or("results object")?;
    if object.len() != 4
        || ![
            "hermes_release",
            "execution_mode",
            "runs",
            "recommendations",
        ]
        .iter()
        .all(|k| object.contains_key(*k))
    {
        return Err("results keys".into());
    }
    if results["hermes_release"] != RELEASE
        || !results["execution_mode"]
            .as_str()
            .is_some_and(|s| ["in-process", "managed-worker", "unknown"].contains(&s))
    {
        return Err("results identity".into());
    }
    let runs = results["runs"].as_array().ok_or("runs")?;
    if runs.len() != matrix().len() {
        return Err("matrix size".into());
    }
    let mut ids = BTreeSet::new();
    for record in runs {
        validate_record(record)?;
        if !ids.insert(record["id"].as_str().unwrap()) {
            return Err("duplicate record".into());
        }
    }
    if recommend(results) != results["recommendations"] {
        return Err("recommendations differ".into());
    }
    Ok(())
}
