#[path = "cron_support/mod.rs"]
mod support;

use talaria_core::cron::{
    CallFailure, CronBody, CronEffect, CronInput, CronKey, CronOpen, CronState, JobRow, ListReply,
    RestFailure, RunRow, RunsView, TriggerOutcome, update, view,
};

fn open_list(rest: Result<(), String>, jobs: Vec<JobRow>) -> CronState {
    let mut state = CronState::new("fixture".into(), rest);
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
    state
}

fn question(state: &CronState) -> String {
    view(state, support::WALL).question.unwrap_or_default()
}

fn run_now(state: &mut CronState) {
    update(
        state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::RunNow)),
        support::clock(),
        support::WALL,
    );
}

#[test]
fn confirms_and_warns() {
    let unpinned = support::job("a1b2c3d4e5f6", "daily digest", "scheduled");
    let mut pinned = support::job("b2c3d4e5f6a1", "pinned job", "scheduled");
    pinned.pinned = true;
    pinned.model = Some("m".into());
    pinned.provider = Some("p".into());
    let mut script = support::job("c3d4e5f6a1b2", "script job", "scheduled");
    script.no_agent = true;
    let mut telegram = support::job("d4e5f6a1b2c3", "telegram job", "scheduled");
    telegram.deliver = "telegram".into();
    let paused = support::job("0f9e8d7c6b5a", "nightly check", "paused");
    let mut state = open_list(
        Ok(()),
        vec![unpinned, pinned, script, telegram.clone(), paused.clone()],
    );
    run_now(&mut state);
    assert_eq!(
        question(&state),
        "Run daily digest now in profile fixture? It uses this profile's cron model (cron.model, else the main model); it delivers to local.  y run · n cancel"
    );
    update(
        &mut state,
        CronInput::Key(CronKey::Down),
        support::clock(),
        support::WALL,
    );
    run_now(&mut state);
    assert_eq!(
        question(&state),
        "Run pinned job now in profile fixture? It uses its pinned model m on p; it delivers to local.  y run · n cancel"
    );
    update(
        &mut state,
        CronInput::Key(CronKey::Down),
        support::clock(),
        support::WALL,
    );
    run_now(&mut state);
    assert!(question(&state).contains("It runs its script only, with no agent"));
    update(
        &mut state,
        CronInput::Key(CronKey::Down),
        support::clock(),
        support::WALL,
    );
    run_now(&mut state);
    assert_eq!(
        question(&state),
        "Run telegram job now in profile fixture? It uses this profile's cron model (cron.model, else the main model); it delivers to telegram. Run now sends from the dashboard without the gateway's live connections, so some platforms may not receive it.  y run · n cancel"
    );
    let declined = update(
        &mut state,
        CronInput::Key(CronKey::Char('n')),
        support::clock(),
        support::WALL,
    );
    assert!(declined.is_empty());
    assert!(view(&state, support::WALL).question.is_none());

    let mut paused_state = open_list(Ok(()), vec![paused]);
    run_now(&mut paused_state);
    assert_eq!(
        question(&paused_state),
        "nightly check is paused. Running it now also resumes its schedule (Hermes does this). Run and resume? It uses this profile's cron model (cron.model, else the main model); it delivers to local.  y run · n cancel"
    );
    let accepted = update(
        &mut paused_state,
        CronInput::Key(CronKey::Char('y')),
        support::clock(),
        support::WALL,
    );
    assert!(accepted.iter().any(|effect| matches!(
        effect,
        CronEffect::Trigger { job_id, name }
            if job_id == "0f9e8d7c6b5a" && name == "nightly check"
    )));

    let mut offline = open_list(
        Err("Hermes did not give a dashboard address (api_origin)".into()),
        vec![telegram],
    );
    let blocked = update(
        &mut offline,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::RunNow)),
        support::clock(),
        support::WALL,
    );
    assert!(blocked.is_empty());
    assert_eq!(
        view(&offline, support::WALL).notice.as_deref(),
        Some(
            "Run now needs Hermes's dashboard routes, which are not available here: Hermes did not give a dashboard address (api_origin)."
        )
    );
}

fn settle(state: &mut CronState, jobs: Vec<JobRow>) -> Vec<CronEffect> {
    update(
        state,
        CronInput::Listed {
            seq: 2,
            result: Ok(ListReply {
                jobs,
                warning: None,
            }),
        },
        support::clock(),
        support::WALL,
    )
}

