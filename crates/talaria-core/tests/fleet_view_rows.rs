//! Row text, groups, filters, the cursor and redraw for the fleet overlay.

use talaria_core::attention::NeedSummary;
use talaria_core::directory::rows::RowKey;
use talaria_core::fleet::{
    FleetRow, NeedDetail, NeedId, NeedItem, NeedSlot, Outcome, ProfileStatus, RowState,
};
use talaria_core::fleet_view::{
    FleetAction, FleetViewInput, FleetViewState, ProfileFilter, glyph, matches, row_detail,
    title_or_untitled, update,
};

const NOW: f64 = 1_792_700_000.0;

fn key(profile: &str, id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: id.to_owned(),
    }
}

fn row(profile: &str, id: &str, title: &str, state: RowState, model: Option<&str>) -> FleetRow {
    FleetRow {
        key: key(profile, id),
        title: title.to_owned(),
        model: model.map(str::to_owned),
        source: Some("tui".to_owned()),
        started_at: Some(NOW - 120.0),
        message_count: 4,
        state,
        state_since: NOW - 30.0,
        outcome: None,
        foreground: false,
        subscribed: false,
        stale: false,
        gone: false,
        unlisted: false,
        activity: None,
        cost_usd: None,
        backend: talaria_core::fleet::FleetBackend::Hermes,
        agent: None,
        occupancy: None,
        cost: None,
    }
}

fn item(row_key: &RowKey, detail: NeedDetail) -> NeedItem {
    let slot = match &detail {
        NeedDetail::Waiting => NeedSlot::Waiting,
        NeedDetail::Known(
            NeedSummary::Sudo
            | NeedSummary::Secret
            | NeedSummary::VaultCode
            | NeedSummary::VaultUnlock,
        ) => NeedSlot::Masked("m1".to_owned()),
        NeedDetail::Known(_) => NeedSlot::Prompt("p1".to_owned()),
    };
    NeedItem {
        id: NeedId {
            key: row_key.clone(),
            slot,
        },
        seq: 1,
        since: NOW - 10.0,
        detail,
        choices: vec!["allow".to_owned(), "deny".to_owned()],
        execution_generation: Some(3),
        timeout_s: None,
        exact: true,
        foreground: false,
        external_options: Vec::new(),
    }
}

fn snap(rows: Vec<FleetRow>, queue: Vec<NeedItem>) -> FleetViewInput {
    FleetViewInput::Snapshot {
        version: 1,
        rows,
        queue,
        profiles: vec![
            (
                "fixture".to_owned(),
                ProfileStatus::Listed { truncated: false },
            ),
            (
                "default".to_owned(),
                ProfileStatus::Listed { truncated: false },
            ),
        ],
        scope_note: None,
        link_note: None,
    }
}

