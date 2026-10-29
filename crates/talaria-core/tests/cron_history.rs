#[path = "cron_support/mod.rs"]
mod support;

use serde_json::Value;
use talaria_core::cron::{
    CardFocus, CronBody, CronEffect, CronInput, CronKey, CronOpen, CronState, JobRow, ListReply,
    RestFailure, RunRow, RunStatus, RunsView, duration_text, rest_reason, run_status,
    run_status_text, update, view,
};

const RUNS: &str = include_str!("../../../fixtures/rest/cron-runs.json");

fn run_from_value(value: &Value) -> RunRow {
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    let number = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_u64)
            .or_else(|| value.get(key).and_then(Value::as_f64).map(|n| n as u64))
            .unwrap_or(0)
    };
    let epoch = |key: &str| value.get(key).and_then(Value::as_f64);
    RunRow {
        id: text("id").unwrap_or_default(),
        source: text("source").unwrap_or_default(),
        title: text("title"),
        preview: text("preview"),
        started_at: epoch("started_at"),
        ended_at: epoch("ended_at"),
        end_reason: text("end_reason"),
        input_tokens: number("input_tokens"),
        output_tokens: number("output_tokens"),
        message_count: number("message_count"),
        tool_call_count: number("tool_call_count"),
        model: text("model"),
        is_active: value
            .get("is_active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        scheduler_owned: value
            .get("scheduler_owned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn fixture_runs() -> Vec<RunRow> {
    let value: Value = serde_json::from_str(RUNS).expect("runs fixture");
    value["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .map(run_from_value)
        .collect()
}

fn open_card(jobs: Vec<JobRow>) -> CronState {
    let mut state = CronState::new("fixture".into(), Ok(()));
    let clock = support::clock();
    update(
        &mut state,
        CronInput::Open(CronOpen::List),
        clock,
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Listed {
            seq: 1,
            result: Ok(ListReply {
                jobs,
                warning: None,
            }),
        },
        clock,
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Key(CronKey::Enter),
        clock,
        support::WALL,
    );
    state
}

#[test]
fn open_run_details_respond_to_scrolling() {
    let job = support::job("a1b2c3d4e5f6", "daily digest", "scheduled");
    let mut state = open_card(vec![job.clone()]);
    let mut runs = fixture_runs();
    runs[1].preview = Some(format!("{}\nlast preview line", "界".repeat(100)));
    update(
        &mut state,
        CronInput::RunsLoaded {
            job_id: job.job_id,
            seq: 2,
            result: Ok(runs),
        },
        support::clock(),
        support::WALL,
    );
    for key in [CronKey::Tab, CronKey::Down, CronKey::Enter] {
        update(
            &mut state,
            CronInput::Key(key),
            support::clock(),
            support::WALL,
        );
    }
    let before = view(&state, support::WALL);
    assert!(matches!(&before.body, CronBody::Detail(card) if card.run_card.is_some()));
    let CronBody::Detail(card) = &before.body else {
        panic!("detail")
    };
    let preview = &card
        .run_card
        .as_ref()
        .unwrap()
        .iter()
        .find(|(name, _)| name == "Preview")
        .unwrap()
        .1;
    assert_eq!(preview.last().unwrap(), "last preview line");
    assert_eq!(preview[..preview.len() - 1].concat(), "界".repeat(100));
    update(
        &mut state,
        CronInput::Key(CronKey::Down),
        support::clock(),
        support::WALL,
    );
    let current = view(&state, support::WALL);
    assert_ne!(current, before);
    let CronBody::Detail(card) = current.body else {
        panic!("detail")
    };
    assert_eq!(card.run_scroll, 1);
    update(
        &mut state,
        CronInput::Key(CronKey::PageDown),
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(card) = view(&state, support::WALL).body else {
        panic!("detail")
    };
    assert!(card.run_scroll > 1);
    update(
        &mut state,
        CronInput::Key(CronKey::PageUp),
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(card) = view(&state, support::WALL).body else {
        panic!("detail")
    };
    assert!(card.run_scroll < 2);
    update(
        &mut state,
        CronInput::Key(CronKey::End),
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(end) = view(&state, support::WALL).body else {
        panic!("detail")
    };
    assert!(end.run_scroll > 0);
    let effects = update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Reload)),
        support::clock(),
        support::WALL,
    );
    let seq = effects
        .iter()
        .find_map(|effect| match effect {
            CronEffect::Runs { seq, .. } => Some(*seq),
            _ => None,
        })
        .expect("refresh requests the open history");
    update(
        &mut state,
        CronInput::RunsLoaded {
            job_id: "a1b2c3d4e5f6".into(),
            seq,
            result: Ok(fixture_runs()),
        },
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(refreshed) = view(&state, support::WALL).body else {
        panic!("detail")
    };
    let count: usize = refreshed
        .run_card
        .as_ref()
        .unwrap()
        .iter()
        .map(|(_, lines)| lines.len().max(1))
        .sum();
    assert!(refreshed.run_scroll < count);
    update(
        &mut state,
        CronInput::Key(CronKey::Up),
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(after_up) = view(&state, support::WALL).body else {
        panic!("detail")
    };
    assert_eq!(after_up.run_scroll, count - 2);
    update(
        &mut state,
        CronInput::Key(CronKey::Home),
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(home) = view(&state, support::WALL).body else {
        panic!("detail")
    };
    assert_eq!(home.run_scroll, 0);
}

#[test]
fn run_rows_from_v0224_reply() {
    let runs = fixture_runs();
    assert_eq!(runs.len(), 6);
    let statuses = [
        (RunStatus::Running, "running", "", "5120/240 tokens"),
        (
            RunStatus::Finished(Some("completed".into())),
            "finished · completed",
            "3m 05s",
            "100/2 tokens",
        ),
        (RunStatus::Unclosed, "did not finish", "", ""),
        (
            RunStatus::Output(Some("failed".into())),
            "output · failed",
            "",
            "",
        ),
        (RunStatus::Output(None), "output", "", ""),
        (RunStatus::Other("other".into()), "other", "", ""),
    ];
    let first = run_status(&runs[0]);
    assert_eq!(first, statuses[0].0);
    for (row, (status, text, duration, _tokens)) in runs.iter().zip(statuses) {
        assert_eq!(run_status(row), status);
        assert_eq!(run_status_text(&run_status(row)), text);
        let shown = if let (Some(start), Some(end)) = (row.started_at, row.ended_at) {
            duration_text((end - start) as i64)
        } else {
            String::new()
        };
        if !duration.is_empty() {
            assert_eq!(shown, duration);
        }
    }
    let mut state = open_card(vec![support::job(
        "a1b2c3d4e5f6",
        "daily digest",
        "scheduled",
    )]);
    update(
        &mut state,
        CronInput::RunsLoaded {
            job_id: "a1b2c3d4e5f6".into(),
            seq: 2,
            result: Ok(runs),
        },
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(card) = view(&state, support::WALL).body else {
        panic!("detail");
    };
    let RunsView::Rows { rows, .. } = card.runs else {
        panic!("run rows");
    };
    assert_eq!(rows[0].status, "running");
    assert_eq!(rows[1].status, "finished · completed");
    assert_eq!(rows[1].duration, "3m 05s");
    let expected_tokens = ["5120/240 tokens", "100/2 tokens", "", "", "", ""];
    for (row, tokens) in rows.iter().zip(expected_tokens) {
        assert_eq!(row.tokens, tokens);
    }
    assert_eq!(rows[2].status, "did not finish");
    assert_eq!(rows[3].status, "output · failed");
    assert_eq!(rows[3].title, "FAILED · boom");
    assert_eq!(rows[4].status, "output");
    assert_eq!(rows[4].title, "Script-only run");
    assert_eq!(rows[5].status, "other");
}

#[test]
fn states_failures_and_open_run() {
    let reason = rest_reason(&RestFailure::Forbidden("profile_scope_mismatch".into()));
    assert_eq!(
        reason,
        "Hermes refused the request (403 profile_scope_mismatch)"
    );
    assert_eq!(
        rest_reason(&RestFailure::NotFound),
        "Hermes answered 404 (the job or the route is not there)"
    );
    assert_eq!(
        rest_reason(&RestFailure::Refused {
            status: 409,
            detail: "Job is already running or was claimed by another scheduler".into(),
        }),
        "Hermes refused the request (409 Job is already running or was claimed by another scheduler)"
    );
    assert_eq!(
        rest_reason(&RestFailure::Server(500)),
        "Hermes failed (500)"
    );
    assert_eq!(
        rest_reason(&RestFailure::Unauthorized),
        "Hermes refused the ticket"
    );
    assert_eq!(rest_reason(&RestFailure::Unknown), "the reply was lost");
    assert_eq!(
        rest_reason(&RestFailure::NotSent("gateway down".into())),
        "gateway down"
    );
    assert_eq!(
        rest_reason(&RestFailure::BadBody("missing runs".into())),
        "the reply did not decode"
    );

    let job = support::job("a1b2c3d4e5f6", "daily digest", "scheduled");
    let mut state = open_card(vec![job.clone()]);
    let CronBody::Detail(card) = view(&state, support::WALL).body else {
        panic!("card opens");
    };
    assert_eq!(card.focus, CardFocus::Fields);
    let RunsView::Loading = card.runs else {
        panic!("history starts loading");
    };
    update(
        &mut state,
        CronInput::Key(CronKey::Tab),
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(focused) = view(&state, support::WALL).body else {
        panic!("card");
    };
    assert_eq!(focused.focus, CardFocus::History);

    let mut empty = open_card(vec![job.clone()]);
    update(
        &mut empty,
        CronInput::RunsLoaded {
            job_id: job.job_id.clone(),
            seq: 2,
            result: Ok(Vec::new()),
        },
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(card) = view(&empty, support::WALL).body else {
        panic!("card");
    };
    assert!(matches!(card.runs, RunsView::Empty));

    let mut failed = open_card(vec![job.clone()]);
    update(
        &mut failed,
        CronInput::RunsLoaded {
            job_id: job.job_id.clone(),
            seq: 2,
            result: Err(RestFailure::NotSent("gateway down".into())),
        },
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(card) = view(&failed, support::WALL).body else {
        panic!("card");
    };
    let RunsView::Failed(text) = card.runs else {
        panic!("failed history");
    };
    assert!(text.contains("Could not load the run history:"));
    assert!(text.contains("gateway down"));
    assert!(text.contains("ctrl+r tries again."));

    let mut offline = CronState::new(
        "fixture".into(),
        Err("Hermes did not give a dashboard address (api_origin)".into()),
    );
    update(
        &mut offline,
        CronInput::Open(CronOpen::List),
        support::clock(),
        support::WALL,
    );
    update(
        &mut offline,
        CronInput::Listed {
            seq: 1,
            result: Ok(ListReply {
                jobs: vec![job.clone()],
                warning: None,
            }),
        },
        support::clock(),
        support::WALL,
    );
    update(
        &mut offline,
        CronInput::Key(CronKey::Enter),
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(card) = view(&offline, support::WALL).body else {
        panic!("card");
    };
    let RunsView::Unavailable(text) = card.runs else {
        panic!("unavailable");
    };
    assert!(text.contains("Run history needs Hermes's dashboard routes"));
    assert!(text.contains("Hermes did not give a dashboard address (api_origin)"));

    let runs = fixture_runs();
    let mut rows = open_card(vec![job.clone()]);
    update(
        &mut rows,
        CronInput::RunsLoaded {
            job_id: job.job_id.clone(),
            seq: 2,
            result: Ok(runs.clone()),
        },
        support::clock(),
        support::WALL,
    );
    let on_fields = update(
        &mut rows,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::OpenRun)),
        support::clock(),
        support::WALL,
    );
    assert!(on_fields.is_empty());
    update(
        &mut rows,
        CronInput::Key(CronKey::Tab),
        support::clock(),
        support::WALL,
    );
    for (index, run) in runs.iter().enumerate() {
        let mut one = open_card(vec![job.clone()]);
        update(
            &mut one,
            CronInput::RunsLoaded {
                job_id: job.job_id.clone(),
                seq: 2,
                result: Ok(runs.clone()),
            },
            support::clock(),
            support::WALL,
        );
        update(
            &mut one,
            CronInput::Key(CronKey::Tab),
            support::clock(),
            support::WALL,
        );
        for _ in 0..index {
            update(
                &mut one,
                CronInput::Key(CronKey::Down),
                support::clock(),
                support::WALL,
            );
        }
        let effects = update(
            &mut one,
            CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::OpenRun)),
            support::clock(),
            support::WALL,
        );
        let finished = matches!(run_status(run), RunStatus::Finished(_));
        if finished {
            assert!(effects.iter().any(|effect| matches!(
                effect,
                CronEffect::OpenSession { session_id } if session_id == &run.id
            )));
        } else {
            assert!(
                effects
                    .iter()
                    .all(|effect| !matches!(effect, CronEffect::OpenSession { .. }))
            );
            assert_eq!(
                view(&one, support::WALL).notice.as_deref(),
                Some("Only a finished agent run can be opened.")
            );
        }
    }
    update(
        &mut rows,
        CronInput::Key(CronKey::Enter),
        support::clock(),
        support::WALL,
    );
    let CronBody::Detail(card) = view(&rows, support::WALL).body else {
        panic!("card");
    };
    assert!(card.run_card.is_some());
    let reloaded = update(
        &mut rows,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Reload)),
        support::clock(),
        support::WALL,
    );
    assert!(
        reloaded
            .iter()
            .any(|effect| matches!(effect, CronEffect::List { .. }))
    );
    assert!(
        reloaded
            .iter()
            .any(|effect| matches!(effect, CronEffect::Runs { .. }))
    );
}
