mod support;

use std::time::Duration;
use support::{ByteSink, Vt, probe};
use talaria_core::doctor::{
    Check, CheckId, DoctorView, Report, RestartOutcome, RestartState, Severity, WorkSummary,
};
use talaria_tui::{
    doctor_view::draw_doctor,
    live::FrameStats,
    terminal::{InlineTerminal, RestoreLink},
};
use unicode_width::UnicodeWidthStr;

fn report() -> Report {
    let rows = [
        (CheckId::Talaria, Severity::Ok, "client ready", None),
        (
            CheckId::Gateway,
            Severity::Warn,
            "starting",
            Some("Wait a few seconds and check again."),
        ),
        (
            CheckId::HermesVersion,
            Severity::Fail,
            "incompatible",
            Some("Update Hermes."),
        ),
        (CheckId::Capabilities, Severity::Skip, "not checked", None),
        (CheckId::Profiles, Severity::Info, "profiles known", None),
        (CheckId::Attach, Severity::Ok, "attached", None),
        (CheckId::RuntimeHealth, Severity::Ok, "healthy", None),
    ];
    Report {
        profile: "fixture".into(),
        checks: rows
            .into_iter()
            .map(|(id, severity, summary, fix)| Check {
                id,
                severity,
                summary: summary.into(),
                details: vec!["additional detail".into()],
                fix: fix.map(str::to_owned),
            })
            .collect(),
    }
}

fn run_at_size(cols: u16, rows: u16) {
    let sink = ByteSink::default();
    let mut term = InlineTerminal::new(
        sink.clone(),
        (cols, rows),
        2,
        probe(false, false, Some((0, rows - 1))),
        RestoreLink::detached(),
    )
    .unwrap();
    let mut vt = Vt::new(cols, rows);
    let mut parser = vt100::Parser::new(rows, cols, 0);
    let bytes = vt.absorb(&sink);
    parser.process(&bytes);
    term.open_overlay().unwrap();
    let bytes = vt.absorb(&sink);
    parser.process(&bytes);
    let mut view = DoctorView::new();
    let mut state = RestartState::Idle;
    let paint = |term: &mut InlineTerminal<ByteSink>,
                 vt: &mut Vt,
                 parser: &mut vt100::Parser,
                 view: &DoctorView,
                 state: &RestartState| {
        term.draw_overlay(|area, buf| draw_doctor(view, state, area, buf))
            .unwrap();
        let bytes = vt.absorb(&sink);
        parser.process(&bytes);
        vt.rows().join("\n")
    };
    let screen = paint(&mut term, &mut vt, &mut parser, &view, &state);
    assert!(screen.contains("Checking the gateway…"), "{screen}");
    assert!(!screen.contains("Doctor:"));
    view.running = false;
    view.report = Some(report());
    let screen = paint(&mut term, &mut vt, &mut parser, &view, &state);
    for expected in [
        "Doctor: 3 ok, 1 warning, 1 failure",
        "ok",
        "warn",
        "FAIL",
        "skip",
        "info",
        "client ready",
        "starting",
        "incompatible",
        "not checked",
        "profiles known",
        "attached",
        "healthy",
        "Fix:",
        "Update Hermes.",
        "For provider keys, tools",
        "hermes doctor.",
    ] {
        assert!(screen.contains(expected), "missing {expected}: {screen}");
    }
    assert!(!screen.contains("additional detail"));
    view.expanded.insert(CheckId::Talaria);
    let screen = paint(&mut term, &mut vt, &mut parser, &view, &state);
    assert!(screen.contains("additional detail"));
    state = RestartState::Asking {
        old_instance: "old-instance".into(),
        profiles: vec!["default".into(), "fixture".into()],
    };
    let screen = paint(&mut term, &mut vt, &mut parser, &view, &state);
    assert!(screen.contains("Asking the gateway what is running…"));
    state = RestartState::Confirming {
        old_instance: "old-instance".into(),
        profiles: vec!["default".into(), "fixture".into()],
        work: Some(WorkSummary { chat: 2, other: 1 }),
        restart_selected: false,
    };
    let screen = paint(&mut term, &mut vt, &mut parser, &view, &state);
    for expected in [
        "Restart the Hermes gateway?",
        "default, fixture",
        "3 running jobs",
        "2 chat turns",
        "1 other job",
        "Cancel",
        "Restart",
    ] {
        assert!(screen.contains(expected), "missing {expected}: {screen}");
    }
    let lines = vt.rows();
    let row = lines
        .iter()
        .position(|line| line.contains("Cancel"))
        .expect("cancel row");
    let byte = lines[row].find("Cancel").unwrap();
    let col = UnicodeWidthStr::width(&lines[row][..byte]) as u16;
    assert!(
        parser.screen().cell(row as u16, col).unwrap().inverse(),
        "Cancel is selected"
    );
    state = RestartState::Waiting {
        old_instance: "old-instance".into(),
        elapsed: Duration::from_secs(3),
        unknown_request: false,
    };
    let screen = paint(&mut term, &mut vt, &mut parser, &view, &state);
    assert!(screen.contains("Restart requested."), "{screen}");
    assert!(screen.contains("gateway to come back"), "{screen}");
    assert!(screen.contains("(3 s)"), "{screen}");
    state = RestartState::Done(RestartOutcome::Declined);
    let screen = paint(&mut term, &mut vt, &mut parser, &view, &state);
    assert!(screen.contains("Not restarted."));
    let frame = term
        .draw_overlay(|area, buf| draw_doctor(&view, &state, area, buf))
        .unwrap();
    assert_eq!(
        frame,
        FrameStats {
            drew: false,
            bytes: 0
        }
    );
    term.close_overlay().unwrap();
    vt.absorb(&sink);
    assert!(!vt.alternate());
    assert!(!vt.rows().join("\n").contains("Not restarted."));
}

#[test]
fn draws_every_state() {
    run_at_size(100, 30);
    run_at_size(60, 30);
}
