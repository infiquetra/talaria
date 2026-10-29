//! Red-run tests 8 to 10 (#231): the picker state machine.

use talaria_core::directory::export::ExportFormat;
use talaria_core::directory::rows::{DirectoryRow, RowKey, RowOrigin};
use talaria_core::directory::state::{
    ExportDialog, PickerAction, PickerEffect, PickerInput, PickerMode, PickerState, WaitingFor,
    update,
};

const PAGE: usize = 10;

fn key(profile: &str, id: &str) -> RowKey {
    RowKey {
        profile: profile.to_owned(),
        session_id: id.to_owned(),
    }
}

fn row(profile: &str, id: &str, title: &str, origin: RowOrigin) -> DirectoryRow {
    DirectoryRow {
        key: key(profile, id),
        title: title.to_owned(),
        source: Some("tui".to_owned()),
        started_at: Some(1_792_700_000.0),
        message_count: 1,
        running: false,
        model: Some("minimax-m3.1-flash".to_owned()),
        origin,
    }
}

fn opened(generation: u64, cached: Vec<DirectoryRow>) -> PickerInput {
    PickerInput::Opened {
        profiles: vec!["default".to_owned(), "fixture".to_owned()],
        foreground: Some(key("fixture", "s-1")),
        scope_note: None,
        cached,
        generation,
    }
}

fn listed(
    generation: u64,
    profile: &str,
    origin: RowOrigin,
    rows: Vec<DirectoryRow>,
) -> PickerInput {
    PickerInput::Listed {
        generation,
        profile: profile.to_owned(),
        origin,
        rows,
        skipped: 0,
    }
}

#[test]
fn rename_keeps_its_target_across_refreshes() {
    for keep_target in [false, true] {
        let mut state = PickerState::new();
        let a = row("fixture", "a", "Alpha", RowOrigin::Live);
        let mut b = row("fixture", "b", "Beta", RowOrigin::Live);
        update(&mut state, opened(1, vec![a.clone(), b.clone()]), PAGE);
        assert_eq!(state.selected().unwrap().key, a.key);
        update(&mut state, PickerInput::Action(PickerAction::Rename), PAGE);
        update(&mut state, PickerInput::Text('!'), PAGE);
        b.started_at = Some(1_800_000_000.0);
        let rows = if keep_target {
            vec![b, a.clone()]
        } else {
            vec![b]
        };
        update(
            &mut state,
            listed(1, "fixture", RowOrigin::Live, rows),
            PAGE,
        );
        let effects = update(&mut state, PickerInput::Enter, PAGE);
        if keep_target {
            assert_eq!(
                effects,
                [PickerEffect::Rename {
                    key: a.key,
                    title: "Alpha!".into()
                }]
            );
        } else {
            assert!(
                effects.is_empty(),
                "a removed session must not rename another row"
            );
            assert_eq!(state.mode(), &PickerMode::List);
            assert!(state.notice().is_some());
        }
    }
}

#[test]
fn open_list_and_failures() {
    let mut state = PickerState::new();
    let effects = update(&mut state, opened(1, Vec::new()), PAGE);
    assert_eq!(effects, [PickerEffect::List { generation: 1 }]);
    assert_eq!(state.mode(), &PickerMode::List);

    update(
        &mut state,
        listed(
            1,
            "fixture",
            RowOrigin::Live,
            vec![row("fixture", "s-1", "First", RowOrigin::Live)],
        ),
        PAGE,
    );
    update(
        &mut state,
        listed(
            1,
            "fixture",
            RowOrigin::Stored,
            vec![row("fixture", "s-2", "Second", RowOrigin::Stored)],
        ),
        PAGE,
    );
    assert_eq!(state.visible_rows().len(), 2);

    update(
        &mut state,
        PickerInput::ListFailed {
            generation: 1,
            profile: "default".to_owned(),
            origin: RowOrigin::Live,
            reason: "profile_mismatch".to_owned(),
        },
        PAGE,
    );
    assert_eq!(
        state.unavailable(),
        [(
            "default".to_owned(),
            RowOrigin::Live,
            "profile_mismatch".to_owned()
        )]
    );

    // An old generation is ignored.
    update(
        &mut state,
        listed(
            0,
            "default",
            RowOrigin::Live,
            vec![row("default", "s-old", "Old", RowOrigin::Live)],
        ),
        PAGE,
    );
    assert_eq!(state.visible_rows().len(), 2);

    // Cached rows show while the new generation lists.
    let mut cached = PickerState::new();
    update(
        &mut cached,
        opened(2, vec![row("fixture", "s-1", "First", RowOrigin::Live)]),
        PAGE,
    );
    assert_eq!(cached.visible_rows().len(), 1);

    // A reply of exactly 100 rows sets the limit notice.
    let mut limited = PickerState::new();
    update(&mut limited, opened(3, Vec::new()), PAGE);
    let hundred: Vec<DirectoryRow> = (0..100)
        .map(|n| row("fixture", &format!("s-{n}"), "Many", RowOrigin::Live))
        .collect();
    update(
        &mut limited,
        listed(3, "fixture", RowOrigin::Live, hundred),
        PAGE,
    );
    assert_eq!(
        limited.notice(),
        Some("fixture: only the newest 100 live sessions are listed.")
    );

    // Skipped rows set the row-read notice.
    let mut skipped = PickerState::new();
    update(&mut skipped, opened(4, Vec::new()), PAGE);
    update(
        &mut skipped,
        PickerInput::Listed {
            generation: 4,
            profile: "fixture".to_owned(),
            origin: RowOrigin::Live,
            rows: Vec::new(),
            skipped: 2,
        },
        PAGE,
    );
    assert_eq!(
        skipped.notice(),
        Some("2 rows from Hermes could not be read and are not shown.")
    );
}

