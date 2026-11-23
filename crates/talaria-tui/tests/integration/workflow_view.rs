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
                cut_short: None,
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
                refused: 0,
                stopped: 0,
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
    let hints = "↑↓ select · ← fold · → unfold · enter open · s steer · x stop · r restart · p pause · f filter · esc close";
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

#[test]
fn the_run_list_reads_token_counts_in_thousands_and_millions() {
    use talaria_workflow::runner::{RunStatus, RunSummary};
    let row = |tokens: Option<u64>, cache: Option<u64>, unknown: u32| RunSummary {
        run_id: "01JB2X9Q4M8Z7K3T5V6W8Y0A1C".into(),
        name: "review-branch".into(),
        profile: "fixture".into(),
        started_at_ms: 1_000,
        status: RunStatus::Completed,
        agents_finished: 3,
        agents_total_known: 3,
        parked: 0,
        tokens_spent: tokens,
        cache_read_tokens: cache,
        cache_unknown_turns: unknown,
        money_spent_usd: None,
        run_dir: "/runs/example".into(),
        warning: None,
    };
    for (tokens, cache, unknown, shown) in [
        (Some(5_812_345), Some(0), 0, "5.8M tokens + 0 cache reads"),
        (
            Some(412_345),
            Some(9_100_000),
            0,
            "412.3k tokens + 9.1M cache reads",
        ),
        (Some(950), Some(0), 0, "950 tokens + 0 cache reads"),
        (
            Some(950),
            Some(0),
            2,
            "950 tokens (cache reads unknown for 2 turns, counted in full)",
        ),
        (None, None, 0, "tokens -"),
    ] {
        let text = talaria_tui::workflow_view::picker_row(&row(tokens, cache, unknown));
        assert!(text.contains(&format!("  {shown}  ")), "{text}");
    }
}

#[test]
fn the_run_list_words_a_status_with_underscores_as_plain_words() {
    use talaria_workflow::runner::{RunStatus, RunSummary};
    let row = RunSummary {
        run_id: "01JB2X9Q4M8Z7K3T5V6W8Y0A1C".into(),
        name: "review-branch".into(),
        profile: "fixture".into(),
        started_at_ms: 1_000,
        status: RunStatus::CompletedWithFailures,
        agents_finished: 3,
        agents_total_known: 3,
        parked: 0,
        tokens_spent: None,
        cache_read_tokens: None,
        cache_unknown_turns: 0,
        money_spent_usd: None,
        run_dir: "/runs/example".into(),
        warning: None,
    };
    let text = talaria_tui::workflow_view::picker_row(&row);
    assert!(
        text.contains("  completed with failures  agents 3/3"),
        "{text}"
    );
    assert!(!text.contains("completed_with_failures"), "{text}");
    let dock = talaria_tui::workflow_view::WorkflowDock::new(
        "review-branch",
        "completed_with_failures",
        3,
        3,
        None,
        ratatui::style::Style::default(),
        ratatui::style::Style::default(),
    );
    let line: String = dock.lines(120)[0]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect();
    assert!(
        line.contains("· completed with failures · 3/3 agents"),
        "{line}"
    );
}

