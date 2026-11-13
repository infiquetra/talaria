use crate::mock_gateway;

use talaria_app::{
    app::{AppEffect, AppMessage, AppState, InputCode, InputKey, UserAction, update},
    config,
};
use talaria_core::layout::{LayoutPreferences, LayoutState, PanelVisibility};

fn key(code: InputCode, alt: bool, ctrl: bool) -> AppMessage {
    AppMessage::Input(InputKey {
        code,
        alt,
        ctrl,
        shift: false,
        super_key: false,
        other: false,
    })
}

#[test]
fn work_panel_config_defaults_false_and_reads_booleans() {
    assert!(!config::parse("").config.ui.work_panel);
    for value in [true, false] {
        let loaded = config::parse(&format!("[ui]\nwork_panel = {value}\n"));
        assert!(loaded.notices.is_empty(), "{:?}", loaded.notices);
        assert_eq!(loaded.config.ui.work_panel, value);
    }
}

#[test]
fn invalid_panel_values_emit_one_notice_and_use_false() {
    for value in ["1", "'yes'", "[]", "{}"] {
        let loaded = config::parse(&format!("[ui]\nwork_panel = {value}\n"));
        assert!(!loaded.config.ui.work_panel);
        assert_eq!(loaded.notices.len(), 1);
        assert!(
            loaded.notices[0].contains("work_panel") && loaded.notices[0].contains("true or false"),
            "{:?}",
            loaded.notices
        );
    }
}

#[test]
fn runtime_toggles_leave_the_config_file_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let path = root.join("config.toml");
    let bytes = b"# preserve this comment\n[ui]\nwork_panel = false\n";
    std::fs::write(&path, bytes).unwrap();
    let loaded = config::load_file(&path);
    let mut state = AppState::new("0123456789abcdef");
    state.layout = LayoutState::new(
        LayoutPreferences {
            work_panel: loaded.config.ui.work_panel,
            ..Default::default()
        },
        80,
        24,
    );
    let effects = update(&mut state, key(InputCode::Char('d'), true, false));
    let enabled = state.layout.preferences.work_panel;
    let view_enabled = state.view().layout.preferences.work_panel;
    update(
        &mut state,
        AppMessage::User(UserAction::Submit("/layout panel".into())),
    );
    let disabled_by_command = !state.layout.preferences.work_panel;
    assert_eq!(&std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_dir_all(root).unwrap();
    assert!(enabled, "Alt+D enables the panel");
    assert!(effects.iter().any(|e| matches!(e, AppEffect::Redraw)));
    assert!(view_enabled);
    assert!(disabled_by_command, "the local command toggles it off");
}

#[test]
fn esc_on_the_overlay_does_not_interrupt() {
    let mut state = AppState::new("0123456789abcdef");
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["running"] = serde_json::json!(true);
    let opened = talaria_app::sessions::OpenedSession::from_snapshot(
        serde_json::from_value(snapshot).unwrap(),
    );
    update(
        &mut state,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    state.layout.preferences.work_panel = true;
    state.layout.panel = PanelVisibility::OverlayOpen;
    let effects = update(&mut state, key(InputCode::Esc, false, false));
    assert_eq!(state.layout.panel, PanelVisibility::Dismissed);
    assert!(state.layout.preferences.work_panel);
    assert!(
        effects
            .iter()
            .all(|effect| matches!(effect, AppEffect::Redraw)),
        "Esc must only dismiss the panel"
    );
}

#[test]
fn ctrl_t_still_toggles_the_checklist_in_the_workspace() {
    let mut state = AppState::new("0123456789abcdef");
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["running"] = serde_json::json!(true);
    snapshot["todo_state"] =
        serde_json::json!({"revision":1,"todos":[{"id":"1","content":"task","status":"pending"}]});
    let opened = talaria_app::sessions::OpenedSession::from_snapshot(
        serde_json::from_value(snapshot).unwrap(),
    );
    update(
        &mut state,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
    state.layout.preferences.work_panel = true;
    state.layout.panel = PanelVisibility::Docked;
    let before = state.checklist.expanded();
    let effects = update(&mut state, key(InputCode::Char('t'), false, true));
    assert_ne!(state.checklist.expanded(), before);
    assert!(state.layout.preferences.work_panel);
    assert_eq!(state.layout.panel, PanelVisibility::Docked);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::Redraw))
    );
}

