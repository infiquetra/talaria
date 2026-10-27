#[path = "cron_support/mod.rs"]
mod support;

use std::time::Duration;
use talaria_core::cron::{
    CallFailure, CronBody, CronEffect, CronInput, CronKey, CronOpen, CronState, JobReply,
    ListReply, RemoveReply, job_state_label, last_run_label, update, view,
};

const LIST: &str = include_str!("../../../fixtures/socket/cron-manage-list.json");

fn listed(seq: u64, jobs: Vec<talaria_core::cron::JobRow>, warning: Option<String>) -> CronInput {
    CronInput::Listed {
        seq,
        result: Ok(ListReply { jobs, warning }),
    }
}

fn open_with_rows() -> (CronState, Vec<talaria_core::cron::JobRow>) {
    let (jobs, warning) = support::jobs_from_fixture(LIST);
    let mut state = CronState::new("fixture".into(), Ok(()));
    let start = support::clock();
    update(
        &mut state,
        CronInput::Open(CronOpen::List),
        start,
        support::WALL,
    );
    update(
        &mut state,
        listed(1, jobs.clone(), warning),
        start,
        support::WALL,
    );
    (state, jobs)
}

#[test]
fn rows_from_v0224_list_reply() {
    let (jobs, warning) = support::jobs_from_fixture(LIST);
    assert_eq!(jobs.len(), 7);
    assert!(warning.is_some());
    assert_eq!(
        jobs[4]
            .last_fire_error
            .as_ref()
            .map(|fire| fire.detail.as_str()),
        Some("forward to gateway failed")
    );
    let (state, _) = open_with_rows();
    let shown = view(&state, support::WALL);
    assert_eq!(shown.title, "Scheduled jobs · profile fixture");
    assert_eq!(
        shown.banner.as_deref(),
        Some("The scheduler is not running.")
    );
    let CronBody::List(list) = shown.body else {
        panic!("expected the list, got {:?}", shown.body);
    };
    let ids: Vec<_> = list.rows.iter().map(|row| row.job_id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "a1b2c3d4e5f6",
            "b2c3d4e5f6a1",
            "0f9e8d7c6b5a",
            "c3d4e5f6a1b2",
            "d4e5f6a1b2c3",
            "f6a1b2c3d4e5",
            "e5f6a1b2c3d4",
        ]
    );
    assert_eq!(list.rows[0].state, "next in 3h 12m");
    assert_eq!(list.rows[1].state, "scheduled");
    assert_eq!(list.rows[2].state, "paused");
    assert_eq!(list.rows[2].last_run, "failed 1d ago");
    assert_eq!(list.rows[3].state, "finished");
    assert_eq!(list.rows[3].last_run, "ok 1h ago");
    assert_eq!(list.rows[4].state, "schedule error");
    assert_eq!(list.rows[5].state, "blocked");
    assert_eq!(list.rows[6].state, "held");
    let _ = state;
}

#[test]
fn state_label_precedence() {
    let wall = support::WALL;
    let mut running = support::job("run", "run", "scheduled");
    running.next_run_at = Some("2026-10-23T12:12:00Z".into());
    let mut outstanding = running.clone();
    let _ = &mut outstanding;
    let paused = support::job("p", "p", "paused");
    let done = support::job("d", "d", "completed");
    let broken = support::job("e", "e", "error");
    let held = support::job("h", "h", "held");
    assert_eq!(job_state_label(&paused, wall), "paused");
    assert_eq!(job_state_label(&done, wall), "finished");
    assert_eq!(job_state_label(&broken, wall), "schedule error");
    assert_eq!(job_state_label(&held, wall), "held");
    let mut soon = support::job("s", "s", "scheduled");
    soon.next_run_at = Some("2026-10-23T09:00:59Z".into());
    let mut due = support::job("d60", "d60", "scheduled");
    due.next_run_at = Some("2026-10-23T09:01:00Z".into());
    let mut minute = support::job("m", "m", "scheduled");
    minute.next_run_at = Some("2026-10-23T09:01:01Z".into());
    assert_eq!(job_state_label(&soon, wall), "due now");
    assert_eq!(job_state_label(&due, wall), "due now");
    assert_eq!(job_state_label(&minute, wall), "next in 1m");
    let mut blank = support::job("b", "b", "scheduled");
    blank.next_run_at = None;
    assert_eq!(job_state_label(&blank, wall), "scheduled");
    assert_eq!(last_run_label(&paused, wall), "");
}