/// A run of three agents: one finished plainly, one failed, one cut short at its step cap.
fn mixed_lines(run: RunOutcomeKind) -> Vec<RecordLine> {
    let intent = |key: &str, label: &str| Record::CallIntent {
        key: key.to_string(),
        attempt: 0,
        call_id: CallId {
            digest: key.to_string(),
            occurrence: 0,
        },
        label: label.to_string(),
        phase: None,
        workflow_path: Vec::new(),
        launch: ResolvedLaunch {
            profile: "fixture".to_string(),
            model: "model-a".to_string(),
            provider: Some("provider-a".to_string()),
            effort: None,
            toolsets: Vec::new(),
            skills: Vec::new(),
            max_turns: None,
            isolation: None,
            cwd: None,
        },
        request_id: format!("wf:run:{key}"),
    };
    let finished = |key: &str, outcome: CallOutcome| Record::CallFinished {
        key: key.to_string(),
        attempt: 0,
        outcome,
        turns: 1,
    };
    let (a, b, c) = (
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "cccccccccccccccccccccccccccccccc",
    );
    let completed = |cut_short| CallOutcome::Completed {
        value: Some(serde_json::json!("ok")),
        value_ref: None,
        cut_short,
    };
    let records = [
        Record::RunStarted {
            run_id: "01JB2X9Q4M8Z7K3T5V6W8Y0A1C".to_string(),
            script_sha256: "a".repeat(64),
        },
        intent(a, "overview"),
        finished(a, completed(None)),
        intent(b, "security pass"),
        finished(
            b,
            CallOutcome::Failed {
                reason: "agent failed: The conversation grew too long. (context_overflow)".into(),
            },
        ),
        intent(c, "style pass"),
        finished(
            c,
            completed(Some(talaria_workflow::engine::CutShort {
                steps: Some(60),
                limit: Some(60),
                saved_branch: None,
            })),
        ),
        Record::RunFinished {
            outcome: run,
            summary: FinishSummary::default(),
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

#[test]
fn a_run_that_completed_with_failures_lists_the_failed_agent_above_the_rows() {
    let state = ProgressState::new(RunProgress::from_records(
        &manifest(),
        &mixed_lines(RunOutcomeKind::CompletedWithFailures),
    ));
    let screen = paint(&state, 140, 24);
    let rows: Vec<&str> = screen.lines().collect();
    let position = |needle: &str| {
        rows.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("no row with {needle:?}\n{screen}"))
    };
    assert!(
        rows[position("review-branch")].contains("completed with failures"),
        "{screen}"
    );
    assert!(
        rows[position("review-branch")].contains("1 cut short"),
        "{screen}"
    );
    let failure = position(
        "✗ security pass · agent failed: The conversation grew too long. (context_overflow)",
    );
    // The failed agent is named before any agent row.
    assert!(failure < position("overview"), "{screen}");
    // The cut short agent's row carries its mark; the plain one does not.
    assert!(
        rows[position("style pass")].contains("cut short (60 of 60 steps)"),
        "{screen}"
    );
    assert!(
        !rows[position("overview")].contains("cut short"),
        "{screen}"
    );
}

#[test]
fn a_plain_completed_run_lists_nothing_above_its_rows() {
    let state = ProgressState::new(RunProgress::from_records(
        &manifest(),
        &mixed_lines(RunOutcomeKind::Completed),
    ));
    let screen = paint(&state, 140, 24);
    assert!(
        !screen.contains("✗ security pass · agent failed"),
        "{screen}"
    );
    assert!(screen.contains("completed ·"), "{screen}");
}

#[test]
fn a_running_agent_row_shows_its_current_step() {
    use talaria_workflow::progress::live::{ProgressInput, update};
    let key = "dddddddddddddddddddddddddddddddd";
    let lines: Vec<RecordLine> = mixed_lines(RunOutcomeKind::Completed)
        .into_iter()
        .filter(|line| matches!(line.record, Record::RunStarted { .. }))
        .chain(
            [
                Record::CallIntent {
                    key: key.to_string(),
                    attempt: 0,
                    call_id: CallId {
                        digest: key.to_string(),
                        occurrence: 0,
                    },
                    label: "long reader".to_string(),
                    phase: None,
                    workflow_path: Vec::new(),
                    launch: ResolvedLaunch {
                        profile: "fixture".to_string(),
                        model: "model-a".to_string(),
                        provider: Some("provider-a".to_string()),
                        effort: None,
                        toolsets: Vec::new(),
                        skills: Vec::new(),
                        max_turns: None,
                        isolation: None,
                        cwd: None,
                    },
                    request_id: "wf:run:d".to_string(),
                },
                Record::SessionBound {
                    key: key.to_string(),
                    attempt: 0,
                    session_id: "sess-d".to_string(),
                    stored_session_id: "sess-d".to_string(),
                },
                Record::TurnAdmitted {
                    key: key.to_string(),
                    attempt: 0,
                    turn: 0,
                    admission_id: "adm-d".to_string(),
                    status: AdmissionState::Started,
                    outcome: None,
                    execution_generation: Some(1),
                    authority_epoch: 1,
                },
            ]
            .into_iter()
            .enumerate()
            .map(|(index, record)| RecordLine {
                v: 1,
                seq: u64::try_from(index).expect("seq") + 2,
                at_ms: 2_000 + i64::try_from(index).expect("time"),
                record,
            }),
        )
        .collect();
    let mut state = ProgressState::new(RunProgress::from_records(&manifest(), &lines));
    let before = paint(&state, 140, 24);
    assert!(!before.contains("step "), "{before}");
    update(
        &mut state,
        ProgressInput::Gateway {
            session_id: "sess-d".to_string(),
            event: hermes_contract::events::Notification::Unknown(serde_json::json!({
                "type": "agent.step", "session_id": "sess-d", "payload": {"iteration": 7}
            })),
        },
    );
    let after = paint(&state, 140, 24);
    let row = after
        .lines()
        .find(|row| row.contains("long reader"))
        .unwrap_or_else(|| panic!("{after}"));
    assert!(row.contains("running · step 7"), "{row}");
}