#[test]
fn row_detail_matches_fleet_list_vocabulary() {
    let approval = row(
        "fixture",
        "s-need",
        "deploy",
        RowState::NeedsYou,
        Some("ollama-cloud/minimax-m3.1-flash"),
    );
    let approval_item = item(
        &approval.key,
        NeedDetail::Known(NeedSummary::Approval {
            command: "deploy".to_owned(),
            description: String::new(),
        }),
    );
    let question = row("fixture", "s-ask", "colour", RowState::NeedsYou, None);
    let question_item = item(
        &question.key,
        NeedDetail::Known(NeedSummary::Question("which colour".to_owned())),
    );
    let sudo = row("fixture", "s-sudo", "root", RowState::NeedsYou, None);
    let sudo_item = item(&sudo.key, NeedDetail::Known(NeedSummary::Sudo));
    let secret = row("fixture", "s-secret", "token", RowState::NeedsYou, None);
    let secret_item = item(&secret.key, NeedDetail::Known(NeedSummary::Secret));
    let waiting = row("fixture", "s-wait", "hold", RowState::NeedsYou, None);
    let waiting_item = item(&waiting.key, NeedDetail::Waiting);

    let mut working = row("fixture", "s-work", "compile", RowState::Working, None);
    working.activity = Some("compiling".to_owned());
    let plain_working = row("fixture", "s-plain", "plain", RowState::Working, None);
    let mut starting = row("fixture", "s-start", "boot", RowState::Working, None);
    starting.message_count = 0;

    let failed = row("fixture", "s-fail", "migrate", RowState::Failed, None);
    let mut completed = row("fixture", "s-ok", "done", RowState::Done, None);
    completed.outcome = Some(Outcome::Completed);
    let mut interrupted = row("fixture", "s-stop", "halt", RowState::Done, None);
    interrupted.outcome = Some(Outcome::Interrupted);
    let mut finished = row("fixture", "s-fin", "wrap", RowState::Done, None);
    finished.outcome = Some(Outcome::Unknown);
    let idle = row("fixture", "s-idle", "rest", RowState::Idle, None);
    let untitled = row("fixture", "s-empty", "", RowState::Idle, None);

    let cases: Vec<(&FleetRow, Vec<&NeedItem>, &str)> = vec![
        (&approval, vec![&approval_item], "approval: deploy"),
        (&question, vec![&question_item], "question: which colour"),
        (&sudo, vec![&sudo_item], "sudo password"),
        (&secret, vec![&secret_item], "secret"),
        (&waiting, vec![&waiting_item], "waiting for an answer"),
        (&working, vec![], "compiling"),
        (&plain_working, vec![], "working"),
        (&starting, vec![], "starting"),
        (&failed, vec![], "failed"),
        (&completed, vec![], "completed"),
        (&interrupted, vec![], "interrupted"),
        (&finished, vec![], "finished"),
        (&idle, vec![], "idle"),
    ];
    for (sample, items, want) in cases {
        assert_eq!(
            row_detail(sample, &items),
            want,
            "{}",
            sample.key.session_id
        );
    }
    assert_eq!(title_or_untitled(&untitled), "(untitled)");
    assert_eq!(title_or_untitled(&idle), "rest");
    assert_eq!(glyph(RowState::NeedsYou), '\u{25cf}');
    assert_eq!(glyph(RowState::Working), '\u{25d0}');
    assert_eq!(glyph(RowState::Failed), '\u{2717}');
    assert_eq!(glyph(RowState::Done), '\u{2713}');
    assert_eq!(glyph(RowState::Idle), '\u{00b7}');
    assert!(matches(&idle, "idle", ""));
    assert!(matches(&idle, "idle", "rest"));
    assert!(matches(&approval, "approval: deploy", "minimax"));
    assert!(matches(&approval, "approval: deploy", "s-need"));
    assert!(!matches(&idle, "idle", "ZZZ"));
}

#[test]
fn visible_groups_order_counts_and_omit_empty() {
    let rows = vec![
        row("fixture", "s-idle", "rest", RowState::Idle, None),
        row("default", "s-work", "compile", RowState::Working, None),
        row("fixture", "s-need", "deploy", RowState::NeedsYou, None),
        row("fixture", "s-done", "ship", RowState::Done, None),
    ];
    let mut state = FleetViewState::new(None, NOW);
    update(&mut state, snap(rows, Vec::new()), 8);
    let groups = state.visible();
    let order: Vec<_> = groups.iter().map(|group| group.state).collect();
    assert_eq!(
        order,
        vec![
            RowState::NeedsYou,
            RowState::Working,
            RowState::Done,
            RowState::Idle
        ]
    );
    assert!(groups.iter().all(|group| !group.rows.is_empty()));
    assert_eq!(
        groups.iter().map(|group| group.rows.len()).sum::<usize>(),
        4
    );
    let failed = groups.iter().find(|group| group.state == RowState::Failed);
    assert!(failed.is_none(), "an empty failed group is omitted");
}

