mod runner_support;

use runner_support::*;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use talaria_workflow::runner::{RunStatus, discover_runs, runs_dir, workflows_dir};

#[test]
fn runs_dir_precedence() {
    const CHILD: &str = "TALARIA_RUNS_DIR_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let expected_runs = std::env::var_os("TALARIA_TEST_EXPECT_RUNS").unwrap();
        let expected_workflows = std::env::var_os("TALARIA_TEST_EXPECT_WORKFLOWS").unwrap();
        let flag = std::env::var_os("TALARIA_TEST_FLAG").map(std::path::PathBuf::from);
        assert_eq!(runs_dir(flag.as_deref()), Path::new(&expected_runs));
        assert_eq!(workflows_dir(None), Path::new(&expected_workflows));
        let workflows_flag = Path::new("chosen-workflows");
        assert_eq!(workflows_dir(Some(workflows_flag)), workflows_flag);
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let config = temp.path().join("config");
    let env_runs = temp.path().join("env-runs");
    let flag = temp.path().join("flag-runs");
    let config_runs = config.join("runs");
    for (configured, explicit, expected) in [
        (
            Some(env_runs.as_os_str()),
            Some(flag.as_os_str()),
            flag.as_path(),
        ),
        (Some(env_runs.as_os_str()), None, env_runs.as_path()),
        (Some(std::ffi::OsStr::new("")), None, config_runs.as_path()),
        (
            Some(std::ffi::OsStr::new("  \t")),
            None,
            config_runs.as_path(),
        ),
        (None, None, config_runs.as_path()),
    ] {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args(["runs_dir_precedence", "--exact", "--nocapture"])
            .env(CHILD, "1")
            .env("HOME", &home)
            .env("TALARIA_CONFIG_DIR", &config)
            .env("TALARIA_TEST_EXPECT_RUNS", expected)
            .env("TALARIA_TEST_EXPECT_WORKFLOWS", config.join("workflows"))
            .env_remove("TALARIA_RUNS_DIR")
            .env_remove("TALARIA_TEST_FLAG");
        if let Some(value) = configured {
            child.env("TALARIA_RUNS_DIR", value);
        }
        if let Some(value) = explicit {
            child.env("TALARIA_TEST_FLAG", value);
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["runs_dir_precedence", "--exact", "--nocapture"])
        .env(CHILD, "1")
        .env("HOME", &home)
        .env_remove("TALARIA_CONFIG_DIR")
        .env_remove("TALARIA_RUNS_DIR")
        .env_remove("TALARIA_TEST_FLAG")
        .env("TALARIA_TEST_EXPECT_RUNS", home.join(".talaria/runs"))
        .env(
            "TALARIA_TEST_EXPECT_WORKFLOWS",
            home.join(".talaria/workflows"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!home.exists() && !config.exists() && !env_runs.exists() && !flag.exists());
}

#[test]
fn discover_empty_and_missing() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    assert!(discover_runs(&missing).is_empty());
    assert!(!missing.exists());
    let empty = temp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    assert!(discover_runs(&empty).is_empty());
    assert_eq!(fs::read_dir(&empty).unwrap().count(), 0);
}

#[test]
fn discover_summarises_each_status() {
    let temp = tempfile::tempdir().unwrap();
    let a = "a".repeat(32);
    let b = "b".repeat(32);
    let running_id = id(6);
    let running = folder(
        temp.path(),
        &running_id,
        600,
        &[started(&running_id), intent(&a)],
    );
    let lock = fs::File::create(running.join("run.lock")).unwrap();
    lock.try_lock().unwrap();
    let parked_id = id(5);
    folder(
        temp.path(),
        &parked_id,
        500,
        &[started(&parked_id), intent(&a), attention(&a)],
    );
    let completed_id = id(4);
    folder(
        temp.path(),
        &completed_id,
        400,
        &[
            started(&completed_id),
            intent(&a),
            settled(
                &a,
                0,
                json!({"total_tokens":10,"session_cost":{"cost_status":"actual","estimated_cost_usd":0.2}}),
            ),
            settled(
                &a,
                1,
                json!({"input_tokens":3,"output_tokens":2,"session_cost":{"cost_status":"estimated","estimated_cost_usd":0.4}}),
            ),
            call_finished(&a, "completed"),
            intent(&b),
            call_finished(&b, "skipped"),
            finished("completed"),
        ],
    );
    let failed_id = id(3);
    folder(
        temp.path(),
        &failed_id,
        300,
        &[
            started(&failed_id),
            intent(&a),
            settled(
                &a,
                0,
                json!({"total_tokens":9,"session_cost":{"cost_status":"unknown","estimated_cost_usd":0.0}}),
            ),
            finished("failed"),
        ],
    );
    let aborted_id = id(2);
    folder(
        temp.path(),
        &aborted_id,
        200,
        &[started(&aborted_id), finished("aborted")],
    );
    let interrupted_id = id(1);
    let interrupted = folder(
        temp.path(),
        &interrupted_id,
        100,
        &[started(&interrupted_id)],
    );

    let rows = discover_runs(temp.path());
    assert_eq!(rows.len(), 6);
    assert_eq!(
        rows.iter().map(|r| r.status).collect::<Vec<_>>(),
        [
            RunStatus::Running,
            RunStatus::Parked,
            RunStatus::Completed,
            RunStatus::Failed,
            RunStatus::Aborted,
            RunStatus::Interrupted,
        ]
    );
    assert_eq!(
        rows.iter().map(|r| r.run_id.as_str()).collect::<Vec<_>>(),
        [
            running_id.as_str(),
            parked_id.as_str(),
            completed_id.as_str(),
            failed_id.as_str(),
            aborted_id.as_str(),
            interrupted_id.as_str()
        ]
    );
    assert_eq!(
        (
            rows[0].agents_finished,
            rows[0].agents_total_known,
            rows[0].parked
        ),
        (0, 1, 0)
    );
    assert_eq!(
        (
            rows[1].agents_finished,
            rows[1].agents_total_known,
            rows[1].parked
        ),
        (0, 1, 1)
    );
    assert_eq!(
        (
            rows[2].agents_finished,
            rows[2].agents_total_known,
            rows[2].parked
        ),
        (2, 2, 0)
    );
    assert_eq!(
        (rows[2].tokens_spent, rows[2].money_spent_usd),
        (Some(15), Some(0.4))
    );
    assert_eq!(
        (rows[3].tokens_spent, rows[3].money_spent_usd),
        (Some(9), None)
    );
    assert_eq!(
        (rows[4].tokens_spent, rows[4].money_spent_usd),
        (None, None)
    );
    assert_eq!(
        (rows[5].tokens_spent, rows[5].money_spent_usd),
        (None, None)
    );
    for row in &rows {
        assert_eq!(row.name, "fixture");
        assert_eq!(row.profile, "fixture");
        assert!(row.warning.is_none());
        let value = serde_json::to_value(row).unwrap();
        assert_eq!(value["warning"], Value::Null);
    }
    assert!(!interrupted.join("run.lock").exists());
    drop(lock);
    let after = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(running.join("run.lock"))
        .unwrap();
    after.try_lock().unwrap();
}

#[test]
fn discover_tolerates_unreadable_folders() {
    let temp = tempfile::tempdir().unwrap();
    let lone = temp.path().join("only-lock");
    fs::create_dir(&lone).unwrap();
    fs::write(lone.join("run.lock"), "").unwrap();
    let bad_manifest = temp.path().join("bad-manifest");
    fs::create_dir(&bad_manifest).unwrap();
    fs::write(bad_manifest.join("manifest.json"), "!").unwrap();
    let bad_id = id(1);
    let bad_journal = folder(temp.path(), &bad_id, 100, &[started(&bad_id)]);
    let mut bytes = fs::read(bad_journal.join("journal.jsonl")).unwrap();
    bytes.extend_from_slice(b"!\n");
    fs::write(bad_journal.join("journal.jsonl"), bytes).unwrap();
    let missing_id = id(2);
    let missing_journal = folder(temp.path(), &missing_id, 200, &[]);
    fs::remove_file(missing_journal.join("journal.jsonl")).unwrap();
    fs::create_dir(temp.path().join("unrelated")).unwrap();
    let good_id = id(3);
    folder(
        temp.path(),
        &good_id,
        300,
        &[started(&good_id), finished("completed")],
    );

    let reason = serde_json::from_slice::<Value>(b"!")
        .unwrap_err()
        .to_string();
    let rows = discover_runs(temp.path());
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].run_id, good_id);
    assert_eq!(rows[0].status, RunStatus::Completed);
    for (run_id, warning) in [
        ("only-lock".to_owned(), "missing manifest.json".to_owned()),
        (
            "bad-manifest".to_owned(),
            format!("corrupt manifest.json: {reason}"),
        ),
        (bad_id, format!("corrupt journal.jsonl at line 2: {reason}")),
        (missing_id, "missing journal.jsonl".to_owned()),
    ] {
        let row = rows.iter().find(|r| r.run_id == run_id).unwrap();
        assert_eq!(row.status, RunStatus::Unreadable);
        assert_eq!(
            (&*row.name, &*row.profile, row.started_at_ms),
            ("(unreadable)", "", 0)
        );
        assert_eq!(
            (row.agents_finished, row.agents_total_known, row.parked),
            (0, 0, 0)
        );
        assert_eq!((row.tokens_spent, row.money_spent_usd), (None, None));
        assert_eq!(row.warning.as_deref(), Some(warning.as_str()));
        assert_eq!(row.run_dir, temp.path().join(&run_id).to_string_lossy());
    }
}

#[test]
fn discover_resumed_execution_status() {
    let temp = tempfile::tempdir().unwrap();
    let run_id = id(1);
    let dir = folder(
        temp.path(),
        &run_id,
        100,
        &[started(&run_id), finished("failed"), resumed()],
    );
    let lock = fs::File::create(dir.join("run.lock")).unwrap();
    lock.try_lock().unwrap();
    assert_eq!(discover_runs(temp.path())[0].status, RunStatus::Running);
    drop(lock);
    assert_eq!(discover_runs(temp.path())[0].status, RunStatus::Interrupted);
    let probe = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("run.lock"))
        .unwrap();
    probe.try_lock().unwrap();
}
