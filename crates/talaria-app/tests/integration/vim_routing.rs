//! Startup wiring and the history-search claim for a vim composer.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use talaria_app::config::{composer_starts_with_vim, parse, vim_redo_claim};
use talaria_tui::comfort_route::{self, Focus, Route};
use talaria_tui::composer::Composer;
use talaria_tui::keys::{Action, KeyChord, KeyContext, KeyMap};
use talaria_tui::live::LiveSection;

#[test]
fn config_reaches_the_application_composer() {
    let absent = startup_rows("");
    assert!(
        absent.iter().all(|row| !row.contains("-- ")),
        "an absent vim key draws no mode segment: {absent:?}"
    );

    let enabled = startup_rows("[composer]\nvim = true\n");
    assert!(
        enabled.iter().any(|row| row.contains("-- NORMAL --")),
        "vim = true draws -- NORMAL -- before any key: {enabled:?}"
    );
}

#[test]
fn ctrl_r_claim_and_other_routes() {
    let map = KeyMap::defaults();
    let ctrl_g = KeyChord::parse("ctrl+g", KeyContext::Composer).expect("ctrl+g");
    assert_eq!(
        map.resolve(&ctrl_g.to_key(), KeyContext::Composer),
        Some(Action::OpenEditor)
    );
    assert_eq!(
        comfort_route::route(Some(Action::OpenEditor), Focus::Composer),
        Route::OpenEditor
    );

    let ctrl_r = KeyChord::parse("ctrl+r", KeyContext::Composer).expect("ctrl+r");
    assert_eq!(
        map.resolve(&ctrl_r.to_key(), KeyContext::Composer),
        Some(Action::HistorySearch)
    );
    for (focus, route) in [
        (Focus::Search, Route::Search),
        (Focus::Card, Route::Card),
        (Focus::Overlay, Route::Overlay),
        (Focus::QueueSelection, Route::QueueSelection),
    ] {
        assert_eq!(
            comfort_route::route(Some(Action::HistorySearch), focus),
            route
        );
    }

    assert!(
        !vim_redo_claim(true, true, false),
        "insert and vim off keep history search"
    );
    assert!(
        !vim_redo_claim(true, false, true),
        "a composer that is not focused keeps history search"
    );
    assert!(
        vim_redo_claim(true, true, true),
        "ctrl+r in normal or visual is redo and opens no search"
    );
}

/// The same construction the session uses: parsed settings, then the composer.
fn startup_rows(text: &str) -> Vec<String> {
    let loaded = parse(text);
    let mut composer = Composer::from_startup(
        KeyMap::defaults(),
        Vec::new(),
        composer_starts_with_vim(&loaded.config),
    );
    let section = composer.section(60, 10);
    let height = section.height(60);
    let area = Rect::new(0, 0, 60, height);
    let mut buffer = Buffer::empty(area);
    section.render(area, &mut buffer);
    (0..height)
        .map(|y| {
            (0..60)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}
