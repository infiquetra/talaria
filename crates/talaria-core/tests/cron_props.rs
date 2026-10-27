#[path = "cron_support/mod.rs"]
mod support;

use proptest::prelude::*;
use talaria_core::cron::{
    CallFailure, CronAction, CronBody, CronEffect, CronInput, CronKey, CronOpen, CronState,
    CronView, DeliveryTarget, JobRow, ListReply, RunRow, RunsView, TriggerOutcome, update, view,
};

fn poison(text: &str) -> String {
    let escape = '\u{1b}';
    let bel = '\u{07}';
    let mark = '\u{202e}';
    let isolate = '\u{2066}';
    format!("{escape}[31m{text}{escape}]0;title{bel}{mark}{isolate}")
}

fn hostile_free(text: &str) -> bool {
    let escape = '\u{1b}';
    let bel = '\u{07}';
    let mark = '\u{202e}';
    let isolate = '\u{2066}';
    !text
        .chars()
        .any(|ch| ch == escape || ch == bel || ch == mark || ch == isolate)
}

fn walk(view: &CronView) -> Vec<String> {
    let mut texts = vec![view.title.clone(), view.hints.clone()];
    if let Some(banner) = &view.banner {
        texts.push(banner.clone());
    }
    if let Some(notice) = &view.notice {
        texts.push(notice.clone());
    }
    if let Some(question) = &view.question {
        texts.push(question.clone());
    }
    match &view.body {
        CronBody::Message(text) => texts.push(text.clone()),
        CronBody::List(list) => {
            if let Some(filter) = &list.filter {
                texts.push(filter.clone());
            }
            if let Some(status) = &list.status {
                texts.push(status.clone());
            }
            for row in &list.rows {
                texts.extend([
                    row.job_id.clone(),
                    row.name.clone(),
                    row.schedule.clone(),
                    row.state.clone(),
                    row.last_run.clone(),
                ]);
            }
        }
        CronBody::Detail(card) => {
            for (label, lines) in &card.fields {
                texts.push(label.clone());
                texts.extend(lines.clone());
            }
            if let Some(run_card) = &card.run_card {
                for (label, lines) in run_card {
                    texts.push(label.clone());
                    texts.extend(lines.clone());
                }
            }
            match &card.runs {
                RunsView::Unavailable(text) | RunsView::Failed(text) => texts.push(text.clone()),
                RunsView::Rows { pending, rows, .. } => {
                    if let Some(pending) = pending {
                        texts.push(pending.clone());
                    }
                    for row in rows {
                        texts.extend([
                            row.start.clone(),
                            row.status.clone(),
                            row.duration.clone(),
                            row.tokens.clone(),
                            row.title.clone(),
                        ]);
                    }
                }
                RunsView::Loading | RunsView::Empty => {}
            }
        }
        CronBody::Form(form) => {
            texts.extend(form.status.clone());
            for (label, _) in &form.targets {
                texts.push(label.clone());
            }
            for field in &form.fields {
                texts.extend([
                    field.label.clone(),
                    field.value.clone(),
                    field.hint.clone().unwrap_or_default(),
                    field.error.clone().unwrap_or_default(),
                ]);
            }
        }
    }
    texts
}

