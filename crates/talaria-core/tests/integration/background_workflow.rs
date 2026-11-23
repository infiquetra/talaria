//! Recognising a background process that runs a workflow, the run it carries, and a readable
//! form of its JSON event lines.

use talaria_core::background::{
    BackgroundEvent, BackgroundWork, ListedProcess, ProcessId, RunIdentity, RunLink, WorkflowVerb,
    last_whole_line, started_run, summarize_event_line, update, workflow_command, workflow_process,
};

const RUN: RunIdentity<'static> = RunIdentity {
    run_id: "01RUNIDEXAMPLE0000000000A",
    run_dir: "/data/runs/01RUNIDEXAMPLE0000000000A",
};

fn work_with(command: &str, output: Option<&str>) -> BackgroundWork {
    let mut work = BackgroundWork::default();
    let _ = update(&mut work, BackgroundEvent::Attached { now_ms: 0 });
    let effects = update(
        &mut work,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 0,
        },
    );
    let request = effects
        .iter()
        .find_map(|effect| match effect {
            talaria_core::background::BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("the foreground lists");
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request,
            processes: vec![ListedProcess {
                id: ProcessId("p1".into()),
                command: command.into(),
                status_raw: "running".into(),
                exit_code: None,
                started_epoch_s: Some(1.0),
                exited_at_s: None,
                completion_reason: None,
                output_tail: None,
            }],
            now_ms: 1,
        },
    );
    if let Some(text) = output {
        let _ = update(
            &mut work,
            BackgroundEvent::TerminalOutput {
                process_id: "p1".into(),
                chunk: text.into(),
                now_ms: 2,
            },
        );
    }
    work
}

#[test]
fn the_label_names_the_workflow_command_and_not_the_first_command_of_the_line() {
    let line = "ls -A /srv/project | wc -l; HERMES_HOME='/srv/home' talaria workflow run --json \
                --profile work --budget-tokens 5 /srv/project/review.workflow --args '{}'";
    let command = workflow_command(line).expect("a workflow run");
    assert_eq!(command.verb, WorkflowVerb::Run);
    assert_eq!(command.label, "talaria workflow run review.workflow");
    assert_eq!(command.resume_target, None);

    let resume = workflow_command("/usr/local/bin/talaria workflow resume --json 01RUN > out.txt")
        .expect("a workflow resume");
    assert_eq!(resume.verb, WorkflowVerb::Resume);
    assert_eq!(resume.label, "talaria workflow resume 01RUN");
    assert_eq!(resume.resume_target.as_deref(), Some("01RUN"));
}

#[test]
fn other_commands_are_not_workflow_commands() {
    for line in [
        "sh tick.sh",
        "talaria workflow list --json",
        "talaria workflow stop 01RUN",
        "echo talaria",
        "talaria workflow",
        "talaria-other workflow run x",
    ] {
        assert_eq!(workflow_command(line), None, "{line}");
    }
}

#[test]
fn a_resume_names_its_run_and_a_run_names_it_in_its_first_line() {
    let resume = work_with(
        "talaria workflow resume 01RUNIDEXAMPLE0000000000A --json",
        None,
    );
    let row = resume.rows().next().unwrap();
    let process = workflow_process(row, resume.output(&row.id)).unwrap();
    assert!(process.carries(RUN));

    let by_path = work_with(
        "talaria workflow resume /data/runs/01RUNIDEXAMPLE0000000000A/",
        None,
    );
    let row = by_path.rows().next().unwrap();
    assert!(
        workflow_process(row, by_path.output(&row.id))
            .unwrap()
            .carries(RUN)
    );

    let other = work_with("talaria workflow resume 01OTHERRUN", None);
    let row = other.rows().next().unwrap();
    assert!(
        !workflow_process(row, other.output(&row.id))
            .unwrap()
            .carries(RUN)
    );

    let started = work_with(
        "talaria workflow run --json a.workflow",
        Some(concat!(
            "{\"event\":\"run_started\",\"run_id\":\"01RUNIDEXAMPLE0000000000A\",",
            "\"run_dir\":\"/data/runs/01RUNIDEXAMPLE0000000000A\"}\n",
            "{\"event\":\"phase\",\"title\":\"Build\"}\n"
        )),
    );
    let row = started.rows().next().unwrap();
    let process = workflow_process(row, started.output(&row.id)).unwrap();
    assert!(process.carries(RUN));
    assert!(matches!(process.link, RunLink::Started { .. }));
}

#[test]
fn a_run_with_no_start_line_has_an_unknown_run_and_carries_nothing() {
    let work = work_with(
        "talaria workflow run --json a.workflow",
        Some("{\"event\":\"phase\",\"title\":\"Build\"}\n"),
    );
    let row = work.rows().next().unwrap();
    let process = workflow_process(row, work.output(&row.id)).unwrap();
    assert_eq!(process.link, RunLink::Unknown);
    assert!(!process.carries(RUN));

    let no_output = work_with("talaria workflow run a.workflow", None);
    let row = no_output.rows().next().unwrap();
    let process = workflow_process(row, no_output.output(&row.id)).unwrap();
    assert_eq!(process.link, RunLink::Unknown);
}

#[test]
fn the_plain_text_start_line_names_the_run_too() {
    let link =
        started_run("run 01RUNIDEXAMPLE0000000000A in /data/runs/01RUNIDEXAMPLE0000000000A\n")
            .expect("a start line");
    assert!(link.is_run(RUN));
    assert_eq!(started_run("run something else entirely\n"), None);
    assert_eq!(started_run("{\"event\":\"phase\"}\n"), None);
}

#[test]
fn event_lines_read_as_the_event_and_its_label() {
    assert_eq!(
        summarize_event_line(r#"{"event":"phase","title":"Build the parser"}"#).as_deref(),
        Some("phase · Build the parser")
    );
    assert_eq!(
        summarize_event_line(
            r#"{"event":"agent_started","label":"fix-1","model":"m","provider":"p"}"#
        )
        .as_deref(),
        Some("agent_started · fix-1")
    );
    assert_eq!(
        summarize_event_line(
            r#"{"event":"scheduler","value":{"at":"2026-11-23T10:00:00Z","event":"queued","label":"fix-2"}}"#
        )
        .as_deref(),
        Some("scheduler queued · fix-2")
    );
    assert_eq!(
        summarize_event_line(r#"{"event":"scheduler","value":{"at":"2026-11-23T10:00:00Z"}}"#)
            .as_deref(),
        Some("scheduler")
    );
    // A line cut short before its end still gives its event name.
    assert_eq!(
        summarize_event_line(r#"{"event":"scheduler","value":{"at":"2026-11-23T10:0"#).as_deref(),
        Some("scheduler")
    );
    // Not an event line: left to the caller.
    assert_eq!(summarize_event_line("tick 185"), None);
    assert_eq!(summarize_event_line(r#"{"other":1}"#), None);
    assert_eq!(summarize_event_line("[1,2]"), None);
}

#[test]
fn the_last_whole_line_skips_blank_lines() {
    assert_eq!(last_whole_line("a\nb\n\n  \n").as_deref(), Some("b"));
    assert_eq!(last_whole_line("\n\n"), None);
}
