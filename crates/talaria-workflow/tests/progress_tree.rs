//! Journal projection and tail. Each test calls the stub named by its red reason first.

#[path = "support/lines.rs"]
mod lines;
#[path = "support/replay_run.rs"]
mod replay;

use talaria_workflow::journal::{JournalError, JournalReader};
use talaria_workflow::progress::{
    AgentState, RunProgress, RunState, Tail, format_money, format_tokens,
};

#[test]
fn tree_from_finished_journal() {
    let rendered = lines::jsonl(&lines::finished_lines());
    let fixture = std::fs::read_to_string(lines::fixture_path()).unwrap_or_default();
    assert_eq!(
        fixture, rendered,
        "recorded fixture does not match the support journal"
    );
    let (records, offset) = JournalReader::read_from(&lines::fixture_path(), 0).expect("fixture");
    assert_eq!(records, lines::finished_lines());
    assert_eq!(offset, fixture.len() as u64);
    let manifest = lines::manifest();
    let progress = RunProgress::from_records(&manifest, &lines::finished_lines());

    assert_eq!(progress.run().started_at_ms, Some(lines::RUN_STARTED_MS));
    assert_ne!(progress.run().started_at_ms, Some(manifest.started_at_ms));
    assert_eq!(progress.run().state, RunState::Completed);
    assert_eq!(
        progress
            .phases
            .iter()
            .filter(|phase| phase.workflow_path.is_empty())
            .map(|phase| phase.title.as_str())
            .collect::<Vec<_>>(),
        ["plan", "build", "check"]
    );
    assert_eq!(progress.run().counts.total, 3);
    assert_eq!(progress.run().counts.done, 3);
    assert_eq!(progress.run().tokens, 1_050 + 1_049 + 999);
    assert_eq!(format_tokens(progress.run().tokens), "3.1k");

    let correctness = progress.agent(lines::KEY_CORRECTNESS).expect("correctness");
    assert_eq!(correctness.state, AgentState::Done);
    assert_eq!(correctness.phase.as_deref(), Some("plan"));
    assert_eq!(correctness.model.as_deref(), Some("minimax-m3.1-flash"));
    assert_eq!(correctness.provider.as_deref(), Some("ollama-cloud"));
    assert_eq!(correctness.tokens, 1_050);
    assert_eq!(format_tokens(correctness.tokens), "1.1k");
    assert_eq!(
        correctness.duration_ms(lines::CORRECTNESS_FINISHED_MS),
        lines::CORRECTNESS_FINISHED_MS - lines::CORRECTNESS_STARTED_MS
    );

    let security = progress.agent(lines::KEY_SECURITY).expect("security");
    assert_eq!(security.tokens, 1_049);
    assert_eq!(format_tokens(security.tokens), "1.0k");
    assert_eq!(
        security.duration_ms(lines::SECURITY_FINISHED_MS),
        lines::SECURITY_FINISHED_MS - lines::SECURITY_STARTED_MS
    );

    let simplicity = progress.agent(lines::KEY_SIMPLICITY).expect("simplicity");
    assert_eq!(simplicity.tokens, 999);
    assert_eq!(format_tokens(simplicity.tokens), "999");
    assert_eq!(
        simplicity.duration_ms(lines::SIMPLICITY_FINISHED_MS),
        lines::SIMPLICITY_FINISHED_MS - lines::SIMPLICITY_STARTED_MS
    );
}

#[test]
fn phases_and_child_workflows_nest() {
    let lines = lines::nested_lines();
    let progress = RunProgress::from_records(&lines::manifest(), &lines);
    assert_eq!(
        progress
            .phases
            .iter()
            .filter(|phase| phase.workflow_path.is_empty())
            .map(|phase| phase.title.as_str())
            .collect::<Vec<_>>(),
        ["plan"]
    );
    assert!(
        progress
            .phases
            .iter()
            .filter(|phase| phase.workflow_path.is_empty())
            .all(|phase| phase.title != "scan")
    );
    let child = progress.children.first().expect("child");
    assert_eq!(child.name, "child");
    assert_eq!(child.workflow_path, vec![lines::CHILD_PATH.to_string()]);
    assert!(progress.phases.iter().any(|phase| {
        phase.title == "scan" && phase.workflow_path == vec![lines::CHILD_PATH.to_string()]
    }));
    assert_eq!(progress.run().logs.len(), 50);
    assert_eq!(
        progress.run().logs.front().map(String::as_str),
        Some("log 10")
    );
    assert_eq!(
        progress.run().logs.back().map(String::as_str),
        Some("log 59")
    );
}