#[test]
fn keys_modes_and_cursor() {
    let mut state = PickerState::new();
    update(&mut state, opened(1, Vec::new()), PAGE);
    update(
        &mut state,
        listed(
            1,
            "fixture",
            RowOrigin::Live,
            vec![
                row("fixture", "s-1", "alpha j", RowOrigin::Live),
                row("fixture", "s-2", "beta", RowOrigin::Live),
            ],
        ),
        PAGE,
    );
    assert_eq!(
        state.selected().map(|row| row.key.session_id.as_str()),
        Some("s-1")
    );

    // Down moves; the cursor keeps its row by key across a refresh.
    update(&mut state, PickerInput::Action(PickerAction::Down), PAGE);
    assert_eq!(
        state.selected().map(|row| row.key.session_id.as_str()),
        Some("s-2")
    );
    update(
        &mut state,
        listed(
            1,
            "fixture",
            RowOrigin::Live,
            vec![
                row("fixture", "s-1", "alpha j", RowOrigin::Live),
                row("fixture", "s-2", "beta", RowOrigin::Live),
            ],
        ),
        PAGE,
    );
    assert_eq!(
        state.selected().map(|row| row.key.session_id.as_str()),
        Some("s-2")
    );

    // Search: q and j are text, Down still moves.
    update(&mut state, PickerInput::Action(PickerAction::Search), PAGE);
    assert_eq!(state.mode(), &PickerMode::Search);
    update(&mut state, PickerInput::Text('q'), PAGE);
    update(&mut state, PickerInput::Text('j'), PAGE);
    assert_eq!(state.visible_rows().len(), 0);
    update(&mut state, PickerInput::Backspace, PAGE);
    update(&mut state, PickerInput::Backspace, PAGE);
    assert_eq!(state.visible_rows().len(), 2);
    update(&mut state, PickerInput::Action(PickerAction::Down), PAGE);
    assert_eq!(
        state.selected().map(|row| row.key.session_id.as_str()),
        Some("s-2")
    );
    update(&mut state, PickerInput::Escape, PAGE);
    assert_eq!(state.mode(), &PickerMode::List);

    // Rename editor.
    update(&mut state, PickerInput::Action(PickerAction::Rename), PAGE);
    assert!(matches!(state.mode(), PickerMode::Rename(_)));
    update(&mut state, PickerInput::Escape, PAGE);
    assert_eq!(state.mode(), &PickerMode::List);

    // Export dialog: j picks JSON.
    update(&mut state, PickerInput::Action(PickerAction::Export), PAGE);
    assert!(matches!(
        state.mode(),
        PickerMode::Export(ExportDialog {
            format: ExportFormat::Markdown,
            ..
        })
    ));
    let effects = update(
        &mut state,
        PickerInput::Action(PickerAction::FormatJson),
        PAGE,
    );
    assert!(effects.is_empty());
    assert!(matches!(
        state.mode(),
        PickerMode::Export(ExportDialog {
            format: ExportFormat::Json,
            ..
        })
    ));
    update(&mut state, PickerInput::Escape, PAGE);
    assert_eq!(state.mode(), &PickerMode::List);

    // Waiting ignores keys except Esc and Ctrl+C.
    let opened_effects = update(&mut state, PickerInput::Action(PickerAction::Open), PAGE);
    assert_eq!(opened_effects, [PickerEffect::Open(key("fixture", "s-2"))]);
    assert!(matches!(state.mode(), PickerMode::Waiting(_)));
    let ignored = update(&mut state, PickerInput::Action(PickerAction::Down), PAGE);
    assert!(ignored.is_empty());
    let closed = update(&mut state, PickerInput::Escape, PAGE);
    assert_eq!(closed, [PickerEffect::Close]);
}

#[test]
fn branch_only_on_foreground_row() {
    let mut state = PickerState::new();
    update(&mut state, opened(1, Vec::new()), PAGE);
    update(
        &mut state,
        listed(
            1,
            "fixture",
            RowOrigin::Live,
            vec![
                row("fixture", "s-1", "First", RowOrigin::Live),
                row("fixture", "s-2", "Second", RowOrigin::Live),
            ],
        ),
        PAGE,
    );
    update(&mut state, PickerInput::Action(PickerAction::Down), PAGE);
    let effects = update(&mut state, PickerInput::Action(PickerAction::Branch), PAGE);
    assert!(effects.is_empty());
    assert_eq!(
        state.notice(),
        Some("Branch works on the session you are in. Open this one first (Enter), then press b.")
    );

    // On the foreground row a branch effect follows.
    update(&mut state, PickerInput::Action(PickerAction::Up), PAGE);
    let effects = update(&mut state, PickerInput::Action(PickerAction::Branch), PAGE);
    assert_eq!(effects, [PickerEffect::Branch { title: None }]);
    assert_eq!(state.mode(), &PickerMode::Waiting(WaitingFor::Branch));
}
