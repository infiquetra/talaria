use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn identity() -> Value {
    json!({"run_id":"1234abcd","talaria_commit":"012345abcdef","date":"2026-10-27"})
}
fn live(series: &str, topology: &str, width: u32) -> Value {
    let mut r = empty_record(series, topology, width, identity());
    let turns = width * if series == "A" { 3 } else { 1 };
    r["evidence"] = json!("live");
    r["notes"] = json!("");
    r["healthy"] = json!(true);
    for key in [
        "create_ms",
        "submit_ms",
        "start_delay_ms",
        "first_delta_ms",
        "turn_ms",
        "foreground_rtt_ms",
    ] {
        r[key] = percentile(&[5, 10]).unwrap();
    }
    for key in [
        "batch_wall_ms",
        "turns_failed",
        "admissions_unknown",
        "create_refused",
        "timeouts",
        "foreground_errors",
    ] {
        r[key] = json!(0);
    }
    r["distinct_sessions"] = json!(width);
    r["turns_completed"] = json!(turns);
    r["rpc_errors"] = json!({});
    r["events"] = json!({"observed":turns*3,"seq_repeats":0,"seq_decreases":0,"seq_gaps":0,"epoch_changes":0,"replay_gaps":0,"unresolved_gaps":0,"peak_events_per_s":20,"complete_per_admission":turns});
    r["resources"] = json!({"rss_mb":{"before":100.0,"peak":100.0+f64::from(width)*2.0,"after":100.0},"cpu_s_estimate":1.0,"children_peak":0,"sampler_bad_lines":0});
    r["provider"] = if series == "B" {
        Value::Null
    } else {
        json!({"turn_requests":turns,"turn_requests_peak_min":width,"aux_requests":0,"unexpected_routes":0})
    };
    r["observations"] = json!({"tool_names":[],"tool_executions":if series=="C" {Some(width)}else{None},"rate_limit_keys":if series=="B"{Some(Vec::<String>::new())}else{None},"failure_examples":[],"admission_statuses":{"queued":0,"started":turns,"unknown":0,"terminal":0}});
    if ["D", "E"].contains(&series) {
        r["recovery"] = json!({"terminal_with_result":turns,"terminal_without_result":0,"unknown":0,"unresolved":0,"receipt_ms":percentile(&[20]),"resume_ms":percentile(&[5]),"next_turn_ms":if series=="E" {Some(50)}else{None},"replay_truncated":0,"submit_after_detach_refused":if series=="E" {Some(true)}else{None},"eviction_evidence":null});
    }
    if series == "D" {
        for key in [
            "turn_ms",
            "batch_wall_ms",
            "turns_completed",
            "turns_failed",
            "complete_per_admission",
        ] {
            if key == "complete_per_admission" {
                r["events"][key] = Value::Null;
            } else {
                r[key] = Value::Null;
            }
            r["unavailable"]
                .as_array_mut()
                .unwrap()
                .push(json!({"field":key,"reason":"connection dropped by design"}));
        }
    }
    let keys: Vec<_> = r
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, v)| v.is_null())
        .map(|(k, _)| k.clone())
        .collect();
    for key in keys {
        if !r["unavailable"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["field"] == key)
        {
            r["unavailable"]
                .as_array_mut()
                .unwrap()
                .push(json!({"field":key,"reason":"not applicable"}));
        }
    }
    r
}
fn event(seq: u64, epoch: &str, kind: &str) -> Value {
    json!({"session_id":"session","replay_epoch":epoch,"seq":seq,"type":kind,"admission_id":"admission","payload":{}})
}

