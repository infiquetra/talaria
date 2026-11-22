//! Progress overlay. The first draw is the view stub.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use talaria_tui::workflow_view::{WorkflowTheme, WorkflowView};
use talaria_workflow::engine::{AgentRequest, CallId, EngineEvent, RunOutcomeKind};
use talaria_workflow::journal::{
    AdmissionState, CallOutcome, FinishSummary, Manifest, Record, RecordLine, ResolvedLaunch,
};
use talaria_workflow::progress::RunProgress;
use talaria_workflow::progress::live::ProgressState;

fn manifest() -> Manifest {
    serde_json::from_value(serde_json::json!({
        "v": 1,
        "run_id": "01JB2X9Q4M8Z7K3T5V6W8Y0A1C",
        "created_at_ms": 1_000,
        "workflow": {
            "name": "review-branch",
            "script_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        },
        "args": null,
        "started_at_ms": 1_000,
        "seed": 4242,
        "profile": "fixture",
        "on_unknown": "wait",
        "talaria_version": env!("CARGO_PKG_VERSION"),
        "hermes": {},
        "roster": {},
        "approval_policy": {}
    }))
    .expect("manifest")
}

fn review_lines() -> Vec<RecordLine> {
    let key = "11111111111111111111111111111111";
    let call_id = CallId {
        digest: key.to_string(),
        occurrence: 0,
    };
    let launch = ResolvedLaunch {
        profile: "fixture".to_string(),
        model: "minimax-m3.1-flash".to_string(),
        provider: Some("ollama-cloud".to_string()),
        effort: None,
        toolsets: Vec::new(),
        skills: Vec::new(),
        max_turns: None,
        isolation: None,
        cwd: None,
    };
    let records = [
        Record::RunStarted {
            run_id: "01JB2X9Q4M8Z7K3T5V6W8Y0A1C".to_string(),
            script_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
        },
        Record::Engine {
            event: Some(EngineEvent::AgentStarted {
                request: AgentRequest {
                    call_id: call_id.clone(),
                    label: "correctness".to_string(),
                    phase: Some("plan".to_string()),
                    prompt: "Review for correctness".to_string(),
                    schema: None,
                    tier: None,
                    model: Some("minimax-m3.1-flash".to_string()),
                    provider: Some("ollama-cloud".to_string()),
                    effort: None,
                    profile: Some("fixture".to_string()),
                    toolsets: None,
                    skills: None,
                    isolation: None,
                    max_turns: None,
                    workflow_path: Vec::new(),
                },
                replayed: false,
            }),
            event_ref: None,
        },
        Record::CallIntent {
            key: key.to_string(),
            attempt: 0,
            call_id,
            label: "correctness".to_string(),
            phase: Some("plan".to_string()),
            workflow_path: Vec::new(),
            launch,
            request_id: format!("wf:01JB2X9Q4M8Z7K3T5V6W8Y0A1C:{key}"),
        },
        Record::TurnAdmitted {
            key: key.to_string(),
            attempt: 0,
            turn: 0,
            admission_id: "adm-correctness".to_string(),
            status: AdmissionState::Started,
            outcome: None,
            execution_generation: Some(1),
            authority_epoch: 1,
        },
        Record::CallFinished {
            key: key.to_string(),
            attempt: 0,
            outcome: CallOutcome::Completed {
                value: Some(serde_json::json!("ok")),
                value_ref: None,
            },
            turns: 1,
        },
        Record::RunFinished {
            outcome: RunOutcomeKind::Completed,
            summary: FinishSummary {
                completed: 1,
                failed: 0,
                skipped: 0,
                parked: 0,
                unfinished: 0,
            },
        },
    ];
    records
        .into_iter()
        .enumerate()
        .map(|(index, record)| RecordLine {
            v: 1,
            seq: u64::try_from(index).expect("seq") + 1,
            at_ms: 1_000 + i64::try_from(index).expect("time") * 100,
            record,
        })
        .collect()
}

fn tree_state() -> ProgressState {
    let manifest = manifest();
    let mut state = ProgressState::new(RunProgress::from_records(&manifest, &review_lines()));
    state.steer_input = Some("look again".to_string());
    state
}

fn paint(state: &ProgressState, cols: u16, rows: u16) -> String {
    let backend = TestBackend::new(cols, rows);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| WorkflowView::draw(frame, frame.area(), state, &WorkflowTheme::default()))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..rows {
        for x in 0..cols {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\r');
        text.push('\n');
    }
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(text.as_bytes());
    let mut screen = String::new();
    for row in parser.screen().rows(0, cols) {
        screen.push_str(row.trim_end());
        screen.push('\n');
    }
    screen
}

/// Paint when the revision changed. The same revision is not another frame.
fn present(
    state: &ProgressState,
    seen: &mut Option<u64>,
    draws: &mut u32,
    cols: u16,
    rows: u16,
) -> Option<String> {
    if *seen == Some(state.revision) {
        return None;
    }
    *seen = Some(state.revision);
    *draws += 1;
    Some(paint(state, cols, rows))
}

#[test]
fn overlay_renders_tree_and_steer_input() {
    let state = tree_state();
    let wide = paint(&state, 80, 24);
    assert!(wide.contains("review-branch"), "{wide}");
    assert!(wide.contains("correctness"), "{wide}");
    assert!(wide.contains("look again"), "{wide}");
    let narrow = paint(&state, 40, 24);
    assert!(narrow.contains("correctness"), "{narrow}");
    assert!(narrow.contains("look again"), "{narrow}");

    let idle = ProgressState::blank(&manifest());
    let mut seen = None;
    let mut draws = 0;
    let first = present(&idle, &mut seen, &mut draws, 80, 24).expect("first idle frame");
    assert!(
        first.contains("run starting — nothing journaled yet"),
        "{first}"
    );
    assert!(present(&idle, &mut seen, &mut draws, 80, 24).is_none());
    assert_eq!(draws, 1);
}

#[test]
fn empty_list_names_the_filter_and_the_key_that_changes_it() {
    use talaria_workflow::progress::live::{ProgressInput, ProgressKey, update};
    let mut state = ProgressState::new(RunProgress::from_records(&manifest(), &review_lines()));
    for _ in 0..3 {
        update(&mut state, ProgressInput::Key(ProgressKey::CycleFilter));
    }
    let screen = paint(&state, 120, 24);
    assert!(
        screen.contains("no agents match the failed filter · f changes it"),
        "{screen}"
    );
    assert!(screen.contains("filter: failed"), "{screen}");
    assert!(!screen.contains("no agents match this filter"), "{screen}");
}

#[test]
fn an_expired_message_gives_the_footer_back_to_the_key_hints() {
    use talaria_workflow::progress::live::{ProgressInput, ProgressKey, update};
    let hints =
        "↑↓ select · enter open · s steer · x stop · r restart · p pause · f filter · esc close";
    let mut state = ProgressState::new(RunProgress::from_records(&manifest(), &review_lines()));
    update(&mut state, ProgressInput::Clock { now_ms: 1_000 });
    update(&mut state, ProgressInput::Key(ProgressKey::PauseRun));
    let shown = paint(&state, 120, 24);
    assert!(shown.contains("run already finished"), "{shown}");
    assert!(!shown.contains(hints), "{shown}");
    update(&mut state, ProgressInput::Clock { now_ms: 6_000 });
    let back = paint(&state, 120, 24);
    let rows: Vec<&str> = back.lines().collect();
    let bottom = rows
        .iter()
        .position(|r| r.starts_with('└'))
        .expect("border");
    assert!(rows[bottom - 2].contains(hints), "{back}");
    assert!(!back.contains("run already finished"), "{back}");
}
