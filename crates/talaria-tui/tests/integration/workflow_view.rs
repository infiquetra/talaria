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
use unicode_width::UnicodeWidthStr;

#[test]
fn evidence_view_wraps_scrolls_and_preserves_selection() {
    use ratatui::{buffer::Buffer, layout::Rect};
    use talaria_workflow::progress::live::{ProgressEffect, ProgressInput, ProgressKey, update};
    let mut state = ProgressState::new(RunProgress::from_records(&manifest(), &review_lines()));
    update(&mut state, ProgressInput::Tick);
    let selection = state.selection.clone();
    let effects = update(&mut state, ProgressInput::Key(ProgressKey::ViewResult));
    let [ProgressEffect::LoadEvidence { id, .. }] = effects.as_slice() else {
        panic!("missing load")
    };
    update(
        &mut state,
        ProgressInput::EvidenceLoaded {
            id: *id,
            result: Ok(vec![
                "a long line that must wrap across the available columns".into(),
                "the final line".into(),
            ]),
        },
    );
    let area = Rect::new(0, 0, 20, 6);
    let mut buf = Buffer::empty(area);
    WorkflowView::render(area, &mut buf, &state, &WorkflowTheme::default());
    let next = WorkflowView::route_key(&state, area, ProgressKey::EvidenceEnd);
    update(&mut state, ProgressInput::Key(next));
    assert!(state.evidence.as_ref().unwrap().offset > 0);
    WorkflowView::render(area, &mut buf, &state, &WorkflowTheme::default());
    assert_eq!(
        WorkflowView::click(&state, area, 2, area.height - 2),
        Some(ProgressKey::CloseView)
    );
    update(&mut state, ProgressInput::Key(ProgressKey::CloseView));
    assert_eq!(state.selection, selection);
}

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
                    report_file: None,
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
            report_baseline: None,
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
    let buffer = terminal.backend().buffer();
    let mut screen = String::new();
    for y in 0..rows {
        let row: String = (0..cols).map(|x| buffer[(x, y)].symbol()).collect();
        screen.push_str(row.trim_end());
        screen.push('\n');
    }
    screen
}

fn card_point(screen: &str, label: &str) -> Option<(u16, u16)> {
    for (y, row) in screen.lines().enumerate() {
        for (offset, _) in row.match_indices(label) {
            let prefix = &row[..offset];
            if ["┌─ ", "┌─ > ", "┌─ ▸ ", "┌─ > ▸ "]
                .iter()
                .any(|ending| prefix.ends_with(ending))
            {
                return Some((prefix.width() as u16, y as u16));
            }
        }
    }
    None
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
fn a_workflow_error_wraps_above_its_close_hint() {
    use talaria_workflow::progress::live::{ProgressInput, ProgressKey, update};

    let mut state = ProgressState::blank(&manifest());
    update(
        &mut state,
        ProgressInput::Error(
            "Could not open the workflow journal for the selected profile. The gateway returned an unavailable connection. Open the workflow again after connecting the profile."
                .into(),
        ),
    );
    for (width, height) in [(60, 12), (40, 8)] {
        let screen = paint(&state, width, height);
        let rows: Vec<_> = screen.lines().collect();
        assert!(rows[0].contains("Workflow unavailable"), "{screen}");
        assert!(rows[1].contains("Could not open the workflow"), "{screen}");
        let footer = usize::from(height - 2);
        let detail = rows[1..footer]
            .iter()
            .map(|row| row.trim_matches('│').trim())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            detail.contains("The gateway returned an unavailable connection."),
            "{screen}"
        );
        assert!(detail.contains("after connecting the profile."), "{screen}");
        assert!(rows[footer].contains("Esc close"), "{screen}");
        assert!(!detail.contains("Esc close"), "{screen}");
    }
    let short = paint(&state, 60, 4);
    assert!(
        short.lines().nth(2).unwrap().contains("Esc close"),
        "{short}"
    );
    update(&mut state, ProgressInput::Key(ProgressKey::OpenAgent));
    assert!(!state.closed);
    update(&mut state, ProgressInput::Key(ProgressKey::CloseView));
    assert!(state.closed);
}

