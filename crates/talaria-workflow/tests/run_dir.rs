#![cfg(feature = "fake-gateway")]
mod support;
use support::harness::*;
use talaria_workflow::journal::{JournalError, Resolution, RunDir, RunId, RunJournal};

#[test]
fn create_writes_manifest_and_script_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let id = RunId::parse("01JB2X9Q4M8Z7K3T5V6W8Y0A1C").unwrap();
    let dir = RunDir::create_with_id(temp.path(), id.clone(), manifest(), SCRIPT).unwrap();
    let stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("manifest.json")).unwrap()).unwrap();
    assert_eq!(stored["run_id"], id.as_str());
    assert_eq!(stored["workflow"]["name"], "fixture");
    let hash = stored["workflow"]["script_sha256"].as_str().unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("scripts").join(format!("{hash}.js"))).unwrap(),
        SCRIPT
    );
    fn no_tmp(path: &std::path::Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            assert!(!entry.file_name().to_string_lossy().contains(".tmp"));
            if path.is_dir() {
                no_tmp(&path)
            }
        }
    }
    no_tmp(dir.path());
    assert!(matches!(
        RunDir::create_with_id(temp.path(), id, manifest(), SCRIPT),
        Err(JournalError::Exists { .. })
    ));
    let generated = RunDir::create(temp.path(), manifest(), SCRIPT).unwrap();
    assert_eq!(generated.run_id().as_str().len(), 26);
    assert!(RunId::parse(generated.run_id().as_str()).is_ok());
}

#[tokio::test(flavor = "current_thread")]
async fn second_writer_is_refused() {
    const CHILD: &str = "TALARIA_LOCK_CHILD_DIR";
    if let Some(path) = std::env::var_os(CHILD) {
        let dir = RunDir::open(std::path::Path::new(&path)).unwrap();
        dir.request_resolution(&key(), 0, 0, Resolution::Fail)
            .unwrap();
        assert!(matches!(
            RunJournal::open(dir, SCRIPT).await,
            Err(JournalError::Locked { .. })
        ));
        return;
    }
    let (_temp, dir) = new_dir();
    let (journal, _, _) = RunJournal::open(dir.clone(), SCRIPT).await.unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["second_writer_is_refused", "--exact", "--nocapture"])
        .env(CHILD, dir.path())
        .status()
        .unwrap();
    assert!(status.success());
    drop(journal);
    assert!(RunJournal::open(dir, SCRIPT).await.is_ok());
}