fn poisoned_job(extra: &str) -> JobRow {
    let mut job = support::job("a1b2c3d4e5f6", &poison("daily digest"), "scheduled");
    job.schedule = poison("0 9 * * *");
    job.prompt_preview = poison(&format!("preview {extra}"));
    job.last_error = Some(poison("provider returned 429"));
    job.last_delivery_error = Some(poison("platform down"));
    job.last_fire_error = Some(talaria_core::cron::FireError {
        at: Some("2026-10-22T03:00:01-04:00".into()),
        detail: poison("forward to gateway failed"),
    });
    job.paused_reason = Some(poison("held"));
    job
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn hostile_text_is_sanitised(extra in "[a-z]{0,12}") {
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
                    jobs: vec![poisoned_job(&extra)],
                    warning: Some(poison("The scheduler is not running.")),
                }),
            },
            clock,
            support::WALL,
        );
        update(
            &mut state,
            CronInput::TargetsLoaded(Ok(vec![DeliveryTarget {
                id: "local".into(),
                name: poison("Local (save only)"),
                home_target_set: true,
            }])),
            clock,
            support::WALL,
        );
        let shown = view(&state, support::WALL);
        for text in walk(&shown) {
            prop_assert!(hostile_free(&text));
        }
        update(
            &mut state,
            CronInput::Key(CronKey::Enter),
            clock,
            support::WALL,
        );
        let run = RunRow {
            id: "local-finished".into(),
            source: "cron".into(),
            title: Some(poison("daily digest")),
            preview: Some(poison("preview")),
            started_at: Some(support::WALL as f64),
            ended_at: Some(support::WALL as f64 + 45.0),
            end_reason: Some(poison("completed")),
            input_tokens: 1,
            output_tokens: 1,
            message_count: 0,
            tool_call_count: 0,
            model: Some(poison("minimax-m3.1-flash")),
            is_active: false,
            scheduler_owned: false,
        };
        update(
            &mut state,
            CronInput::RunsLoaded {
                job_id: "a1b2c3d4e5f6".into(),
                seq: 1,
                result: Ok(vec![run]),
            },
            clock,
            support::WALL,
        );
        for text in walk(&view(&state, support::WALL)) {
            prop_assert!(hostile_free(&text));
        }
    }

    #[test]
    fn writes_need_a_deliberate_action(steps in prop::collection::vec(any::<u8>(), 0..8)) {
        let mut state = CronState::new("fixture".into(), Ok(()));
        let clock = support::clock();
        let job = support::job("a1b2c3d4e5f6", "daily digest", "scheduled");
        let fixed = update(
            &mut state,
            CronInput::Key(CronKey::Action(CronAction::TogglePause)),
            clock,
            support::WALL,
        );
        let fixed_ok = fixed.is_empty()
            || fixed
                .iter()
                .any(|effect| matches!(effect, CronEffect::SetPaused { .. }));
        prop_assert!(fixed_ok);
        let stray = update(
            &mut state,
            CronInput::Key(CronKey::Char('y')),
            clock,
            support::WALL,
        );
        let stray_ok = stray.iter().all(|effect| {
            !matches!(
                effect,
                CronEffect::Add(_)
                    | CronEffect::SetPaused { .. }
                    | CronEffect::Remove { .. }
                    | CronEffect::Trigger { .. }
            )
        });
        prop_assert!(stray_ok);
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
                    jobs: vec![job],
                    warning: None,
                }),
            },
            clock,
            support::WALL,
        );
        let pause = update(
            &mut state,
            CronInput::Key(CronKey::Action(CronAction::TogglePause)),
            clock,
            support::WALL,
        );
        let pause_ok = pause.iter().any(|effect| {
            matches!(
                effect,
                CronEffect::SetPaused { job_id, paused: true } if job_id == "a1b2c3d4e5f6"
            )
        });
        prop_assert!(pause_ok);
        let again = update(
            &mut state,
            CronInput::Key(CronKey::Char('y')),
            clock,
            support::WALL,
        );
        let again_ok = again.iter().all(|effect| {
            !matches!(
                effect,
                CronEffect::Remove { .. } | CronEffect::Trigger { .. }
            )
        });
        prop_assert!(again_ok);
        for byte in steps {
            let input = input_from(byte);
            let effects = update(&mut state, input.clone(), clock, support::WALL);
            for effect in effects {
                match effect {
                    CronEffect::Add(_) => prop_assert!(matches!(
                        input,
                        CronInput::Key(CronKey::Action(CronAction::Save))
                    )),
                    CronEffect::SetPaused { .. } => prop_assert!(matches!(
                        input,
                        CronInput::Key(CronKey::Action(CronAction::TogglePause))
                    )),
                    CronEffect::Remove { .. } => prop_assert!(matches!(
                        input,
                        CronInput::Key(CronKey::Char('y'))
                    )),
                    CronEffect::Trigger { .. } => prop_assert!(matches!(
                        input,
                        CronInput::Key(CronKey::Char('y'))
                    )),
                    CronEffect::OpenSession { .. } => prop_assert!(matches!(
                        input,
                        CronInput::Key(CronKey::Action(CronAction::OpenRun))
                    )),
                    CronEffect::List { .. }
                    | CronEffect::Runs { .. }
                    | CronEffect::DeliveryTargets
                    | CronEffect::ScheduleTick { .. }
                    | CronEffect::Notice(_)
                    | CronEffect::CloseOverlay => {}
                }
            }
        }
    }
}

fn input_from(byte: u8) -> CronInput {
    match byte % 20 {
        0 => CronInput::Open(CronOpen::List),
        1 => CronInput::Open(CronOpen::NewJob),
        2 => CronInput::Key(CronKey::Action(CronAction::New)),
        3 => CronInput::Key(CronKey::Action(CronAction::RunNow)),
        4 => CronInput::Key(CronKey::Action(CronAction::TogglePause)),
        5 => CronInput::Key(CronKey::Action(CronAction::Remove)),
        6 => CronInput::Key(CronKey::Action(CronAction::Reload)),
        7 => CronInput::Key(CronKey::Action(CronAction::Save)),
        8 => CronInput::Key(CronKey::Action(CronAction::OpenRun)),
        9 => CronInput::Key(CronKey::Char('y')),
        10 => CronInput::Key(CronKey::Char('n')),
        11 => CronInput::Key(CronKey::Esc),
        12 => CronInput::Tick,
        13 => CronInput::Changed,
        14 => CronInput::Close,
        15 => CronInput::Connection { up: byte % 2 == 0 },
        16 => CronInput::Listed {
            seq: 1,
            result: Err(CallFailure::NotSent("down".into())),
        },
        17 => CronInput::Added(Err(CallFailure::Unknown)),
        18 => CronInput::Triggered {
            job_id: "a1b2c3d4e5f6".into(),
            outcome: TriggerOutcome::Accepted,
        },
        _ => CronInput::Removed {
            job_id: "a1b2c3d4e5f6".into(),
            result: Err(CallFailure::Unknown),
        },
    }
}
