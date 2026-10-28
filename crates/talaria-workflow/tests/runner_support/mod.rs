use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn id(number: u32) -> String {
    format!("{number:026}")
}

pub fn manifest(run_id: &str, started_at_ms: i64) -> Value {
    let mut value: Value =
        serde_json::from_str(include_str!("../data/runner/folders/manifest.json")).unwrap();
    value["run_id"] = json!(run_id);
    value["started_at_ms"] = json!(started_at_ms);
    value
}

pub fn folder(parent: &Path, run_id: &str, started_at_ms: i64, records: &[Value]) -> PathBuf {
    let dir = parent.join(run_id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec(&manifest(run_id, started_at_ms)).unwrap(),
    )
    .unwrap();
    write_records(&dir, records);
    dir
}

pub fn write_records(dir: &Path, records: &[Value]) {
    let mut bytes = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let mut line = record.clone();
        line["v"] = json!(1);
        line["seq"] = json!(index + 1);
        line["at_ms"] = json!(index);
        serde_json::to_writer(&mut bytes, &line).unwrap();
        bytes.push(b'\n');
    }
    fs::write(dir.join("journal.jsonl"), bytes).unwrap();
}

pub fn started(run_id: &str) -> Value {
    json!({"t":"run_started", "run_id":run_id, "script_sha256":"b".repeat(64)})
}

pub fn resumed() -> Value {
    json!({"t":"run_resumed", "script_sha256":"b".repeat(64), "resume_count":1})
}

pub fn intent(key: &str) -> Value {
    json!({"t":"call_intent", "key":key, "attempt":0,
        "call_id":{"digest":"1".repeat(64),"occurrence":0}, "label":"review",
        "phase":null, "workflow_path":[], "launch":{"profile":"fixture","model":"model", "toolsets":[],"skills":[]},
        "request_id":format!("wf:fixture:{key}")})
}

pub fn settled(key: &str, turn: u32, usage: Value) -> Value {
    json!({"t":"turn_settled", "key":key, "attempt":0, "turn":turn,
        "admission_id":format!("admission-{turn}"), "outcome":"completed",
        "final_text":"ok", "result_meta":{}, "usage":usage})
}

pub fn call_finished(key: &str, kind: &str) -> Value {
    let outcome = if kind == "completed" {
        json!({"kind":kind,"value":"ok"})
    } else {
        json!({"kind":kind})
    };
    json!({"t":"call_finished", "key":key, "attempt":0, "outcome":outcome, "turns":1})
}

pub fn attention(key: &str) -> Value {
    json!({"t":"call_attention", "key":key, "attempt":0, "episode":0,
        "cause":"unknown", "turn":0, "admission_id":"admission-0", "execution_generation":1,
        "detail":"receipt unknown"})
}

pub fn finished(outcome: &str) -> Value {
    json!({"t":"run_finished", "outcome":outcome,
        "summary":{"completed":0,"failed":0,"skipped":0,"parked":0,"unfinished":0}})
}
