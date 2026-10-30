//! Journal lines built for the progress tests. A test does not write a line by hand.
#![allow(dead_code)]

use serde_json::{Value, json};
use talaria_workflow::engine::{
    AgentRequest, CallId, EngineEvent, OutcomeKind, PhaseMeta, RunOutcomeKind, WorkflowMeta,
};
use talaria_workflow::journal::{
    AdmissionState, AttentionCause, CallOutcome, FinishSummary, Manifest, Record, RecordLine,
    ResolvedLaunch,
};

pub const RUN_ID: &str = "01JB2X9Q4M8Z7K3T5V6W8Y0A1C";
pub const SCRIPT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const KEY_CORRECTNESS: &str = "11111111111111111111111111111111";
pub const KEY_SECURITY: &str = "22222222222222222222222222222222";
pub const KEY_SIMPLICITY: &str = "33333333333333333333333333333333";
pub const KEY_FAILED: &str = "44444444444444444444444444444444";
pub const KEY_SKIPPED: &str = "55555555555555555555555555555555";
pub const KEY_PARKED: &str = "66666666666666666666666666666666";
pub const KEY_ENDED: &str = "77777777777777777777777777777777";
pub const KEY_RUNNING: &str = "88888888888888888888888888888888";
pub const KEY_QUEUED: &str = "99999999999999999999999999999999";
pub const KEY_USAGE: &str = "abababababababababababababababab";
pub const CHILD_PATH: &str = "name:child@0123456789ab";

pub const RUN_STARTED_MS: i64 = 1_000;
pub const CORRECTNESS_STARTED_MS: i64 = 1_400;
pub const CORRECTNESS_FINISHED_MS: i64 = 2_600;
pub const SECURITY_STARTED_MS: i64 = 2_900;
pub const SECURITY_FINISHED_MS: i64 = 4_100;
pub const SIMPLICITY_STARTED_MS: i64 = 4_400;
pub const SIMPLICITY_FINISHED_MS: i64 = 5_100;

pub fn manifest() -> Manifest {
    serde_json::from_value(json!({
        "v": 1,
        "run_id": RUN_ID,
        "created_at_ms": RUN_STARTED_MS,
        "workflow": {
            "name": "review-branch",
            "script_sha256": SCRIPT_SHA
        },
        "args": null,
        "started_at_ms": 1,
        "seed": 4242,
        "profile": "fixture",
        "on_unknown": "wait",
        "talaria_version": env!("CARGO_PKG_VERSION"),
        "hermes": {},
        "roster": {},
        "approval_policy": {}
    }))
    .expect("manifest")
}

pub fn manifest_json() -> String {
    serde_json::to_string_pretty(&manifest()).expect("manifest")
}

pub fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/frames/p5-05-progress.jsonl"
    ))
}

pub fn jsonl(lines: &[RecordLine]) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(&serde_json::to_string(line).expect("line"));
        out.push('\n');
    }
    out
}

fn stamp(rows: Vec<(i64, Record)>) -> Vec<RecordLine> {
    rows.into_iter()
        .enumerate()
        .map(|(index, (at_ms, record))| RecordLine {
            v: 1,
            seq: u64::try_from(index).expect("seq") + 1,
            at_ms,
            record,
        })
        .collect()
}

fn launch() -> ResolvedLaunch {
    ResolvedLaunch {
        profile: "fixture".to_string(),
        model: "minimax-m3.1-flash".to_string(),
        provider: Some("ollama-cloud".to_string()),
        effort: None,
        toolsets: Vec::new(),
        skills: Vec::new(),
        max_turns: None,
        isolation: None,
        cwd: None,
    }
}

fn call_id(digest: &str) -> CallId {
    CallId {
        digest: digest.to_string(),
        occurrence: 0,
    }
}

