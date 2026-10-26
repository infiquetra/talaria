//! Process acceptance checks reuse the original independently authored suites.

#[path = "background_model.rs"]
mod model;
#[path = "background_output.rs"]
mod output;

use talaria_core::background::{
    BackgroundEffect, BackgroundEvent, BackgroundSignal, BackgroundWork, ListedProcess, ProcessId,
    ProcessStatus, update,
};
use talaria_core::side::RequestId;

fn request(effects: &[BackgroundEffect]) -> RequestId {
    effects
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("a list effect")
}

fn process(status: &str) -> ListedProcess {
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

#[test]
fn running_to_exited_signals_once_even_after_a_switch() {
    let mut work = BackgroundWork::default();
    let _ = update(&mut work, BackgroundEvent::Attached { now_ms: 0 });
    let launch = update(
        &mut work,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 0,
        },
    );
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request: request(&launch),
            processes: vec![process("running")],
            now_ms: 1,
        },
    );
    assert_eq!(work.summary().running, 1);
    let refresh = update(&mut work, BackgroundEvent::RefreshRequested { now_ms: 2 });
    let _ = update(
        &mut work,
        BackgroundEvent::ForegroundChanged {
            foreground: false,
            now_ms: 3,
        },
    );
    let reply = BackgroundEvent::Listed {
        request: request(&refresh),
        processes: vec![process("exited")],
        now_ms: 4,
    };
    let effects = update(&mut work, reply.clone());
    assert_eq!(effects.iter().filter(|effect| matches!(effect, BackgroundEffect::Signal(BackgroundSignal::ProcessFinished { id, status: ProcessStatus::Done }) if id.0 == "tick")).count(), 1);
    assert_eq!(work.summary().running, 0);
    assert_eq!(work.summary().finished_recently, 1);
    assert!(
        update(&mut work, reply).is_empty(),
        "a repeated reply cannot signal twice"
    );
}

#[test]
fn process_tools_trigger_a_list_for_the_foreground_only() {
    for name in ["terminal", "process_manage", "process", "read_file"] {
        for foreground in [false, true] {
            let mut work = BackgroundWork::default();
            let _ = update(&mut work, BackgroundEvent::Attached { now_ms: 0 });
            if foreground {
                let effects = update(
                    &mut work,
                    BackgroundEvent::ForegroundChanged {
                        foreground: true,
                        now_ms: 0,
                    },
                );
                let _ = update(
                    &mut work,
                    BackgroundEvent::Listed {
                        request: request(&effects),
                        processes: vec![],
                        now_ms: 1,
                    },
                );
            }
            let effects = update(
                &mut work,
                BackgroundEvent::ToolCompleted {
                    name: name.into(),
                    now_ms: 2,
                },
            );
            assert_eq!(
                effects
                    .iter()
                    .filter(|effect| matches!(effect, BackgroundEffect::ListProcesses { .. }))
                    .count(),
                usize::from(foreground && name != "read_file"),
                "{name}, foreground={foreground}"
            );
        }
    }
}
