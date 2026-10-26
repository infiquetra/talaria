//! Output buffers: the cap, snapshot merge, unknown ids, and the live line.

use talaria_core::background::{
    BackgroundEffect, BackgroundEvent, BackgroundWork, KillState, ListedProcess,
    MAX_PENDING_UNKNOWN, MAX_TRACKED_OUTPUTS, OUTPUT_CAP_CHARS, OUTPUT_TOTAL_CHARS, ProcessId,
    ProcessOutput, ProcessStatus, UNKNOWN_CHUNK_LIST_MS, update,
};
use talaria_core::side::RequestId;

fn pid(id: &str) -> ProcessId {
    ProcessId(id.to_string())
}

fn row(id: &str, status: &str, started: Option<f64>) -> ListedProcess {
    ListedProcess {
        id: pid(id),
        command: id.to_string(),
        status_raw: status.to_string(),
        exit_code: None,
        started_epoch_s: started,
        exited_at_s: None,
        completion_reason: None,
        output_tail: None,
    }
}

fn foreground(work: &mut BackgroundWork) -> RequestId {
    let _ = update(work, BackgroundEvent::Attached { now_ms: 0 });
    let effects = update(
        work,
        BackgroundEvent::ForegroundChanged {
            foreground: true,
            now_ms: 0,
        },
    );
    effects
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("a list was requested")
}

fn refresh(work: &mut BackgroundWork, now_ms: u64) -> RequestId {
    let effects = update(work, BackgroundEvent::RefreshRequested { now_ms });
    effects
        .iter()
        .find_map(|effect| match effect {
            BackgroundEffect::ListProcesses { request } => Some(*request),
            _ => None,
        })
        .expect("a list was requested")
}

#[test]
fn output_keeps_the_last_quarter_million_characters() {
    let mut output = ProcessOutput::default();
    let mut body = "é".repeat(OUTPUT_CAP_CHARS);
    body.push('\n');
    body.push_str(&"你".repeat(100));
    output.push_chunk(&body, 1);
    let raw = output.raw();
    assert!(raw.is_char_boundary(0));
    assert!(output.trimmed_front());
    assert!(raw.chars().count() <= OUTPUT_CAP_CHARS);
    assert!(
        !raw.starts_with('é'),
        "the cut starts after the first newline in the kept head"
    );
    let fence = output.raw().to_string();
    output.merge_snapshot(&fence, 2);
    assert_eq!(output.raw(), fence);
}

#[test]
fn snapshot_merge_follows_the_tail_rules() {
    let mut output = ProcessOutput::default();
    output.push_chunk("body-text", 1);
    output.merge_snapshot("", 2);
    assert_eq!(output.raw(), "body-text");

    output.merge_snapshot("body-text", 3);
    assert_eq!(output.raw(), "body-text");
    assert!(!output.trimmed_front());

    output.merge_snapshot("body-text-more", 4);
    assert_eq!(output.raw(), "body-text-more");

    let mut fresh = ProcessOutput::default();
    let wide = "w".repeat(32_768);
    fresh.merge_snapshot(&wide, 1);
    assert_eq!(fresh.raw(), wide);
    assert!(fresh.trimmed_front());

    let mut fenced = ProcessOutput::default();
    fenced.push_chunk("aaaa", 1);
    fenced.merge_snapshot("aaaa", 2);
    fenced.merge_snapshot("aaaabbbb", 3);
    assert_eq!(fenced.raw(), "aaaabbbb");

    let mut slid = ProcessOutput::default();
    slid.push_chunk("held", 1);
    slid.merge_snapshot("other", 2);
    assert_eq!(slid.raw(), "other");
    assert!(!slid.trimmed_front());
    let rolled = "r".repeat(32_768);
    slid.merge_snapshot(&rolled, 3);
    assert_eq!(slid.raw(), rolled);
    assert!(slid.trimmed_front());
}