#[test]
fn percentile_edges() {
    assert_eq!(percentile(&[]), None);
    assert_eq!(
        percentile(&[5, 1, 3, 2, 4]),
        Some(json!({"n":5,"p50":3,"p95":5,"max":5}))
    );
    assert_eq!(percentile(&[8]).unwrap()["p95"], 8);
}
#[test]
fn process_descendants() {
    let p = parse_ps(
        "10 1 1024 00:01:02\n11 10 2048 1:02.50\n12 11 1024 0:01\n13 1 9999 0:00\nbad\n",
        10,
    )
    .unwrap();
    assert_eq!(p.rss_mb, 4.0);
    assert_eq!(p.children, 2);
    assert_eq!(p.bad_lines, 1);
    assert!((p.cpu_s - 125.5).abs() < 0.001);
    assert!(parse_ps("10 1 1024 0:01", 20).is_none());
}
#[test]
fn cpu_time_formats() {
    assert_eq!(parse_cpu_time("1-02:03:04.5"), Some(93784.5));
    assert_eq!(parse_cpu_time("1:02.50"), Some(62.5));
    assert_eq!(parse_cpu_time("bad"), None);
    assert_eq!(parse_cpu_time("1:nan"), None);
}
#[test]
fn descriptor_fields() {
    let mut d = json!({"session_create":{"sources":["tool"],"parameters":["request_id","source","model","provider","base_url","api_key","toolsets"]}});
    assert_eq!(descriptor_gate(&d), Ok(()));
    d["session_create"]["parameters"]
        .as_array_mut()
        .unwrap()
        .retain(|v| v != "provider");
    assert_eq!(descriptor_gate(&d), Err("descriptor lacks provider".into()));
}
#[test]
fn outbound_shapes() {
    let mut ids = BTreeSet::new();
    for series in ["A", "B", "C"] {
        let p = create_params("1234abcd", series, 0, "http://127.0.0.1:1/v1");
        for k in ["unattended", "yolo", "title", "cwd", "profile"] {
            assert!(p.get(k).is_none());
        }
        assert_eq!(p["source"], "tool");
        assert_eq!(p["hidden"], true);
        if series == "B" {
            assert!(p.get("api_key").is_none());
            assert!(p.get("base_url").is_none());
        }
        for n in 0..2 {
            for rep in 0..3 {
                let p = submit_params("1234abcd", series, n, rep, "s");
                assert_eq!(p.as_object().unwrap().len(), 4);
                assert!(ids.insert(p["submission_id"].as_str().unwrap().to_owned()));
            }
        }
    }
}
#[test]
fn sequences_and_epochs() {
    let mut t = EventTracker::default();
    for (s, e) in [(1, "a"), (2, "a"), (2, "a"), (1, "a"), (5, "a"), (1, "b")] {
        t.observe(&event(s, e, "message.delta"), 0);
    }
    let r = t.report();
    assert_eq!(r["seq_repeats"], 1);
    assert_eq!(r["seq_decreases"], 1);
    assert_eq!(r["seq_gaps"], 1);
    assert_eq!(r["epoch_changes"], 1);
}
#[test]
fn replay_repairs_ranges() {
    let mut t = EventTracker::default();
    t.observe(&event(1, "a", "message.delta"), 0);
    t.observe(&event(4, "a", "message.delta"), 10);
    assert_eq!(t.report()["unresolved_gaps"], 1);
    t.resolve("session","a",&json!({"truncated":true,"events":[event(2,"a","message.delta"),event(3,"a","message.delta")]}));
    assert_eq!(t.report()["unresolved_gaps"], 1);
    t.resolve("session","a",&json!({"truncated":false,"events":[event(2,"a","message.delta"),event(3,"a","message.delta")]}));
    assert_eq!(t.report()["unresolved_gaps"], 0);
}
#[test]
fn rate_and_duplicate_completion() {
    let mut t = EventTracker::default();
    for (i, at) in [0, 100, 200, 300, 400, 1400].into_iter().enumerate() {
        t.observe(&event(i as u64 + 1, "a", "message.delta"), at);
    }
    assert_eq!(t.report()["peak_events_per_s"], 5);
    t.observe(&event(7, "a", "message.complete"), 1500);
    assert_eq!(t.report()["complete_per_admission"], 1);
    t.observe(&event(8, "a", "message.complete"), 1600);
    assert_eq!(t.report()["complete_per_admission"], 0);
    t.observe(&json!({"session_id":"session","type":"session.replay_gap","payload":{"replay_epoch":"a","latest_seq":10}}),1700);
    assert_eq!(t.report()["replay_gaps"], 1);
    assert_eq!(t.position("session").1, "a");
}
#[test]
fn early_event_clock() {
    let mut c = TurnClock::default();
    c.sent("session", 10);
    c.event(&event(1, "a", "message.start"), 15);
    c.admitted("session", "admission");
    c.event(&event(2, "a", "message.delta"), 20);
    c.event(&event(3, "a", "message.complete"), 50);
    assert_eq!(c.start_delay_ms, [5]);
    assert_eq!(c.first_delta_ms, [10]);
    assert_eq!(c.turn_ms, [40]);
}
#[tokio::test(start_paused = true)]
async fn missing_completion_times_out() {
    let (_tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let start = tokio::time::Instant::now();
    assert_eq!(
        wait_for_turn(&mut rx, "s", Duration::from_secs(5)).await,
        Err("timeout")
    );
    assert_eq!(start.elapsed(), Duration::from_secs(5));
}
#[test]
fn each_health_failure() {
    let baseline = live("A", "shared", 1);
    assert_eq!(healthy(&baseline, &baseline), (true, vec![]));
    for criterion in 1..=7 {
        let mut r = live(
            if criterion == 6 {
                "E"
            } else if criterion == 7 {
                "C"
            } else {
                "A"
            },
            "shared",
            if criterion == 6 {
                20
            } else if criterion == 7 {
                10
            } else {
                1
            },
        );
        match criterion {
            1 => r["create_refused"] = json!(1),
            2 => r["start_delay_ms"]["p95"] = json!(1011),
            3 => r["foreground_rtt_ms"]["p95"] = json!(51),
            4 => r["resources"]["rss_mb"]["after"] = json!(121.0),
            5 => r["events"]["seq_repeats"] = json!(1),
            6 => r["recovery"]["unknown"] = json!(1),
            7 => r["observations"]["tool_executions"] = json!(0),
            _ => unreachable!(),
        };
        assert_eq!(healthy(&r, &baseline), (false, vec![criterion]));
    }
}
#[test]
fn healthy_width_is_contiguous() {
    let mut runs: Vec<_> = WIDTHS.iter().map(|w| live("A", "shared", *w)).collect();
    runs[2]["turns_failed"] = json!(1);
    assert_eq!(highest_healthy(&runs), Some(5));
    runs[0]["turns_failed"] = json!(1);
    assert_eq!(highest_healthy(&runs), None);
    assert_eq!(highest_healthy(&[]), None);
}
#[test]
fn recommendations_choose_topology() {
    let mut runs: Vec<_> = matrix()
        .into_iter()
        .filter(|(s, _, _)| s == "A")
        .map(|(s, t, w)| live(&s, &t, w))
        .collect();
    for r in &mut runs {
        if r["topology"] == "shared" && r["width"].as_u64().unwrap() >= 10 {
            r["turns_failed"] = json!(1);
        }
    }
    let r = recommend(&json!({"runs":runs}));
    assert_eq!(r["topology"], "per-session");
    assert_eq!(r["default_max_in_flight"], 10);
    assert_eq!(r["memory_mb_per_session"], 2.0);
    assert_eq!(recommend(&json!({"runs":[]}))["topology"], Value::Null);
}
#[test]
fn rejects_record_shape() {
    let r = live("A", "shared", 1);
    assert_eq!(validate_record(&r), Ok(()));
    let mut bad = r.clone();
    bad["extra"] = json!(1);
    assert!(validate_record(&bad).is_err());
    let mut bad = r.clone();
    bad["create_ms"]["p50"] = json!(100);
    assert!(validate_record(&bad).is_err());
    let mut bad = r;
    bad["turn_ms"] = Value::Null;
    assert!(validate_record(&bad).is_err());
}
#[test]
fn validates_series_specific_fields() {
    for s in ["B", "C", "D", "E"] {
        let width = if s == "B" {
            1
        } else if s == "E" {
            20
        } else {
            10
        };
        let r = live(s, "shared", width);
        assert_eq!(validate_record(&r), Ok(()), "{s}");
    }
    let mut r = live("D", "shared", 10);
    r["turn_ms"] = percentile(&[1]).unwrap();
    assert!(validate_record(&r).is_err());
    let mut r = live("A", "shared", 1);
    r["observations"]["failure_examples"] = json!([{"status":"error","kind":"unsafe text!"}]);
    assert!(validate_record(&r).is_err());
}
#[test]
fn matrix_validation_and_gate() {
    let r: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/gateway-concurrency/results.json"
    ))
    .unwrap();
    assert_eq!(validate_results(&r), Ok(()));
    assert_eq!(r["recommendations"], recommend(&r));
    let unrun: Vec<_> = matrix()
        .into_iter()
        .map(|(s, t, w)| empty_record(&s, &t, w, identity()))
        .collect();
    assert!(evidence_gate(&json!({"runs":unrun})).is_err());
}
#[test]
fn gate_requires_recovery() {
    let runs: Vec<_> = matrix()
        .into_iter()
        .map(|(s, t, w)| live(&s, &t, w))
        .collect();
    let mut r = json!({"runs":runs});
    assert_eq!(evidence_gate(&r), Ok(()));
    r["runs"]
        .as_array_mut()
        .unwrap()
        .retain(|v| v["series"] != "D");
    assert!(evidence_gate(&r).is_err());
    assert!(!allowed_not_run("A", "operator waived"));
    assert!(allowed_not_run("B", "operator waived"));
    assert!(!allowed_not_run("D", "timebox reached"));
}
#[test]
fn records_replace_and_refuse_release() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("results.json");
    let mut r = live("A", "shared", 1);
    merge_record(&path, RELEASE, "unknown", &r).unwrap();
    r["batch_wall_ms"] = json!(123);
    let result = merge_record(&path, RELEASE, "unknown", &r).unwrap();
    assert_eq!(result["runs"].as_array().unwrap().len(), 1);
    assert_eq!(result["runs"][0]["batch_wall_ms"], 123);
    assert!(merge_record(&path, "different", "unknown", &r).is_err());
}
#[test]
fn import_validates_before_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("results.json");
    let r = live("B", "shared", 1);
    assert!(
        import_records(
            &path,
            &format!("RECORD {r}\nRECORD {{}}"),
            RELEASE,
            "unknown"
        )
        .is_err()
    );
    assert!(!path.exists());
    assert_eq!(
        import_records(&path, &format!("RECORD {r}"), RELEASE, "unknown"),
        Ok(1)
    );
}
async fn request(provider: &LoopbackProvider, route: &str, body: Value) -> String {
    let authority = provider
        .base_url()
        .strip_prefix("http://")
        .unwrap()
        .strip_suffix("/v1")
        .unwrap();
    let mut stream = tokio::net::TcpStream::connect(authority).await.unwrap();
    let body = body.to_string();
    stream
        .write_all(
            format!(
                "POST {route} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut output = String::new();
    stream.read_to_string(&mut output).await.unwrap();
    output
}
fn turn(role: &str) -> Value {
    json!({"stream":true,"messages":[{"role":role,"content":PROMPT}]})
}
#[tokio::test]
async fn provider_counts_concurrency() {
    let p = LoopbackProvider::start("ready").await.unwrap();
    let (a, b) = tokio::join!(
        request(&p, "/v1/chat/completions", turn("user")),
        request(&p, "/v1/chat/completions", turn("user"))
    );
    assert!(a.ends_with("data: [DONE]\n\n"));
    assert!(b.contains("chat.completion.chunk"));
    let stats = p.stats().await;
    assert_eq!(stats.turn_requests, 2);
    assert_eq!(stats.turn_requests_peak_min, 2);
    p.reset_peak().await;
    request(&p, "/v1/chat/completions", json!({"stream":false})).await;
    assert!(
        request(&p, "/v1/other", json!({}))
            .await
            .starts_with("HTTP/1.1 404")
    );
    let stats = p.stats().await;
    assert_eq!(stats.aux_requests, 1);
    assert_eq!(stats.unexpected_routes, 1);
    assert_eq!(stats.turn_requests_peak_min, 0);
}
#[tokio::test]
async fn provider_tool_round_trip() {
    let p = LoopbackProvider::start("todo").await.unwrap();
    let tool = request(&p, "/v1/chat/completions", turn("user")).await;
    assert!(tool.contains("todo_list"));
    assert!(tool.contains("\"finish_reason\":\"tool_calls\""));
    let followup = request(&p, "/v1/chat/completions", turn("tool")).await;
    assert!(followup.contains("\"finish_reason\":\"stop\""));
    assert!(!followup.contains("todo_list"));
    assert_eq!(p.stats().await.turn_requests, 2);
}
