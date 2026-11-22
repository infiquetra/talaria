//! While Hivecode builds a workflow, the turn's tool calls say which step it is on: planning,
//! writing the script, checking it, waiting for the person, and starting the run. The words are
//! plain, the script is named once written, and the line committed at the end says how it ended.

use crate::events;

use events::{SESSION, ev};
use serde_json::{Value, json};
use talaria_core::hivecode::authoring::{
    AuthoringEnd, AuthoringProgress, AuthoringStage, RunFacts, STEPS, elapsed_text, progress,
    resume,
};
use talaria_core::hivecode::notice;
use talaria_core::session::{SessionId, SessionState, update};

const DRAFT: &str = "/home/fixture/.talaria/drafts/fix-parser.workflow";

fn tool(seq: u64, id: &str, name: &str, args: Value) -> talaria_core::session::Event {
    ev(
        seq,
        "tool.start",
        json!({"tool_id": id, "name": name, "args": args, "context": name}),
    )
}

fn done(seq: u64, id: &str, name: &str) -> talaria_core::session::Event {
    ev(
        seq,
        "tool.complete",
        json!({"tool_id": id, "name": name, "summary": "done"}),
    )
}

fn turn() -> SessionState {
    let mut state = SessionState::empty(SessionId::new(SESSION));
    update(&mut state, ev(1, "message.start", json!({})));
    state
}

fn now(state: &SessionState) -> AuthoringProgress {
    progress(state.cells())
}

#[test]
fn each_tool_call_moves_the_block_to_the_step_it_belongs_to() {
    let mut state = turn();
    assert_eq!(now(&state).stage, AuthoringStage::Planning);
    assert_eq!(now(&state).label(), "planning the workflow");
    assert_eq!(now(&state).step(), 1);

    // Reading the roster and the guide is still planning.
    update(
        &mut state,
        tool(
            2,
            "t1",
            "terminal",
            json!({"command": "talaria workflow roster --profile fixture --json"}),
        ),
    );
    update(&mut state, done(3, "t1", "terminal"));
    update(
        &mut state,
        tool(4, "t2", "skill_view", json!({"name": "fixture-guide"})),
    );
    assert_eq!(now(&state).stage, AuthoringStage::Planning);
    assert_eq!(now(&state).script, None);

    // Writing the draft names the script.
    update(
        &mut state,
        tool(
            5,
            "t3",
            "write_file",
            json!({"path": DRAFT, "content": "export const meta = {name:'fix-parser'};"}),
        ),
    );
    let writing = now(&state);
    assert_eq!(writing.stage, AuthoringStage::Writing);
    assert_eq!(writing.label(), "writing the workflow script");
    assert_eq!(writing.step(), 2);
    assert_eq!(writing.script.as_deref(), Some("fix-parser.workflow"));
    update(&mut state, done(6, "t3", "write_file"));

    // Reading it back is checking.
    update(
        &mut state,
        tool(7, "t4", "read_file", json!({"path": DRAFT})),
    );
    let checking = now(&state);
    assert_eq!(checking.stage, AuthoringStage::Checking);
    assert_eq!(checking.label(), "checking the script");
    assert_eq!(checking.step(), 3);
    update(&mut state, done(8, "t4", "read_file"));

    // An open question waits for the person.
    update(
        &mut state,
        ev(
            9,
            "clarify.request",
            json!({"prompt_id": "fixture-prompt", "execution_generation": 1, "kind": "clarify",
                "question": "Start the workflow?", "choices": ["yes", "no"], "multi_select": false}),
        ),
    );
    let waiting = now(&state);
    assert_eq!(waiting.stage, AuthoringStage::Waiting);
    assert_eq!(waiting.label(), "waiting for your approval");
    assert_eq!(waiting.step(), 4);

    // Hermes settles the question; the run starts in a background terminal.
    update(
        &mut state,
        ev(
            10,
            "clarify.settled",
            json!({"prompt_id": "fixture-prompt", "execution_generation": 1}),
        ),
    );
    update(
        &mut state,
        tool(
            11,
            "t5",
            "terminal",
            json!({"command": format!("talaria workflow run --profile fixture --json {DRAFT}"), "background": true}),
        ),
    );
    let running = now(&state);
    assert_eq!(running.stage, AuthoringStage::Running);
    assert_eq!(running.label(), "running the workflow");
    assert_eq!(running.step(), STEPS);
    assert_eq!(running.script.as_deref(), Some("fix-parser.workflow"));
}