#[test]
fn dock_is_suppressed_only_while_the_panel_is_visible() {
    assert!(
        talaria_core::layout::inline_subagent_dock_visible(false),
        "the inline dock stays when the panel is hidden"
    );
    assert!(
        !talaria_core::layout::inline_subagent_dock_visible(true),
        "a visible work panel suppresses the inline subagent dock"
    );
}

fn config_file(name: &str) -> (tempfile::TempDir, std::path::PathBuf, Vec<u8>) {
    let directory = tempfile::Builder::new().prefix(name).tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let bytes = b"# preserve this comment\n[ui]\nwork_panel = false\n".to_vec();
    std::fs::write(&path, &bytes).unwrap();
    (directory, path, bytes)
}

fn state_from_file(path: &std::path::Path) -> AppState {
    let loaded = config::load_file(path);
    let mut state = AppState::new("0123456789abcdef");
    state.layout = LayoutState::new(
        LayoutPreferences {
            work_panel: loaded.config.ui.work_panel,
            menu_bar: loaded.config.ui.menu_bar,
        },
        80,
        24,
    );
    state.draft = "keep this draft".into();
    state
}

fn open_running(state: &mut AppState) {
    let mut snapshot = mock_gateway::recorded_resume_snapshot();
    snapshot["running"] = serde_json::json!(true);
    let opened = talaria_app::sessions::OpenedSession::from_snapshot(
        serde_json::from_value(snapshot).unwrap(),
    );
    update(
        state,
        AppMessage::Opened {
            opened,
            notice: None,
        },
    );
}

#[test]
fn alt_s_and_alt_a_keep_their_jobs_with_the_panel_on() {
    for panel in [PanelVisibility::Docked, PanelVisibility::Hidden] {
        let mut state = AppState::new("0123456789abcdef");
        state.layout.preferences.work_panel = panel != PanelVisibility::Hidden;
        state.layout.panel = panel;
        let agents = update(&mut state, AppMessage::User(UserAction::ToggleAgents));
        assert!(
            agents
                .iter()
                .any(|effect| matches!(effect, AppEffect::Redraw)),
            "agents toggle still redraws with the panel {panel:?}"
        );
        assert_eq!(state.layout.panel, panel);

        let mut state = AppState::new("0123456789abcdef");
        state.layout.preferences.work_panel = panel != PanelVisibility::Hidden;
        state.layout.panel = panel;
        let attention = update(&mut state, AppMessage::User(UserAction::AttentionNext));
        assert!(
            attention.iter().any(|effect| matches!(
                effect,
                AppEffect::CommitLines(lines)
                    if lines.iter().any(|line| line.contains("still loading"))
            )),
            "the jump key still reports the fleet with the panel {panel:?}"
        );
        assert_eq!(state.layout.panel, panel);
    }
}

#[test]
fn menu_bar_defaults_false_and_rejects_other_values() {
    let enabled = config::parse("[ui]\nmenu_bar = true\n");
    assert!(enabled.notices.is_empty(), "{:?}", enabled.notices);
    assert!(enabled.config.ui.menu_bar);
    let disabled = config::parse("[ui]\nmenu_bar = false\n");
    assert!(disabled.notices.is_empty(), "{:?}", disabled.notices);
    assert!(!disabled.config.ui.menu_bar);
    assert!(!config::parse("").config.ui.menu_bar);
    for value in ["1", "'yes'", "[]", "{}"] {
        let loaded = config::parse(&format!("[ui]\nmenu_bar = {value}\n"));
        assert!(!loaded.config.ui.menu_bar);
        assert_eq!(loaded.notices.len(), 1, "{:?}", loaded.notices);
        assert!(
            loaded.notices[0].contains("menu_bar") && loaded.notices[0].contains("true or false"),
            "{:?}",
            loaded.notices
        );
    }
}