#[test]
fn open_lists_once_and_reopen_keeps_state() {
    let mut state = CronState::new("fixture".into(), Ok(()));
    let start = support::clock();
    let effects = update(
        &mut state,
        CronInput::Open(CronOpen::List),
        start,
        support::WALL,
    );
    assert!(matches!(
        effects.as_slice(),
        [CronEffect::List { seq: 1 }, CronEffect::ScheduleTick { .. }]
    ));
    let shown = view(&state, support::WALL);
    assert!(matches!(
        shown.body,
        CronBody::Message(ref text) if text == "Loading scheduled jobs…"
    ));
    let (jobs, warning) = support::jobs_from_fixture(LIST);
    update(&mut state, listed(1, jobs, warning), start, support::WALL);
    update(&mut state, CronInput::Close, start, support::WALL);
    let again = update(
        &mut state,
        CronInput::Open(CronOpen::List),
        start,
        support::WALL,
    );
    assert_eq!(
        again
            .iter()
            .filter(|effect| matches!(effect, CronEffect::List { .. }))
            .count(),
        1
    );
    let shown = view(&state, support::WALL);
    let CronBody::List(list) = shown.body else {
        panic!("reopen should show the kept rows");
    };
    assert!(list.filter.is_none());
    assert_eq!(list.rows.len(), 7);
}

#[test]
fn changed_is_debounced_and_coalesced() {
    let mut state = CronState::new("fixture".into(), Ok(()));
    let start = support::clock();
    update(
        &mut state,
        CronInput::Open(CronOpen::List),
        start,
        support::WALL,
    );
    update(
        &mut state,
        listed(1, Vec::new(), None),
        start,
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Changed,
        start + Duration::from_millis(10),
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Changed,
        start + Duration::from_millis(20),
        support::WALL,
    );
    let third = update(
        &mut state,
        CronInput::Changed,
        start + Duration::from_millis(30),
        support::WALL,
    );
    assert!(
        third
            .iter()
            .all(|effect| !matches!(effect, CronEffect::List { .. }))
    );
    let fired = update(
        &mut state,
        CronInput::Tick,
        start + Duration::from_millis(310),
        support::WALL,
    );
    assert_eq!(
        fired
            .iter()
            .filter(|effect| matches!(effect, CronEffect::List { .. }))
            .count(),
        1
    );
    let queued = update(
        &mut state,
        CronInput::Changed,
        start + Duration::from_millis(320),
        support::WALL,
    );
    assert!(
        queued
            .iter()
            .all(|effect| !matches!(effect, CronEffect::List { .. }))
    );
    update(
        &mut state,
        CronInput::Tick,
        start + Duration::from_millis(620),
        support::WALL,
    );
    let after = update(
        &mut state,
        listed(2, Vec::new(), None),
        start + Duration::from_millis(620),
        support::WALL,
    );
    assert_eq!(
        after
            .iter()
            .filter(|effect| matches!(effect, CronEffect::List { .. }))
            .count(),
        1
    );
    update(&mut state, CronInput::Close, start, support::WALL);
    let closed = update(&mut state, CronInput::Changed, start, support::WALL);
    assert!(closed.is_empty());
}

#[test]
fn backstop_and_minute_tick_only() {
    let mut state = CronState::new("fixture".into(), Ok(()));
    let start = support::clock();
    // 09:00:30Z, so the next whole minute is 30 seconds away.
    let wall = support::WALL + 30;
    let mut row = support::job("a1b2c3d4e5f6", "daily digest", "scheduled");
    row.next_run_at = Some("2026-10-23T09:02:00Z".into());
    let opened = update(&mut state, CronInput::Open(CronOpen::List), start, wall);
    assert!(
        opened
            .iter()
            .any(|effect| matches!(effect, CronEffect::List { seq: 1 }))
    );
    assert!(opened.iter().any(|effect| matches!(
        effect,
        CronEffect::ScheduleTick { after } if *after == Duration::from_secs(30)
    )));
    update(&mut state, listed(1, vec![row], None), start, wall);
    assert_eq!(row_state(&state, wall), "next in 1m");
    let mut lists = Vec::new();
    let mut seq = 1_u64;
    for step in 1..=600 {
        let now = start + Duration::from_secs(step);
        let effects = update(&mut state, CronInput::Tick, now, wall + step as i64);
        for effect in &effects {
            match effect {
                CronEffect::List { .. } => lists.push(step),
                CronEffect::ScheduleTick { after } => {
                    assert_eq!(*after, Duration::from_secs(30), "tick {step}");
                }
                other => panic!("idle emitted {other:?} at {step}"),
            }
        }
        if step % 60 == 0 {
            seq += 1;
            update(
                &mut state,
                listed(seq, vec![row_at()], None),
                now,
                wall + step as i64,
            );
        }
        if step == 30 {
            assert_eq!(row_state(&state, wall + step as i64), "due now");
            assert!(
                !effects
                    .iter()
                    .any(|effect| matches!(effect, CronEffect::List { .. }))
            );
        }
    }
    assert_eq!(lists, (1..=10).map(|n| n * 60).collect::<Vec<_>>());
    let failed_request = update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Reload)),
        start + Duration::from_secs(600),
        wall + 600,
    );
    update(
        &mut state,
        CronInput::Listed {
            seq: support::list_seq(&failed_request),
            result: Err(CallFailure::NotSent("down".into())),
        },
        start + Duration::from_secs(600),
        wall + 600,
    );
    let rearmed = update(
        &mut state,
        CronInput::Tick,
        start + Duration::from_secs(660),
        wall + 660,
    );
    assert!(
        rearmed
            .iter()
            .any(|effect| matches!(effect, CronEffect::List { .. }))
    );
}

