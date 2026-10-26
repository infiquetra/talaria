//! Background process rows: status, order, list replies, misses, and dismiss.

use hermes_contract::schemas::ProcessEntry;
use serde_json::{Value, json};
use talaria_core::background::{
    BackgroundEffect, BackgroundEvent, BackgroundWork, ListedProcess, ProcessId, ProcessStatus,
    status_of, update,
};
use talaria_core::side::RequestId;

fn pid(id: &str) -> ProcessId {
    ProcessId(id.to_string())
}

fn listed(
    id: &str,
    status: &str,
    reason: Option<&str>,
    exit_code: Option<i64>,
    started: Option<f64>,
    exited: Option<f64>,
    tail: Option<&str>,
) -> ListedProcess {
    ListedProcess {
        id: pid(id),
        command: format!("cmd {id}"),
        status_raw: status.to_string(),
        exit_code,
        started_epoch_s: started,
        exited_at_s: exited,
        completion_reason: reason.map(str::to_owned),
        output_tail: tail.map(str::to_owned),
    }
}

fn entry(value: Value) -> ProcessEntry {
    serde_json::from_value(value).expect("process entry")
}

fn foreground(work: &mut BackgroundWork, now_ms: u64) -> RequestId {
    let _ = update(work, BackgroundEvent::Attached { now_ms });
    let effects = update(
        work,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms,
        },
    );
    list_request(&effects)
}

fn list_request(effects: &[BackgroundEffect]) -> RequestId {
    effects
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("a list was requested")
}

fn apply_list(
    work: &mut BackgroundWork,
    request: RequestId,
    rows: Vec<ListedProcess>,
    now_ms: u64,
) -> Vec<BackgroundEffect> {
    update(
        work,
        BackgroundEvent::Listed {
            request,
            processes: rows,
            now_ms,
        },
    )
}

fn ids(work: &BackgroundWork) -> Vec<String> {
    work.rows().map(|row| row.id.0.clone()).collect()
}

#[test]
fn status_reads_every_listed_outcome() {
    let running = listed("p-run", "running", None, None, None, None, None);
    assert!(matches!(status_of(&running), ProcessStatus::Running));
    let other = listed("p-odd", "paused", None, Some(0), None, None, None);
    assert!(matches!(status_of(&other), ProcessStatus::Running));
    let killed = listed(
        "p-kill",
        "exited",
        Some("killed"),
        Some(1),
        None,
        None,
        None,
    );
    assert!(matches!(status_of(&killed), ProcessStatus::Killed));
    let lost = listed("p-lost", "exited", Some("lost"), None, None, None, None);
    assert!(matches!(status_of(&lost), ProcessStatus::Lost));
    let failed_start = listed(
        "p-start",
        "exited",
        Some("failed_start"),
        None,
        None,
        None,
        None,
    );
    assert!(matches!(
        status_of(&failed_start),
        ProcessStatus::FailedToStart
    ));
    let done = listed("p-done", "exited", None, Some(0), None, None, None);
    assert!(matches!(status_of(&done), ProcessStatus::Done));
    let failed = listed("p-fail", "exited", Some("error"), Some(3), None, None, None);
    assert!(matches!(
        status_of(&failed),
        ProcessStatus::Failed { code: 3 }
    ));
    let ended = listed("p-end", "exited", None, None, None, None, None);
    assert!(matches!(status_of(&ended), ProcessStatus::Ended));

    let parsed = ListedProcess::from_entry(&entry(json!({
        "session_id": "proc-missing",
        "cwd": "kept-out",
        "pid": 4,
    })));
    assert_eq!(parsed.command, "");
    assert_eq!(parsed.status_raw, "running");
    assert_eq!(parsed.exit_code, None);
    assert_eq!(parsed.started_epoch_s, None);
    assert_eq!(parsed.exited_at_s, None);
    assert_eq!(parsed.completion_reason, None);
    assert_eq!(parsed.output_tail, None);

    let mut work = BackgroundWork::default();
    let request = foreground(&mut work, 1);
    let effects = apply_list(
        &mut work,
        request,
        vec![listed(
            "proc-odd",
            "paused",
            None,
            None,
            Some(1.0),
            None,
            None,
        )],
        2,
    );
    let logs: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            BackgroundEffect::Log { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(logs, ["unknown process status paused for proc-odd"]);
    let again = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 3 });
    let request = list_request(&again);
    let second = apply_list(
        &mut work,
        request,
        vec![listed(
            "proc-odd",
            "paused",
            None,
            None,
            Some(1.0),
            None,
            None,
        )],
        4,
    );
    assert!(
        !second
            .iter()
            .any(|effect| matches!(effect, BackgroundEffect::Log { .. }))
    );
}