#[test]
fn filter_and_profile_cycle() {
    let rows = vec![
        row(
            "fixture",
            "s-need",
            "deploy",
            RowState::NeedsYou,
            Some("ollama-cloud/minimax-m3.1-flash"),
        ),
        row(
            "default",
            "s-work",
            "compile",
            RowState::Working,
            Some("local/quiet"),
        ),
    ];
    let mut state = FleetViewState::new(None, NOW);
    update(&mut state, snap(rows, Vec::new()), 8);
    update(&mut state, FleetViewInput::Action(FleetAction::Filter), 8);
    update(&mut state, FleetViewInput::Text('d'), 8);
    update(&mut state, FleetViewInput::Text('e'), 8);
    // "de" also matches the profile "default"; "dep" names only the deploy row.
    update(&mut state, FleetViewInput::Text('p'), 8);
    assert_eq!(
        state.visible().iter().flat_map(|group| &group.rows).count(),
        1
    );
    update(&mut state, FleetViewInput::Backspace, 8);
    update(&mut state, FleetViewInput::Enter, 8);
    assert!(matches!(
        state.focus(),
        talaria_core::fleet_view::Focus::List
    ));
    update(&mut state, FleetViewInput::Action(FleetAction::Filter), 8);
    update(&mut state, FleetViewInput::Escape, 8);
    assert_eq!(
        state.visible().iter().flat_map(|group| &group.rows).count(),
        2
    );

    update(
        &mut state,
        FleetViewInput::Action(FleetAction::ProfileNext),
        8,
    );
    update(
        &mut state,
        FleetViewInput::Action(FleetAction::ProfileNext),
        8,
    );
    update(
        &mut state,
        FleetViewInput::Action(FleetAction::ProfileNext),
        8,
    );
    update(
        &mut state,
        FleetViewInput::Action(FleetAction::ProfilePrev),
        8,
    );
    let _ = ProfileFilter::All;
    let gone = FleetViewInput::Snapshot {
        version: 2,
        rows: vec![row("fixture", "s-need", "deploy", RowState::NeedsYou, None)],
        queue: Vec::new(),
        profiles: vec![(
            "fixture".to_owned(),
            ProfileStatus::Listed { truncated: false },
        )],
        scope_note: None,
        link_note: None,
    };
    update(&mut state, gone, 8);
    assert_eq!(state.visible().len(), 1);
}

#[test]
fn cursor_follows_its_session() {
    let first = row("fixture", "s-need", "deploy", RowState::NeedsYou, None);
    let second = row("fixture", "s-work", "compile", RowState::Working, None);
    let third = row("fixture", "s-idle", "rest", RowState::Idle, None);
    let mut state = FleetViewState::new(Some(second.key.clone()), NOW);
    update(
        &mut state,
        snap(
            vec![second.clone(), third.clone(), first.clone()],
            Vec::new(),
        ),
        2,
    );
    assert_eq!(
        state.cursor().map(|item| item.session_id.as_str()),
        Some("s-need")
    );
    update(&mut state, FleetViewInput::Action(FleetAction::Down), 2);
    assert_eq!(
        state.cursor().map(|item| item.session_id.as_str()),
        Some("s-work")
    );
    update(&mut state, FleetViewInput::Action(FleetAction::Down), 2);
    update(&mut state, FleetViewInput::Action(FleetAction::Down), 2);
    assert_eq!(
        state.cursor().map(|item| item.session_id.as_str()),
        Some("s-idle")
    );
    update(&mut state, FleetViewInput::Action(FleetAction::PageUp), 2);
    update(&mut state, FleetViewInput::Action(FleetAction::Top), 2);
    assert_eq!(
        state.cursor().map(|item| item.session_id.as_str()),
        Some("s-need")
    );
    update(&mut state, FleetViewInput::Action(FleetAction::Bottom), 2);
    assert_eq!(
        state.cursor().map(|item| item.session_id.as_str()),
        Some("s-idle")
    );

    let moved = FleetViewInput::Snapshot {
        version: 2,
        rows: vec![third.clone(), second.clone()],
        queue: Vec::new(),
        profiles: vec![(
            "fixture".to_owned(),
            ProfileStatus::Listed { truncated: false },
        )],
        scope_note: None,
        link_note: None,
    };
    update(&mut state, moved, 2);
    assert_eq!(
        state.cursor().map(|item| item.session_id.as_str()),
        Some("s-idle")
    );
}

#[test]
fn unchanged_snapshot_needs_no_frame() {
    let rows = vec![row("fixture", "s-idle", "rest", RowState::Idle, None)];
    let mut state = FleetViewState::new(None, NOW);
    update(&mut state, snap(rows.clone(), Vec::new()), 8);
    assert!(state.needs_redraw());
    state.take_frame();
    assert!(!state.needs_redraw());
    update(&mut state, snap(rows, Vec::new()), 8);
    assert!(
        !state.needs_redraw(),
        "an identical snapshot does not need a frame"
    );
    update(&mut state, FleetViewInput::Tick { now: NOW + 10.0 }, 8);
    assert!(
        !state.needs_redraw(),
        "an age that is still in the same minute does not need a frame"
    );
    update(&mut state, FleetViewInput::Tick { now: NOW + 120.0 }, 8);
    assert!(
        state.needs_redraw(),
        "a minute change of an age needs one frame"
    );
}