fn row_at() -> talaria_core::cron::JobRow {
    let mut row = support::job("a1b2c3d4e5f6", "daily digest", "scheduled");
    row.next_run_at = Some("2026-10-23T09:02:00Z".into());
    row
}

fn row_state(state: &CronState, wall: i64) -> String {
    let CronBody::List(list) = view(state, wall).body else {
        panic!("expected the list");
    };
    list.rows[0].state.clone()
}

#[test]
fn stale_list_reply_is_ignored() {
    let (mut state, jobs) = open_with_rows();
    let row = jobs.iter().find(|job| job.state == "scheduled").unwrap();
    let start = support::clock();
    let stale_request = update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Reload)),
        start,
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::TogglePause)),
        start,
        support::WALL,
    );
    let mut paused = row.clone();
    paused.state = "paused".into();
    paused.enabled = false;
    update(
        &mut state,
        CronInput::PauseSet {
            job_id: row.job_id.clone(),
            result: Ok(JobReply { job: paused }),
        },
        start,
        support::WALL,
    );
    let fresh_request = update(
        &mut state,
        listed(support::list_seq(&stale_request), jobs.clone(), None),
        start,
        support::WALL,
    );
    let shown = view(&state, support::WALL);
    let CronBody::List(list) = shown.body else {
        panic!("list");
    };
    let kept = list
        .rows
        .iter()
        .find(|item| item.job_id == row.job_id)
        .unwrap();
    assert_eq!(kept.state, "paused");
    let mut fresh = jobs.clone();
    if let Some(item) = fresh.iter_mut().find(|item| item.job_id == row.job_id) {
        item.state = "paused".into();
    }
    update(
        &mut state,
        listed(support::list_seq(&fresh_request), fresh, None),
        start,
        support::WALL,
    );
    let shown = view(&state, support::WALL);
    let CronBody::List(list) = shown.body else {
        panic!("list");
    };
    assert!(
        list.rows
            .iter()
            .any(|item| item.job_id == row.job_id && item.state == "paused")
    );
}

#[test]
fn filter_and_empty_states() {
    let (mut state, _) = open_with_rows();
    let start = support::clock();
    update(
        &mut state,
        CronInput::Key(CronKey::Char('z')),
        start,
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Key(CronKey::Char('z')),
        start,
        support::WALL,
    );
    let shown = view(&state, support::WALL);
    let CronBody::List(list) = shown.body else {
        panic!("list");
    };
    assert_eq!(list.status.as_deref(), Some("No job matches \"zz\"."));
    let empty_request = update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Reload)),
        start,
        support::WALL,
    );
    update(
        &mut state,
        listed(support::list_seq(&empty_request), Vec::new(), None),
        start,
        support::WALL,
    );
    let shown = view(&state, support::WALL);
    assert!(matches!(
        shown.body,
        CronBody::Message(ref text) if text == "No scheduled jobs in profile fixture. ctrl+n creates one."
    ));
    let mut down = CronState::new("fixture".into(), Ok(()));
    update(
        &mut down,
        CronInput::Connection { up: false },
        start,
        support::WALL,
    );
    let shown = view(&down, support::WALL);
    assert!(matches!(
        shown.body,
        CronBody::Message(ref text) if text == "Not connected to Hermes. Scheduled jobs need the gateway."
    ));
    let mut failed = CronState::new("fixture".into(), Ok(()));
    update(
        &mut failed,
        CronInput::Open(CronOpen::List),
        start,
        support::WALL,
    );
    update(
        &mut failed,
        CronInput::Listed {
            seq: 1,
            result: Err(CallFailure::NotSent("gateway down".into())),
        },
        start,
        support::WALL,
    );
    let shown = view(&failed, support::WALL);
    let text = match shown.body {
        CronBody::Message(text) => text,
        CronBody::List(list) => list.status.unwrap_or_default(),
        other => panic!("failed list {other:?}"),
    };
    assert!(text.contains("Could not load scheduled jobs:"));
    assert!(text.contains("ctrl+r tries again."));
}