#[test]
fn alt_i_shows_focuses_and_hides() {
    let (_directory, path, bytes) = config_file("alt-i");
    let mut state = state_from_file(&path);
    update(&mut state, key(InputCode::Char('i'), true, false));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    assert!(state.layout.preferences.menu_bar, "Alt+I shows the menu");
    assert!(state.layout.menu_focused, "Alt+I focuses the menu");
    assert_eq!(state.draft, "keep this draft");
    update(&mut state, key(InputCode::Char('i'), true, false));
    assert!(!state.layout.preferences.menu_bar, "Alt+I hides the menu");
    assert!(!state.layout.menu_focused);
    assert_eq!(state.draft, "keep this draft");
}

#[test]
fn layout_bar_and_menu_forms() {
    let (_directory, path, bytes) = config_file("layout-bar");
    let mut state = state_from_file(&path);
    update(
        &mut state,
        AppMessage::User(UserAction::Submit("/layout bar".into())),
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(
        state.layout.preferences.menu_bar,
        "/layout bar shows the menu"
    );
    assert!(state.layout.menu_focused);
    assert_eq!(state.draft, "keep this draft");
    update(
        &mut state,
        AppMessage::User(UserAction::Submit("/layout bar".into())),
    );
    assert!(
        !state.layout.preferences.menu_bar,
        "/layout bar hides the menu"
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);

    let mut hidden = state_from_file(&path);
    update(
        &mut hidden,
        AppMessage::User(UserAction::Submit("/layout menu".into())),
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    assert!(
        !hidden.layout.preferences.menu_bar,
        "/layout menu leaves a hidden bar hidden"
    );
    assert!(
        hidden.layout.menu_transient,
        "/layout menu opens a transient menu"
    );
    assert!(hidden.layout.menu_focused);
    assert_eq!(hidden.draft, "keep this draft");

    let mut shown = AppState::new("0123456789abcdef");
    shown.layout.preferences.menu_bar = true;
    shown.draft = "keep this draft".into();
    update(
        &mut shown,
        AppMessage::User(UserAction::Submit("/layout menu".into())),
    );
    assert!(shown.layout.preferences.menu_bar);
    assert!(!shown.layout.menu_transient);
    assert!(
        shown.layout.menu_focused,
        "/layout menu focuses a shown bar"
    );
    assert_eq!(shown.draft, "keep this draft");
}

#[test]
fn esc_in_the_menu_does_not_interrupt_a_running_turn() {
    let mut state = AppState::new("0123456789abcdef");
    open_running(&mut state);
    state.draft = "keep this draft".into();
    state.layout.preferences.menu_bar = true;
    state.layout.menu_focused = true;
    state.layout.menu_dropdown = true;
    state.layout.panel = PanelVisibility::Hidden;
    let effects = update(&mut state, key(InputCode::Esc, false, false));
    assert!(!state.layout.menu_dropdown, "Esc closes the dropdown first");
    assert!(state.layout.menu_focused);
    assert!(state.layout.preferences.menu_bar);
    assert_eq!(state.draft, "keep this draft");
    assert!(
        effects
            .iter()
            .all(|effect| matches!(effect, AppEffect::Redraw)),
        "Esc in the menu must not interrupt the turn"
    );
    let effects = update(&mut state, key(InputCode::Esc, false, false));
    assert!(
        !state.layout.menu_focused,
        "the second Esc returns to the composer"
    );
    assert!(state.layout.preferences.menu_bar);
    assert!(
        effects
            .iter()
            .all(|effect| matches!(effect, AppEffect::Redraw)),
        "the second Esc must not interrupt the turn"
    );
}