#[test]
fn overlay_order_follows_the_process_id() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work, 10);
    apply_list(
        &mut work,
        request,
        vec![
            listed("late", "running", None, None, Some(30.0), None, None),
            listed("early", "running", None, None, Some(10.0), None, None),
            listed("nostart-b", "running", None, None, None, None, None),
            listed("nostart-a", "running", None, None, None, None, None),
            listed(
                "old-finish",
                "exited",
                None,
                Some(0),
                Some(1.0),
                Some(40.0),
                None,
            ),
            listed(
                "new-finish",
                "exited",
                None,
                Some(0),
                Some(1.0),
                Some(90.0),
                None,
            ),
            listed("tie-b", "running", None, None, Some(10.0), None, None),
            listed("tie-a", "running", None, None, Some(10.0), None, None),
        ],
        10,
    );
    assert_eq!(
        ids(&work),
        [
            "early",
            "tie-a",
            "tie-b",
            "late",
            "nostart-a",
            "nostart-b",
            "new-finish",
            "old-finish",
        ]
    );
    let _ = update(&mut work, BackgroundEvent::Selected { id: pid("late") });
    let refresh = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 11 });
    let request = list_request(&refresh);
    apply_list(
        &mut work,
        request,
        vec![
            listed("early", "running", None, None, Some(1.0), None, None),
            listed("late", "running", None, None, Some(99.0), None, None),
        ],
        12,
    );
    let effects = update(
        &mut work,
        BackgroundEvent::KillRequested {
            id: pid("late"),
            now_ms: 13,
        },
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        BackgroundEffect::KillProcess { id, .. } if id.0 == "late"
    )));
}

#[test]
fn list_reply_adds_and_updates_in_place() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work, 1);
    let _ = update(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "proc-a".to_string(),
            chunk: "chunk-a\n".to_string(),
            now_ms: 2,
        },
    );
    let effects = apply_list(
        &mut work,
        request,
        vec![
            listed(
                "proc-a",
                "running",
                None,
                None,
                Some(1.0),
                None,
                Some("chunk-a\ntail-a\n"),
            ),
            listed("proc-b", "running", None, None, Some(2.0), None, None),
        ],
        3,
    );
    assert!(matches!(
        work.list_state(),
        talaria_core::background::ListState::Fresh { at_ms: 3 }
    ));
    assert_eq!(ids(&work), ["proc-a", "proc-b"]);
    let raw = work.output(&pid("proc-a")).expect("output").raw();
    assert!(
        raw.starts_with("chunk-a\n"),
        "pending chunks come before the snapshot: {raw}"
    );
    assert!(raw.contains("tail-a"));
    let stale = apply_list(
        &mut work,
        RequestId(request.0.saturating_sub(1)),
        vec![listed(
            "proc-z",
            "running",
            None,
            None,
            Some(1.0),
            None,
            None,
        )],
        4,
    );
    assert!(stale.is_empty());
    assert_eq!(ids(&work), ["proc-a", "proc-b"]);
    for effect in &effects {
        if let BackgroundEffect::ListProcesses { request } = effect {
            apply_list(
                &mut work,
                *request,
                vec![
                    listed("proc-a", "running", None, None, Some(1.0), None, None),
                    listed("proc-b", "running", None, None, Some(2.0), None, None),
                ],
                4,
            );
        }
    }
    let refresh = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 5 });
    let request = list_request(&refresh);
    let before = work.version();
    let _ = effects;
    apply_list(
        &mut work,
        request,
        vec![
            listed(
                "proc-b",
                "exited",
                None,
                Some(0),
                Some(2.0),
                Some(6.0),
                None,
            ),
            listed("proc-a", "running", None, None, Some(1.0), None, None),
        ],
        6,
    );
    assert_eq!(
        work.rows().find(|row| row.id.0 == "proc-b").unwrap().status,
        ProcessStatus::Done
    );
    assert!(work.version() > before);
}

