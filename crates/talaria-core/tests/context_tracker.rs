//! Red-run tests 20 to 25 (#234): the context tracker.

use proptest::prelude::*;
use serde_json::{Value, json};
use talaria_core::context::compact::CompactEnd;
use talaria_core::context::tracker::{
    AlreadyRunning, CompactingLine, ContextNotice, ContextTracker, HeldBlocks,
};

const SID: &str = "local-session-1";

fn usage_payload(context_percent: Option<Value>) -> Value {
    let mut usage = json!({"model": "minimax-m3.1-flash"});
    if let Some(percent) = context_percent {
        usage["context_percent"] = percent;
    }
    json!({"usage": usage})
}

fn complete_payload(context_percent: Option<Value>) -> Value {
    let mut payload = json!({"status": "complete"});
    if let Some(percent) = context_percent {
        payload["usage"] = json!({"context_percent": percent});
    }
    payload
}

fn status_payload(kind: Value) -> Value {
    json!({"kind": kind, "text": "relay"})
}

fn updated_payload(revision: Option<Value>, headline: Option<&str>) -> Value {
    let mut payload = json!({"operation": "compress"});
    if let Some(number) = revision {
        payload["revision"] = number;
    }
    if let Some(headline) = headline {
        payload["summary"] = json!({"headline": headline});
    }
    payload
}

fn observe(
    tracker: &mut ContextTracker,
    type_name: &str,
    payload: &Value,
    now_ms: u64,
) -> Vec<ContextNotice> {
    tracker.observe(SID, type_name, payload, now_ms)
}

#[test]
fn test_20_commit_hides_until_a_fresh_reading() {
    let mut tracker = ContextTracker::new();
    tracker.begin_compact(SID, 100).expect("first compact runs");
    tracker.finish_compact(SID, CompactEnd::Committed { revision: 14 });
    assert!(tracker.context_hidden(SID));

    let silent = observe(&mut tracker, "session.usage", &usage_payload(None), 200);
    assert!(silent.is_empty());
    assert!(tracker.context_hidden(SID));

    let fresh = observe(
        &mut tracker,
        "session.usage",
        &usage_payload(Some(json!(2))),
        300,
    );
    assert!(fresh.is_empty());
    assert!(!tracker.context_hidden(SID));
}

#[test]
fn test_21_foreign_commits_notice_once_per_revision() {
    let mut tracker = ContextTracker::new();
    tracker.begin_compact(SID, 100).expect("compact runs");
    let during = observe(
        &mut tracker,
        "session.updated",
        &updated_payload(Some(json!(14)), Some("Compressed: 32 → 24 messages")),
        200,
    );
    assert!(during.is_empty());
    assert!(tracker.context_hidden(SID));
    tracker.finish_compact(SID, CompactEnd::Committed { revision: 14 });

    let echo = observe(
        &mut tracker,
        "session.updated",
        &updated_payload(Some(json!(14)), None),
        300,
    );
    assert!(echo.is_empty());

    let foreign = observe(
        &mut tracker,
        "session.updated",
        &updated_payload(Some(json!(15)), Some("Compressed: 40 → 31 messages")),
        400,
    );
    assert_eq!(
        foreign,
        [ContextNotice::CompactedElsewhere {
            revision: 15,
            headline: Some("Compressed: 40 → 31 messages".to_owned()),
        }]
    );
    let again = observe(
        &mut tracker,
        "session.updated",
        &updated_payload(Some(json!(15)), None),
        500,
    );
    assert!(again.is_empty());

    let mut unsettled = ContextTracker::new();
    observe(
        &mut unsettled,
        "session.usage",
        &usage_payload(Some(json!(8))),
        100,
    );
    unsettled.begin_compact(SID, 200).expect("compact runs");
    unsettled.finish_compact(SID, CompactEnd::Unknown);
    assert!(unsettled.context_hidden(SID));
    let late = observe(
        &mut unsettled,
        "session.updated",
        &updated_payload(Some(json!(16)), None),
        300,
    );
    assert!(late.is_empty());
    let next = observe(
        &mut unsettled,
        "session.updated",
        &updated_payload(Some(json!(17)), None),
        400,
    );
    assert!(matches!(
        next.as_slice(),
        [ContextNotice::CompactedElsewhere { revision: 17, .. }]
    ));

    let mut revisionless = ContextTracker::new();
    let notices = observe(
        &mut revisionless,
        "session.updated",
        &updated_payload(None, None),
        100,
    );
    assert!(notices.is_empty());
    assert!(revisionless.context_hidden(SID));
}