#[test]
fn chunks_for_an_unknown_id_wait() {
    let mut work = BackgroundWork::default();
    let _ = foreground(&mut work);
    let mut lists = 0;
    for index in 0..MAX_PENDING_UNKNOWN + 1 {
        let effects = update(
            &mut work,
            BackgroundEvent::TerminalOutput {
                process_id: format!("unk-{index}"),
                chunk: format!("c{index}\n"),
                now_ms: index as u64 * UNKNOWN_CHUNK_LIST_MS,
            },
        );
        lists += effects
            .iter()
            .filter(|effect| matches!(effect, BackgroundEffect::ListProcesses { .. }))
            .count();
    }
    assert!(lists >= 1);
    let request = refresh(&mut work, 20_000);
    let rows: Vec<_> = (1..=MAX_PENDING_UNKNOWN)
        .map(|index| row(&format!("unk-{index}"), "running", Some(1.0)))
        .collect();
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request,
            processes: rows,
            now_ms: 20_001,
        },
    );
    assert!(work.output(&pid("unk-0")).is_none());
    let kept = work
        .output(&pid("unk-1"))
        .expect("pending buffer moved")
        .raw();
    assert!(kept.contains("c1"));
}

#[test]
fn output_memory_drops_the_least_recent() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work);
    let rows: Vec<_> = (0..MAX_TRACKED_OUTPUTS + 1)
        .map(|index| row(&format!("p{index}"), "running", Some(index as f64)))
        .collect();
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request,
            processes: rows,
            now_ms: 1,
        },
    );
    let _ = update(&mut work, BackgroundEvent::Selected { id: pid("p0") });
    for index in 0..=MAX_TRACKED_OUTPUTS {
        let _ = update(
            &mut work,
            BackgroundEvent::TerminalOutput {
                process_id: format!("p{index}"),
                chunk: "x".repeat(OUTPUT_TOTAL_CHARS / 8),
                now_ms: 10 + index as u64,
            },
        );
    }
    assert!(work.rows().any(|row| row.id.0 == "p1"));
    assert!(
        work.output(&pid("p0")).is_some(),
        "the selected buffer stays"
    );
    let tracked: Vec<_> = work.rows().map(|row| row.id.clone()).collect();
    let kept = tracked
        .iter()
        .filter(|id| work.output(id).is_some())
        .count();
    assert_eq!(kept, MAX_TRACKED_OUTPUTS);
}

#[test]
fn closing_the_view_does_not_stop_the_process() {
    let mut work = BackgroundWork::default();
    let _ = update(
        &mut work,
        BackgroundEvent::TerminalClosed {
            process_id: "later".to_string(),
        },
    );
    let request = foreground(&mut work);
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request,
            processes: vec![
                row("later", "running", Some(1.0)),
                row("now", "running", Some(2.0)),
            ],
            now_ms: 5,
        },
    );
    let _ = update(
        &mut work,
        BackgroundEvent::TerminalClosed {
            process_id: "now".to_string(),
        },
    );
    let later = work.rows().find(|row| row.id.0 == "later").unwrap();
    let now = work.rows().find(|row| row.id.0 == "now").unwrap();
    assert!(later.agent_closed_view);
    assert!(now.agent_closed_view);
    assert!(matches!(later.status, ProcessStatus::Running));
    assert!(matches!(now.kill, KillState::Idle));
    let effects = update(
        &mut work,
        BackgroundEvent::Clock {
            now_ms: 5 + talaria_core::background::PROCESS_POLL_MS,
            wall_s: 10.0,
        },
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, BackgroundEffect::ListProcesses { .. }))
    );
}

