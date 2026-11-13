use ratatui::{buffer::Buffer, layout::Rect};
use talaria_core::models::{BrowseCounts, BrowseView, ListScope, PickerRow, PickerView, RowKey};
use talaria_tui::picker_frame::draw_picker_frame;

fn screen(message: &str, details: &[String], refreshing: bool, scope: ListScope) -> String {
    let rows = [PickerRow {
        key: Some(RowKey::Model {
            provider: "example".into(),
            model: "reasoner-large".into(),
        }),
        text: "● reasoner-large".into(),
        badges: vec!["effort ?".into()],
        price: None,
        dim: false,
        header: false,
    }];
    let view = PickerView {
        title: "Switch model · this session",
        filter: "",
        effort: "effort: high",
        hints: "Ctrl+R reload · Ctrl+U clear · Tab view · Esc close",
        refreshing,
        rows: &rows,
        selected: Some(0),
        banner: Some(message),
        browse: Some(BrowseView {
            scope,
            counts: BrowseCounts::default(),
            details,
            details_open: false,
            details_scroll: 0,
        }),
    };
    let area = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(area);
    draw_picker_frame(&view, area, &mut buffer);
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn state_messages_keep_their_recovery_hint() {
    for (message, required) in [
        ("Loading models…", "Loading models…"),
        (
            "Refreshing models; showing cached results",
            "showing cached results",
        ),
        (
            "No models found. Set up a provider with hermes model, then press ctrl+r.",
            "hermes model",
        ),
        (
            "No matches for \"zzzz\" in All. Ctrl+U clears; Tab changes view.",
            "Ctrl+U clears",
        ),
        (
            "No starred models. Tab for All; Ctrl+F stars a model.",
            "Ctrl+F stars a model",
        ),
        (
            "No configured providers. Enter on a setup row opens /providers.",
            "/providers",
        ),
        (
            "No models listed for this provider. Ctrl+R reloads.",
            "Ctrl+R reloads",
        ),
        (
            "Could not load models. Ctrl+R retries; Escape closes.",
            "Ctrl+R retries",
        ),
        (
            "Partial load failed: remaining availability unknown. Ctrl+R retries.",
            "availability unknown",
        ),
        (
            "Offline: changes disabled. Ctrl+R retries; Esc closes.",
            "changes disabled",
        ),
        (
            "Outcome unknown; reloading. Nothing is resent automatically.",
            "Nothing is resent automatically",
        ),
        (
            "Unavailable model: this route is not available. Enter explains.",
            "Enter explains",
        ),
    ] {
        let painted = screen(
            message,
            &[],
            message.starts_with("Refreshing"),
            ListScope::All,
        );
        assert!(painted.contains(required), "message {message:?}: {painted}");
        assert!(painted.contains("Ctrl+R"), "recovery: {painted}");
        assert!(
            painted.contains("reasoner-large"),
            "cached selection remains: {painted}"
        );
    }
}

#[test]
fn effort_evidence_and_unknown_values_are_literal_details() {
    for extra in [
        "Support: unverified for this route",
        "Support: could not be read",
        "Catalogue: reasoning not advertised",
        "Requested: high; pending turn end",
        "Outcome unknown; reloading",
    ] {
        let details = [
            "Selected: Example provider / reasoner-large".into(),
            "Requested: high".into(),
            extra.into(),
            "Applied: unknown".into(),
            "Context: unknown".into(),
        ];
        let painted = screen("", &details, false, ListScope::All);
        for phrase in [
            extra,
            "Requested: high",
            "Applied: unknown",
            "Context: unknown",
        ] {
            assert!(painted.contains(phrase), "{painted}");
        }
        assert!(!painted.contains("supports high"));
        assert!(!painted.contains('$'), "no fabricated price");
    }
}

#[test]
fn active_view_chip_is_visible_without_colour() {
    for (scope, label) in [
        (ListScope::All, "All"),
        (ListScope::Starred, "Starred"),
        (ListScope::Configured, "Configured"),
        (ListScope::SetUp, "Set up"),
    ] {
        let painted = screen("", &[], false, scope);
        assert!(painted.contains(&format!("[{label}]")), "{painted}");
        for other in ["All", "Starred", "Configured", "Set up"] {
            assert!(painted.contains(other));
        }
    }
}

#[test]
fn unavailable_badge_and_reason_remain_visible() {
    let rows = [PickerRow {
        key: Some(RowKey::Model {
            provider: "example".into(),
            model: "unavailable-route".into(),
        }),
        text: "unavailable-route".into(),
        badges: vec!["unavailable".into()],
        price: None,
        dim: true,
        header: false,
    }];
    let details = ["Availability: not included in the current plan".into()];
    let view = PickerView {
        title: "Switch model",
        filter: "",
        effort: "effort: high",
        hints: "Enter explains · Esc close",
        refreshing: false,
        rows: &rows,
        selected: Some(0),
        banner: None,
        browse: Some(BrowseView {
            scope: ListScope::All,
            counts: BrowseCounts::default(),
            details: &details,
            details_open: false,
            details_scroll: 0,
        }),
    };
    let area = Rect::new(0, 0, 80, 24);
    let mut buffer = Buffer::empty(area);
    draw_picker_frame(&view, area, &mut buffer);
    let painted = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(painted.contains("unavailable"));
    assert!(painted.contains("not included in the current plan"));
    assert!(painted.contains("Enter explains"));
}
mod independent {
    use ratatui::{buffer::Buffer, layout::Rect};
    use talaria_core::models::{PickerRow, PickerView, RowKey};
    use talaria_tui::model_picker::draw_model_picker;

    #[test]
    fn every_state_preserves_its_message_and_recovery_keys() {
        for (message, recovery) in [
            ("Loading models…", "Esc"),
            ("Refreshing…", "Esc"),
            ("No model matches \"missing\".", "Ctrl+U"),
            (
                "No starred models. Tab for All; Ctrl+F stars a model.",
                "Tab",
            ),
            ("Needs setup (1)", "Enter"),
            ("No models listed for this provider", "Ctrl+R"),
            ("Could not load models", "Ctrl+R"),
            ("Provider inventory unknown; cached data stale", "Ctrl+R"),
            ("Offline; changes disabled", "Esc"),
            ("Outcome unknown; reloading", "Esc"),
            ("Unavailable model; access is not listed", "Esc"),
        ] {
            for (w, h) in [(80, 24), (120, 40)] {
                let rows = [PickerRow {
                    key: Some(RowKey::Model {
                        provider: "example-00".into(),
                        model: "reasoner-large".into(),
                    }),
                    text: "reasoner-large".into(),
                    badges: vec!["effort ?".into()],
                    price: None,
                    dim: false,
                    header: false,
                }];
                let empty = message.starts_with("Loading")
                    || message.starts_with("Could not load")
                    || message.starts_with("No model")
                    || message.starts_with("No starred");
                let view = PickerView {
                    browse: Default::default(),
                    title: "Switch model · this session",
                    filter: "",
                    effort: "effort: high",
                    hints: "Enter choose · Esc close · Ctrl+R reload · Ctrl+U clear · Tab views · F2/Ctrl+E details",
                    refreshing: message.starts_with("Refreshing"),
                    rows: if empty { &[] } else { &rows },
                    selected: if empty { None } else { Some(0) },
                    banner: Some(message),
                };
                let mut buffer = Buffer::empty(Rect::new(0, 0, w, h));
                draw_model_picker(&view, buffer.area, &mut buffer);
                let actual = (0..h)
                    .map(|y| {
                        (0..w)
                            .map(|x| buffer[(x, y)].symbol())
                            .collect::<String>()
                            .trim()
                            .to_owned()
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                assert!(actual.contains(message), "{w}x{h}: {message}: {actual}");
                assert!(actual.contains(recovery), "recovery {recovery}: {actual}");
                if empty {
                    assert!(!actual.contains("reasoner-large"));
                }
            }
        }
    }
}
