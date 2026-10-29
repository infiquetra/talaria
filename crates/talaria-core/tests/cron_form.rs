#[path = "cron_support/mod.rs"]
mod support;

use std::time::Duration;
use talaria_core::cron::{
    AddForm, AddReply, AddRequest, CallFailure, CronBody, CronEffect, CronInput, CronKey, CronOpen,
    CronState, DeliveryTarget, FieldError, FormField, RestFailure, parse_arguments, update,
    validate_form, view,
};

const BLUE_MOON: &str = "Invalid schedule 'blue moon'. Use:\n  - Interval: '30m', 'every 30m', 'every 2h' (recurring)\n  - One-shot delay: 'in 30m', 'in 2h' (fires once)\n  - Weekly/daily: 'every monday 9am', 'weekdays at 9am' (recurring)\n  - Cron: '0 9 * * *' (cron expression)\n  - Timestamp: '2026-02-03T14:00:00' (one-shot at time)";
const EVERY_BLUE: &str = "Invalid duration: 'blue moon'. Use format like '30m', '2h', '1d', or a bare unit like 'hour' (defaults to 1).";
const USAGE: &str = "Usage: /cron [list|new]. Talaria creates jobs in a form; it does not read /cron add arguments.";

fn form(name: &str, schedule: &str, prompt: &str, repeat: &str, deliver: &str) -> AddForm {
    AddForm {
        name: name.into(),
        schedule: schedule.into(),
        prompt: prompt.into(),
        repeat: repeat.into(),
        deliver: deliver.into(),
    }
}

fn texts(errors: &[FieldError], field: FormField) -> Vec<&str> {
    errors
        .iter()
        .filter(|error| error.field == field)
        .map(|error| error.text.as_str())
        .collect()
}

fn open_form() -> CronState {
    let mut state = CronState::new("fixture".into(), Ok(()));
    update(
        &mut state,
        CronInput::Open(CronOpen::NewJob),
        support::clock(),
        support::WALL,
    );
    state
}

#[test]
fn field_rules() {
    let long_name = "n".repeat(101);
    let ok_name = "n".repeat(100);
    let long_prompt = "p".repeat(16_001);
    let ok_prompt = "p".repeat(16_000);
    let too_long = validate_form(&form(
        &long_name,
        "every day 9am",
        "Run the checks.",
        "",
        "local",
    ))
    .expect_err("101 characters");
    assert_eq!(
        texts(&too_long, FormField::Name),
        ["Name: at most 100 characters."]
    );
    assert!(
        validate_form(&form(
            &ok_name,
            "every day 9am",
            "Run the checks.",
            "",
            "local"
        ))
        .is_ok()
    );
    let prompt_long = validate_form(&form("digest", "every day 9am", &long_prompt, "", "local"))
        .expect_err("16001 characters");
    assert_eq!(
        texts(&prompt_long, FormField::Prompt),
        ["Prompt: at most 16,000 characters."]
    );
    assert!(validate_form(&form("digest", "every day 9am", &ok_prompt, "", "local")).is_ok());
    let missing = validate_form(&form("", "  ", "   ", "", "local")).expect_err("required");
    assert_eq!(
        texts(&missing, FormField::Schedule),
        ["Schedule: required."]
    );
    assert_eq!(texts(&missing, FormField::Prompt), ["Prompt: required."]);
    let seven = validate_form(&form(
        "",
        "every day 9am",
        "Run the checks.",
        "007",
        "local",
    ))
    .unwrap();
    assert_eq!(seven.repeat, Some(7));
    let five = validate_form(&form(
        "",
        "every day 9am",
        "Run the checks.",
        " 5 ",
        "local",
    ))
    .unwrap();
    assert_eq!(five.repeat, Some(5));
    for repeat in ["+5", "-1", "0", "3.5", "abc", "10000"] {
        let errors = validate_form(&form(
            "",
            "every day 9am",
            "Run the checks.",
            repeat,
            "local",
        ))
        .expect_err(repeat);
        assert_eq!(
            texts(&errors, FormField::Repeat),
            ["Repeat: empty for forever, or a whole number from 1 to 9999."]
        );
    }
    let mut state = open_form();
    update(
        &mut state,
        CronInput::Key(CronKey::Paste("hello\nworld\r\nthere".into())),
        support::clock(),
        support::WALL,
    );
    let CronBody::Form(shown) = view(&state, support::WALL).body else {
        panic!("pasted name should stay on the form");
    };
    let name = shown
        .fields
        .iter()
        .find(|field| field.field == FormField::Name)
        .unwrap();
    assert_eq!(name.value, "hello world there");
}