#[test]
fn pause_resume_effects_and_texts() {
    let (mut state, jobs) = open_with_rows();
    let start = support::clock();
    let scheduled = jobs
        .iter()
        .find(|job| job.job_id == "a1b2c3d4e5f6")
        .unwrap();
    let effects = update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::TogglePause)),
        start,
        support::WALL,
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        CronEffect::SetPaused { paused: true, job_id } if job_id == &scheduled.job_id
    )));
    let paused_state = state.clone_via_reopen();
    let _ = paused_state;
    let mut paused = CronState::new("fixture".into(), Ok(()));
    let mut row = support::job("0f9e8d7c6b5a", "nightly check", "paused");
    update(
        &mut paused,
        CronInput::Open(CronOpen::List),
        start,
        support::WALL,
    );
    update(
        &mut paused,
        listed(1, vec![row.clone()], None),
        start,
        support::WALL,
    );
    let effects = update(
        &mut paused,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::TogglePause)),
        start,
        support::WALL,
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, CronEffect::SetPaused { paused: false, .. }))
    );
    row.state = "scheduled".into();
    row.next_run_at = Some("2026-10-23T08:00:00Z".into());
    update(
        &mut paused,
        CronInput::PauseSet {
            job_id: row.job_id.clone(),
            result: Ok(JobReply { job: row }),
        },
        start,
        support::WALL,
    );
    let shown = view(&paused, support::WALL);
    assert_eq!(
        shown.notice.as_deref(),
        Some("Resumed nightly check. Next run: due now")
    );
    let mut done = CronState::new("fixture".into(), Ok(()));
    update(
        &mut done,
        CronInput::Open(CronOpen::List),
        start,
        support::WALL,
    );
    update(
        &mut done,
        listed(
            1,
            vec![support::job("c3", "yearly once", "completed")],
            None,
        ),
        start,
        support::WALL,
    );
    let effects = update(
        &mut done,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::TogglePause)),
        start,
        support::WALL,
    );
    assert!(effects.is_empty());
    let shown = view(&done, support::WALL);
    assert_eq!(
        shown.notice.as_deref(),
        Some("yearly once has finished; it cannot be paused or resumed.")
    );
}

#[test]
fn remove_confirms_first() {
    let (mut state, jobs) = open_with_rows();
    let start = support::clock();
    let id = jobs[0].job_id.clone();
    let name = jobs[0].name.clone();
    update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Remove)),
        start,
        support::WALL,
    );
    let declined = update(
        &mut state,
        CronInput::Key(CronKey::Char('n')),
        start,
        support::WALL,
    );
    assert!(declined.is_empty());
    update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Remove)),
        start,
        support::WALL,
    );
    let accepted = update(
        &mut state,
        CronInput::Key(CronKey::Char('y')),
        start,
        support::WALL,
    );
    assert!(accepted.iter().any(|effect| matches!(
        effect,
        CronEffect::Remove { job_id } if job_id == &id
    )));
    update(
        &mut state,
        CronInput::Removed {
            job_id: id.clone(),
            result: Ok(RemoveReply {
                job_id: id,
                name: name.clone(),
                message: format!("Cron job '{name}' removed."),
            }),
        },
        start,
        support::WALL,
    );
    let shown = view(&state, support::WALL);
    assert_eq!(shown.notice.as_deref(), Some("Removed daily digest."));
}

