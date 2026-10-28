#![cfg(feature = "fake-gateway")]
mod support;
use serde_json::json;
use sha2::{Digest, Sha256};
use support::harness::*;
use talaria_workflow::journal::{
    CallOutcome, JournalError, NoGate, Record, RunJournal,
    fake_gateway::{FakeGateway, FakeOutcome},
};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn large_payloads_round_trip_and_verify() {
    let script = source("return await agent('x'.repeat(600*1024),{label:'large'});");
    let mut harness = Harness::new(&script);
    let result_text = "r".repeat(2 * 1024 * 1024);
    harness.gateway = FakeGateway::new().with_script(vec![rule("large", &result_text, 10, None)]);
    let paused = harness.crash_after_submit(&script).await;
    let original = paused.sent.lock().unwrap()[0].text.clone();
    assert_eq!(original.len(), 600 * 1024);
    let port = TracePort::new(harness.gateway.clone());
    let trace = port.requests.clone();
    let run = harness.run_port(&script, port, NoGate).await;
    assert_eq!(result(run.outcome), json!(result_text));
    {
        let requests = trace.lock().unwrap();
        let retried = requests.iter().find(|r| r["method"] == "submit").unwrap();
        assert_eq!(retried["params"]["text"], original);
    }
    let text = std::fs::read_to_string(harness.path().join("journal.jsonl")).unwrap();
    assert!(text.lines().all(|line| line.len() < 1024 * 1024));
    let records = harness.records();
    let blob = records
        .iter()
        .find_map(|line| match &line.record {
            Record::CallFinished {
                outcome:
                    CallOutcome::Completed {
                        value_ref: Some(blob),
                        ..
                    },
                ..
            } => Some(blob.clone()),
            _ => None,
        })
        .expect("large result must be externalised");
    let blob_path = harness.path().join("blobs").join(&blob.sha256);
    let bytes = std::fs::read(&blob_path).unwrap();
    let actual = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(actual, blob.sha256);
    assert_eq!(bytes.len() as u64, blob.bytes);
    std::fs::remove_file(&blob_path).unwrap();
    assert!(matches!(
        RunJournal::open(harness.dir.clone(), &script).await,
        Err(JournalError::Corrupt { .. })
    ));
    std::fs::write(&blob_path, &bytes).unwrap();
    std::fs::write(&blob_path, &bytes[..bytes.len() - 1]).unwrap();
    assert!(matches!(
        RunJournal::open(harness.dir.clone(), &script).await,
        Err(JournalError::Corrupt { .. })
    ));
    std::fs::write(&blob_path, &bytes).unwrap();
    let mut corrupt = bytes.clone();
    corrupt[0] ^= 1;
    std::fs::write(&blob_path, &corrupt).unwrap();
    let (journal, _, _) = RunJournal::open(harness.dir.clone(), &script)
        .await
        .unwrap();
    assert!(matches!(
        journal.engine_config(&harness.resolver),
        Err(JournalError::Corrupt { .. })
    ));
    drop(journal);
    std::fs::write(&blob_path, &bytes).unwrap();

    let script = source(
        "return await agent('schema',{label:'schema',schema:{type:'object',properties:{text:{type:'string'}}}});",
    );
    let mut harness = Harness::new(&script);
    let object = json!({"text":"s".repeat(700*1024)});
    harness.gateway =
        FakeGateway::new().with_script(vec![rule("schema", &object.to_string(), 1, None)]);
    let run = harness.run(&script).await;
    assert_eq!(result(run.outcome), object);
    assert!(
        std::fs::read_to_string(harness.path().join("journal.jsonl"))
            .unwrap()
            .lines()
            .all(|line| line.len() < 1024 * 1024)
    );

    let script = source("return await agent('x' + '\\n'.repeat(600*1024),{label:'escaped'});");
    let mut harness = Harness::new(&script);
    harness.gateway = FakeGateway::new().with_script(vec![rule("escaped", "ok", 1, None)]);
    assert_eq!(result(harness.run(&script).await.outcome), json!("ok"));
    assert!(
        std::fs::read_to_string(harness.path().join("journal.jsonl"))
            .unwrap()
            .lines()
            .all(|line| line.len() < 1024 * 1024)
    );

    let script = source("return await agent('attempt',{label:'attempt'});");
    let mut harness = Harness::new(&script);
    let mut failed = rule("attempt", &"a".repeat(300 * 1024), 1, None);
    failed.outcome = FakeOutcome::Failed;
    harness.gateway = FakeGateway::new().with_script(vec![failed]);
    harness.run(&script).await;
    harness.gateway = harness.gateway.clone().with_script(vec![rule(
        "attempt",
        &"b".repeat(300 * 1024),
        1,
        None,
    )]);
    harness.run(&script).await;
    let blobs = std::fs::read_dir(harness.path().join("blobs"))
        .unwrap()
        .map(|entry| std::fs::read(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert!(
        blobs
            .iter()
            .any(|bytes| bytes.windows(100).any(|w| w == [b'a'; 100]))
    );
    assert!(
        blobs
            .iter()
            .any(|bytes| bytes.windows(100).any(|w| w == [b'b'; 100]))
    );
}