#[test]
fn add_request_is_exact() {
    let blank = validate_form(&form(
        "   ",
        " every day 9am ",
        "line one\nline two  ",
        "",
        "telegram",
    ))
    .expect("blank name and forever repeat");
    assert_eq!(
        blank,
        AddRequest {
            name: None,
            schedule: "every day 9am".into(),
            prompt: "line one\nline two".into(),
            repeat: None,
            deliver: "telegram".into(),
        }
    );
    let named = validate_form(&form(
        "  daily digest  ",
        "every day 9am",
        "Summarise yesterday's merged pull requests in five lines.",
        "3",
        "local",
    ))
    .expect("named");
    assert_eq!(
        named,
        AddRequest {
            name: Some("daily digest".into()),
            schedule: "every day 9am".into(),
            prompt: "Summarise yesterday's merged pull requests in five lines.".into(),
            repeat: Some(3),
            deliver: "local".into(),
        }
    );
    let mut state = open_form();
    let clock = support::clock();
    for key in [
        CronKey::Paste("daily digest".into()),
        CronKey::Tab,
        CronKey::Paste("every day 9am".into()),
        CronKey::Tab,
        CronKey::Paste("Summarise yesterday's merged pull requests in five lines.".into()),
        CronKey::Tab,
        CronKey::Paste("3".into()),
    ] {
        let _ = update(&mut state, CronInput::Key(key), clock, support::WALL);
    }
    let saved = update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Save)),
        clock,
        support::WALL,
    );
    assert!(saved.iter().any(|effect| matches!(
        effect,
        CronEffect::Add(request) if request == &named
    )));
    let CronBody::Form(shown) = view(&state, support::WALL).body else {
        panic!("saving keeps the form open until the reply");
    };
    assert!(shown.locked);
    assert!(shown.status.iter().any(|line| line == "Saving…"));
    let _ = Duration::from_millis(1);
}