#[test]
fn the_outline_wraps_usage_and_preserves_each_key_hint() {
    let lines = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/frames/p5-05-progress.jsonl"
    ))
    .lines()
    .map(|line| serde_json::from_str::<RecordLine>(line).expect("recorded journal line"))
    .collect::<Vec<_>>();
    let mut state = ProgressState::new(RunProgress::from_records(&manifest(), &lines));
    state.connected = true;
    let agent = state.progress.agents()[0].key.clone();
    state.select_agent(&agent);
    for width in [60, 80] {
        let height = 24;
        let screen = paint(&state, width, height);
        for hint in [
            "Enter",
            "Esc close",
            "f filter",
            "p pause",
            "Shift+x stop run",
        ] {
            assert!(screen.contains(hint), "missing {hint:?}\n{screen}");
        }
        let rows = screen.lines().collect::<Vec<_>>();
        let preview = rows
            .iter()
            .map(|row| row.trim_matches('│').trim())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            preview.contains("cache reads unknown for 1 turn, counted in full) · $0.10"),
            "{screen}"
        );
        let area = ratatui::layout::Rect::new(0, 0, width, height);
        let footer = rows
            .iter()
            .position(|line| line.contains("p pause"))
            .unwrap();
        assert_eq!(WorkflowView::click(&state, area, 5, footer as u16), None);
    }
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
        "↑↓ select at level · ← fold / parent · → expand / child · Enter transcript · Esc close";
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
fn graph_follows_recorded_child_and_phase_edges_and_opens_the_selected_session() {
    use ratatui::layout::Rect;
    use talaria_workflow::engine::WorkflowMeta;
    use talaria_workflow::progress::live::{
        Filter, NodeKey, ProgressEffect, ProgressInput, ProgressKey, update,
    };

    let mut lines = mixed_lines(RunOutcomeKind::Completed);
    lines.pop();
    let path = vec!["security-review".into()];
    let security_key = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let stored_id = format!("security-transcript-{}-last", "i".repeat(70));
    for line in &mut lines {
        if let Record::CallIntent {
            key,
            phase,
            workflow_path,
            ..
        } = &mut line.record
            && key == security_key
        {
            *workflow_path = path.clone();
            *phase = Some("checks".into());
        }
    }
    lines.insert(
        3,
        RecordLine {
            v: 1,
            seq: 0,
            at_ms: 1300,
            record: Record::Engine {
                event: Some(EngineEvent::ChildStarted {
                    workflow_path: path.clone(),
                    meta: WorkflowMeta {
                        name: "Security review".into(),
                        description: "Review branch changes".into(),
                        when_to_use: None,
                        phases: Vec::new(),
                    },
                    warnings: Vec::new(),
                }),
                event_ref: None,
            },
        },
    );
    lines.insert(
        5,
        RecordLine {
            v: 1,
            seq: 0,
            at_ms: 1400,
            record: Record::SessionBound {
                key: security_key.into(),
                attempt: 0,
                session_id: "security-runtime".into(),
                stored_session_id: stored_id.clone(),
            },
        },
    );
    for (i, line) in (1..).zip(&mut lines) {
        line.seq = i;
    }
    let mut state = ProgressState::new(RunProgress::from_records(&manifest(), &lines));
    state.connected = true;
    let screen = paint(&state, 80, 24);
    for expected in [
        "Workflow structure",
        "├─ Agents",
        "│ ├─ ✓ overview",
        "│ └─ ✓ style pass",
        "└─ Security review · 1 needs attention",
        "  └─ checks",
        "    └─ ✗ security pass",
    ] {
        assert!(screen.contains(expected), "missing {expected:?}\n{screen}");
    }
    let agent_rows: Vec<_> = screen
        .lines()
        .filter(|line| {
            [
                "overview · done",
                "style pass · cut short",
                "security pass · failed",
            ]
            .iter()
            .any(|label| line.contains(label))
        })
        .collect();
    assert_eq!(agent_rows.len(), 3);
    for row in agent_rows {
        assert!(row.ends_with("0s│"), "{row}");
        assert!(
            !row.contains("provider-a") && !row.contains("tokens"),
            "{row}"
        );
    }

    let child = NodeKey::Child(path.clone());
    let phase = NodeKey::Phase {
        path,
        title: "checks".into(),
    };
    let map_area = Rect::new(3, 5, 140, 24);
    let map = paint(&state, map_area.width, map_area.height);
    assert!(
        map.contains("Branch map") && map.contains("Inspector"),
        "{map}"
    );
    let (x, y) = card_point(&map, "Security review").expect("drawn child workflow card");
    assert_eq!(
        WorkflowView::click(&state, map_area, map_area.x + x, map_area.y + y),
        Some(ProgressKey::SelectNode(child.clone()))
    );
    assert_eq!(
        WorkflowView::click(&state, map_area, map_area.x + x, map_area.y + 5),
        None
    );
    let click = |state: &ProgressState, screen: &str, needle: &str| {
        let row = screen
            .lines()
            .position(|line| line.contains(needle))
            .expect(needle);
        WorkflowView::click(state, Rect::new(3, 5, 80, 24), 8, 5 + row as u16)
    };
    let target = click(&state, &screen, "Security review").expect("child row targets a node");
    assert_eq!(target, ProgressKey::SelectNode(child.clone()));
    assert!(update(&mut state, ProgressInput::Key(target)).is_empty());
    assert_eq!(state.selection, child);
    update(&mut state, ProgressInput::Key(ProgressKey::Right));
    assert_eq!(state.selection, phase);
    update(&mut state, ProgressInput::Key(ProgressKey::Left));
    let folded = paint(&state, 80, 24);
    assert!(folded.contains("▸ checks"), "{folded}");
    assert!(!folded.contains("security pass"), "{folded}");
    let folded_map = paint(&state, map_area.width, map_area.height);
    assert!(card_point(&folded_map, "checks").is_some(), "{folded_map}");
    assert!(
        card_point(&folded_map, "security pass").is_none(),
        "{folded_map}"
    );

    state.filter = Filter::Working;
    update(&mut state, ProgressInput::Key(ProgressKey::NextAttention));
    assert_eq!(state.selection, NodeKey::Agent(security_key.into()));
    assert_eq!(state.filter, Filter::All);
    assert!(!state.collapsed.contains(&phase));
    assert!(matches!(
        update(&mut state, ProgressInput::Key(ProgressKey::OpenAgent)).as_slice(),
        [ProgressEffect::OpenSession { session_id }] if session_id == &stored_id
    ));

    let narrow = paint(&state, 40, 8);
    assert!(
        narrow.contains(">     └─ ✗ security pass · failed"),
        "{narrow}"
    );
    let row = narrow
        .lines()
        .position(|line| line.contains("security pass"))
        .unwrap() as u16;
    let area = Rect::new(0, 0, 40, 8);
    assert_eq!(
        WorkflowView::click(&state, area, 9, row),
        Some(ProgressKey::SelectNode(state.selection.clone()))
    );
    assert_eq!(WorkflowView::click(&state, area, 0, row), None);
    assert_eq!(WorkflowView::click(&state, area, 9, 6), None);
    let compact = paint(&state, 45, 14);
    assert!(
        compact.contains("review-branch · running · 2/3 agents"),
        "{compact}"
    );
    assert!(
        compact.contains("agent failed: The conversation grew"),
        "{compact}"
    );
    assert!(
        !compact.contains("security-runtime") && !compact.contains("security-transcript"),
        "{compact}"
    );
    assert!(compact.contains("v evidence  d info/report"), "{compact}");
    update(&mut state, ProgressInput::Key(ProgressKey::Details));
    let area = Rect::new(0, 0, 45, 14);
    assert_eq!(WorkflowView::click(&state, area, 9, row), None);
    assert!(update(&mut state, ProgressInput::Key(ProgressKey::OpenAgent)).is_empty());
    let detail = paint(&state, 45, 14);
    assert!(detail.contains("Workflow agent details"), "{detail}");
    assert!(detail.contains("Model model-a · provider-a"), "{detail}");
    assert!(
        detail.contains("Runtime session security-runtime"),
        "{detail}"
    );
    assert!(detail.contains("security-transcript-"), "{detail}");
    assert!(detail.contains("iiii-last"), "{detail}");
    for _ in 0..3 {
        let scroll = WorkflowView::route_key(&state, area, ProgressKey::Down);
        update(&mut state, ProgressInput::Key(scroll));
    }
    assert!(state.detail_offset.is_some_and(|offset| offset > 0));
    update(&mut state, ProgressInput::Key(ProgressKey::CloseView));
    assert_eq!(state.detail_offset, None);
    assert_eq!(state.selection, NodeKey::Agent(security_key.into()));
    assert!(!state.closed);

    state.steer_input = Some("message".into());
    assert_eq!(WorkflowView::click(&state, area, 9, row), None);
}

