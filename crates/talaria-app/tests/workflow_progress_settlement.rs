#[path = "../../talaria-workflow/tests/support/lines.rs"]
mod lines;
#[path = "support/mock_gateway.rs"]
mod mock_gateway;
#[path = "../../talaria-workflow/tests/support/replay_run.rs"]
mod replay;

use serde_json::json;
use talaria_app::{
    app::{self, AppEffect, AppMessage, AppState, UserAction},
    workflow_progress::{PollBatch, ProgressService},
};
use talaria_workflow::{
    journal::{CallOutcome, Record, RecordLine},
    progress::{
        AgentState, RunProgress,
        live::{self, ProgressEffect, ProgressInput, ProgressKey, ProgressState},
    },
};

#[test]
fn app_polls_each_tick_and_after_stop_ack_then_redraws_the_settled_row() {
    let key = lines::KEY_RUNNING;
    let run = std::path::PathBuf::from("fixture-run");
    let records = lines::live_running(key, "sess-running", 7);
    let seq = records.last().unwrap().seq;
    let mut view = ProgressState::new(RunProgress::from_records(&lines::manifest(), &records));
    live::update(&mut view, ProgressInput::Connected(true));
    view.select_agent(key);
    let mut app = AppState::new("progress-test");
    app.workflow_views.active = Some(run.clone());
    app.workflow_views.states.insert(run.clone(), view);
    app.workflow_views.offsets.insert(run.clone(), 42);
    let tick = app::update(&mut app, AppMessage::Tick { now_ms: 0 });
    assert!(
        tick.iter()
            .any(|e| matches!(e, AppEffect::WorkflowPoll { offset: 42, .. }))
    );
    let empty = PollBatch {
        lines: vec![],
        offset: 42,
        controls: Default::default(),
    };
    app::update(
        &mut app,
        AppMessage::WorkflowPolled {
            run: run.clone(),
            result: Ok(empty),
        },
    );
    let stop = app::update(
        &mut app,
        AppMessage::User(UserAction::Workflow(ProgressKey::StopAgent)),
    );
    assert!(stop.iter().any(|e|matches!(e,AppEffect::WorkflowExecute {effects,..} if matches!(effects.as_slice(),[ProgressEffect::Interrupt {..}]))));
    let ack = app::update(
        &mut app,
        AppMessage::WorkflowProgress {
            run: run.clone(),
            input: ProgressInput::GatewayResponse {
                key: key.into(),
                session_id: "sess-running".into(),
                result: Ok(json!({"interrupted":true})),
            },
        },
    );
    assert!(
        ack.iter()
            .any(|e| matches!(e, AppEffect::WorkflowPoll { offset: 42, .. }))
    );
    let redraw = app::update(
        &mut app,
        AppMessage::WorkflowPolled {
            run: run.clone(),
            result: Ok(PollBatch {
                lines: vec![RecordLine {
                    v: 1,
                    seq: seq + 1,
                    at_ms: 9000,
                    record: Record::CallFinished {
                        key: key.into(),
                        attempt: 0,
                        outcome: CallOutcome::Failed {
                            reason: "interrupted".into(),
                        },
                        turns: 1,
                    },
                }],
                offset: 99,
                controls: Default::default(),
            }),
        },
    );
    assert!(redraw.iter().any(|e| matches!(e, AppEffect::Redraw)));
    let state = &app.workflow_views.states[&run];
    assert_eq!(state.progress.agent(key).unwrap().state, AgentState::Failed);
    assert_eq!(
        state.footer.as_deref(),
        Some("stopped — restart (r) runs it again")
    );
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(220, 20)).unwrap();
    terminal
        .draw(|frame| {
            talaria_tui::workflow_view::WorkflowView::draw(
                frame,
                frame.area(),
                state,
                &Default::default(),
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let text = (0..20)
        .map(|y| {
            (0..220)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("stopped — restart (r) runs it again"));
    let second = app::update(&mut app, AppMessage::Tick { now_ms: 1 });
    assert!(
        second
            .iter()
            .any(|e| matches!(e, AppEffect::WorkflowPoll { offset: 99, .. }))
    );
}

#[tokio::test]
async fn service_checks_exact_admission_receipt_without_writing_the_journal() {
    let (_temp, dir) = replay::empty_run();
    std::fs::write(dir.path().join("journal.jsonl"), "").unwrap();
    let before = std::fs::read(dir.path().join("journal.jsonl")).unwrap();
    let (gateway, client, _inbound) = mock_gateway::MockGateway::start().await;
    let mut receipt = mock_gateway::canonical_result("prompt.receipt", "sess-running").unwrap();
    receipt["admission_id"] = json!("adm-running");
    receipt["status"] = json!("terminal");
    receipt["outcome"] = json!("interrupted");
    gateway.push_result(receipt);
    let mut service = ProgressService::new(dir.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    service.set_connection(Some(client), tx);
    let mut state = ProgressState::blank(dir.manifest());
    service
        .execute(
            &mut state,
            vec![ProgressEffect::CheckStopped {
                session_id: "sess-running".into(),
                admission_id: "adm-running".into(),
                attempt: 2,
            }],
        )
        .await
        .unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result,AppMessage::WorkflowProgress {input:ProgressInput::StopReceipt {attempt:2,outcome:Some(outcome),..},..} if outcome=="interrupted")
    );
    let requests = gateway.requests("prompt.receipt");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].params,
        json!({"session_id":"sess-running","admission_id":"adm-running","include_result":true})
    );
    assert_eq!(
        std::fs::read(dir.path().join("journal.jsonl")).unwrap(),
        before
    );
}