#[test]
fn a_patch_to_the_draft_counts_as_writing_and_other_files_do_not() {
    let mut state = turn();
    update(
        &mut state,
        tool(
            2,
            "t1",
            "write_file",
            json!({"path": "src/parser.rs", "content": "fn main() {}"}),
        ),
    );
    assert_eq!(now(&state).stage, AuthoringStage::Planning);
    assert_eq!(now(&state).script, None);
    update(
        &mut state,
        tool(
            3,
            "t2",
            "patch",
            json!({"mode": "patch", "patch": "*** Begin Patch\n*** Update File: /home/fixture/.talaria/drafts/tidy.workflow\n@@\n-a\n+b\n*** End Patch"}),
        ),
    );
    assert_eq!(now(&state).stage, AuthoringStage::Writing);
    assert_eq!(now(&state).script.as_deref(), Some("tidy.workflow"));
}

#[test]
fn a_question_before_any_script_waits_for_an_answer_not_an_approval() {
    let mut state = turn();
    update(
        &mut state,
        tool(
            2,
            "t1",
            "clarify",
            json!({"questions": [{"question": "Which folder?", "choices": ["a", "b"]}]}),
        ),
    );
    let waiting = now(&state);
    assert_eq!(waiting.stage, AuthoringStage::Waiting);
    assert_eq!(waiting.label(), "waiting for your answer");
}

#[test]
fn the_answer_to_an_approval_question_starts_from_the_written_script() {
    let mut state = turn();
    let answered = resume(Some("fix-parser.workflow"), state.cells());
    assert_eq!(answered.stage, AuthoringStage::Answered);
    assert_eq!(answered.label(), "reading your answer");
    assert_eq!(answered.step(), 4);
    assert_eq!(answered.script.as_deref(), Some("fix-parser.workflow"));
    // Reading the carried script back is checking it; the run command is running.
    update(
        &mut state,
        tool(2, "t1", "read_file", json!({"path": DRAFT})),
    );
    assert_eq!(
        resume(Some("fix-parser.workflow"), state.cells()).stage,
        AuthoringStage::Checking
    );
    update(
        &mut state,
        tool(
            3,
            "t2",
            "terminal",
            json!({"command": format!("talaria workflow run --profile fixture --json {DRAFT}")}),
        ),
    );
    assert_eq!(
        resume(Some("fix-parser.workflow"), state.cells()).stage,
        AuthoringStage::Running
    );
    // With nothing carried, a turn starts at planning.
    assert_eq!(resume(None, &[]).stage, AuthoringStage::Planning);
}

#[test]
fn elapsed_time_reads_in_seconds_then_minutes() {
    assert_eq!(elapsed_text(0), "0s");
    assert_eq!(elapsed_text(59_999), "59s");
    assert_eq!(elapsed_text(65_000), "1m 05s");
    assert_eq!(elapsed_text(3_725_000), "62m 05s");
}

#[test]
fn the_block_names_the_step_and_the_script_in_plain_words() {
    let writing = AuthoringProgress {
        stage: AuthoringStage::Writing,
        script: Some("fix-parser.workflow".into()),
    };
    assert_eq!(notice::AUTHORING_TITLE, "Hivecode · building a workflow");
    assert_eq!(
        notice::authoring_step(&writing),
        "Step 2 of 5: writing the workflow script · fix-parser.workflow"
    );
    assert_eq!(
        notice::authoring_short(&writing, 65_000),
        "Hivecode · writing the workflow script · 1m 05s"
    );
    let planning = AuthoringProgress {
        stage: AuthoringStage::Planning,
        script: None,
    };
    assert_eq!(
        notice::authoring_step(&planning),
        "Step 1 of 5: planning the workflow"
    );
}