#[test]
fn the_map_follows_focus_through_a_scrolled_branch_and_hits_only_cards() {
    use ratatui::layout::Rect;
    use talaria_workflow::progress::live::{NodeKey, ProgressInput, ProgressKey, update};

    let mut lines = review_lines();
    let seed = lines
        .iter()
        .find(|line| matches!(line.record, Record::CallIntent { .. }))
        .unwrap()
        .record
        .clone();
    lines.truncate(1);
    for i in 1..=8 {
        let mut record = seed.clone();
        if let Record::CallIntent {
            key,
            call_id,
            label,
            ..
        } = &mut record
        {
            *key = format!("{i:032x}");
            call_id.digest = key.clone();
            *label = format!("Reader {i}");
        }
        lines.push(RecordLine {
            v: 1,
            seq: i + 1,
            at_ms: 1_000 + i as i64 * 100,
            record,
        });
    }
    let mut state = ProgressState::new(RunProgress::from_records(&manifest(), &lines));
    state.select_agent(&format!("{:032x}", 1));
    let area = Rect::new(0, 0, 120, 24);
    let opening = paint(&state, area.width, area.height);
    assert!(opening.contains("↓ more branches"), "{opening}");
    assert!(card_point(&opening, "Reader 1").is_some(), "{opening}");
    assert!(card_point(&opening, "Reader 8").is_none(), "{opening}");
    assert!(opening.contains("Agents  0/8 done"), "{opening}");
    assert!(opening.contains("Usage"), "{opening}");
    assert!(opening.contains("minimax-m3.1-flash"), "{opening}");
    assert!(opening.contains("8 queued"), "{opening}");

    for i in 2..=8 {
        let key = WorkflowView::route_key(&state, area, ProgressKey::Down);
        assert_eq!(
            key,
            ProgressKey::SelectNode(NodeKey::Agent(format!("{i:032x}")))
        );
        update(&mut state, ProgressInput::Key(key));
    }
    let scrolled = paint(&state, area.width, area.height);
    assert!(scrolled.contains("↑ more branches"), "{scrolled}");
    assert!(card_point(&scrolled, "Reader 1").is_none(), "{scrolled}");
    let (x, y) = card_point(&scrolled, "Reader 8").expect("focused agent card in the viewport");
    assert_eq!(
        WorkflowView::click(&state, area, x, y),
        Some(ProgressKey::SelectNode(state.selection.clone()))
    );
    assert_eq!(WorkflowView::click(&state, area, x, y - 1), None);
    assert_eq!(WorkflowView::click(&state, area, area.width - 4, y), None);
    let key = WorkflowView::route_key(&state, area, ProgressKey::Up);
    assert_eq!(
        key,
        ProgressKey::SelectNode(NodeKey::Agent(format!("{:032x}", 7)))
    );
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
        attention: Default::default(),
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
            "up to 950 tokens (cache reads unknown for 2 turns, counted in full)",
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
        attention: Default::default(),
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
        report_baseline: None,
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

pub(super) fn mixed_outcome_state() -> ProgressState {
    ProgressState::new(RunProgress::from_records(
        &manifest(),
        &mixed_lines(RunOutcomeKind::CompletedWithFailures),
    ))
}

#[test]
fn a_run_that_completed_with_failures_lists_the_failed_agent_above_the_rows() {
    let state = mixed_outcome_state();
    let screen = paint(&state, 140, 28);
    let rows: Vec<&str> = screen.lines().collect();
    let position = |needle: &str| {
        rows.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("no row with {needle:?}\n{screen}"))
    };
    assert!(screen.contains("! completed with failures"), "{screen}");
    assert!(screen.contains("1 cut short"), "{screen}");
    let failure = position(
        "✗ security pass · agent failed: The conversation grew too long. (context_overflow)",
    );
    // The failed agent is named before any agent row.
    assert!(failure < position("overview"), "{screen}");
    assert!(
        rows[position("style pass") + 1].contains("cut short"),
        "{screen}"
    );
    assert!(
        rows[position("overview") + 1].contains("✓ done"),
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
    assert!(screen.contains("✓ completed"), "{screen}");
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
                    report_baseline: None,
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
    let (_, y) = card_point(&after, "long reader").unwrap_or_else(|| panic!("{after}"));
    assert!(
        after
            .lines()
            .nth(y as usize + 1)
            .unwrap()
            .contains("running · step 7"),
        "{after}"
    );
}