#[test]
fn test_22_automatic_compaction_line_and_notice() {
    let text = include_str!("data/context/auto_compaction_frames.jsonl");
    let records: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("frame log parses"))
        .collect();
    assert_eq!(records.len(), 8);
    let kinds: Vec<&str> = records[1..]
        .iter()
        .map(|record| {
            record["frame"]["params"]["type"]
                .as_str()
                .expect("event type")
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "message.start",
            "status.update",
            "status.update",
            "status.update",
            "status.update",
            "session.usage",
            "message.complete",
        ]
    );

    let mut tracker = ContextTracker::new();
    let compacting = status_payload(json!("compacting"));
    let compacted = status_payload(json!("compacted"));
    assert!(observe(&mut tracker, "status.update", &compacting, 1_000).is_empty());
    assert_eq!(
        tracker.compacting_line(SID),
        Some(CompactingLine::Auto { since_ms: 1_000 })
    );
    assert!(observe(&mut tracker, "status.update", &compacting, 5_000).is_empty());
    assert_eq!(
        tracker.compacting_line(SID),
        Some(CompactingLine::Auto { since_ms: 1_000 })
    );

    let first = observe(&mut tracker, "status.update", &compacted, 9_000);
    assert_eq!(first, [ContextNotice::AutoCompacted]);
    assert_eq!(tracker.compacting_line(SID), None);
    assert!(tracker.context_hidden(SID));

    let second = observe(&mut tracker, "status.update", &compacted, 9_100);
    assert!(second.is_empty());

    let done = observe(
        &mut tracker,
        "message.complete",
        &complete_payload(None),
        12_000,
    );
    assert!(done.is_empty());
    assert_eq!(tracker.compacting_line(SID), None);
    assert!(tracker.context_hidden(SID));

    let tick = observe(
        &mut tracker,
        "session.usage",
        &usage_payload(Some(json!(3))),
        12_500,
    );
    assert!(tick.is_empty());
    assert!(!tracker.context_hidden(SID));
}

#[test]
fn test_23_second_compact_is_refused_while_one_runs() {
    let mut tracker = ContextTracker::new();
    tracker.begin_compact(SID, 100).expect("first compact runs");
    assert_eq!(tracker.begin_compact(SID, 200), Err(AlreadyRunning));
}

#[derive(Clone, Debug)]
enum Step {
    Begin,
    Finish(CompactEnd),
    Observe { type_name: String, payload: Value },
}

fn payload_strategy() -> impl Strategy<Value = Value> {
    let scalar = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        "[a-z]{0,8}".prop_map(Value::from),
    ];
    let shallow = prop_oneof![
        scalar.clone(),
        prop::collection::vec(scalar.clone(), 0..3).prop_map(Value::from),
        prop::collection::hash_map("[a-z]{1,4}", scalar.clone(), 0..3)
            .prop_map(|map| Value::Object(map.into_iter().collect())),
    ];
    prop_oneof![
        shallow.clone(),
        prop::collection::vec(shallow.clone(), 0..2).prop_map(Value::from),
        prop::collection::hash_map("[a-z]{1,4}", shallow, 0..2)
            .prop_map(|map| Value::Object(map.into_iter().collect())),
    ]
}

fn typed_payload(type_name: &str) -> impl Strategy<Value = Value> {
    let percent = prop_oneof![
        (0..=100).prop_map(|n: i32| json!(n)),
        Just(Value::Null),
        Just(json!("5")),
    ];
    let kind = prop_oneof![
        Just(json!("compacting")),
        Just(json!("compacted")),
        Just(json!("other")),
        Just(json!(5)),
    ];
    let operation = prop_oneof![
        Just(json!("compress")),
        Just(json!("model")),
        Just(Value::Null),
    ];
    let revision = prop_oneof![
        (0..=20).prop_map(|n: i32| json!(n)),
        Just(Value::Null),
        Just(json!("7")),
    ];
    let headline = prop_oneof![
        Just(Value::Null),
        "[a-z]{0,8}".prop_map(|text| json!({"headline": text})),
    ];
    match type_name {
        "session.usage" => percent
            .prop_map(|percent| {
                if percent.is_null() {
                    json!({"usage": {"model": "m"}})
                } else {
                    json!({"usage": {"context_percent": percent}})
                }
            })
            .boxed(),
        "message.complete" => percent
            .prop_map(|percent| {
                if percent.is_null() {
                    json!({"status": "complete"})
                } else {
                    json!({"status": "complete", "usage": {"context_percent": percent}})
                }
            })
            .boxed(),
        "status.update" => kind.prop_map(|kind| json!({"kind": kind})).boxed(),
        "session.updated" => (operation, revision, headline)
            .prop_map(|(operation, revision, summary)| {
                let mut payload = json!({});
                if !operation.is_null() {
                    payload["operation"] = operation;
                }
                if !revision.is_null() {
                    payload["revision"] = revision;
                }
                if !summary.is_null() {
                    payload["summary"] = summary;
                }
                payload
            })
            .boxed(),
        _ => payload_strategy().boxed(),
    }
}

