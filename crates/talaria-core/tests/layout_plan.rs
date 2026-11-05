use talaria_core::layout::{LayoutPreferences, LayoutState, PanelVisibility};

fn enabled(columns: u16, rows: u16) -> LayoutState {
    LayoutState::new(
        LayoutPreferences {
            work_panel: true,
            ..Default::default()
        },
        columns,
        rows,
    )
}

#[test]
fn preference_pair_defaults_to_inline() {
    let preferences = LayoutPreferences::default();
    assert!(!preferences.work_panel && !preferences.menu_bar);
    let state = LayoutState::new(preferences, 120, 40);
    assert!(!state.workspace_enabled());
    assert_eq!(state.panel, PanelVisibility::Hidden);
    assert_eq!(state.plan(120, 40).panel_columns, 0);
}

#[test]
fn docking_thresholds_are_inclusive_and_compact_startup_is_dismissed() {
    for (columns, rows, expected) in [
        (111, 28, PanelVisibility::Dismissed),
        (112, 28, PanelVisibility::Docked),
        (112, 27, PanelVisibility::Dismissed),
        (112, 16, PanelVisibility::Dismissed),
        (80, 24, PanelVisibility::Dismissed),
        (120, 40, PanelVisibility::Docked),
        (100, 15, PanelVisibility::Dismissed),
        (59, 28, PanelVisibility::Dismissed),
    ] {
        let state = enabled(columns, rows);
        assert!(
            state.workspace_enabled(),
            "enabled preference at {columns}x{rows}"
        );
        assert_eq!(state.panel, expected, "{columns}x{rows}");
        let plan = state.plan(columns, rows);
        let allocation = if expected == PanelVisibility::Docked {
            33
        } else {
            0
        };
        assert_eq!(plan.panel_columns, allocation, "{columns}x{rows}");
        assert_eq!(plan.transcript_columns, columns - allocation);
        assert_eq!(plan.menu_rows, 0, "panel alone must not reserve a menu row");
    }
}

#[test]
fn panel_transitions() {
    let mut state = LayoutState::new(LayoutPreferences::default(), 80, 24);
    state.toggle_panel(80, 24);
    assert!(state.preferences.work_panel);
    assert_eq!(state.panel, PanelVisibility::OverlayOpen);
    assert!(state.dismiss_panel());
    assert!(state.preferences.work_panel);
    assert_eq!(state.panel, PanelVisibility::Dismissed);
    state.resize(90, 25);
    assert_eq!(
        state.panel,
        PanelVisibility::Dismissed,
        "resize must not reopen it"
    );
    state.toggle_panel(90, 25);
    assert_eq!(state.panel, PanelVisibility::OverlayOpen);
    state.toggle_panel(90, 25);
    assert!(!state.preferences.work_panel);
    assert_eq!(state.panel, PanelVisibility::Hidden);
    state.toggle_panel(112, 28);
    assert_eq!(state.panel, PanelVisibility::Docked);
    assert!(!state.dismiss_panel(), "Esc has no panel job while docked");
    state.resize(111, 28);
    assert_eq!(state.panel, PanelVisibility::Dismissed);
    state.resize(112, 28);
    assert_eq!(state.panel, PanelVisibility::Docked);
    state.toggle_panel(112, 28);
    assert!(!state.workspace_enabled());
}

#[test]
fn short_terminal_reserves_no_space_even_after_an_explicit_action() {
    let mut state = LayoutState::new(LayoutPreferences::default(), 100, 15);
    state.toggle_panel(100, 15);
    let plan = state.plan(100, 15);
    assert!(state.workspace_enabled());
    assert_eq!(
        (plan.transcript_columns, plan.panel_columns, plan.menu_rows),
        (100, 0, 0)
    );
}