#[test]
fn the_live_line_changes_on_the_clock_only() {
    let mut work = BackgroundWork::default();
    let request = foreground(&mut work);
    let _ = update(
        &mut work,
        BackgroundEvent::Listed {
            request,
            processes: vec![row("live", "running", Some(1.0))],
            now_ms: 1,
        },
    );
    let version = work.version();
    let _ = update(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "live".to_string(),
            chunk: "alpha\nbeta\n".to_string(),
            now_ms: 2,
        },
    );
    assert_eq!(work.version(), version);
    assert_eq!(work.rows().next().unwrap().live_line, "");
    let _ = update(
        &mut work,
        BackgroundEvent::Clock {
            now_ms: 1_000,
            wall_s: 2.0,
        },
    );
    assert_eq!(work.rows().next().unwrap().live_line, "beta");
    let _ = update(&mut work, BackgroundEvent::OverlayOpened { now_ms: 1_100 });
    let _ = update(&mut work, BackgroundEvent::Selected { id: pid("live") });
    let version = work.version();
    let _ = update(
        &mut work,
        BackgroundEvent::TerminalOutput {
            process_id: "live".to_string(),
            chunk: "gamma\n".to_string(),
            now_ms: 1_200,
        },
    );
    assert!(work.version() > version);
    let output = work.output(&pid("live")).unwrap();
    assert_eq!(output.last_line(), "gamma");
    assert!(!output.visible_tail(20).contains('\u{1b}'));
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..proptest::prelude::ProptestConfig::default()
    })]

    #[test]
    fn snapshot_merge_matches_the_concatenated_tail(
        bodies in proptest::collection::vec(chunk_body(), 1..=400),
        pick in proptest::prelude::any::<u64>(),
    ) {
        let chunks: Vec<String> = bodies
            .into_iter()
            .enumerate()
            .map(|(index, body)| format!("\u{1}{index}\u{2}{body}"))
            .collect();
        let n = chunks.len();
        let k = (pick as usize) % (n + 1);
        let full: String = chunks.concat();
        let count = full.chars().count();
        proptest::prelude::prop_assume!(count != 32_768);
        proptest::prelude::prop_assume!(count < 80_000);
        let snapshot: String = {
            let chars: Vec<char> = full.chars().collect();
            if chars.len() <= 32_768 {
                full.clone()
            } else {
                chars[chars.len() - 32_768..].iter().collect()
            }
        };
        let mut output = ProcessOutput::default();
        for chunk in chunks.iter().take(k) {
            output.push_chunk(chunk, 1);
        }
        output.merge_snapshot(&snapshot, 2);
        if count < 32_768 || k == n {
            proptest::prop_assert_eq!(output.raw(), full.as_str());
            proptest::prop_assert_eq!(output.trimmed_front(), false);
        } else {
            proptest::prop_assert_eq!(output.raw(), snapshot.as_str());
            proptest::prop_assert_eq!(output.trimmed_front(), true);
        }
    }

    #[test]
    fn a_second_list_with_a_fresh_request_is_quiet(
        first in proptest::collection::vec(generated_entry(), 0..=10),
        mut second in proptest::collection::vec(generated_entry(), 0..=20),
    ) {
        for entry in &first {
            if entry.running && second.iter().all(|other| other.id != entry.id) {
                second.push(entry.clone());
            }
        }
        let mut work = BackgroundWork::default();
        let request = foreground(&mut work);
        let _ = update(
            &mut work,
            BackgroundEvent::Listed {
                request,
                processes: first.iter().map(Generated::to_listed).collect(),
                now_ms: 1,
            },
        );
        let request = refresh(&mut work, 2);
        let _ = update(
            &mut work,
            BackgroundEvent::Listed {
                request,
                processes: second.iter().map(Generated::to_listed).collect(),
                now_ms: 3,
            },
        );
        let rows: Vec<_> = work.rows().cloned().collect();
        let version = work.version();
        let request = refresh(&mut work, 4);
        let effects = update(
            &mut work,
            BackgroundEvent::Listed {
                request,
                processes: second.iter().map(Generated::to_listed).collect(),
                now_ms: 5,
            },
        );
        proptest::prop_assert!(effects.is_empty());
        proptest::prop_assert_eq!(work.version(), version);
        proptest::prop_assert_eq!(work.rows().cloned().collect::<Vec<_>>(), rows);
    }
}

fn chunk_body() -> impl proptest::prelude::Strategy<Value = String> {
    use proptest::prelude::*;
    proptest::collection::vec(
        prop_oneof![
            Just('\n'),
            proptest::char::range('a', 'z'),
            proptest::char::range('A', 'Z'),
            Just('é'),
            Just('你'),
            Just('あ'),
            Just('Ω'),
        ],
        0..=200,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

#[derive(Clone, Debug)]
struct Generated {
    id: u8,
    running: bool,
    tail: String,
}

impl Generated {
    fn to_listed(&self) -> ListedProcess {
        ListedProcess {
            id: pid(&format!("id-{}", self.id)),
            command: format!("cmd {}", self.id),
            status_raw: if self.running { "running" } else { "exited" }.to_string(),
            exit_code: if self.running { None } else { Some(0) },
            started_epoch_s: Some(f64::from(self.id)),
            exited_at_s: if self.running { None } else { Some(100.0) },
            completion_reason: None,
            output_tail: Some(self.tail.clone()),
        }
    }
}

fn generated_entry() -> impl proptest::prelude::Strategy<Value = Generated> {
    use proptest::prelude::*;
    (
        0u8..30,
        proptest::bool::ANY,
        proptest::string::string_regex("[a-z\n]{0,12}").expect("pattern"),
    )
        .prop_map(|(id, running, tail)| Generated { id, running, tail })
}
