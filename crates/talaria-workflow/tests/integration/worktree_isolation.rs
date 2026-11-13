use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Arc,
    thread,
};
use talaria_workflow::{
    engine::{AgentRequest, CallId, Isolation},
    hermes::{ExplicitLaunch, LaunchDefaults, WorktreeProvider},
    worktree::GitWorktrees,
};
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git not available")
}
fn good(dir: &Path, args: &[&str]) -> String {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}
fn request(digest: &str, occurrence: u32) -> AgentRequest {
    AgentRequest {
        call_id: CallId {
            digest: digest.into(),
            occurrence,
        },
        label: "test".into(),
        phase: None,
        prompt: "test".into(),
        schema: None,
        tier: None,
        model: Some("scripted".into()),
        provider: Some("scripted".into()),
        effort: None,
        profile: None,
        toolsets: None,
        skills: None,
        isolation: Some(Isolation::Worktree),
        max_turns: None,
        workflow_path: vec![],
    }
}
fn commit(dir: &Path, message: &str) {
    good(dir, &["add", "--all"]);
    good(
        dir,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            message,
        ],
    );
}
struct Fixture {
    _tmp: TempDir,
    repo: PathBuf,
    run: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let run = tmp.path().join("run");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&run).unwrap();
        good(&repo, &["init", "-b", "main"]);
        fs::write(repo.join("tracked"), "initial").unwrap();
        commit(&repo, "initial");
        Self {
            _tmp: tmp,
            repo: repo.canonicalize().unwrap(),
            run: run.canonicalize().unwrap(),
        }
    }
    fn provider(&self) -> GitWorktrees {
        GitWorktrees::new(self.run.clone(), self.repo.clone(), "run1".into())
    }
    fn req(&self) -> AgentRequest {
        request(&"a".repeat(64), 0)
    }
    fn name(r: &AgentRequest) -> String {
        format!("{}-{}", r.call_id.digest, r.call_id.occurrence)
    }
    fn path(&self, r: &AgentRequest) -> PathBuf {
        self.run.join("worktrees").join(Self::name(r))
    }
    fn side(&self, r: &AgentRequest) -> PathBuf {
        self.run
            .join("worktrees")
            .join(format!("{}.json", Self::name(r)))
    }
    fn branch(r: &AgentRequest) -> String {
        format!("talaria/wf/run1/{}", Self::name(r))
    }
}
#[test]
fn creates_worktree_on_new_branch_at_head() {
    let f = Fixture::new();
    let r = f.req();
    let p = f.provider();
    let base = good(&f.repo, &["rev-parse", "HEAD"]);
    let path = p.worktree_for(&r).unwrap();
    assert_eq!(path, f.path(&r).canonicalize().unwrap());
    assert_ne!(path, f.repo);
    assert_eq!(
        good(&path, &["branch", "--show-current"]),
        Fixture::branch(&r)
    );
    assert_eq!(good(&path, &["rev-parse", "HEAD"]), base);
    let v: serde_json::Value = serde_json::from_slice(&fs::read(f.side(&r)).unwrap()).unwrap();
    let obj = v.as_object().unwrap();
    assert_eq!(obj.len(), 6);
    assert_eq!(v["call_digest"], r.call_id.digest);
    assert_eq!(v["occurrence"], 0);
    assert_eq!(v["branch"], Fixture::branch(&r));
    assert_eq!(v["base_commit"], base);
    assert_eq!(v["repo_root"], f.repo.to_str().unwrap());
    assert_eq!(v["base_dirty"], false);
}
#[test]
fn same_call_same_folder_with_edits_intact() {
    let f = Fixture::new();
    let r = f.req();
    let p = f.provider();
    let a = p.worktree_for(&r).unwrap();
    let side = fs::read(f.side(&r)).unwrap();
    fs::write(a.join("tracked"), "edited").unwrap();
    let b = p.worktree_for(&r).unwrap();
    assert_eq!(a, b);
    assert_eq!(fs::read_to_string(b.join("tracked")).unwrap(), "edited");
    assert_eq!(fs::read(f.side(&r)).unwrap(), side);
    assert_eq!(
        good(&f.repo, &["branch", "--list", &Fixture::branch(&r)])
            .lines()
            .count(),
        1
    );
}
#[test]
fn different_calls_different_folders_and_branches() {
    let f = Fixture::new();
    let p = f.provider();
    let a = f.req();
    let b = request(&format!("{}{}", "a".repeat(12), "b".repeat(52)), 0);
    let c = request(&a.call_id.digest, 1);
    let paths: Vec<_> = [&a, &b, &c]
        .iter()
        .map(|r| p.worktree_for(r).unwrap())
        .collect();
    for i in 0..3 {
        assert_ne!(paths[i], f.repo);
        for j in i + 1..3 {
            assert_ne!(paths[i], paths[j]);
        }
    }
    for (i, r) in [&a, &b, &c].iter().enumerate() {
        assert_eq!(
            good(&paths[i], &["branch", "--show-current"]),
            Fixture::branch(r)
        );
    }
}
#[test]
fn non_git_source_fails_without_fallback() {
    let f = Fixture::new();
    let dir = f._tmp.path().join("plain");
    fs::create_dir(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let out = git(&dir, &["rev-parse", "--show-toplevel"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let msg = stderr.lines().next().unwrap_or("").trim();
    let expected = format!(
        "worktree isolation needs a git repository: '{}' is not inside one{}",
        dir.display(),
        if msg.is_empty() {
            String::new()
        } else {
            format!(": {msg}")
        }
    );
    let p = GitWorktrees::new(f.run.clone(), dir, "run1".into());
    assert_eq!(p.worktree_for(&f.req()).unwrap_err(), expected);
    assert!(!f.path(&f.req()).exists());
    assert!(!f.side(&f.req()).exists());
}
#[test]
fn foreign_folder_at_the_path_fails_closed() {
    let f = Fixture::new();
    let r = f.req();
    let path = f.path(&r);
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("keep"), "untouched").unwrap();
    let p = f.provider();
    assert_eq!(
        p.worktree_for(&r).unwrap_err(),
        format!(
            "worktree path exists but is not this run's worktree: {}",
            path.display()
        )
    );
    assert_eq!(fs::read_to_string(path.join("keep")).unwrap(), "untouched");
    assert!(!f.side(&r).exists());
    fs::remove_dir_all(&path).unwrap();
    good(&f.repo, &["init", "-b", "main", path.to_str().unwrap()]);
    assert!(p.worktree_for(&r).is_err());
    assert!(!f.side(&r).exists());
}
#[test]
fn branch_survives_folder_loss_and_reattaches() {
    let f = Fixture::new();
    let r = f.req();
    let p = f.provider();
    let path = p.worktree_for(&r).unwrap();
    fs::write(path.join("extra"), "persist").unwrap();
    commit(&path, "work");
    let head = good(&path, &["rev-parse", "HEAD"]);
    fs::remove_dir_all(&path).unwrap();
    let restored = p.worktree_for(&r).unwrap();
    assert_eq!(restored, path);
    assert_eq!(good(&path, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        good(&path, &["branch", "--show-current"]),
        Fixture::branch(&r)
    );
    assert_eq!(fs::read_to_string(path.join("extra")).unwrap(), "persist");
}
#[test]
fn base_is_pinned_when_head_moves() {
    let f = Fixture::new();
    let p = f.provider();
    let a = f.req();
    let old = p.worktree_for(&a).unwrap();
    let old_head = good(&old, &["rev-parse", "HEAD"]);
    let old_side = fs::read(f.side(&a)).unwrap();
    fs::write(f.repo.join("new"), "new").unwrap();
    commit(&f.repo, "next");
    let new_head = good(&f.repo, &["rev-parse", "HEAD"]);
    assert_ne!(old_head, new_head);
    assert_eq!(p.worktree_for(&a).unwrap(), old);
    assert_eq!(good(&old, &["rev-parse", "HEAD"]), old_head);
    assert_eq!(fs::read(f.side(&a)).unwrap(), old_side);
    let b = request(&"b".repeat(64), 0);
    let path = p.worktree_for(&b).unwrap();
    assert_eq!(good(&path, &["rev-parse", "HEAD"]), new_head);
    let v: serde_json::Value = serde_json::from_slice(&fs::read(f.side(&b)).unwrap()).unwrap();
    assert_eq!(v["base_commit"], new_head);
}
#[test]
fn parallel_calls_create_distinct_worktrees() {
    let f = Fixture::new();
    let p = Arc::new(f.provider());
    let joins: Vec<_> = (0..10)
        .map(|i| {
            let p = p.clone();
            thread::spawn(move || p.worktree_for(&request(&format!("{i:064x}"), 0)).unwrap())
        })
        .collect();
    let paths: std::collections::HashSet<_> =
        joins.into_iter().map(|j| j.join().unwrap()).collect();
    assert_eq!(paths.len(), 10);
    let joins: Vec<_> = (0..10)
        .map(|_| {
            let p = p.clone();
            thread::spawn(move || p.worktree_for(&request(&"f".repeat(64), 0)).unwrap())
        })
        .collect();
    let same: std::collections::HashSet<_> = joins.into_iter().map(|j| j.join().unwrap()).collect();
    assert_eq!(same.len(), 1);
}
#[test]
fn remove_refuses_dirty_and_force_removes() {
    let f = Fixture::new();
    let p = f.provider();
    let a = f.req();
    let path = p.worktree_for(&a).unwrap();
    fs::write(path.join("new"), "dirty").unwrap();
    assert_eq!(
        p.remove(&path, false).unwrap_err(),
        format!(
            "worktree has uncommitted changes: {} (pass force to remove anyway)",
            path.display()
        )
    );
    assert_eq!(fs::read_to_string(path.join("new")).unwrap(), "dirty");
    fs::remove_file(path.join("new")).unwrap();
    p.remove(&path, false).unwrap();
    assert!(!path.exists());
    assert!(!f.side(&a).exists());
    assert!(p.list().is_empty());
    assert!(
        git(&f.repo, &["rev-parse", "--verify", &Fixture::branch(&a)])
            .status
            .success()
    );
    let b = request(&"b".repeat(64), 0);
    let other = p.worktree_for(&b).unwrap();
    fs::write(other.join("new"), "dirty").unwrap();
    p.remove(&other, true).unwrap();
    assert!(!other.exists() && !f.side(&b).exists());
    assert!(
        !git(&f.repo, &["rev-parse", "--verify", &Fixture::branch(&b)])
            .status
            .success()
    );
    assert!(
        p.remove(&f.repo, false)
            .unwrap_err()
            .starts_with("not a workflow worktree:")
    );
    let foreign = f.run.join("foreign");
    good(
        &f.repo,
        &["worktree", "add", "--detach", foreign.to_str().unwrap()],
    );
    assert!(
        p.remove(&foreign, false)
            .unwrap_err()
            .starts_with("not a workflow worktree:")
    );
    assert!(foreign.exists());
}
#[test]
fn list_reports_dirty_state_live() {
    let f = Fixture::new();
    let p = f.provider();
    let a = f.req();
    let b = request(&"b".repeat(64), 0);
    let pa = p.worktree_for(&a).unwrap();
    let pb = p.worktree_for(&b).unwrap();
    let rows = p.list();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].call, Fixture::name(&a));
    assert_eq!(rows[1].call, Fixture::name(&b));
    assert!(!rows[0].dirty);
    fs::write(pa.join("untracked"), "x").unwrap();
    assert!(p.list()[0].dirty);
    fs::remove_dir_all(&pb).unwrap();
    let rows = p.list();
    assert_eq!(rows[1].path, pb);
    assert!(!rows[1].dirty);
    fs::write(f.run.join("worktrees/bad.json"), "invalid").unwrap();
    assert_eq!(p.list().len(), 2);
    fs::remove_file(f.side(&a)).unwrap();
    assert_eq!(p.list().len(), 1);
}
#[test]
fn resolver_uses_the_worktree_folder() {
    let f = Fixture::new();
    let p = Arc::new(f.provider());
    let r = f.req();
    let resolver = ExplicitLaunch::new(LaunchDefaults {
        profile: "fixture".into(),
        cwd: f.repo.clone(),
        toolsets: Some(vec!["file".into()]),
        worktrees: Some(p.clone()),
    });
    let launch = resolver
        .complete(&r, "model".into(), "provider".into(), None)
        .unwrap();
    assert_eq!(launch.cwd.as_deref(), p.worktree_for(&r).unwrap().to_str());
    let mut plain = r.clone();
    plain.isolation = None;
    assert_eq!(
        resolver
            .complete(&plain, "model".into(), "provider".into(), None)
            .unwrap()
            .cwd
            .as_deref(),
        f.repo.to_str()
    );
    struct Bad;
    impl WorktreeProvider for Bad {
        fn worktree_for(&self, _: &AgentRequest) -> Result<PathBuf, String> {
            Err("distinct provider failure".into())
        }
    }
    let fail = ExplicitLaunch::new(LaunchDefaults {
        profile: "fixture".into(),
        cwd: f.repo,
        toolsets: Some(vec!["file".into()]),
        worktrees: Some(Arc::new(Bad)),
    });
    assert_eq!(
        fail.complete(&r, "model".into(), "provider".into(), None)
            .err()
            .unwrap(),
        "distinct provider failure"
    );
}
#[cfg(unix)]
#[test]
fn canonical_paths_through_a_symlink() {
    let f = Fixture::new();
    let link = f._tmp.path().join("link");
    std::os::unix::fs::symlink(&f.run, &link).unwrap();
    let p = GitWorktrees::new(link, f.repo.clone(), "run1".into());
    let r = f.req();
    let path = p.worktree_for(&r).unwrap();
    assert_eq!(path, path.canonicalize().unwrap());
    assert_eq!(p.list()[0].path, path);
    p.remove(&path, false).unwrap();
}
#[test]
fn crash_before_sidecar_finalise_recovers_or_fails_plainly() {
    let f = Fixture::new();
    let r = f.req();
    let path = f.path(&r);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let base = good(&f.repo, &["rev-parse", "HEAD"]);
    good(
        &f.repo,
        &[
            "worktree",
            "add",
            "-b",
            &Fixture::branch(&r),
            path.to_str().unwrap(),
            &base,
        ],
    );
    let p = f.provider();
    match p.worktree_for(&r) {
        Ok(got) => {
            assert_eq!(got, path);
            assert_eq!(p.list().len(), 1);
            p.remove(&got, false).unwrap();
        }
        Err(e) => assert!(e.starts_with(&format!(
            "worktree provenance cannot be recovered for {}:",
            path.display()
        ))),
    }
}
#[test]
fn lost_worktree_fails_without_recreating() {
    let f = Fixture::new();
    let p = f.provider();
    let r = f.req();
    let path = p.worktree_for(&r).unwrap();
    let side = fs::read(f.side(&r)).unwrap();
    fs::remove_dir_all(&path).unwrap();
    good(&f.repo, &["worktree", "prune"]);
    good(&f.repo, &["branch", "-D", &Fixture::branch(&r)]);
    assert_eq!(
        p.worktree_for(&r).unwrap_err(),
        format!(
            "worktree for call {} was lost: folder and branch are both missing (sidecar at {})",
            Fixture::name(&r),
            f.side(&r).display()
        )
    );
    assert!(!path.exists());
    assert!(
        !git(&f.repo, &["rev-parse", "--verify", &Fixture::branch(&r)])
            .status
            .success()
    );
    assert_eq!(fs::read(f.side(&r)).unwrap(), side);
}
#[test]
fn poisoned_lock_does_not_fail_later_calls() {
    let f = Fixture::new();
    let p = f.provider();
    let blocked = f.req();
    let foreign = f.run.join("branch-checked-out-elsewhere");
    good(
        &f.repo,
        &[
            "worktree",
            "add",
            "-b",
            &Fixture::branch(&blocked),
            foreign.to_str().unwrap(),
        ],
    );
    assert!(p.worktree_for(&blocked).is_err());
    let good_request = request(&"b".repeat(64), 0);
    assert!(p.worktree_for(&good_request).unwrap().is_dir());
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires the operator-provisioned fixture gateway"]
async fn live_worktree_agent_edits_only_the_worktree() {
    use hermes_client::{
        DiscoverOptions, EnsurePolicy, Recorder, RecorderOptions, liveness::ConnectHook,
    };
    use serde_json::json;
    use std::{io::Write, sync::Mutex, time::Duration};
    use talaria_workflow::{
        engine::{AgentBackend, AgentOutcome, EngineEvent},
        hermes::{HermesPort, LinkWireFactory, PortConfig, WireFactory},
        journal::{NewManifest, NoGate, OnUnknown, RetryPolicy, RunDir, RunJournal},
    };
    struct Memory(Arc<Mutex<Vec<u8>>>);
    impl Write for Memory {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let home = std::env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(hermes_client::default_home);
    let options = DiscoverOptions {
        ensure: EnsurePolicy::Never,
        ..DiscoverOptions::default()
    };
    let endpoint = match hermes_client::discover_with(&home, &options).await {
        Ok(endpoint) => endpoint,
        Err(_) => {
            eprintln!("skipped: gateway unavailable");
            return;
        }
    };
    if endpoint.resolve_profile("fixture").is_err() {
        eprintln!("skipped: fixture profile unavailable");
        return;
    }
    let fixture = Fixture::new();
    let script = "export const meta={name:'fixture',description:'fixture'}; return null;";
    let dir = RunDir::create(
        &fixture.run,
        NewManifest {
            workflow_name: "fixture".into(),
            args: None,
            started_at_ms: 0,
            seed: 0,
            profile: "fixture".into(),
            on_unknown: OnUnknown::Wait,
            talaria_version: "test".into(),
            hermes: json!({}),
            roster: json!({}),
            approval_policy: json!({}),
        },
        script,
    )
    .unwrap();
    let provider = Arc::new(GitWorktrees::new(
        dir.path().to_path_buf(),
        fixture.repo.clone(),
        dir.run_id().as_str().into(),
    ));
    let (journal, _, _) = RunJournal::open(dir, script).await.unwrap();
    let journal = Arc::new(journal);
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let substitutions = hermes_client::local_forbidden_literals()
        .into_iter()
        .filter(|s| s.len() >= 4)
        .map(|s| (s, "<local>".into()))
        .collect();
    let recorder = Recorder::from_writer(
        Memory(buffer.clone()),
        RecorderOptions {
            endpoint: "ws://<gateway>/api/ws".into(),
            subprotocols: vec!["hermes-gateway-v1".into()],
            substitutions,
            ..Default::default()
        },
    )
    .unwrap();
    let copy = recorder.clone();
    let hook: ConnectHook =
        Arc::new(move |connection, generation| connection.set_recorder(copy.clone(), generation));
    let wires: Arc<dyn WireFactory> = Arc::new(LinkWireFactory::new(home.clone()).on_connect(hook));
    let (port, _feed) = HermesPort::with_wires(
        PortConfig {
            home,
            ..PortConfig::default()
        },
        wires,
        "fixture",
    )
    .await
    .unwrap();
    let mut req = fixture.req();
    req.prompt="Use the file tool to create talaria-wt-proof.txt in your current directory containing the word proof. Do not modify any other file.".into();
    req.model = Some("minimax-m3.1-flash".into());
    req.provider = Some("ollama-cloud".into());
    req.profile = Some("fixture".into());
    req.toolsets = Some(vec!["file".into()]);
    journal
        .record_engine_event(&EngineEvent::AgentStarted {
            request: req.clone(),
            replayed: false,
        })
        .await
        .unwrap();
    let defaults = LaunchDefaults {
        profile: "fixture".into(),
        cwd: fixture.repo.clone(),
        toolsets: Some(vec!["file".into()]),
        worktrees: Some(provider.clone()),
    };
    let backend =
        talaria_workflow::hermes::backend(journal, port, defaults, NoGate, RetryPolicy::default());
    let outcome = tokio::time::timeout(Duration::from_secs(180), backend.run_agent(req.clone()))
        .await
        .expect("agent timed out");
    assert!(matches!(outcome, AgentOutcome::Completed(_)), "{outcome:?}");
    let path = provider.worktree_for(&req).unwrap();
    assert!(path.join("talaria-wt-proof.txt").is_file());
    assert!(!fixture.repo.join("talaria-wt-proof.txt").exists());
    recorder.finish().unwrap();
    let frames = buffer.lock().unwrap();
    for line in frames.split(|b| *b == b'\n').filter(|b| !b.is_empty()) {
        let value: serde_json::Value = serde_json::from_slice(line).unwrap();
        if value["frame"]["method"] == "prompt.submit" {
            assert!(value["frame"]["params"].get("unattended").is_none());
        }
    }
}