#[test]
fn agent_states_follow_priority() {
    let lines = lines::state_lines();
    let progress = RunProgress::from_records(&lines::manifest(), &lines);
    let state = |key| progress.agent(key).expect(key).state;
    assert_eq!(state(lines::KEY_CORRECTNESS), AgentState::Done);
    assert_eq!(state(lines::KEY_FAILED), AgentState::Failed);
    assert_eq!(state(lines::KEY_SKIPPED), AgentState::Skipped);
    assert_eq!(state(lines::KEY_PARKED), AgentState::Unknown);
    assert_eq!(
        progress
            .agent(lines::KEY_PARKED)
            .expect("parked")
            .attention
            .as_deref(),
        Some("unknown")
    );
    assert_eq!(state(lines::KEY_ENDED), AgentState::Ended);
    assert_eq!(state(lines::KEY_RUNNING), AgentState::Running);
    assert_eq!(state(lines::KEY_QUEUED), AgentState::Queued);
    assert_eq!(progress.run().counts.needs_you, 1);
}

#[test]
fn usage_merges_without_inventing_zero() {
    let manifest = lines::manifest();
    let merged = RunProgress::from_records(&manifest, &lines::usage_lines());
    let usage = merged.agent(lines::KEY_USAGE).expect("usage");
    assert_eq!(usage.tokens, 140);
    assert_eq!(merged.run().tokens, 140);

    let unknown = RunProgress::from_records(&manifest, &lines::cost_lines("unknown"));
    assert_eq!(
        format_money(unknown.agent(lines::KEY_USAGE).expect("unknown").money),
        "cost unknown"
    );
    let known_zero = RunProgress::from_records(&manifest, &lines::cost_lines("actual"));
    assert_eq!(
        format_money(known_zero.agent(lines::KEY_USAGE).expect("zero").money),
        "$0.00"
    );
}

#[test]
fn tail_holds_torn_bytes_and_refuses_corrupt() {
    let (kept, dir) = replay::empty_run();
    let journal = kept.path().join(lines::RUN_ID).join("journal.jsonl");
    let mut raw = lines::jsonl(&[lines::one_started()]).into_bytes();
    raw.extend_from_slice(br#"{"v":1,"seq":2,"at_ms":2"#);
    std::fs::write(&journal, &raw).expect("torn journal");

    let (mut progress, mut tail) = Tail::open(&dir).expect("tail");
    assert_eq!(progress.agents().len(), 0);
    assert_eq!(progress.last_seq, 1);
    assert_eq!(progress.run().state, RunState::Running);
    let held = tail.poll(&mut progress).expect("torn tail is not an error");
    assert_eq!(held, 0);

    raw.extend_from_slice(b"}\n");
    std::fs::write(&journal, &raw).expect("seq-only line");
    let error = tail.poll(&mut progress).expect_err("missing record tag");
    match error {
        JournalError::Corrupt { line, reason } => {
            assert_eq!(line, 2);
            assert!(
                reason.contains("`t`") || reason.contains("\"t\"") || reason.contains("missing t"),
                "{reason}"
            );
        }
        other => panic!("expected a corrupt line, got {other:?}"),
    }
    let after = std::fs::read(&journal).expect("journal still present");
    assert_eq!(after, raw);
}

#[test]
#[ignore = "release budget"]
fn ten_thousand_records_build_within_budget() {
    eprintln!(
        "machine: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let lines = lines::many_lines(10_000);
    let started = std::time::Instant::now();
    let progress = RunProgress::from_records(&lines::manifest(), &lines);
    let elapsed = started.elapsed();
    eprintln!("build time: {} ms", elapsed.as_millis());
    assert!(
        elapsed.as_millis() < 250,
        "build took {} ms",
        elapsed.as_millis()
    );
    assert_eq!(progress.run().logs.len(), 50);
}

#[test]
fn executions_do_not_leak_terminal_state() {
    let lines = lines::execution_lines();
    let progress = RunProgress::from_records(&lines::manifest(), &lines);
    assert_eq!(progress.run().state, RunState::Running);
    assert_eq!(progress.run().previous_execution, Some(RunState::Aborted));
    assert_eq!(
        progress
            .agent(lines::KEY_FAILED)
            .expect("new attempt")
            .state,
        AgentState::Queued
    );
}

#[test]
fn projection_reads_timestamps_from_the_journal() {
    let lines = lines::timestamp_lines();
    let progress = RunProgress::from_records(&lines::manifest(), &lines);
    assert_eq!(progress.run().started_at_ms, Some(5_000));
    assert_ne!(
        progress.run().started_at_ms,
        Some(lines::manifest().started_at_ms)
    );
    assert_eq!(
        progress
            .agent(lines::KEY_CORRECTNESS)
            .expect("span")
            .duration_ms(0),
        3_000
    );
    assert_eq!(
        progress
            .agent(lines::KEY_SECURITY)
            .expect("short")
            .duration_ms(0),
        100
    );
}
