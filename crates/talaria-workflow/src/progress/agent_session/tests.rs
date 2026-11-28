use super::*;
use serde_json::{Value, json};
use std::fs;

const RUN: &str = "01JB2X9Q4M8Z7K3T5V6W8Y0A1C";

/// What the app does in two steps: build the watch, then apply a read made elsewhere.
fn open_watch(
    dir: PathBuf,
    call: Option<(String, u32)>,
    stored: &str,
    runtime: &str,
) -> AgentSessionWatch {
    let mut watch = AgentSessionWatch::new(dir.clone(), call, stored, runtime);
    watch.apply(read_agent_journal(&dir, 0));
    watch
}

/// A live runner: it holds the run's lock for as long as the returned file lives.
fn hold_runner(dir: &Path) -> fs::File {
    let file = fs::File::open(dir.join("run.lock")).unwrap();
    file.try_lock().unwrap();
    file
}

fn key() -> String {
    "8".repeat(32)
}

fn digest() -> String {
    "a".repeat(64)
}

/// A run folder named like a real one, with the records given after the agent's call began.
fn run_folder(root: &Path, extra: &[Value]) -> PathBuf {
    let dir = root.join(RUN);
    fs::create_dir_all(&dir).unwrap();
    let manifest = json!({
        "v":1,"run_id":RUN,"created_at_ms":1000,
        "workflow":{"name":"review-branch","script_sha256":"a".repeat(64)},
        "args":null,"started_at_ms":1,"seed":4242,"profile":"fixture",
        "on_unknown":"wait","talaria_version":env!("CARGO_PKG_VERSION"), "hermes":{},
        "roster":{},"approval_policy":{}
    });
    fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
    fs::write(dir.join("run.lock"), "").unwrap();
    let mut records = vec![
        json!({"t":"run_started","run_id":RUN,"script_sha256":"a".repeat(64)}),
        json!({"t":"call_intent","key":key(),"attempt":0,
            "call_id":{"digest":digest(),"occurrence":0},"label":"review:gameplay",
            "phase":"Verify","workflow_path":[],
            "launch":{"profile":"fixture","model":"fixture-model","provider":null,
            "effort":null,"toolsets":[],"skills":[],"max_turns":null,"isolation":null,
            "cwd":null},"request_id":"wf:fixture"}),
        json!({"t":"session_bound","key":key(),"attempt":0,
            "session_id":"runtime-bound","stored_session_id":"stored-bound"}),
    ];
    records.extend(extra.iter().cloned());
    append(&dir, &records, 1);
    dir
}

fn append(dir: &Path, records: &[Value], first_seq: u64) {
    let mut text = fs::read_to_string(dir.join("journal.jsonl")).unwrap_or_default();
    for (i, record) in records.iter().enumerate() {
        let mut record = record.clone();
        record["v"] = 1.into();
        record["seq"] = (first_seq + i as u64).into();
        record["at_ms"] = (1000 + first_seq as i64 + i as i64).into();
        text.push_str(&record.to_string());
        text.push('\n');
    }
    fs::write(dir.join("journal.jsonl"), text).unwrap();
}

fn failed() -> Value {
    json!({"t":"call_finished","key":key(),"attempt":0,
        "outcome":{"kind":"failed","reason":"context too long"},"turns":1})
}

fn started() -> Vec<Value> {
    vec![
        json!({"t":"turn_intent","key":key(),"attempt":0,"turn":0,
            "submission_id":"s-1","text":"review"}),
        json!({"t":"turn_admitted","key":key(),"attempt":0,"turn":0,"admission_id":"adm-1",
            "status":"started","outcome":null,"execution_generation":1,"authority_epoch":1}),
    ]
}

#[test]
fn a_folder_under_a_runs_worktrees_names_its_run_and_call() {
    let call = format!("{}-2", digest());
    for cwd in [
        format!("/data/runs/{RUN}/worktrees/{call}"),
        format!("/data/runs/{RUN}/worktrees/{call}/src/deep"),
    ] {
        let owner = worktree_owner(&cwd).expect(&cwd);
        assert_eq!(owner.run_dir, Path::new(&format!("/data/runs/{RUN}")));
        assert_eq!(owner.run_id, RUN);
        assert_eq!(owner.digest, digest());
        assert_eq!(owner.occurrence, 2);
    }
}

#[test]
fn a_folder_that_only_looks_similar_names_nobody() {
    let call = format!("{}-0", digest());
    for cwd in [
        format!("/data/runs/{RUN}/worktrees"),
        format!("/data/runs/{RUN}/worktrees-old/{call}"),
        format!("/data/runs/not-a-run/worktrees/{call}"),
        format!("/data/runs/{RUN}/worktrees/{}-0", "a".repeat(63)),
        format!("/data/runs/{RUN}/worktrees/{}-0", "A".repeat(64)),
        format!("/data/runs/{RUN}/worktrees/{}-x", digest()),
        format!("/data/runs/{RUN}/{call}"),
        "/home/someone/project".to_owned(),
        String::new(),
    ] {
        assert_eq!(worktree_owner(&cwd), None, "{cwd}");
    }
}

