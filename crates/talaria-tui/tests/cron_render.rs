mod support;

use std::time::{Duration, Instant};

use ratatui::{buffer::Buffer, layout::Rect};
use support::Vt;
use talaria_core::cron::{
    CardFocus, CronBody, CronInput, CronKey, CronOpen, CronState, CronView, DetailView, FieldView,
    FormField, FormView, ListReply, ListView, RowView, RunRowView, RunsView, update, view,
};
use talaria_tui::cron::{CronFrameGate, draw_cron};

fn paint(shown: &CronView, cols: u16, rows: u16) -> String {
    let area = Rect::new(0, 0, cols, rows);
    let mut buf = Buffer::empty(area);
    draw_cron(shown, area, &mut buf);
    let mut vt = Vt::new(cols, rows);
    for y in 0..rows {
        let mut line = String::new();
        for x in 0..cols {
            line.push_str(buf[(x, y)].symbol());
        }
        line.push_str("\r\n");
        vt.feed(line.as_bytes());
    }
    vt.rows().join("\n")
}

fn list_view(status: Option<&str>, rows: Vec<RowView>, banner: Option<&str>) -> CronView {
    CronView {
        title: "Scheduled jobs · profile fixture".into(),
        banner: banner.map(str::to_owned),
        body: CronBody::List(ListView {
            filter: None,
            status: status.map(str::to_owned),
            rows,
            selected: Some(0),
        }),
        notice: None,
        question: None,
        hints: "enter details · ctrl+n new · ctrl+t run now · ctrl+e pause/resume · ctrl+d remove · ctrl+r reload · esc close".into(),
    }
}

fn row(name: &str, state: &str) -> RowView {
    RowView {
        job_id: "a1b2c3d4e5f6".into(),
        name: name.into(),
        schedule: "0 9 * * *".into(),
        state: state.into(),
        last_run: "ok 1h ago".into(),
    }
}

#[test]
fn list_snapshots() {
    let loading = CronView {
        title: "Scheduled jobs · profile fixture".into(),
        banner: None,
        body: CronBody::Message("Loading scheduled jobs…".into()),
        notice: None,
        question: None,
        hints: String::new(),
    };
    let wide = paint(&loading, 160, 48);
    assert!(wide.contains("Loading scheduled jobs"));
    assert!(wide.contains("Scheduled jobs · profile fixture"));
    let narrow = paint(&loading, 80, 24);
    assert!(narrow.contains("Loading scheduled jobs"));
    let rows = list_view(
        None,
        vec![
            row("daily digest", "next in 3h 12m"),
            row("nightly check", "paused"),
        ],
        Some("The scheduler is not running."),
    );
    let screen = paint(&rows, 160, 48);
    assert!(screen.contains("daily digest"));
    assert!(screen.contains("next in 3h 12m"));
    assert!(screen.contains("The scheduler is not running."));
    assert!(screen.contains("ctrl+n new"));
    let compact = paint(&rows, 80, 24);
    assert!(compact.contains("daily digest"));
    let empty = CronView {
        body: CronBody::Message("No scheduled jobs in profile fixture. ctrl+n creates one.".into()),
        ..loading.clone()
    };
    assert!(paint(&empty, 80, 24).contains("No scheduled jobs in profile fixture"));
    let failed = list_view(
        Some("Could not load scheduled jobs: gateway down. ctrl+r tries again."),
        vec![row("daily digest", "scheduled")],
        None,
    );
    assert!(paint(&failed, 80, 24).contains("Could not load scheduled jobs:"));
    let unmatched = list_view(Some("No job matches \"zz\"."), Vec::new(), None);
    assert!(paint(&unmatched, 80, 24).contains("No job matches"));
    let tiny = paint(&rows, 59, 12);
    assert!(tiny.contains("Make the terminal at least 60 by 12 to manage scheduled jobs."));
}

