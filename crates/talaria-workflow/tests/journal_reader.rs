#![cfg(feature = "fake-gateway")]
mod support;
use std::io::Write;
use support::harness::*;
use talaria_workflow::{
    engine::EngineEvent,
    journal::{JournalReader, RunJournal},
};

#[tokio::test(flavor = "current_thread")]
async fn tail_reads_incrementally() {
    let (_temp, dir) = new_dir();
    let (journal, _, _) = RunJournal::open(dir.clone(), SCRIPT).await.unwrap();
    let path = records_path(&dir);
    let (first, offset) = JournalReader::read_from(&path, 0).unwrap();
    assert!(!first.is_empty());
    journal
        .record_engine_event(&EngineEvent::Log {
            message: "new".into(),
            workflow_path: vec![],
        })
        .await
        .unwrap();
    let (second, next) = JournalReader::read_from(&path, offset).unwrap();
    assert_eq!(second.len(), 1);
    assert!(next > offset);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{\"v\":1")
        .unwrap();
    let (partial, last) = JournalReader::read_from(&path, next).unwrap();
    assert!(partial.is_empty());
    assert_eq!(last, next);
}