#[test]
fn the_line_left_behind_says_how_the_build_ended() {
    let checking = AuthoringProgress {
        stage: AuthoringStage::Checking,
        script: Some("fix-parser.workflow".into()),
    };
    let planning = AuthoringProgress {
        stage: AuthoringStage::Planning,
        script: None,
    };
    let running = AuthoringProgress {
        stage: AuthoringStage::Running,
        script: Some("fix-parser.workflow".into()),
    };
    let run = RunFacts {
        name: "fix-parser",
        status: "running",
        agents: 7,
    };
    let lines = [
        (
            notice::authoring_ended(&checking, AuthoringEnd::Interrupted, 65_000, None),
            "Hivecode stopped: interrupted while checking the script (fix-parser.workflow) after 1m 05s. No workflow started.",
        ),
        (
            notice::authoring_ended(&planning, AuthoringEnd::Failed, 4_000, None),
            "Hivecode stopped: the turn failed while planning the workflow after 4s. No workflow started.",
        ),
        (
            notice::authoring_ended(&running, AuthoringEnd::Completed, 200_000, Some(run)),
            "Hivecode built the workflow in 3m 20s: fix-parser is running (7 agents so far).",
        ),
        (
            notice::authoring_ended(
                &running,
                AuthoringEnd::Completed,
                200_000,
                Some(RunFacts {
                    status: "completed",
                    ..run
                }),
            ),
            "Hivecode built the workflow in 3m 20s: fix-parser completed (7 agents).",
        ),
        // The count is the agents seen so far: none yet says nothing, and one is singular.
        (
            notice::authoring_ended(
                &running,
                AuthoringEnd::Completed,
                200_000,
                Some(RunFacts { agents: 0, ..run }),
            ),
            "Hivecode built the workflow in 3m 20s: fix-parser is running.",
        ),
        (
            notice::authoring_ended(
                &running,
                AuthoringEnd::Completed,
                200_000,
                Some(RunFacts {
                    status: "parked",
                    agents: 1,
                    ..run
                }),
            ),
            "Hivecode built the workflow in 3m 20s: fix-parser is waiting for an answer (1 agent so far).",
        ),
        (
            notice::authoring_ended(
                &running,
                AuthoringEnd::Completed,
                200_000,
                Some(RunFacts {
                    status: "failed",
                    agents: 1,
                    ..run
                }),
            ),
            "Hivecode built the workflow in 3m 20s: fix-parser failed (1 agent).",
        ),
        // A turn stopped after it sent the run command, before Talaria found the run (always
        // so on a remote gateway), does not say that no workflow started.
        (
            notice::authoring_ended(&running, AuthoringEnd::Interrupted, 130_000, None),
            "Hivecode stopped: interrupted after starting fix-parser.workflow, 2m 10s in. The workflow may still be running.",
        ),
        (
            notice::authoring_ended(&running, AuthoringEnd::Failed, 130_000, None),
            "Hivecode stopped: the turn failed after starting fix-parser.workflow, 2m 10s in. The workflow may still be running.",
        ),
        (
            notice::authoring_ended(
                &AuthoringProgress {
                    stage: AuthoringStage::Running,
                    script: None,
                },
                AuthoringEnd::Interrupted,
                5_000,
                None,
            ),
            "Hivecode stopped: interrupted after starting the workflow, 5s in. The workflow may still be running.",
        ),
        (
            notice::authoring_ended(&running, AuthoringEnd::Completed, 200_000, None),
            "Hivecode built the workflow in 3m 20s and started fix-parser.workflow.",
        ),
        (
            notice::authoring_ended(&checking, AuthoringEnd::Completed, 90_000, None),
            "Hivecode wrote the workflow script fix-parser.workflow in 1m 30s and is waiting for your approval to start it.",
        ),
        (
            notice::authoring_ended(&planning, AuthoringEnd::Completed, 12_000, None),
            "Hivecode finished in 12s without writing a workflow script.",
        ),
    ];
    for (line, expected) in lines {
        assert_eq!(line, expected);
        assert_eq!(line.lines().count(), 1, "one line: {line}");
        assert!(!line.contains("talaria-workflows"), "{line}");
    }
}
