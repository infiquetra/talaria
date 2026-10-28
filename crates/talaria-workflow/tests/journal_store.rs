#![cfg(feature = "fake-gateway")]
mod support;
use std::{
    io::Write,
    sync::{Arc, Mutex},
};
use support::harness::*;
use talaria_workflow::{
    engine::EngineEvent,
    journal::{
        AdmissionState, JournalError, JournalReader, JournalWarning, Record, RunJournal,
        testing::Writer,
    },
};

#[derive(Clone, Default)]
struct FileDouble {
    actions: Arc<Mutex<Vec<&'static str>>>,
    bytes: Arc<Mutex<Vec<u8>>>,
}
impl talaria_workflow::journal::testing::JournalFile for FileDouble {
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.actions.lock().unwrap().push("write");
        self.bytes.lock().unwrap().extend_from_slice(buf);
        Ok(())
    }
    fn sync_data(&mut self) -> std::io::Result<()> {
        self.actions.lock().unwrap().push("sync");
        Ok(())
    }
}
fn event(n: usize) -> Record {
    Record::Engine {
        event: Some(EngineEvent::Log {
            message: n.to_string(),
            workflow_path: vec![],
        }),
        event_ref: None,
    }
}
fn sample(dir: &talaria_workflow::journal::RunDir) -> Vec<Record> {
    vec![start_record(dir), event(1), event(2), event(3), event(4)]
}

#[tokio::test(flavor = "current_thread")]
async fn appends_are_durable_before_reply() {
    let file = FileDouble::default();
    let actions = file.actions.clone();
    let writer = Writer::spawn(file, 1);
    for i in 0..3 {
        let seq = writer.append(event(i)).await.unwrap();
        assert_eq!(seq, i as u64 + 1);
        assert_eq!(actions.lock().unwrap().last(), Some(&"sync"));
    }
    let log = actions.lock().unwrap();
    assert_eq!(&*log, &["write", "sync", "write", "sync", "write", "sync"]);
}

#[tokio::test(flavor = "current_thread")]
async fn group_commit_shares_syncs() {
    let file = FileDouble::default();
    let actions = file.actions.clone();
    let bytes = file.bytes.clone();
    let writer = Writer::spawn(file, 1);
    let futures = (0..100).map(|i| writer.append(event(i)));
    let mut seqs = futures_util::future::join_all(futures)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    seqs.sort_unstable();
    assert_eq!(seqs, (1..=100).collect::<Vec<_>>());
    let data = bytes.lock().unwrap();
    let records = data
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 100);
    for (i, record) in records.iter().enumerate() {
        assert_eq!(record["seq"], i as u64 + 1);
    }
    assert!(
        actions
            .lock()
            .unwrap()
            .iter()
            .filter(|action| **action == "sync")
            .count()
            < 100
    );
}

#[tokio::test(flavor = "current_thread")]
async fn torn_tail_is_dropped_and_truncated() {
    let (_temp, dir) = new_dir();
    write_records(&dir, &sample(&dir));
    let path = records_path(&dir);
    let baseline = std::fs::read(&path).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{\"v\":1")
        .unwrap();
    let (journal, _, warnings) = RunJournal::open(dir.clone(), SCRIPT).await.unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w, JournalWarning::TornTail { line: 6, .. }))
    );
    let (lines, _) = JournalReader::read_from(&path, 0).unwrap();
    assert_eq!(lines.len(), 6);
    assert_eq!(lines[5].seq, 6);
    assert!(matches!(lines[5].record, Record::RunResumed { .. }));
    journal
        .record_engine_event(&EngineEvent::Log {
            message: "next".into(),
            workflow_path: vec![],
        })
        .await
        .unwrap();
    let (lines, _) = JournalReader::read_from(&path, 0).unwrap();
    assert_eq!(lines.last().unwrap().seq, 7);
    drop(journal);
    std::fs::write(&path, &baseline).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{not json\n")
        .unwrap();
    let broken = std::fs::read(&path).unwrap();
    assert!(matches!(
        RunJournal::open(dir, SCRIPT).await,
        Err(JournalError::Corrupt { line: 6, .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), broken);
}

#[tokio::test(flavor = "current_thread")]
async fn corrupt_middle_line_refuses() {
    for bad in ["not json", r#"{"v":1,"seq":3,"at_ms":0,"t":"call_intent"}"#] {
        let (_temp, dir) = new_dir();
        write_records(&dir, &sample(&dir));
        let path = records_path(&dir);
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
        lines[2] = bad.into();
        let bytes = format!("{}\n", lines.join("\n"));
        std::fs::write(&path, &bytes).unwrap();
        assert!(matches!(
            RunJournal::open(dir, SCRIPT).await,
            Err(JournalError::Corrupt { line: 3, .. })
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn out_of_order_records_refuse() {
    let (_temp, dir) = new_dir();
    let records = vec![
        start_record(&dir),
        Record::TurnAdmitted {
            key: key(),
            attempt: 0,
            turn: 0,
            admission_id: "admission".into(),
            status: AdmissionState::Started,
            outcome: None,
            execution_generation: Some(1),
            authority_epoch: 0,
        },
    ];
    write_records(&dir, &records);
    assert!(matches!(
        RunJournal::open(dir, SCRIPT).await,
        Err(JournalError::Corrupt { line: 2, .. })
    ));
}
