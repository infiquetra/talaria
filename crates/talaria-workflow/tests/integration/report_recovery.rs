#![cfg(all(unix, feature = "fake-gateway"))]

use crate::support::harness::{launch, manifest, result, rule, source};
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::symlink,
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use talaria_workflow::{
    engine::{
        self, AgentRequest, BudgetSnapshot, CallId, EngineEvent, MapResolver, OutcomeKind,
        RunOutcomeKind,
    },
    hermes::SchemaTurnPolicy,
    journal::{
        AdmissionGate, GateDecision, GatePermit, GateReason, JournalReader, JournaledBackend,
        LaunchResolver, Record, ResolvedLaunch, RetryPolicy, RunDir, RunJournal,
        fake_gateway::{FakeGateway, FakeOutcome},
    },
    runner::agents::{RestartTarget, agent_result, request_restart},
    worktree::read_report_file,
};
use tokio_util::sync::CancellationToken;

const FILE: &str = "report.json";
const REPORT: &[u8] = b"{ \"finding\": \"src/lib.rs:17\" }\n";
const SCRIPT: &str = r#"
return await agent('Review the change', {
    label: 'review',
    isolation: 'worktree',
    reportFile: 'report.json',
    schema: {
        type: 'object',
        properties: {finding: {type: 'string'}},
        required: ['finding'],
    },
});
"#;

#[derive(Clone)]
struct Resolver {
    root: PathBuf,
    initial: Option<&'static [u8]>,
    can_launch: bool,
}
impl LaunchResolver for Resolver {
    fn resolve(&self, request: &AgentRequest) -> Result<ResolvedLaunch, String> {
        assert!(self.can_launch, "replay resolved a deleted worktree");
        let folder = self.root.join(format!(
            "{}-{}",
            request.call_id.digest, request.call_id.occurrence
        ));
        fs::create_dir_all(&folder).unwrap();
        if let Some(bytes) = self.initial {
            fs::write(folder.join(FILE), bytes).unwrap();
        }
        let mut launch = launch();
        launch.isolation = request.isolation;
        launch.cwd = Some(folder.to_str().unwrap().into());
        Ok(launch)
    }
}

async fn execute(
    dir: &RunDir,
    script: &str,
    gateway: FakeGateway,
    resolver: Resolver,
    written: Option<&'static [u8]>,
) -> Value {
    execute_with_stop(dir, script, gateway, resolver, written, None).await
}

#[derive(Clone, Copy)]
enum Stop {
    Run,
    Budget,
}

struct StopGate {
    stop: Option<Stop>,
    settled: Arc<AtomicBool>,
}
impl AdmissionGate for StopGate {
    fn acquire(
        &self,
        _: &AgentRequest,
        _: &ResolvedLaunch,
        _: GateReason,
    ) -> BoxFuture<'static, GateDecision> {
        Box::pin(async { GateDecision::Proceed(GatePermit::noop()) })
    }
    fn budget(&self) -> BudgetSnapshot {
        BudgetSnapshot::default()
    }
    fn budget_stop(&self) -> Option<String> {
        (matches!(self.stop, Some(Stop::Budget)) && self.settled.load(Ordering::SeqCst))
            .then(|| "token budget exhausted".into())
    }
    fn run_stopping(&self) -> bool {
        matches!(self.stop, Some(Stop::Run))
    }
}

async fn execute_with_stop(
    dir: &RunDir,
    script: &str,
    gateway: FakeGateway,
    resolver: Resolver,
    written: Option<&'static [u8]>,
    stop: Option<Stop>,
) -> Value {
    let (journal, _, _) = RunJournal::open(dir.clone(), script).await.unwrap();
    let journal = Arc::new(journal);
    let gate = StopGate {
        stop,
        settled: Arc::new(AtomicBool::new(false)),
    };
    let settled = gate.settled.clone();
    let root = resolver.root.clone();
    journal.observe_appends(move |record| {
        if matches!(record, Record::TurnSettled { .. }) {
            if let Some(bytes) = written {
                let folder = fs::read_dir(&root).unwrap().next().unwrap().unwrap().path();
                fs::write(folder.join(FILE), bytes).unwrap();
            }
            settled.store(true, Ordering::SeqCst);
        }
        Box::pin(async {})
    });
    let config = journal.engine_config(&resolver).unwrap();
    let backend = Arc::new(JournaledBackend::new(
        journal.clone(),
        gateway,
        SchemaTurnPolicy,
        resolver,
        gate,
        RetryPolicy::default(),
    ));
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<EngineEvent>();
    let forward = journal.forward_events(rx);
    let outcome = engine::run(
        engine::parse_script(script).unwrap(),
        config,
        backend,
        Arc::new(MapResolver::new()),
        tx,
        CancellationToken::new(),
    )
    .await;
    forward.await.unwrap().unwrap();
    journal.finish(RunOutcomeKind::Completed).await.unwrap();
    result(outcome)
}