#[test]
fn add_failures() {
    let mut state = open_form();
    let clock = support::clock();
    update(
        &mut state,
        CronInput::Listed {
            seq: 1,
            result: Ok(talaria_core::cron::ListReply {
                jobs: Vec::new(),
                warning: None,
            }),
        },
        clock,
        support::WALL,
    );
    for key in [
        CronKey::Paste("daily digest".into()),
        CronKey::Tab,
        CronKey::Paste("every day 9am".into()),
        CronKey::Tab,
        CronKey::Paste("Summarise yesterday's merged pull requests in five lines.".into()),
        CronKey::Tab,
        CronKey::Paste("3".into()),
    ] {
        let _ = update(&mut state, CronInput::Key(key), clock, support::WALL);
    }
    let _ = update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Save)),
        clock,
        support::WALL,
    );
    let schedule_failure = update(
        &mut state,
        CronInput::Added(Err(CallFailure::Tool {
            error: BLUE_MOON.into(),
            job_saved: false,
            job_id: None,
            matches: 0,
        })),
        clock,
        support::WALL,
    );
    let shown = view(&state, support::WALL);
    let CronBody::Form(form_view) = shown.body else {
        panic!("a schedule failure leaves the form open");
    };
    assert!(!form_view.locked);
    let mut expected = vec!["Hermes did not create the job:".to_string()];
    expected.extend(BLUE_MOON.lines().map(str::to_owned));
    assert_eq!(form_view.status, expected);
    assert_eq!(expected.len(), 7);

    update(
        &mut state,
        CronInput::Listed {
            seq: support::list_seq(&schedule_failure),
            result: Ok(talaria_core::cron::ListReply {
                jobs: Vec::new(),
                warning: None,
            }),
        },
        clock,
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Save)),
        clock,
        support::WALL,
    );
    let duration_failure = update(
        &mut state,
        CronInput::Added(Err(CallFailure::Tool {
            error: EVERY_BLUE.into(),
            job_saved: false,
            job_id: None,
            matches: 0,
        })),
        clock,
        support::WALL,
    );
    let CronBody::Form(duration) = view(&state, support::WALL).body else {
        panic!("a duration failure leaves the form open");
    };
    assert_eq!(
        duration.status,
        vec![
            "Hermes did not create the job:".to_string(),
            EVERY_BLUE.to_string(),
        ]
    );

    update(
        &mut state,
        CronInput::Listed {
            seq: support::list_seq(&duration_failure),
            result: Ok(talaria_core::cron::ListReply {
                jobs: Vec::new(),
                warning: None,
            }),
        },
        clock,
        support::WALL,
    );
    update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Save)),
        clock,
        support::WALL,
    );
    let saved = update(
        &mut state,
        CronInput::Added(Err(CallFailure::Tool {
            error: "scheduler rejected the job".into(),
            job_saved: true,
            job_id: Some("a1b2c3d4e5f6".into()),
            matches: 0,
        })),
        clock,
        support::WALL,
    );
    assert!(
        saved
            .iter()
            .any(|effect| matches!(effect, CronEffect::List { .. }))
    );
    let after = view(&state, support::WALL);
    assert!(!matches!(after.body, CronBody::Form(_)));
    assert_eq!(
        after.notice.as_deref(),
        Some(
            "Hermes saved daily digest (a1b2c3d4e5f6) but could not register it with its scheduler: scheduler rejected the job. Do not create it again; check the list."
        )
    );
    let _ = AddReply {
        job_id: "a1b2c3d4e5f6".into(),
        message: "Cron job 'daily digest' created.".into(),
        next_run_at: None,
        job: None,
    };
}

#[test]
fn delivery_target_choices() {
    let mut state = open_form();
    let clock = support::clock();
    let opened = update(
        &mut state,
        CronInput::Open(CronOpen::NewJob),
        clock,
        support::WALL,
    );
    assert!(
        opened
            .iter()
            .any(|effect| matches!(effect, CronEffect::DeliveryTargets))
    );
    update(
        &mut state,
        CronInput::TargetsLoaded(Ok(vec![
            DeliveryTarget {
                id: "local".into(),
                name: "Local (save only)".into(),
                home_target_set: true,
            },
            DeliveryTarget {
                id: "telegram".into(),
                name: "Telegram".into(),
                home_target_set: true,
            },
            DeliveryTarget {
                id: "discord".into(),
                name: "Discord".into(),
                home_target_set: false,
            },
        ])),
        clock,
        support::WALL,
    );
    let CronBody::Form(shown) = view(&state, support::WALL).body else {
        panic!("targets stay on the form");
    };
    assert_eq!(
        shown.targets,
        vec![
            ("Local (save only) (local)".into(), true),
            ("Telegram (telegram)".into(), true),
            ("Discord (discord) · no home channel set".into(), false),
        ]
    );
    for _ in 0..4 {
        update(
            &mut state,
            CronInput::Key(CronKey::Tab),
            clock,
            support::WALL,
        );
    }
    for _ in 0..3 {
        update(
            &mut state,
            CronInput::Key(CronKey::Right),
            clock,
            support::WALL,
        );
    }
    let CronBody::Form(after_right) = view(&state, support::WALL).body else {
        panic!("still the form");
    };
    assert_ne!(after_right.deliver_index, 2);

    let mut missing_local = open_form();
    update(
        &mut missing_local,
        CronInput::TargetsLoaded(Ok(vec![DeliveryTarget {
            id: "telegram".into(),
            name: "Telegram".into(),
            home_target_set: true,
        }])),
        clock,
        support::WALL,
    );
    let CronBody::Form(prepended) = view(&missing_local, support::WALL).body else {
        panic!("form");
    };
    assert_eq!(prepended.targets[0].0, "Local (save only) (local)");
    assert!(
        prepended
            .targets
            .iter()
            .filter(|target| target.0.contains("(local)"))
            .count()
            == 1
    );

    let mut failed = open_form();
    update(
        &mut failed,
        CronInput::TargetsLoaded(Err(RestFailure::NotSent("dashboard down".into()))),
        clock,
        support::WALL,
    );
    let CronBody::Form(only_local) = view(&failed, support::WALL).body else {
        panic!("form");
    };
    assert!(
        only_local
            .status
            .iter()
            .any(|line| line == "Could not load delivery targets; only local is offered.")
    );
    assert_eq!(
        only_local.targets,
        vec![("Local (save only) (local)".into(), true)]
    );
}