#[test]
fn one_write_at_a_time() {
    let (mut state, _) = open_with_rows();
    let start = support::clock();
    update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::TogglePause)),
        start,
        support::WALL,
    );
    let blocked = update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Remove)),
        start,
        support::WALL,
    );
    assert!(
        blocked.is_empty()
            || blocked
                .iter()
                .all(|effect| !matches!(effect, CronEffect::Remove { .. }))
    );
    let shown = view(&state, support::WALL);
    assert_eq!(shown.notice.as_deref(), Some("Waiting for Hermes."));
    update(
        &mut state,
        CronInput::Key(CronKey::Down),
        start,
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::RunNow)),
        start,
        support::WALL,
    );
    let triggered = update(
        &mut state,
        CronInput::Key(CronKey::Char('y')),
        start,
        support::WALL,
    );
    assert!(
        triggered
            .iter()
            .any(|effect| matches!(effect, CronEffect::Trigger { .. }))
    );
}

#[test]
fn tool_failures_are_failures() {
    let cases = [
        (
            CallFailure::Tool {
                error: "two jobs".into(),
                job_saved: false,
                job_id: None,
                matches: 2,
            },
            "More than one job matches; reload with ctrl+r and try again.",
        ),
        (
            CallFailure::Tool {
                error: "no".into(),
                job_saved: false,
                job_id: None,
                matches: 0,
            },
            "Hermes refused: no.",
        ),
        (
            CallFailure::Rpc {
                code: 4000,
                message: "bad".into(),
            },
            "Hermes and Talaria disagree about cron.manage (code 4000): bad. Talaria may be newer or older than this Hermes.",
        ),
        (
            CallFailure::Rpc {
                code: 4016,
                message: "nope".into(),
            },
            "This Hermes does not know that cron action (code 4016).",
        ),
        (
            CallFailure::Rpc {
                code: 4064,
                message: "missing".into(),
            },
            "Hermes cannot find profile fixture (code 4064).",
        ),
        (
            CallFailure::Rpc {
                code: 5023,
                message: "boom".into(),
            },
            "Hermes failed: boom.",
        ),
        (
            CallFailure::Rpc {
                code: -32601,
                message: "unknown method: cron.manage".into(),
            },
            "This Hermes does not offer cron.manage on Talaria's connection.",
        ),
        (
            CallFailure::Rpc {
                code: 7,
                message: "odd".into(),
            },
            "Hermes error 7: odd.",
        ),
        (
            CallFailure::BadResult("shape".into()),
            "Hermes's reply did not decode: shape.",
        ),
        (
            CallFailure::NotSent("down".into()),
            "Not connected to Hermes. Scheduled jobs need the gateway.",
        ),
    ];
    for (failure, text) in cases {
        let (mut state, jobs) = open_with_rows();
        let before = view(&state, support::WALL);
        let start = support::clock();
        update(
            &mut state,
            CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::TogglePause)),
            start,
            support::WALL,
        );
        update(
            &mut state,
            CronInput::PauseSet {
                job_id: jobs[0].job_id.clone(),
                result: Err(failure),
            },
            start,
            support::WALL,
        );
        let shown = view(&state, support::WALL);
        assert_eq!(shown.notice.as_deref(), Some(text));
        assert_eq!(shown.body, before.body);
        let _ = jobs;
    }
}

#[test]
fn unknown_write_outcome_is_never_resent() {
    let mut state = CronState::new("fixture".into(), Ok(()));
    let start = support::clock();
    update(
        &mut state,
        CronInput::Open(CronOpen::NewJob),
        start,
        support::WALL,
    );
    update(
        &mut state,
        listed(1, Vec::new(), None),
        start,
        support::WALL,
    );
    for key in [
        CronKey::Tab,
        CronKey::Paste("every 2h".into()),
        CronKey::Tab,
        CronKey::Paste("Reply ok.".into()),
    ] {
        update(&mut state, CronInput::Key(key), start, support::WALL);
    }
    update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Save)),
        start,
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Connection { up: false },
        start,
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Added(Err(CallFailure::Unknown)),
        start,
        support::WALL,
    );
    let shown = view(&state, support::WALL);
    assert_eq!(
        shown.notice.as_deref(),
        Some(
            "Talaria lost the reply; the change may or may not have happened. The list reloads when the connection returns."
        )
    );
    update(
        &mut state,
        CronInput::Connection { up: false },
        start,
        support::WALL,
    );
    let effects = update(
        &mut state,
        CronInput::Connection { up: true },
        start,
        support::WALL,
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, CronEffect::List { .. }))
    );
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, CronEffect::Add(_)))
    );
}

trait Reopen {
    fn clone_via_reopen(&self) -> CronState;
}
impl Reopen for CronState {
    fn clone_via_reopen(&self) -> CronState {
        let _ = self;
        CronState::new("fixture".into(), Ok(()))
    }
}