#[test]
fn outcomes_map_to_text() {
    let mut base = support::job("a1b2c3d4e5f6", "daily digest", "scheduled");
    base.last_run_at = Some("2026-10-22T09:00:00Z".into());
    let mut state = open_list(Ok(()), vec![base.clone()]);
    run_now(&mut state);
    update(
        &mut state,
        CronInput::Key(CronKey::Char('y')),
        support::clock(),
        support::WALL,
    );
    let accepted = update(
        &mut state,
        CronInput::Triggered {
            job_id: base.job_id.clone(),
            outcome: TriggerOutcome::Accepted,
        },
        support::clock(),
        support::WALL,
    );
    assert!(
        accepted
            .iter()
            .any(|effect| matches!(effect, CronEffect::List { .. }))
    );

    let cases = [
        ("ok", None, None, "Ran daily digest: finished ok."),
        (
            "error",
            Some("provider returned 429"),
            None,
            "Ran daily digest: failed: provider returned 429.",
        ),
        (
            "delivery_failed",
            None,
            Some("platform down"),
            "Ran daily digest, but delivery failed: platform down.",
        ),
        (
            "blocked_config",
            None,
            None,
            "Ran daily digest: blocked config.",
        ),
    ];
    for (status, last_error, delivery_error, notice) in cases {
        let mut row = base.clone();
        row.last_run_at = Some("2026-10-23T09:05:00Z".into());
        row.last_status = Some(status.into());
        row.last_error = last_error.map(str::to_owned);
        row.last_delivery_error = delivery_error.map(str::to_owned);
        let mut fresh = open_list(Ok(()), vec![base.clone()]);
        run_now(&mut fresh);
        update(
            &mut fresh,
            CronInput::Key(CronKey::Char('y')),
            support::clock(),
            support::WALL,
        );
        update(
            &mut fresh,
            CronInput::Triggered {
                job_id: base.job_id.clone(),
                outcome: TriggerOutcome::Accepted,
            },
            support::clock(),
            support::WALL,
        );
        settle(&mut fresh, vec![row]);
        assert_eq!(view(&fresh, support::WALL).notice.as_deref(), Some(notice));
    }

    let mut unchanged = open_list(Ok(()), vec![base.clone()]);
    run_now(&mut unchanged);
    update(
        &mut unchanged,
        CronInput::Key(CronKey::Char('y')),
        support::clock(),
        support::WALL,
    );
    update(
        &mut unchanged,
        CronInput::Triggered {
            job_id: base.job_id.clone(),
            outcome: TriggerOutcome::Accepted,
        },
        support::clock(),
        support::WALL,
    );
    settle(&mut unchanged, vec![base.clone()]);
    assert_eq!(
        view(&unchanged, support::WALL).notice.as_deref(),
        Some(
            "Hermes accepted the run of daily digest but has not recorded a result yet. Its history shows the run."
        )
    );

    let mut gone = open_list(Ok(()), vec![base.clone()]);
    run_now(&mut gone);
    update(
        &mut gone,
        CronInput::Key(CronKey::Char('y')),
        support::clock(),
        support::WALL,
    );
    update(
        &mut gone,
        CronInput::Triggered {
            job_id: base.job_id.clone(),
            outcome: TriggerOutcome::Accepted,
        },
        support::clock(),
        support::WALL,
    );
    settle(&mut gone, Vec::new());
    assert_eq!(
        view(&gone, support::WALL).notice.as_deref(),
        Some(
            "Ran daily digest. Hermes no longer lists it (a one-shot job removes itself after its last run)."
        )
    );

    let failures = [
        (
            RestFailure::Refused {
                status: 409,
                detail: "Job is already running or was claimed by another scheduler".into(),
            },
            "daily digest did not run: Job is already running or was claimed by another scheduler.",
        ),
        (
            RestFailure::Forbidden("profile_scope_mismatch".into()),
            "Hermes did not run daily digest: Hermes refused the request (403 profile_scope_mismatch).",
        ),
        (
            RestFailure::Server(500),
            "Hermes did not run daily digest: Hermes failed (500).",
        ),
        (
            RestFailure::NotSent("gateway down".into()),
            "Could not ask Hermes to run daily digest: gateway down.",
        ),
    ];
    for (failure, notice) in failures {
        let mut fresh = open_list(Ok(()), vec![base.clone()]);
        run_now(&mut fresh);
        update(
            &mut fresh,
            CronInput::Key(CronKey::Char('y')),
            support::clock(),
            support::WALL,
        );
        update(
            &mut fresh,
            CronInput::Triggered {
                job_id: base.job_id.clone(),
                outcome: TriggerOutcome::Failed(failure),
            },
            support::clock(),
            support::WALL,
        );
        assert_eq!(view(&fresh, support::WALL).notice.as_deref(), Some(notice));
    }

    let mut lost = open_list(Ok(()), vec![base.clone()]);
    run_now(&mut lost);
    update(
        &mut lost,
        CronInput::Key(CronKey::Char('y')),
        support::clock(),
        support::WALL,
    );
    update(
        &mut lost,
        CronInput::Triggered {
            job_id: base.job_id.clone(),
            outcome: TriggerOutcome::Failed(RestFailure::Unknown),
        },
        support::clock(),
        support::WALL,
    );
    assert_eq!(
        view(&lost, support::WALL).notice.as_deref(),
        Some(
            "Talaria lost the reply for the run of daily digest; it may still be running. Its history will show the result."
        )
    );
    update(
        &mut lost,
        CronInput::Connection { up: false },
        support::clock(),
        support::WALL,
    );
    let restored = update(
        &mut lost,
        CronInput::Connection { up: true },
        support::clock(),
        support::WALL,
    );
    assert!(
        restored
            .iter()
            .any(|effect| matches!(effect, CronEffect::List { .. }))
    );

    let mut missing = open_list(Ok(()), vec![base.clone()]);
    run_now(&mut missing);
    update(
        &mut missing,
        CronInput::Key(CronKey::Char('y')),
        support::clock(),
        support::WALL,
    );
    update(
        &mut missing,
        CronInput::Triggered {
            job_id: base.job_id.clone(),
            outcome: TriggerOutcome::Failed(RestFailure::NotFound),
        },
        support::clock(),
        support::WALL,
    );
    settle(&mut missing, Vec::new());
    assert_eq!(
        view(&missing, support::WALL).notice.as_deref(),
        Some("daily digest is no longer in profile fixture.")
    );
    let mut still = open_list(Ok(()), vec![base.clone()]);
    run_now(&mut still);
    update(
        &mut still,
        CronInput::Key(CronKey::Char('y')),
        support::clock(),
        support::WALL,
    );
    update(
        &mut still,
        CronInput::Triggered {
            job_id: base.job_id.clone(),
            outcome: TriggerOutcome::Failed(RestFailure::NotFound),
        },
        support::clock(),
        support::WALL,
    );
    settle(&mut still, vec![base.clone()]);
    assert_eq!(
        view(&still, support::WALL).notice.as_deref(),
        Some(
            "Hermes answered 404 for the run of daily digest, but the job is still listed; this Hermes may not serve run now."
        )
    );

    for failure in [
        CallFailure::NotSent("gateway down".into()),
        CallFailure::Unknown,
    ] {
        let mut fresh = open_list(Ok(()), vec![base.clone()]);
        run_now(&mut fresh);
        update(
            &mut fresh,
            CronInput::Key(CronKey::Char('y')),
            support::clock(),
            support::WALL,
        );
        update(
            &mut fresh,
            CronInput::Triggered {
                job_id: base.job_id.clone(),
                outcome: TriggerOutcome::Accepted,
            },
            support::clock(),
            support::WALL,
        );
        update(
            &mut fresh,
            CronInput::Listed {
                seq: 2,
                result: Err(failure),
            },
            support::clock(),
            support::WALL,
        );
        let notice = view(&fresh, support::WALL).notice.unwrap_or_default();
        assert!(
            notice.starts_with("Talaria could not check the result of the run of daily digest:")
        );
        assert!(notice.ends_with("ctrl+r reloads."));
    }

    let stray = update(
        &mut state,
        CronInput::Triggered {
            job_id: "not-a-job".into(),
            outcome: TriggerOutcome::Accepted,
        },
        support::clock(),
        support::WALL,
    );
    assert!(stray.is_empty());
}