/// No screen animation or input timer is available to wake the real application loop.
struct IdleWorkflowScreen {
    frames: tokio::sync::mpsc::UnboundedSender<Option<talaria_workflow::progress::RunState>>,
}
impl app::ComposerDeadline for IdleWorkflowScreen {
    fn tick(&mut self, _: std::time::Instant) {}
    fn next_tick(&self) -> Option<std::time::Instant> {
        None
    }
}
impl app::StatusHost for IdleWorkflowScreen {
    fn on_tick(&mut self, _: std::time::Instant) {}
    fn next_wake(&self, _: std::time::Instant) -> Option<std::time::Instant> {
        None
    }
    async fn shutdown(&mut self) {}
}
impl app::Screen for IdleWorkflowScreen {
    type Input = UserAction;
    fn input(
        &mut self,
        input: UserAction,
        _: &app::View<'_>,
        _: std::time::Instant,
    ) -> Vec<UserAction> {
        vec![input]
    }
    fn draw(&mut self, view: &app::View<'_>) {
        if view.session_id.is_some() {
            let _ = self
                .frames
                .send(view.workflow_progress.map(|s| s.progress.run.state));
        }
    }
    fn commit_context(&mut self, _: &talaria_app::context::ContextBlock) {}
    fn commit_lines(&mut self, _: Vec<String>) {}
    fn commit_cells(&mut self, _: &[talaria_core::session::Cell]) {}
    fn load_draft(&mut self, _: &str) {}
    fn restore(&mut self) {}
}

#[test]
fn idle_interactive_loop_consumes_journal_without_keypresses_or_animation() {
    use talaria_app::launch::{Launch, StartMode};
    use talaria_workflow::{engine::RunOutcomeKind, progress::RunState};
    let home = tempfile::tempdir().unwrap();
    let gateway = mock_gateway::StandaloneGateway::start(home.path());
    let (_runs, dir) = replay::empty_run();
    let initial = lines::finished_lines().remove(0);
    std::fs::write(dir.path().join("journal.jsonl"), lines::jsonl(&[initial])).unwrap();
    gateway.block_on(async {
        let (frames, mut rendered) = tokio::sync::mpsc::unbounded_channel();
        let mut screen = IdleWorkflowScreen { frames };
        let (tx, inputs) = tokio::sync::mpsc::unbounded_channel();
        let launch = Launch {
            mode: StartMode::New,
            model: None,
            provider: None,
            effort: None,
            profile: "default".into(),
            folder: home.path().to_path_buf(),
            record: None,
            bench_replay: None,
            no_ensure: true,
        };
        let setup = app::RunSetup {
            home: home.path().to_path_buf(),
            launch_id: "idle-workflow-test".into(),
            preopened: None,
            list_title: None,
            notices: vec![],
            keys: Default::default(),
            fleet: None,
        };
        let script = async {
            tokio::time::timeout(std::time::Duration::from_secs(5), rendered.recv())
                .await
                .unwrap()
                .unwrap();
            tx.send(UserAction::Submit(format!(
                "/workflows {}",
                dir.path().display()
            )))
            .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while rendered.recv().await.unwrap() != Some(RunState::Running) {}
            })
            .await
            .unwrap();
            // Drain opening traffic, then observe an idle view for several poll intervals.
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            while rendered.try_recv().is_ok() {}
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            assert!(
                rendered.try_recv().is_err(),
                "unchanged workflow ticks must not redraw"
            );
            use std::io::Write;
            let mut journal = std::fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join("journal.jsonl"))
                .unwrap();
            writeln!(
                journal,
                "{}",
                serde_json::to_string(&RecordLine {
                    v: 1,
                    seq: 2,
                    at_ms: 9000,
                    record: Record::RunFinished {
                        outcome: RunOutcomeKind::Aborted,
                        summary: Default::default()
                    }
                })
                .unwrap()
            )
            .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                while rendered.recv().await.unwrap() != Some(RunState::Aborted) {}
            })
            .await
            .unwrap();
            tx.send(UserAction::Quit).unwrap();
        };
        let (outcome, ()) = tokio::join!(app::run(launch, setup, &mut screen, inputs), script);
        assert_eq!(outcome.code, 0, "{}", outcome.stderr);
    });
}