#[tokio::test]
async fn attaching_an_older_report_preserves_the_run_and_requires_the_original_schema() {
    use talaria_workflow::runner::evidence::associate;
    let temp = tempfile::tempdir().unwrap();
    let script = source(&SCRIPT.replace("reportFile: 'report.json',", ""));
    let dir = RunDir::create(temp.path(), manifest(), &script).unwrap();
    let root = dir.path().canonicalize().unwrap().join("worktrees");
    let mut failure = rule("review", "worker failed", 0, None);
    failure.outcome = FakeOutcome::Failed;
    let resolver = Resolver {
        root: root.clone(),
        initial: None,
        can_launch: true,
    };
    let gateway = FakeGateway::new().with_script(vec![failure]);
    execute(&dir, &script, gateway, resolver, Some(b"{}")).await;
    let journal_path = dir.path().join("journal.jsonl");
    let original = fs::read(&journal_path).unwrap();
    let id = dir.run_id().as_str();
    let attach = || associate(temp.path(), id, "review", FILE);
    assert!(attach().is_err());
    let file = fs::read_dir(&root)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join(FILE);
    fs::write(file, REPORT).unwrap();
    assert_eq!(attach().unwrap(), attach().unwrap());
    fs::remove_dir_all(&root).unwrap();
    let agent = agent_result(temp.path(), id, "review").unwrap();
    assert_eq!(agent.state, "failed");
    assert!(agent.reason.is_some());
    assert_eq!(agent.value, Some(json!({"finding":"src/lib.rs:17"})));
    assert_eq!(fs::read(journal_path).unwrap(), original);
}