#[test]
fn pending_row_and_fast_poll() {
    let job = support::job("a1b2c3d4e5f6", "daily digest", "scheduled");
    let mut state = open_list(Ok(()), vec![job.clone()]);
    let start = support::clock();
    update(
        &mut state,
        CronInput::Key(CronKey::Enter),
        start,
        support::WALL,
    );
    run_now(&mut state);
    update(
        &mut state,
        CronInput::Key(CronKey::Char('y')),
        start,
        support::WALL,
    );
    let CronBody::Detail(card) = view(&state, support::WALL).body else {
        panic!("the card stays open while the run starts");
    };
    let RunsView::Rows { pending, .. } = card.runs else {
        panic!("pending history");
    };
    assert_eq!(pending.as_deref(), Some("starting… (requested now)"));
    let early = update(
        &mut state,
        CronInput::Tick,
        start + std::time::Duration::from_secs(1),
        support::WALL + 1,
    );
    assert!(
        !early
            .iter()
            .any(|effect| matches!(effect, CronEffect::Runs { .. }))
    );
    let polled = update(
        &mut state,
        CronInput::Tick,
        start + std::time::Duration::from_secs(2),
        support::WALL + 2,
    );
    assert!(polled.iter().any(|effect| matches!(
        effect,
        CronEffect::Runs { job_id, .. } if job_id == "a1b2c3d4e5f6"
    )));
    let too_old = RunRow {
        id: "old".into(),
        source: "cron".into(),
        title: Some("daily digest".into()),
        preview: None,
        started_at: Some((support::WALL - 3) as f64),
        ended_at: None,
        end_reason: None,
        input_tokens: 1,
        output_tokens: 1,
        message_count: 0,
        tool_call_count: 0,
        model: None,
        is_active: true,
        scheduler_owned: true,
    };
    update(
        &mut state,
        CronInput::RunsLoaded {
            job_id: job.job_id.clone(),
            seq: 1,
            result: Ok(vec![too_old.clone()]),
        },
        start,
        support::WALL,
    );
    let CronBody::Detail(still) = view(&state, support::WALL).body else {
        panic!("card");
    };
    let RunsView::Rows { pending, .. } = still.runs else {
        panic!("still pending");
    };
    assert!(pending.is_some());
    let fresh = RunRow {
        started_at: Some((support::WALL - 2) as f64),
        id: "new".into(),
        ..too_old
    };
    update(
        &mut state,
        CronInput::RunsLoaded {
            job_id: job.job_id.clone(),
            seq: 2,
            result: Ok(vec![fresh]),
        },
        start,
        support::WALL,
    );
    let CronBody::Detail(settled) = view(&state, support::WALL).body else {
        panic!("card");
    };
    let RunsView::Rows { pending, .. } = settled.runs else {
        panic!("rows");
    };
    assert!(pending.is_none());

    let mut expiring = open_list(Ok(()), vec![job.clone()]);
    update(
        &mut expiring,
        CronInput::Key(CronKey::Enter),
        start,
        support::WALL,
    );
    run_now(&mut expiring);
    update(
        &mut expiring,
        CronInput::Key(CronKey::Char('y')),
        start,
        support::WALL,
    );
    update(
        &mut expiring,
        CronInput::Tick,
        start + std::time::Duration::from_secs(90),
        support::WALL + 90,
    );
    let CronBody::Detail(ended) = view(&expiring, support::WALL + 90).body else {
        panic!("card");
    };
    let RunsView::Rows { pending, .. } = ended.runs else {
        panic!("rows");
    };
    assert!(pending.is_none());
}