#[test]
fn the_bound_session_of_a_failed_agent_is_followed_and_says_failed() {
    let temp = tempfile::tempdir().unwrap();
    let dir = run_folder(temp.path(), &[failed()]);
    let watch = open_watch(dir, None, "stored-bound", "runtime-bound");
    let note = watch.note();
    assert_eq!(note.tracking, Tracking::Followed);
    assert_eq!(note.phase, Some(AgentPhase::Failed));
    assert_eq!(note.label.as_deref(), Some("review:gameplay"));
    assert_eq!(note.run_name.as_deref(), Some("review-branch"));
    assert_eq!(
        note.banner(),
        "Workflow agent review:gameplay · failed · run review-branch (…8Y0A1C)"
    );
    let warning = note.first_send_warning();
    assert!(warning.contains("has failed"), "{warning}");
    assert!(warning.contains("will not reach the run"), "{warning}");
    assert!(warning.ends_with("Submit again to send."), "{warning}");
}

#[test]
fn a_follow_on_session_is_found_by_its_worktree_and_called_untracked() {
    let temp = tempfile::tempdir().unwrap();
    let dir = run_folder(temp.path(), &[failed()]);
    let call = Some((digest(), 0));
    let watch = open_watch(dir, call, "stored-follow-on", "runtime-follow-on");
    let note = watch.note();
    assert_eq!(note.tracking, Tracking::NotFollowed);
    assert_eq!(note.phase, Some(AgentPhase::Failed));
    assert_eq!(note.label.as_deref(), Some("review:gameplay"));
    let banner = note.banner();
    assert!(
        banner.contains("this conversation is not tracked by the run"),
        "{banner}"
    );
    assert!(
        note.status_text().ends_with("(untracked)"),
        "{}",
        note.status_text()
    );
    let warning = note.first_send_warning();
    assert!(
        warning.contains("does not track this conversation"),
        "{warning}"
    );
    assert!(warning.contains("edits and commits"), "{warning}");
}

#[test]
fn a_running_bound_agent_warns_that_a_message_changes_its_work() {
    let temp = tempfile::tempdir().unwrap();
    let dir = run_folder(temp.path(), &started());
    let note = open_watch(dir, None, "stored-bound", "").note();
    assert_eq!(note.tracking, Tracking::Followed);
    assert_eq!(note.phase, Some(AgentPhase::Running));
    assert!(!note.finished());
    let warning = note.first_send_warning();
    assert!(warning.contains("mid-run"), "{warning}");
    assert!(!warning.contains("will not reach"), "{warning}");
}

#[test]
fn a_stopped_parked_agent_says_how_to_make_a_reply_count() {
    let temp = tempfile::tempdir().unwrap();
    let mut records = started();
    records.push(json!({"t":"turn_settled","key":key(),"attempt":0,"turn":0,
        "admission_id":"adm-1","outcome":"interrupted","result_meta":{"interrupted":true},
        "usage":null}));
    records.push(
        json!({"t":"call_attention","key":key(),"attempt":0,"episode":0,
        "cause":"stopped","turn":0,"admission_id":"adm-1","execution_generation":1,
        "detail":"agent stopped"}),
    );
    let dir = run_folder(temp.path(), &records);
    let note = open_watch(dir, None, "stored-bound", "").note();
    assert_eq!(note.phase, Some(AgentPhase::Stopped));
    assert!(!note.finished());
    assert!(
        note.banner().contains("stopped, waiting for you"),
        "{}",
        note.banner()
    );
    let warning = note.first_send_warning();
    assert!(warning.contains("use-reply"), "{warning}");
    assert!(!warning.contains("will not reach"), "{warning}");
}

#[test]
fn a_kept_reply_from_the_step_cap_reads_cut_short() {
    let temp = tempfile::tempdir().unwrap();
    let done = json!({"t":"call_finished","key":key(),"attempt":0,
        "outcome":{"kind":"completed","value":"partial",
        "cut_short":{"steps":60,"limit":60}},"turns":1});
    let dir = run_folder(temp.path(), &[done]);
    let note = open_watch(dir, None, "stored-bound", "").note();
    assert_eq!(note.phase, Some(AgentPhase::CutShort));
    assert!(note.banner().contains("cut short"), "{}", note.banner());
    assert!(note.first_send_warning().contains("cut short"));
}

#[test]
fn an_unreadable_journal_says_so_and_claims_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join(RUN);
    let watch = open_watch(missing, Some((digest(), 0)), "stored-x", "runtime-x");
    let note = watch.note();
    assert_eq!(note.tracking, Tracking::Unknown);
    assert_eq!(note.phase, None);
    assert_eq!(note.label, None);
    assert_eq!(
        note.banner(),
        "Workflow agent session · journal state not readable here · run …8Y0A1C"
    );
    assert!(
        note.first_send_warning()
            .contains("cannot say whether the run follows")
    );
    assert!(!watch.wants_refresh());
}

