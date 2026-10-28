use serde_json::json;
use talaria_workflow::{
    engine::{AgentRequest, CallId},
    journal::ResolvedLaunch,
    journal::{AdmissionGate, CallOutcome, NoGate, Record, RecordLine, SettledTurn},
};
#[test]
fn failure_reason_and_retry_finish() {
    let key = "a".repeat(32);
    let record = RecordLine {
        v: 1,
        seq: 1,
        at_ms: 0,
        record: Record::CallRetryFinished {
            key: key.clone(),
            attempt: 0,
            outcome: CallOutcome::Failed {
                reason: "rate_limit".into(),
            },
            turns: 1,
        },
    };
    let encoded = serde_json::to_value(&record).unwrap();
    assert_eq!(encoded["t"], "call_finished");
    assert_eq!(encoded["retry"], true);
    assert_eq!(
        serde_json::from_value::<RecordLine>(encoded).unwrap(),
        record
    );
    let old = json!({"v":1,"seq":1,"at_ms":0,"t":"call_finished","key":key,"attempt":0,"outcome":{"kind":"failed","reason":"old"},"turns":1});
    let decoded: RecordLine = serde_json::from_value(old.clone()).unwrap();
    assert!(matches!(decoded.record, Record::CallFinished { .. }));
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("journal.jsonl");
    std::fs::write(&file, format!("{}\n", old)).unwrap();
    let (old_records, _) = talaria_workflow::journal::JournalReader::read_from(&file, 0).unwrap();
    assert_eq!(old_records, vec![decoded]);
    let request = AgentRequest {
        call_id: CallId {
            digest: "a".repeat(64),
            occurrence: 0,
        },
        label: "fixture".into(),
        phase: None,
        prompt: "fixture".into(),
        schema: None,
        tier: None,
        model: None,
        provider: None,
        effort: None,
        profile: None,
        toolsets: None,
        skills: None,
        isolation: None,
        max_turns: None,
        workflow_path: vec![],
    };
    let launch = ResolvedLaunch {
        profile: "fixture".into(),
        model: "m".into(),
        provider: Some("alpha".into()),
        effort: None,
        toolsets: vec![],
        skills: vec![],
        max_turns: None,
        isolation: None,
        cwd: None,
    };
    let settled = SettledTurn {
        turn: 0,
        outcome: "failed".into(),
        final_text: None,
        result_meta: json!({"failure_reason":"rate_limit"}),
        usage: None,
    };
    assert_eq!(settled.result_meta["failure_reason"], "rate_limit");
    assert!(!NoGate.retry_after_failure(&request, &launch, &settled, 0));
}