#[test]
fn survives_overlay_close() {
    let job = support::job("a1b2c3d4e5f6", "daily digest", "scheduled");
    let mut state = open_list(Ok(()), vec![job.clone()]);
    let start = support::clock();
    run_now(&mut state);
    update(
        &mut state,
        CronInput::Key(CronKey::Char('y')),
        start,
        support::WALL,
    );
    update(&mut state, CronInput::Close, start, support::WALL);
    update(
        &mut state,
        CronInput::Triggered {
            job_id: job.job_id.clone(),
            outcome: TriggerOutcome::Accepted,
        },
        start,
        support::WALL,
    );
    let mut finished = job.clone();
    finished.last_status = Some("ok".into());
    finished.last_run_at = Some("2026-10-23T09:05:00Z".into());
    let closed = update(
        &mut state,
        CronInput::Listed {
            seq: 2,
            result: Ok(ListReply {
                jobs: vec![finished],
                warning: None,
            }),
        },
        start,
        support::WALL,
    );
    assert!(closed.iter().any(|effect| matches!(
        effect,
        CronEffect::Notice(text) if text == "Ran daily digest: finished ok."
    )));

    let mut waiting = open_list(Ok(()), vec![job.clone()]);
    run_now(&mut waiting);
    update(
        &mut waiting,
        CronInput::Key(CronKey::Char('y')),
        start,
        support::WALL,
    );
    update(&mut waiting, CronInput::Close, start, support::WALL);
    update(
        &mut waiting,
        CronInput::Open(CronOpen::List),
        start,
        support::WALL,
    );
    run_now(&mut waiting);
    assert_eq!(
        view(&waiting, support::WALL).notice.as_deref(),
        Some("daily digest is already running from here.")
    );
}