#[tokio::test]
async fn a_failed_or_unusable_reply_replays_with_its_report_after_worktree_removal() {
    for outcome in [FakeOutcome::Completed, FakeOutcome::Failed] {
        let temp = tempfile::tempdir().unwrap();
        let script = source(SCRIPT);
        let dir = RunDir::create(temp.path(), manifest(), &script).unwrap();
        let root = dir.path().canonicalize().unwrap().join("worktrees");
        let mut answer = rule("review", "review could not return its result", 0, None);
        answer.outcome = outcome;
        let gateway = FakeGateway::new().with_script(vec![answer]);
        let resolver = Resolver {
            root: root.clone(),
            initial: None,
            can_launch: true,
        };

        let first = execute(
            &dir,
            &script,
            gateway.clone(),
            resolver.clone(),
            Some(REPORT),
        )
        .await;
        assert_eq!(first["value"], json!({"finding": "src/lib.rs:17"}));
        assert_eq!(first["recoveredReport"]["path"], FILE);
        assert!(
            !first["recoveredReport"]["reason"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        assert_eq!(gateway.stats().admissions, 1, "no correction or retry");
        fs::remove_dir_all(&root).unwrap();

        let run = dir.run_id().as_str();
        let saved = agent_result(temp.path(), run, "review").unwrap();
        assert_eq!(saved.state, "failed");
        assert_eq!(
            saved.reason.as_deref(),
            first["recoveredReport"]["reason"].as_str()
        );
        assert_eq!(saved.value, Some(first["value"].clone()));
        let report = saved.recovered_report.unwrap();
        assert_eq!(report.path, FILE);
        assert_eq!(report.bytes, REPORT.len() as u64);
        assert_eq!(
            fs::read(dir.path().join("blobs").join(report.sha256)).unwrap(),
            REPORT
        );
        let replay = execute(
            &dir,
            &script,
            gateway.clone(),
            Resolver {
                can_launch: false,
                ..resolver.clone()
            },
            None,
        )
        .await;
        assert_eq!(replay, first);
        assert_eq!(gateway.stats().admissions, 1);
        let (lines, _) = JournalReader::read_from(dir.path(), 0).unwrap();
        assert!(lines.iter().any(|line| matches!(
            &line.record,
            Record::Engine {
                event: Some(EngineEvent::AgentFinished {
                    outcome: OutcomeKind::Failed { .. },
                    ..
                }),
                ..
            }
        )));
        assert!(
            lines
                .iter()
                .filter(|line| matches!(&line.record, Record::RunFinished {
            summary, ..
        } if summary.failed == 1 && summary.completed == 0))
                .count()
                == 2
        );
        assert!(
            lines
                .iter()
                .any(|line| matches!(&line.record, Record::TurnIntent {
            text: Some(text), ..
        } if text.contains("plain JSON") && text.contains(FILE)))
        );

        let restart =
            request_restart(temp.path(), run, &RestartTarget::Agent("review".into())).unwrap();
        let (journal, _, _) = RunJournal::open(dir.clone(), &script).await.unwrap();
        journal
            .request_restart_keys(&[restart.requested[0].key.clone()])
            .await
            .unwrap();
        drop(journal);
        // A crash after recording the request does not discard the prior report or authorization.
        assert_eq!(
            agent_result(temp.path(), run, "review").unwrap().value,
            saved.value
        );
        let gateway =
            gateway.with_script(vec![rule("review", r#"{"finding":"new review"}"#, 0, None)]);
        let retried = execute(&dir, &script, gateway.clone(), resolver, None).await;
        assert_eq!(
            retried,
            json!({"value":{"finding":"new review"},"recoveredReport":null})
        );
        assert_eq!(gateway.stats().admissions, 2);
    }
}

#[tokio::test]
async fn usable_replies_take_precedence_and_stale_or_invalid_reports_stay_unavailable() {
    for (reply, initial, written, expected) in [
        (
            r#"{"finding":"final reply"}"#,
            None,
            REPORT,
            json!({
                "value":{"finding":"final reply"},"recoveredReport":null
            }),
        ),
        ("unusable reply", Some(REPORT), REPORT, Value::Null),
        (
            "unusable reply",
            None,
            b"{\"wrong\":true}".as_slice(),
            Value::Null,
        ),
        (
            "unusable reply",
            None,
            b"markdown {\"finding\":\"test\"}".as_slice(),
            Value::Null,
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let script = source(SCRIPT);
        let dir = RunDir::create(temp.path(), manifest(), &script).unwrap();
        let resolver = Resolver {
            root: dir.path().canonicalize().unwrap().join("worktrees"),
            initial,
            can_launch: true,
        };
        let gateway = FakeGateway::new().with_script(vec![rule("review", reply, 0, None)]);
        assert_eq!(
            execute(&dir, &script, gateway, resolver, Some(written)).await,
            expected
        );
    }
}

#[tokio::test]
async fn interrupted_workers_keep_the_run_stop_or_budget_outcome() {
    for (stop, state, expected) in [
        (Stop::Run, "failed", Value::Null),
        (Stop::Budget, "stopped", json!("BudgetExhaustedError")),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let script = source(
            &SCRIPT
                .replace("return await agent", "try { return await agent")
                .replace("});", "}); } catch (error) { return error.name; }"),
        );
        let dir = RunDir::create(temp.path(), manifest(), &script).unwrap();
        let resolver = Resolver {
            root: dir.path().canonicalize().unwrap().join("worktrees"),
            initial: None,
            can_launch: true,
        };
        let mut reply = rule("review", "interrupted", 0, None);
        reply.outcome = FakeOutcome::Interrupted;
        let gateway = FakeGateway::new().with_script(vec![reply]);
        assert_eq!(
            execute_with_stop(&dir, &script, gateway, resolver, Some(REPORT), Some(stop)).await,
            expected
        );
        let saved = agent_result(temp.path(), dir.run_id().as_str(), "review").unwrap();
        assert_eq!(saved.state, state);
        assert!(saved.value.is_none());
        assert!(saved.recovered_report.is_none());
    }
}

#[test]
fn report_reads_reject_links_sibling_worktrees_and_non_relative_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let digest = "b".repeat(64);
    let id = CallId {
        digest: digest.clone(),
        occurrence: 0,
    };
    let tree = root.join("worktrees").join(format!("{digest}-0"));
    let sibling = root.join("worktrees").join(format!("{digest}-1"));
    fs::create_dir_all(&tree).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    fs::write(sibling.join(FILE), REPORT).unwrap();
    symlink(sibling.join(FILE), tree.join("linked.json")).unwrap();
    symlink(&sibling, tree.join("linked-dir")).unwrap();
    fs::hard_link(sibling.join(FILE), tree.join("hardlink.json")).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(tree.join("pipe.json"))
            .status()
            .unwrap()
            .success()
    );
    fs::write(tree.join("large.json"), vec![b' '; 1024 * 1024 + 1]).unwrap();
    let schema = json!({"type": "object"});
    for path in [
        "../report.json",
        "/report.json",
        "linked.json",
        "linked-dir/report.json",
        "hardlink.json",
        "pipe.json",
        "large.json",
    ] {
        assert!(
            read_report_file(&root, &tree, &id, path, &schema).is_err(),
            "{path}"
        );
    }
    assert!(read_report_file(&root, &sibling, &id, FILE, &schema).is_err());
}