#[test]
fn a_call_the_journal_does_not_name_is_unknown_rather_than_untracked() {
    let temp = tempfile::tempdir().unwrap();
    let dir = run_folder(temp.path(), &[failed()]);
    let other = Some(("b".repeat(64), 0));
    let note = open_watch(dir, other, "stored-x", "runtime-x").note();
    assert_eq!(note.tracking, Tracking::Unknown);
    assert_eq!(note.label, None);
    assert_eq!(note.run_name.as_deref(), Some("review-branch"));
    assert!(note.banner().contains("not found in the run's journal"));
    assert!(
        note.first_send_warning()
            .contains("does not name this agent")
    );
}

#[test]
fn a_later_read_picks_up_the_agents_new_state_and_reads_stop_once_it_is_final() {
    let temp = tempfile::tempdir().unwrap();
    let dir = run_folder(temp.path(), &started());
    let _runner = hold_runner(&dir);
    let mut watch = open_watch(dir.clone(), None, "stored-bound", "");
    assert_eq!(watch.note().phase, Some(AgentPhase::Running));
    assert!(watch.wants_refresh());
    assert!(
        !watch.apply(read_agent_journal(&dir, watch.offset())),
        "nothing new"
    );
    append(&dir, &[failed()], 6);
    assert!(watch.apply(read_agent_journal(&dir, watch.offset())));
    assert_eq!(watch.note().phase, Some(AgentPhase::Failed));
    assert!(!watch.wants_refresh());
}

#[test]
fn nothing_is_known_until_the_first_read_is_applied() {
    let temp = tempfile::tempdir().unwrap();
    let dir = run_folder(temp.path(), &started());
    let watch = AgentSessionWatch::new(dir, None, "stored-bound", "");
    let note = watch.note();
    assert!(note.pending);
    assert_eq!(note.phase, None);
    assert!(
        note.banner().contains("reading the run's journal"),
        "{}",
        note.banner()
    );
    assert!(note.first_send_warning().contains("still reading"));
    assert!(!watch.wants_refresh());
}

#[test]
fn reads_stop_when_the_run_has_ended_or_its_runner_is_gone_or_the_agent_is_not_named() {
    let temp = tempfile::tempdir().unwrap();
    // The run finished while the agent never reported an outcome.
    let finished = json!({"t":"run_finished","outcome":"aborted","summary":{"completed":0,"failed":0,"skipped":0,"parked":0,"unfinished":1}});
    let dir = run_folder(temp.path(), &[started(), vec![finished]].concat());
    let _runner = hold_runner(&dir);
    let watch = open_watch(dir, None, "stored-bound", "");
    assert_eq!(watch.note().phase, Some(AgentPhase::Running));
    assert!(!watch.wants_refresh(), "the run is over");

    // The run never wrote its end, but no runner holds the lock: it was interrupted.
    let gone = tempfile::tempdir().unwrap();
    let dir = run_folder(gone.path(), &started());
    let watch = open_watch(dir, None, "stored-bound", "");
    assert_eq!(watch.note().phase, Some(AgentPhase::Running));
    assert!(!watch.wants_refresh(), "no runner is alive");

    // The journal does not name this agent.
    let other = tempfile::tempdir().unwrap();
    let dir = run_folder(other.path(), &started());
    let _runner = hold_runner(&dir);
    let stranger = open_watch(dir, Some(("b".repeat(64), 0)), "stored-x", "runtime-x");
    assert_eq!(stranger.note().phase, None);
    assert!(!stranger.wants_refresh());
}

#[test]
fn a_failed_read_stops_further_reads_and_says_unreadable() {
    let temp = tempfile::tempdir().unwrap();
    let mut watch = AgentSessionWatch::new(temp.path().join(RUN), None, "s", "r");
    assert!(watch.note().pending);
    assert!(watch.apply(Err("no such folder".into())));
    let note = watch.note();
    assert!(!note.pending);
    assert!(
        note.banner().contains("not readable here"),
        "{}",
        note.banner()
    );
    assert!(!watch.wants_refresh());
}

#[test]
fn journal_text_with_control_characters_is_flattened_and_cut() {
    let note = AgentSessionNote {
        run_id: RUN.into(),
        run_name: Some("a\u{1b}[31mb".into()),
        label: Some(format!("line\nbreak {}", "x".repeat(200))),
        phase: Some(AgentPhase::Failed),
        tracking: Tracking::Followed,
        pending: false,
    };
    let banner = note.banner();
    assert!(!banner.contains(['\u{1b}', '\n']), "{banner:?}");
    assert!(banner.contains('…'), "{banner}");
    assert!(banner.chars().count() < 200, "{banner}");
}