fn request(label: &str, phase: &str, digest: &str, workflow_path: Vec<String>) -> AgentRequest {
    AgentRequest {
        call_id: call_id(digest),
        label: label.to_string(),
        phase: Some(phase.to_string()),
        prompt: format!("Review for {label}"),
        schema: None,
        tier: None,
        model: Some("minimax-m3.1-flash".to_string()),
        provider: Some("ollama-cloud".to_string()),
        effort: None,
        profile: Some("fixture".to_string()),
        toolsets: None,
        skills: None,
        isolation: None,
        max_turns: None,
        workflow_path,
    }
}

fn meta() -> WorkflowMeta {
    WorkflowMeta {
        name: "review-branch".to_string(),
        description: "Review a branch.".to_string(),
        when_to_use: None,
        phases: ["plan", "build", "check"]
            .into_iter()
            .map(|title| PhaseMeta {
                title: title.to_string(),
                detail: None,
                model: None,
            })
            .collect(),
    }
}

fn engine(at_ms: i64, event: EngineEvent) -> (i64, Record) {
    (
        at_ms,
        Record::Engine {
            event: Some(event),
            event_ref: None,
        },
    )
}

fn intent(at_ms: i64, key: &str, label: &str, phase: &str, path: Vec<String>) -> (i64, Record) {
    (
        at_ms,
        Record::CallIntent {
            key: key.to_string(),
            attempt: 0,
            call_id: call_id(key),
            label: label.to_string(),
            phase: Some(phase.to_string()),
            workflow_path: path,
            launch: launch(),
            request_id: format!("wf:{RUN_ID}:{key}"),
        },
    )
}

fn attempt_intent(at_ms: i64, key: &str, attempt: u32) -> (i64, Record) {
    (
        at_ms,
        Record::CallIntent {
            key: key.to_string(),
            attempt,
            call_id: call_id(key),
            label: "failed-row".to_string(),
            phase: Some("plan".to_string()),
            workflow_path: Vec::new(),
            launch: launch(),
            request_id: format!("wf:{RUN_ID}:{key}:a{attempt}"),
        },
    )
}

fn bound(at_ms: i64, key: &str, session: &str) -> (i64, Record) {
    (
        at_ms,
        Record::SessionBound {
            key: key.to_string(),
            attempt: 0,
            session_id: session.to_string(),
            stored_session_id: session.to_string(),
        },
    )
}

fn admitted(at_ms: i64, key: &str, admission: &str, generation: i64) -> (i64, Record) {
    (
        at_ms,
        Record::TurnAdmitted {
            key: key.to_string(),
            attempt: 0,
            turn: 0,
            admission_id: admission.to_string(),
            status: AdmissionState::Started,
            outcome: None,
            execution_generation: Some(generation),
            authority_epoch: 1,
        },
    )
}

fn settled(at_ms: i64, key: &str, admission: &str, usage: Value) -> (i64, Record) {
    (
        at_ms,
        Record::TurnSettled {
            key: key.to_string(),
            attempt: 0,
            turn: 0,
            admission_id: admission.to_string(),
            outcome: "completed".to_string(),
            final_text: Some("done".to_string()),
            final_text_ref: None,
            result_meta: json!({}),
            usage: Some(usage),
        },
    )
}

fn finished(at_ms: i64, key: &str, outcome: CallOutcome) -> (i64, Record) {
    (
        at_ms,
        Record::CallFinished {
            key: key.to_string(),
            attempt: 0,
            outcome,
            turns: 1,
        },
    )
}

fn completed() -> CallOutcome {
    CallOutcome::Completed {
        value: Some(json!("ok")),
        value_ref: None,
    }
}