#[test]
fn a_running_row_survives_one_miss() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work, 1);
    apply_list(
        &mut work,
        request,
        vec![
            listed(
                "stay",
                "running",
                None,
                None,
                Some(1.0),
                None,
                Some("kept\n"),
            ),
            listed(
                "done",
                "exited",
                None,
                Some(0),
                Some(1.0),
                Some(2.0),
                Some("bye\n"),
            ),
        ],
        2,
    );
    let refresh = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 3 });
    let request = list_request(&refresh);
    apply_list(&mut work, request, vec![], 4);
    let stay = work
        .rows()
        .find(|row| row.id.0 == "stay")
        .expect("running row kept");
    assert_eq!(stay.misses, 1);
    assert!(work.rows().all(|row| row.id.0 != "done"));
    assert!(work.output(&pid("done")).is_none());
    let refresh = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 5 });
    let request = list_request(&refresh);
    apply_list(&mut work, request, vec![], 6);
    assert!(work.rows().all(|row| row.id.0 != "stay"));
    assert!(work.output(&pid("stay")).is_none());
}

#[test]
fn dismissed_rows_stay_hidden() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work, 1);
    apply_list(
        &mut work,
        request,
        vec![
            listed("run", "running", None, None, Some(1.0), None, None),
            listed("fin", "exited", None, Some(0), Some(1.0), Some(2.0), None),
        ],
        2,
    );
    let refused = update(&mut work, BackgroundEvent::Dismiss { id: pid("run") });
    assert!(refused.is_empty());
    assert_eq!(
        work.overlay_note(),
        Some("A running process cannot be dismissed. Kill it first with x.")
    );
    assert!(ids(&work).contains(&"run".to_string()));
    let _ = update(&mut work, BackgroundEvent::Dismiss { id: pid("fin") });
    assert!(!ids(&work).contains(&"fin".to_string()));
    let refresh = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 3 });
    let request = list_request(&refresh);
    apply_list(
        &mut work,
        request,
        vec![listed("run", "running", None, None, Some(1.0), None, None)],
        4,
    );
    let refresh = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 5 });
    let request = list_request(&refresh);
    apply_list(
        &mut work,
        request,
        vec![
            listed("run", "running", None, None, Some(1.0), None, None),
            listed("fin", "exited", None, Some(0), Some(1.0), Some(2.0), None),
        ],
        6,
    );
    assert!(ids(&work).contains(&"fin".to_string()));
    let _ = update(&mut work, BackgroundEvent::Dismiss { id: pid("fin") });
    let refresh = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 7 });
    let request = list_request(&refresh);
    apply_list(
        &mut work,
        request,
        vec![
            listed("run", "running", None, None, Some(1.0), None, None),
            listed("fin", "exited", None, Some(0), Some(1.0), Some(2.0), None),
        ],
        8,
    );
    assert!(!ids(&work).contains(&"fin".to_string()));
}

#[test]
fn an_identical_list_changes_nothing() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work, 1);
    let rows = vec![listed("only", "running", None, None, Some(1.0), None, None)];
    apply_list(&mut work, request, rows.clone(), 2);
    let version = work.version();
    let refresh = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 3 });
    let request = list_request(&refresh);
    let effects = apply_list(&mut work, request, rows, 4);
    assert!(effects.is_empty());
    assert_eq!(work.version(), version);
}
