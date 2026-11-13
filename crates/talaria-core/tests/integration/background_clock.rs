//! Polling and wake-up boundaries use the real reducer and an explicit application clock.

use talaria_core::background::{
    BackgroundEffect, BackgroundEvent, BackgroundWork, FINISHED_ROW_MS, ListedProcess,
    OVERLAY_POLL_MS, PROCESS_POLL_MS, ProcessId, next_deadline, update,
};
use talaria_core::side::RequestId;

fn requests(effects: &[BackgroundEffect]) -> Vec<RequestId> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .collect()
}

fn row(status: &str) -> ListedProcess {
    ListedProcess {
        id: ProcessId("tick".into()),
        command: "sh tick.sh".into(),
        status_raw: status.into(),
        exit_code: (status == "exited").then_some(0),
        started_epoch_s: Some(0.0),
        exited_at_s: None,
        completion_reason: None,
        output_tail: None,
    }
}

fn listed(
    work: &mut BackgroundWork,
    request: RequestId,
    status: &str,
    now_ms: u64,
) -> Vec<BackgroundEffect> {
    update(
        work,
        BackgroundEvent::Listed {
            request,
            processes: vec![row(status)],
            now_ms,
        },
    )
}

fn foreground(work: &mut BackgroundWork) -> RequestId {
    assert!(update(work, BackgroundEvent::Attached { now_ms: 0 }).is_empty());
    let effects = update(
        work,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 0,
        },
    );
    assert_eq!(requests(&effects).len(), 1);
    requests(&effects)[0]
}

fn clock(work: &mut BackgroundWork, now_ms: u64) -> Vec<BackgroundEffect> {
    update(
        work,
        BackgroundEvent::Clock {
            now_ms,
            wall_s: now_ms as f64 / 1_000.0,
        },
    )
}

#[test]
fn poll_schedule_and_single_flight() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work);
    listed(&mut work, request, "running", 1);
    assert!(requests(&clock(&mut work, PROCESS_POLL_MS - 1)).is_empty());
    let due = requests(&clock(&mut work, PROCESS_POLL_MS));
    assert_eq!(due.len(), 1);
    for now_ms in [5_001, 6_000, 7_000] {
        assert!(
            requests(&update(
                &mut work,
                BackgroundEvent::RefreshRequested { now_ms }
            ))
            .is_empty()
        );
    }
    let follow = requests(&listed(&mut work, due[0], "running", 7_001));
    assert_eq!(
        follow.len(),
        1,
        "many triggers produce exactly one follow-up"
    );
    assert!(requests(&listed(&mut work, follow[0], "running", 7_002)).is_empty());
    let opened = requests(&update(
        &mut work,
        BackgroundEvent::OverlayOpened { now_ms: 8_000 },
    ));
    assert_eq!(opened.len(), 1);
    listed(&mut work, opened[0], "running", 8_001);
    let _ = update(
        &mut work,
        BackgroundEvent::Selected {
            id: ProcessId("tick".into()),
        },
    );
    assert!(requests(&clock(&mut work, 8_000 + OVERLAY_POLL_MS - 1)).is_empty());
    assert_eq!(
        requests(&clock(&mut work, 8_000 + OVERLAY_POLL_MS)).len(),
        1
    );
}

#[test]
fn next_deadline_and_finished_row_expiry() {
    let mut work = BackgroundWork::default();
    assert_eq!(next_deadline(&work, 0), None);
    let request = foreground(&mut work);
    listed(&mut work, request, "running", 1);
    assert_eq!(next_deadline(&work, 1), Some(1_000));
    let opened = requests(&update(
        &mut work,
        BackgroundEvent::OverlayOpened { now_ms: 2 },
    ));
    listed(&mut work, opened[0], "running", 3);
    let _ = update(
        &mut work,
        BackgroundEvent::Selected {
            id: ProcessId("tick".into()),
        },
    );
    let _ = update(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "tick".into(),
            chunk: "tick 1\n".into(),
            now_ms: 4,
        },
    );
    assert_eq!(next_deadline(&work, 4), Some(37));
    let _ = update(&mut work, BackgroundEvent::OverlayClosed);
    let effects = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 5 });
    listed(&mut work, requests(&effects)[0], "exited", 6);
    assert_eq!(next_deadline(&work, 6), Some(1_000));
    assert_eq!(
        next_deadline(&work, FINISHED_ROW_MS + 5),
        Some(FINISHED_ROW_MS + 6)
    );
    clock(&mut work, FINISHED_ROW_MS + 6);
    assert_eq!(next_deadline(&work, FINISHED_ROW_MS + 6), None);
}

#[test]
fn initial_overlay_selection_uses_display_order_for_polling_and_output() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work);
    let mut finished = row("exited");
    finished.id = ProcessId("finished".into());
    let apply_list = |work: &mut BackgroundWork, request, now_ms| {
        let _ = update(
            work,
            BackgroundEvent::Listed {
                request,
                processes: vec![finished.clone(), row("running")],
                now_ms,
            },
        );
    };
    apply_list(&mut work, request, 1);
    let request = requests(&update(
        &mut work,
        BackgroundEvent::OverlayOpened { now_ms: 1_000 },
    ))[0];
    apply_list(&mut work, request, 1_001);
    assert_eq!(
        work.rows().next().expect("running row").id,
        ProcessId("tick".into())
    );
    let version = work.version();
    let _ = update(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "tick".into(),
            chunk: "tick 1\n".into(),
            now_ms: 1_002,
        },
    );
    assert!(work.version() > version);
    assert!(requests(&clock(&mut work, 2_999)).is_empty());
    assert_eq!(requests(&clock(&mut work, 3_000)).len(), 1);
}