fn run_started(at_ms: i64) -> (i64, Record) {
    (
        at_ms,
        Record::RunStarted {
            run_id: RUN_ID.to_string(),
            script_sha256: SCRIPT_SHA.to_string(),
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn agent_span(
    key: &str,
    label: &str,
    phase: &str,
    session: &str,
    admission: &str,
    started_ms: i64,
    finished_ms: i64,
    tokens: u64,
    workflow_path: Vec<String>,
) -> Vec<(i64, Record)> {
    vec![
        engine(
            started_ms,
            EngineEvent::AgentStarted {
                request: request(label, phase, key, workflow_path.clone()),
                replayed: false,
            },
        ),
        intent(started_ms + 10, key, label, phase, workflow_path),
        bound(started_ms + 100, key, session),
        admitted(started_ms + 200, key, admission, 1),
        settled(
            finished_ms - 100,
            key,
            admission,
            json!({
                "total_tokens": tokens,
                "session_cost": { "cost_status": "actual", "estimated_cost_usd": 0.1 }
            }),
        ),
        finished(finished_ms, key, completed()),
    ]
}

/// The recorded three-phase review, in file order.
pub fn finished_lines() -> Vec<RecordLine> {
    let mut rows = vec![
        run_started(RUN_STARTED_MS),
        engine(
            1_100,
            EngineEvent::Started {
                meta: meta(),
                warnings: Vec::new(),
            },
        ),
        engine(
            1_200,
            EngineEvent::Phase {
                title: "plan".to_string(),
                workflow_path: Vec::new(),
            },
        ),
    ];
    rows.extend(agent_span(
        KEY_CORRECTNESS,
        "correctness",
        "plan",
        "sess-correctness",
        "adm-correctness",
        CORRECTNESS_STARTED_MS,
        CORRECTNESS_FINISHED_MS,
        1_050,
        Vec::new(),
    ));
    rows.push(engine(
        2_700,
        EngineEvent::Phase {
            title: "build".to_string(),
            workflow_path: Vec::new(),
        },
    ));
    rows.extend(agent_span(
        KEY_SECURITY,
        "security",
        "build",
        "sess-security",
        "adm-security",
        SECURITY_STARTED_MS,
        SECURITY_FINISHED_MS,
        1_049,
        Vec::new(),
    ));
    rows.push(engine(
        4_200,
        EngineEvent::Phase {
            title: "check".to_string(),
            workflow_path: Vec::new(),
        },
    ));
    rows.extend(agent_span(
        KEY_SIMPLICITY,
        "simplicity",
        "check",
        "sess-simplicity",
        "adm-simplicity",
        SIMPLICITY_STARTED_MS,
        SIMPLICITY_FINISHED_MS,
        999,
        Vec::new(),
    ));
    rows.push((
        5_200,
        Record::RunFinished {
            outcome: RunOutcomeKind::Completed,
            summary: FinishSummary {
                completed: 3,
                failed: 0,
                skipped: 0,
                parked: 0,
                unfinished: 0,
            },
        },
    ));
    stamp(rows)
}

/// Parent phase, a nested workflow, and 60 log lines (the cap is 50).
pub fn nested_lines() -> Vec<RecordLine> {
    let mut rows = vec![
        run_started(RUN_STARTED_MS),
        engine(
            1_100,
            EngineEvent::Phase {
                title: "plan".to_string(),
                workflow_path: Vec::new(),
            },
        ),
        engine(
            1_200,
            EngineEvent::ChildStarted {
                workflow_path: vec![CHILD_PATH.to_string()],
                meta: WorkflowMeta {
                    name: "child".to_string(),
                    description: "Nested scan.".to_string(),
                    when_to_use: None,
                    phases: vec![PhaseMeta {
                        title: "scan".to_string(),
                        detail: None,
                        model: None,
                    }],
                },
                warnings: Vec::new(),
            },
        ),
        engine(
            1_300,
            EngineEvent::Phase {
                title: "scan".to_string(),
                workflow_path: vec![CHILD_PATH.to_string()],
            },
        ),
    ];
    for index in 0..60 {
        rows.push(engine(
            1_400 + i64::from(index),
            EngineEvent::Log {
                message: format!("log {index}"),
                workflow_path: Vec::new(),
            },
        ));
    }
    stamp(rows)
}

/// One row for each journal-derived state, plus a late start after a finish.
pub fn state_lines() -> Vec<RecordLine> {
    let mut rows = vec![run_started(RUN_STARTED_MS)];
    rows.extend(agent_span(
        KEY_CORRECTNESS,
        "correctness",
        "plan",
        "sess-correctness",
        "adm-correctness",
        1_400,
        2_600,
        10,
        Vec::new(),
    ));
    rows.push(engine(
        2_700,
        EngineEvent::AgentStarted {
            request: request("correctness", "plan", KEY_CORRECTNESS, Vec::new()),
            replayed: false,
        },
    ));
    rows.push(intent(3_000, KEY_FAILED, "failed-row", "plan", Vec::new()));
    rows.push(finished(
        3_100,
        KEY_FAILED,
        CallOutcome::Failed {
            reason: "checks failed".to_string(),
        },
    ));
    rows.push(intent(
        3_200,
        KEY_SKIPPED,
        "skipped-row",
        "plan",
        Vec::new(),
    ));
    rows.push(finished(3_300, KEY_SKIPPED, CallOutcome::Skipped));
    rows.push(intent(3_400, KEY_PARKED, "parked-row", "plan", Vec::new()));
    rows.push(admitted(3_500, KEY_PARKED, "adm-parked", 2));
    rows.push((
        3_600,
        Record::CallAttention {
            key: KEY_PARKED.to_string(),
            attempt: 0,
            episode: 0,
            cause: AttentionCause::Unknown,
            turn: Some(0),
            admission_id: Some("adm-parked".to_string()),
            execution_generation: Some(2),
            detail: "admission fate unproven".to_string(),
        },
    ));
    rows.push(intent(3_700, KEY_ENDED, "ended-row", "plan", Vec::new()));
    rows.push(engine(
        3_800,
        EngineEvent::AgentFinished {
            call_id: call_id(KEY_ENDED),
            completion_seq: None,
            outcome: OutcomeKind::Aborted,
        },
    ));
    rows.push(intent(
        3_900,
        KEY_RUNNING,
        "running-row",
        "plan",
        Vec::new(),
    ));
    rows.push(admitted(4_000, KEY_RUNNING, "adm-running", 3));
    rows.push(intent(4_100, KEY_QUEUED, "queued-row", "plan", Vec::new()));
    stamp(rows)
}

/// Settled 100, the same receipt again, then 40, each carrying cumulative 140.
pub fn usage_lines() -> Vec<RecordLine> {
    stamp(vec![
        run_started(RUN_STARTED_MS),
        intent(1_100, KEY_USAGE, "usage-row", "plan", Vec::new()),
        settled(
            1_200,
            KEY_USAGE,
            "adm-1",
            json!({ "total_tokens": 100, "total": 140 }),
        ),
        settled(
            1_300,
            KEY_USAGE,
            "adm-1",
            json!({ "total_tokens": 100, "total": 140 }),
        ),
        settled(
            1_400,
            KEY_USAGE,
            "adm-2",
            json!({ "total_tokens": 40, "total": 140 }),
        ),
    ])
}

pub fn cost_lines(status: &str) -> Vec<RecordLine> {
    stamp(vec![
        run_started(RUN_STARTED_MS),
        intent(1_100, KEY_USAGE, "usage-row", "plan", Vec::new()),
        settled(
            1_200,
            KEY_USAGE,
            "adm-cost",
            json!({
                "total_tokens": 10,
                "session_cost": { "cost_status": status, "estimated_cost_usd": 0.0 }
            }),
        ),
    ])
}

/// An aborted execution, then a resume whose new attempt is only queued.
pub fn execution_lines() -> Vec<RecordLine> {
    stamp(vec![
        run_started(RUN_STARTED_MS),
        intent(1_100, KEY_FAILED, "failed-row", "plan", Vec::new()),
        finished(
            1_200,
            KEY_FAILED,
            CallOutcome::Failed {
                reason: "stopped".to_string(),
            },
        ),
        (
            1_300,
            Record::RunFinished {
                outcome: RunOutcomeKind::Aborted,
                summary: FinishSummary {
                    completed: 0,
                    failed: 1,
                    skipped: 0,
                    parked: 0,
                    unfinished: 0,
                },
            },
        ),
        (
            2_000,
            Record::RunResumed {
                script_sha256: SCRIPT_SHA.to_string(),
                resume_count: 1,
            },
        ),
        attempt_intent(2_100, KEY_FAILED, 1),
    ])
}

/// Durations live on the line's `at_ms`. Sequence numbers are not times.
pub fn timestamp_lines() -> Vec<RecordLine> {
    stamp(vec![
        run_started(5_000),
        engine(
            5_000,
            EngineEvent::AgentStarted {
                request: request("span-row", "plan", KEY_CORRECTNESS, Vec::new()),
                replayed: false,
            },
        ),
        intent(5_000, KEY_CORRECTNESS, "span-row", "plan", Vec::new()),
        finished(8_000, KEY_CORRECTNESS, completed()),
        engine(
            5_000,
            EngineEvent::AgentStarted {
                request: request("short-row", "plan", KEY_SECURITY, Vec::new()),
                replayed: false,
            },
        ),
        intent(5_000, KEY_SECURITY, "short-row", "plan", Vec::new()),
        finished(5_100, KEY_SECURITY, completed()),
    ])
}

/// `count` engine log lines after the opening record.
pub fn many_lines(count: u64) -> Vec<RecordLine> {
    let mut rows = vec![run_started(RUN_STARTED_MS)];
    for index in 0..count {
        rows.push(engine(
            RUN_STARTED_MS,
            EngineEvent::Log {
                message: format!("row {index}"),
                workflow_path: Vec::new(),
            },
        ));
    }
    stamp(rows)
}

pub fn one_started() -> RecordLine {
    stamp(vec![run_started(RUN_STARTED_MS)]).remove(0)
}

/// A running row with a session and generation, built only from journal records.
pub fn live_running(key: &str, session: &str, generation: i64) -> Vec<RecordLine> {
    stamp(vec![
        run_started(RUN_STARTED_MS),
        intent(1_100, key, "running-row", "plan", Vec::new()),
        bound(1_200, key, session),
        admitted(1_300, key, "adm-1", generation),
    ])
}

/// One finished row and one failed row.
pub fn live_restart() -> Vec<RecordLine> {
    let mut rows = agent_span(
        KEY_CORRECTNESS,
        "done-row",
        "plan",
        "sess-done",
        "adm-done",
        1_400,
        2_600,
        10,
        Vec::new(),
    );
    rows.push(intent(3_000, KEY_FAILED, "failed-row", "plan", Vec::new()));
    rows.push(bound(3_100, KEY_FAILED, "sess-failed"));
    rows.push(finished(
        3_200,
        KEY_FAILED,
        CallOutcome::Failed {
            reason: "checks failed".to_string(),
        },
    ));
    stamp(rows)
}

/// A parked row under plan and a running row under build.
pub fn live_filter() -> Vec<RecordLine> {
    stamp(vec![
        run_started(RUN_STARTED_MS),
        intent(1_100, KEY_PARKED, "parked-row", "plan", Vec::new()),
        admitted(1_200, KEY_PARKED, "adm-parked", 2),
        (
            1_300,
            Record::CallAttention {
                key: KEY_PARKED.to_string(),
                attempt: 0,
                episode: 0,
                cause: AttentionCause::Unknown,
                turn: Some(0),
                admission_id: Some("adm-parked".to_string()),
                execution_generation: Some(2),
                detail: "admission fate unproven".to_string(),
            },
        ),
        intent(1_400, KEY_RUNNING, "running-row", "build", Vec::new()),
        admitted(1_500, KEY_RUNNING, "adm-running", 4),
    ])
}
