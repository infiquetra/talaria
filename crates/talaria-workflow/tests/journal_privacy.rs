#![cfg(feature = "fake-gateway")]
mod support;
use serde_json::json;
use support::harness::*;
use talaria_workflow::journal::{JournalError, RunDir};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn no_absolute_paths_in_records() {
    let mut harness = Harness::new(SCRIPT);
    let cwd = harness
        .temp
        .path()
        .join("checkout")
        .to_string_lossy()
        .into_owned();
    harness.resolver.cwd = Some(cwd.clone());
    harness.run(SCRIPT).await;
    let contents = std::fs::read_to_string(harness.path().join("journal.jsonl")).unwrap();
    let mut count = 0;
    for line in contents.lines() {
        let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
        if value["t"] == "call_intent" {
            assert_eq!(value["launch"]["cwd"], cwd);
            value["launch"]["cwd"] = json!(null);
            count += 1;
        }
        let scrubbed = serde_json::to_string(&value).unwrap();
        assert!(!scrubbed.contains(&cwd));
        if let Ok(home) = std::env::var("HOME") {
            assert!(!scrubbed.contains(&home));
        }
        if let Ok(user) = std::env::var("USER") {
            assert!(!scrubbed.contains(&user));
        }
    }
    assert_eq!(count, 3);
}

#[tokio::test(flavor = "current_thread")]
async fn credentials_refused_and_files_private() {
    let temp = tempfile::tempdir().unwrap();
    for (field, value) in [
        ("roster", json!({"apiKey":"x"})),
        (
            "hermes",
            json!({"ticket_holder":"hermes-gateway-ticket.fixture"}),
        ),
    ] {
        let mut data = manifest();
        if field == "roster" {
            data.roster = value;
        } else {
            data.hermes = value;
        }
        match RunDir::create(temp.path(), data, SCRIPT) {
            Err(JournalError::Manifest(message)) => assert!(message.contains(field)),
            _ => panic!("credential was accepted"),
        }
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
    }
    let dir = RunDir::create(temp.path(), manifest(), SCRIPT).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        fn check(path: &std::path::Path) {
            for item in std::fs::read_dir(path).unwrap() {
                let path = item.unwrap().path();
                if path.is_dir() {
                    check(&path)
                } else {
                    assert_eq!(
                        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
                        0o600
                    )
                }
            }
        }
        check(dir.path());
    }
}