#[test]
fn card_and_form_snapshots() {
    let card = CronView {
        title: "Scheduled jobs · profile fixture".into(),
        banner: None,
        body: CronBody::Detail(DetailView {
            fields: vec![("Last fire".into(), vec!["forward to gateway failed".into()])],
            runs: RunsView::Rows {
                pending: Some("starting… (requested now)".into()),
                rows: vec![RunRowView {
                    start: "1h ago".into(),
                    status: "finished · completed".into(),
                    duration: "3m 05s".into(),
                    tokens: "100/2 tokens".into(),
                    title: "daily digest".into(),
                }],
                selected: Some(0),
            },
            run_card: None,
            focus: CardFocus::History,
            fields_scroll: 0,
        }),
        notice: None,
        question: None,
        hints: String::new(),
    };
    let screen = paint(&card, 160, 48);
    assert!(screen.contains("forward to gateway failed"));
    assert!(screen.contains("starting…"));
    assert!(screen.contains("finished · completed"));
    let form = CronView {
        title: "Scheduled jobs · profile fixture".into(),
        banner: None,
        body: CronBody::Form(FormView {
            fields: vec![
                FieldView {
                    field: FormField::Name,
                    label: "Name".into(),
                    value: String::new(),
                    hint: None,
                    error: Some("Name: at most 100 characters.".into()),
                },
                FieldView {
                    field: FormField::Schedule,
                    label: "Schedule".into(),
                    value: String::new(),
                    hint: Some("Examples: 30m · every 2h".into()),
                    error: Some("Schedule: required.".into()),
                },
                FieldView {
                    field: FormField::Prompt,
                    label: "Prompt".into(),
                    value: String::new(),
                    hint: None,
                    error: Some("Prompt: required.".into()),
                },
            ],
            focus: FormField::Schedule,
            targets: vec![("Local (save only) (local)".into(), true)],
            deliver_index: 0,
            status: vec![
                "Hermes did not create the job:".into(),
                "Invalid schedule 'blue moon'.".into(),
            ],
            locked: false,
            cursor: Some((0, 0)),
        }),
        notice: None,
        question: None,
        hints: String::new(),
    };
    let form_screen = paint(&form, 80, 24);
    assert!(form_screen.contains("Name: at most 100 characters."));
    assert!(form_screen.contains("Schedule: required."));
    assert!(form_screen.contains("Hermes did not create the job:"));
    for question in [
        "Run daily digest now in profile fixture? It uses this profile's cron model (cron.model, else the main model); it delivers to local.  y run · n cancel",
        "Discard this new job?  y discard · n keep editing",
        "Remove daily digest from profile fixture? This cannot be undone.  y remove · n cancel",
    ] {
        let shown = CronView {
            question: Some(question.into()),
            ..list_view(None, vec![row("daily digest", "scheduled")], None)
        };
        assert!(paint(&shown, 160, 48).contains(question));
    }
}

#[test]
fn opens_without_waiting_and_idles() {
    let started = Instant::now();
    let mut state = CronState::new("fixture".into(), Ok(()));
    let now = Instant::now();
    update(
        &mut state,
        CronInput::Open(CronOpen::List),
        now,
        1_792_746_030,
    );
    let shown = view(&state, 1_792_746_030);
    let mut gate = CronFrameGate::new();
    assert!(gate.needs_draw(&shown));
    let _ = paint(&shown, 80, 24);
    assert!(started.elapsed() < Duration::from_millis(50));
    let idle = view(&state, 1_792_746_032);
    assert!(!gate.needs_draw(&idle));
    update(
        &mut state,
        CronInput::Listed {
            seq: 1,
            result: Ok(ListReply {
                jobs: Vec::new(),
                warning: None,
            }),
        },
        now + Duration::from_secs(2),
        1_792_746_032,
    );
    let same = view(&state, 1_792_746_032);
    assert!(!gate.needs_draw(&same) || same == idle);
    update(
        &mut state,
        CronInput::Tick,
        now + Duration::from_secs(30),
        1_792_746_060,
    );
    let ticked = view(&state, 1_792_746_060);
    if ticked != same {
        assert!(gate.needs_draw(&ticked));
    }
}

#[test]
fn fifty_jobs_within_budget() {
    let mut state = CronState::new("fixture".into(), Ok(()));
    let now = Instant::now();
    let wall = 1_792_746_000;
    let mut jobs = Vec::new();
    for index in 0..50 {
        let mut job = talaria_core::cron::JobRow {
            job_id: format!("job{index:02}"),
            name: format!("job {index}"),
            schedule: "0 9 * * *".into(),
            repeat: "forever".into(),
            deliver: "local".into(),
            next_run_at: Some("2026-10-23T12:12:00Z".into()),
            last_run_at: None,
            last_status: None,
            last_error: None,
            last_delivery_error: None,
            last_fire_error: None,
            enabled: true,
            state: ["scheduled", "paused", "completed", "error", "held"][index % 5].into(),
            paused_at: None,
            paused_reason: None,
            prompt_preview: "preview".into(),
            skills: Vec::new(),
            model: None,
            provider: None,
            pinned: false,
            workdir: None,
            script: false,
            no_agent: false,
            continuity: false,
        };
        if index == 0 {
            job.state = "scheduled".into();
        }
        jobs.push(job);
    }
    update(&mut state, CronInput::Open(CronOpen::List), now, wall);
    update(
        &mut state,
        CronInput::Listed {
            seq: 1,
            result: Ok(ListReply {
                jobs,
                warning: None,
            }),
        },
        now,
        wall,
    );
    let mut samples = Vec::new();
    for _ in 0..200 {
        let sample = Instant::now();
        update(&mut state, CronInput::Key(CronKey::Down), sample, wall);
        let shown = view(&state, wall);
        let area = Rect::new(0, 0, 160, 48);
        let mut buf = Buffer::empty(area);
        draw_cron(&shown, area, &mut buf);
        samples.push(sample.elapsed());
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    #[cfg(debug_assertions)]
    {
        println!("median_us={}", median.as_micros());
        let max = samples.iter().max().copied().unwrap_or_default();
        println!("max_us={}", max.as_micros());
    }
    #[cfg(not(debug_assertions))]
    {
        assert!(median <= Duration::from_millis(2));
        assert!(
            samples
                .iter()
                .all(|sample| *sample <= Duration::from_millis(50))
        );
    }
}