fn step_strategy() -> impl Strategy<Value = Step> {
    let finish = prop_oneof![
        (0..=20).prop_map(|n: i32| Step::Finish(CompactEnd::Committed {
            revision: i64::from(n)
        })),
        Just(Step::Finish(CompactEnd::Unknown)),
        Just(Step::Finish(CompactEnd::NothingChanged)),
    ];
    let observe = prop_oneof![
        Just("session.usage"),
        Just("message.complete"),
        Just("status.update"),
        Just("session.updated"),
        Just("other.event"),
    ]
    .prop_flat_map(|type_name| {
        prop_oneof![typed_payload(type_name).boxed(), payload_strategy().boxed(),].prop_map(
            move |payload| Step::Observe {
                type_name: type_name.to_owned(),
                payload,
            },
        )
    });
    prop_oneof![Just(Step::Begin), finish, observe]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn test_24_tracker_steps_never_panic_and_hold_invariants(
        steps in prop::collection::vec(step_strategy(), 0..64),
        gaps in prop::collection::vec(0..=10_000u64, 0..64),
    ) {
        let mut tracker = ContextTracker::new();
        let mut now_ms = 0_u64;
        let mut compacted_events = 0_usize;
        let mut auto_notices = 0_usize;
        let mut compacting_since_auto = true;
        let mut elsewhere: Vec<i64> = Vec::new();
        let mut seen_revisions: Vec<i64> = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            now_ms += gaps.get(index).copied().unwrap_or(0);
            match step {
                Step::Begin => {
                    let _ = tracker.begin_compact(SID, now_ms);
                }
                Step::Finish(end) => {
                    if let CompactEnd::Committed { revision } = end {
                        seen_revisions.push(*revision);
                    }
                    tracker.finish_compact(SID, *end);
                    if !matches!(end, CompactEnd::NothingChanged) {
                        prop_assert!(
                            tracker.context_hidden(SID),
                            "hidden right after finish {end:?}"
                        );
                    }
                }
                Step::Observe { type_name, payload } => {
                    if type_name == "status.update" && payload["kind"] == json!("compacted") {
                        compacted_events += 1;
                    }
                    if type_name == "status.update" && payload["kind"] == json!("compacting") {
                        compacting_since_auto = true;
                    }
                    let earlier = seen_revisions.clone();
                    if type_name == "session.updated"
                        && payload["operation"] == json!("compress")
                        && let Some(revision) = payload["revision"].as_i64()
                    {
                        seen_revisions.push(revision);
                    }
                    let notices = tracker.observe(SID, type_name, payload, now_ms);
                    for notice in &notices {
                        prop_assert!(
                            tracker.context_hidden(SID),
                            "hidden right after notice {notice:?}"
                        );
                    }
                    for notice in notices {
                        match notice {
                            ContextNotice::AutoCompacted => {
                                auto_notices += 1;
                                prop_assert!(
                                    compacting_since_auto,
                                    "two automatic notices without a compacting event between them"
                                );
                                compacting_since_auto = false;
                            }
                            ContextNotice::CompactedElsewhere { revision, .. } => {
                                prop_assert!(
                                    !elsewhere.contains(&revision),
                                    "duplicate foreign revision {revision}"
                                );
                                prop_assert!(
                                    earlier.iter().all(|seen| revision > *seen),
                                    "foreign revision {revision} is not above earlier revisions"
                                );
                                elsewhere.push(revision);
                            }
                        }
                    }
                }
            }
        }
        prop_assert!(
            auto_notices <= compacted_events,
            "more automatic notices than compacted events"
        );
    }
}

#[test]
fn test_25_blocks_wait_for_the_turn_end() {
    let mut held = HeldBlocks::new();
    assert!(held.push("first".to_owned(), true).is_empty());
    assert!(held.push("second".to_owned(), true).is_empty());
    assert_eq!(held.release(), ["first".to_owned(), "second".to_owned()]);

    let mut idle = HeldBlocks::new();
    assert_eq!(idle.push("now".to_owned(), false), ["now".to_owned()]);
}