#[test]
fn selected_delivery_target_is_the_saved_destination() {
    let mut state = open_form();
    let clock = support::clock();
    update(
        &mut state,
        CronInput::TargetsLoaded(Ok(vec![
            DeliveryTarget {
                id: "local".into(),
                name: "Local".into(),
                home_target_set: true,
            },
            DeliveryTarget {
                id: "telegram".into(),
                name: "Telegram".into(),
                home_target_set: true,
            },
        ])),
        clock,
        support::WALL,
    );
    for key in [
        CronKey::Paste("job".into()),
        CronKey::Tab,
        CronKey::Paste("30m".into()),
        CronKey::Tab,
        CronKey::Paste("prompt".into()),
        CronKey::Tab,
        CronKey::Tab,
        CronKey::Right,
    ] {
        update(&mut state, CronInput::Key(key), clock, support::WALL);
    }
    let CronBody::Form(shown) = view(&state, support::WALL).body else {
        panic!("form")
    };
    assert!(shown.targets[shown.deliver_index].0.contains("telegram"));
    let effects = update(
        &mut state,
        CronInput::Key(CronKey::Action(talaria_core::cron::CronAction::Save)),
        clock,
        support::WALL,
    );
    assert!(
        effects.iter().any(
            |effect| matches!(effect, CronEffect::Add(request) if request.deliver == "telegram")
        )
    );
}

#[test]
fn slash_arguments() {
    assert_eq!(parse_arguments("").unwrap(), CronOpen::List);
    assert_eq!(parse_arguments("list").unwrap(), CronOpen::List);
    assert_eq!(parse_arguments(" list").unwrap(), CronOpen::List);
    assert_eq!(parse_arguments("new").unwrap(), CronOpen::NewJob);
    assert_eq!(parse_arguments("add").unwrap(), CronOpen::NewJob);
    assert_eq!(parse_arguments("add ").unwrap(), CronOpen::NewJob);
    for args in ["add \"every 2h\" x", "pause", "LIST", "new x"] {
        assert_eq!(parse_arguments(args).unwrap_err(), USAGE);
    }
    let mut state = CronState::new("fixture".into(), Ok(()));
    let effects = update(
        &mut state,
        CronInput::Open(CronOpen::NewJob),
        support::clock(),
        support::WALL,
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, CronEffect::DeliveryTargets))
    );
    assert!(effects.iter().all(|effect| matches!(
        effect,
        CronEffect::DeliveryTargets | CronEffect::ScheduleTick { .. } | CronEffect::List { .. }
    )));
}
