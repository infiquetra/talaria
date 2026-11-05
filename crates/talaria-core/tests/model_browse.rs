#[path = "support/model_drive.rs"]
mod drive;

use drive::{SID, open_loaded, press, selected_key, type_filter};
use hermes_contract::schemas::ModelOptionsResult;
use serde_json::json;
use talaria_core::models::{
    ListScope, ModelsEffect, ModelsInput, ModelsState, PickerKey, RowKey, SavedModels,
};
use talaria_core::mutation::MutationOutcome;

fn catalog() -> ModelOptionsResult {
    serde_json::from_value(json!({"provider":"a","model":"shared","providers":[
        {"slug":"a","name":"Alpha Provider","authenticated":true,"models":["shared","quiet","blocked"],
         "capabilities":{"quiet":{"reasoning":false,"fast":true}},"unavailable_models":["blocked"],
         "pricing":{"shared":{"input":"$1/M","output":"$2/M","free":false}}},
        {"slug":"b","name":"Beta Provider","authenticated":true,"models":["shared","other"]},
        {"slug":"c","name":"Setup Provider","authenticated":false,"models":["cached-hidden"],"key_env":"EXAMPLE_KEY"}
    ]})).unwrap()
}
fn starred() -> SavedModels {
    let mut saved = SavedModels::empty();
    saved.favourites = vec!["b::other".into(), "a::shared".into(), "a::missing".into()];
    saved
}
fn state() -> ModelsState {
    open_loaded(catalog(), Some("high"), starred())
}
fn route(key: &RowKey) -> (&str, &str) {
    match key {
        RowKey::Model { provider, model } | RowKey::Favourite { provider, model } => {
            (provider, model)
        }
        RowKey::NotSetUp { provider } => (provider, ""),
    }
}
fn routes(state: &ModelsState) -> Vec<(String, String)> {
    state
        .view()
        .unwrap()
        .rows
        .iter()
        .filter_map(|r| r.key.as_ref())
        .map(|key| {
            let (p, m) = route(key);
            (p.into(), m.into())
        })
        .collect()
}
fn details(state: &ModelsState) -> String {
    state
        .view()
        .unwrap()
        .browse
        .expect("browse facts")
        .details
        .join("\n")
}

#[test]
fn all_groups_starred_once_and_current_independent() {
    let mut s = state();
    assert_eq!(
        routes(&s),
        [
            ("b", "other"),
            ("a", "shared"),
            ("a", "missing"),
            ("a", "quiet"),
            ("a", "blocked"),
            ("b", "shared"),
            ("c", "")
        ]
        .map(|(p, m)| (p.into(), m.into()))
    );
    assert_eq!(route(&selected_key(&s).unwrap()), ("a", "shared"));
    let view = s.view().unwrap();
    let headers: Vec<_> = view
        .rows
        .iter()
        .filter(|r| r.header)
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(
        headers,
        [
            "Starred",
            "Alpha Provider",
            "Beta Provider",
            "Needs setup (1)"
        ]
    );
    assert_eq!(view.rows.iter().filter(|r| r.text.contains('●')).count(), 1);
    press(&mut s, PickerKey::Home);
    assert_eq!(
        s.view()
            .unwrap()
            .rows
            .iter()
            .filter(|r| r.text.contains('●'))
            .count(),
        1
    );
    assert!(!routes(&s).iter().any(|(_, m)| m == "cached-hidden"));
}
#[test]
fn scopes_counts_search_and_reload_keep_identity() {
    let mut s = state();
    let b = s.view().unwrap().browse.unwrap();
    assert_eq!(
        (
            b.counts.models,
            b.counts.providers,
            b.counts.starred,
            b.counts.configured,
            b.counts.setup
        ),
        (5, 3, 3, 2, 1)
    );
    for (key, scope, count) in [
        (PickerKey::ChipsNext, ListScope::Starred, 3),
        (PickerKey::ChipsNext, ListScope::Configured, 5),
        (PickerKey::ChipsNext, ListScope::SetUp, 1),
        (PickerKey::ChipsNext, ListScope::All, 7),
        (PickerKey::ChipsPrev, ListScope::SetUp, 1),
    ] {
        press(&mut s, key);
        assert_eq!(s.view().unwrap().browse.unwrap().scope, scope);
        assert_eq!(routes(&s).len(), count);
    }
    press(&mut s, PickerKey::ChipsNext);
    type_filter(&mut s, "AlPhA SHARED");
    assert_eq!(routes(&s), [("a".into(), "shared".into())]);
    let identity = selected_key(&s);
    press(&mut s, PickerKey::ChipsNext);
    assert_eq!(s.view().unwrap().filter, "AlPhA SHARED");
    press(&mut s, PickerKey::Refresh);
    s.update(ModelsInput::OptionsLoaded {
        session_id: SID.into(),
        result: catalog(),
    });
    assert_eq!(s.view().unwrap().browse.unwrap().scope, ListScope::Starred);
    assert_eq!(selected_key(&s), identity);
    press(&mut s, PickerKey::ClearFilter);
    type_filter(&mut s, "missing");
    assert_eq!(route(&selected_key(&s).unwrap()), ("a", "missing"));
}
#[test]
fn provider_matches_keep_children_and_headers() {
    let mut s = state();
    type_filter(&mut s, "Beta");
    assert_eq!(
        routes(&s),
        [("b".into(), "other".into()), ("b".into(), "shared".into())]
    );
    press(&mut s, PickerKey::ClearFilter);
    type_filter(&mut s, "quiet");
    assert_eq!(s.view().unwrap().rows[0].text, "Alpha Provider");
    assert_eq!(route(&selected_key(&s).unwrap()), ("a", "quiet"));
    for key in [
        PickerKey::Up,
        PickerKey::Down,
        PickerKey::Home,
        PickerKey::End,
    ] {
        press(&mut s, key);
        assert!(
            s.view().unwrap().rows[s.view().unwrap().selected.unwrap()]
                .key
                .is_some()
        );
    }
    press(&mut s, PickerKey::ClearFilter);
    type_filter(&mut s, "shared");
    assert_eq!(routes(&s).len(), 2);
}
#[test]
fn starring_preserves_route_even_when_its_row_variant_changes() {
    let mut s = open_loaded(catalog(), Some("high"), SavedModels::empty());
    let before = selected_key(&s).unwrap();
    press(&mut s, PickerKey::ToggleFavourite);
    assert_eq!(route(&before), route(&selected_key(&s).unwrap()));
    press(&mut s, PickerKey::ToggleFavourite);
    assert_eq!(route(&before), route(&selected_key(&s).unwrap()));
}
#[test]
fn badge_priority() {
    let mut result = catalog();
    result.providers[0].unavailable_models = Some(vec!["quiet".into()]);
    let mut s = open_loaded(result, Some("high"), starred());
    let view = s.view().unwrap();
    for (model, badge) in [
        ("missing", "not listed"),
        ("quiet", "unavailable"),
        ("shared", "effort ?"),
    ] {
        let r = view
            .rows
            .iter()
            .find(|r| r.key.as_ref().is_some_and(|k| route(k) == ("a", model)))
            .unwrap();
        assert_eq!(r.badges, [badge]);
    }
    s.update(ModelsInput::OptionsLoaded {
        session_id: SID.into(),
        result: catalog(),
    });
    type_filter(&mut s, "quiet");
    assert_eq!(
        s.view().unwrap().rows.last().unwrap().badges,
        ["not offered"]
    );
    assert!(details(&s).contains("Catalogue: reasoning not advertised"));
    let mut limited = catalog();
    limited.providers[0].limit = Some(
        serde_json::from_value(
            json!({"scope":"models","models":{"shared":"2026-11-04T20:00:00Z"}}),
        )
        .unwrap(),
    );
    s.update(ModelsInput::OptionsLoaded {
        session_id: SID.into(),
        result: limited,
    });
    press(&mut s, PickerKey::ClearFilter);
    type_filter(&mut s, "alpha shared");
    assert!(s.view().unwrap().rows.last().unwrap().badges[0].starts_with("rate-limited"));
}
#[test]
fn effort_details_never_invent_support_context_or_price() {
    let mut s = state();
    let d = details(&s);
    for text in [
        "Requested: high",
        "Support: unverified for this route",
        "Applied: unknown",
        "Context: unknown",
        "$1/M",
    ] {
        assert!(d.contains(text), "{d}");
    }
    assert!(!d.contains("supports high"));
    press(&mut s, PickerKey::ClearFilter);
    type_filter(&mut s, "quiet");
    assert!(details(&s).contains("Catalogue: reasoning not advertised"));
    assert!(!details(&s).contains("Price:"));
    s.update(ModelsInput::EffortReadFailed {
        session_id: SID.into(),
        reason: "failed".into(),
        unknown_method: false,
    });
    assert!(details(&s).contains("Support: could not be read"));
}
#[test]
fn details_layer_never_confirms() {
    let mut s = state();
    type_filter(&mut s, "shared");
    let selected = selected_key(&s);
    press(&mut s, PickerKey::Details);
    assert!(s.view().unwrap().browse.unwrap().details_open);
    for key in [
        PickerKey::Enter,
        PickerKey::ToggleFavourite,
        PickerKey::ToggleScope,
        PickerKey::ChipsNext,
        PickerKey::Refresh,
        PickerKey::Right,
        PickerKey::Char('x'),
        PickerKey::ClearFilter,
        PickerKey::Down,
        PickerKey::PageDown,
        PickerKey::End,
        PickerKey::Close,
    ] {
        assert!(
            press(&mut s, key).is_empty(),
            "details emitted effects for {key:?}"
        );
        assert_eq!(selected_key(&s), selected);
        assert_eq!(s.view().unwrap().filter, "shared");
    }
    assert!(s.view().unwrap().browse.unwrap().details_scroll > 0);
    assert!(press(&mut s, PickerKey::Escape).is_empty());
    assert!(!s.view().unwrap().browse.unwrap().details_open);
    assert!(press(&mut s, PickerKey::Escape).is_empty());
    assert_eq!(s.view().unwrap().filter, "");
    assert_eq!(
        press(&mut s, PickerKey::Escape),
        [ModelsEffect::CloseOverlay]
    );
}
#[test]
fn loading_has_no_selectable_placeholder_and_failures_keep_rows() {
    let mut s = ModelsState::new("L", Ok(SavedModels::empty()));
    s.update(ModelsInput::OpenPicker {
        session_id: Some(SID.into()),
        profile: "example".into(),
    });
    assert!(s.view().unwrap().rows.is_empty());
    assert!(s.view().unwrap().banner.unwrap().contains("Loading models"));
    s.update(ModelsInput::OptionsLoaded {
        session_id: SID.into(),
        result: catalog(),
    });
    let before = selected_key(&s);
    press(&mut s, PickerKey::Refresh);
    s.update(ModelsInput::OptionsFailed {
        session_id: SID.into(),
        reason: "network".into(),
        unknown_method: false,
    });
    assert_eq!(selected_key(&s), before);
    assert!(!routes(&s).is_empty());
    assert!(s.view().unwrap().banner.unwrap().contains("stale"));
}
#[test]
fn offline_and_unknown_disable_new_changes_without_resending() {
    let mut s = state();
    s.update(ModelsInput::Connection { connected: false });
    let selected = selected_key(&s);
    for key in [
        PickerKey::Enter,
        PickerKey::ToggleFavourite,
        PickerKey::ToggleScope,
        PickerKey::Right,
    ] {
        assert!(press(&mut s, key).is_empty());
    }
    assert_eq!(selected_key(&s), selected);
    assert!(s.view().unwrap().banner.unwrap().contains("Offline"));
    assert!(
        s.update(ModelsInput::Connection { connected: true })
            .is_empty()
    );
    type_filter(&mut s, "quiet");
    let request = drive::one_mutate(&press(&mut s, PickerKey::Enter));
    s.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Unknown,
    });
    s.update(ModelsInput::OpenPicker {
        session_id: Some(SID.into()),
        profile: "example".into(),
    });
    assert!(details(&s).contains("Outcome unknown; reloading"));
    assert!(press(&mut s, PickerKey::Enter).is_empty());
    assert!(
        s.update(ModelsInput::OptionsLoaded {
            session_id: SID.into(),
            result: catalog()
        })
        .is_empty()
    );
    assert!(
        press(&mut s, PickerKey::Enter).is_empty(),
        "effort readback is still pending"
    );
    s.update(ModelsInput::EffortReadFailed {
        session_id: SID.into(),
        reason: "timeout".into(),
        unknown_method: false,
    });
    assert!(details(&s).contains("Outcome unknown; reloading"));
    assert!(press(&mut s, PickerKey::Enter).is_empty());
    assert!(
        press(&mut s, PickerKey::Refresh)
            .iter()
            .any(|effect| matches!(effect, ModelsEffect::ReadEffort { .. }))
    );
    assert!(
        s.update(ModelsInput::EffortRead {
            session_id: SID.into(),
            value: Some("high".into())
        })
        .is_empty()
    );
    assert!(!details(&s).contains("Outcome unknown; reloading"));
    type_filter(&mut s, "quiet");
    assert_eq!(
        drive::mutates(&press(&mut s, PickerKey::Enter)).len(),
        1,
        "an explicit new choice works after readback"
    );
}
#[test]
fn deferred_request_is_marked_pending() {
    let mut s = state();
    type_filter(&mut s, "quiet");
    let request = drive::one_mutate(&press(&mut s, PickerKey::Enter));
    s.update(ModelsInput::MutationReturned {
        request,
        outcome: MutationOutcome::Deferred(json!({"model":"quiet","status":"deferred"})),
    });
    s.update(ModelsInput::OpenPicker {
        session_id: Some(SID.into()),
        profile: "example".into(),
    });
    assert!(details(&s).contains("Requested: high; pending turn end"));
    s.update(ModelsInput::SessionEvent { session_id: SID.into(), type_name: "session.updated".into(), payload: json!({"operation":"model","model":"quiet","provider":"a","revision":13,"execution_generation":5}) });
    assert!(!details(&s).contains("pending turn end"));
    assert!(
        s.view()
            .unwrap()
            .rows
            .iter()
            .any(|row| row.text.contains('●') && row.text.contains("quiet"))
    );
}

#[test]
fn details_scroll_clamps_to_the_reported_physical_viewport() {
    let mut s = state();
    s.set_details_scroll_limit(Some(3));
    press(&mut s, PickerKey::Details);
    for _ in 0..100 {
        press(&mut s, PickerKey::Down);
    }
    assert_eq!(s.view().unwrap().browse.unwrap().details_scroll, 3);
    press(&mut s, PickerKey::Up);
    assert_eq!(s.view().unwrap().browse.unwrap().details_scroll, 2);
    press(&mut s, PickerKey::End);
    press(&mut s, PickerKey::Up);
    assert_eq!(s.view().unwrap().browse.unwrap().details_scroll, 2);
    s.set_details_scroll_limit(Some(1));
    assert_eq!(s.view().unwrap().browse.unwrap().details_scroll, 1);
}
#[test]
fn large_catalogue_windows_and_counts() {
    let providers:Vec<_>=(0..20).map(|p|json!({"slug":format!("p{p}"),"name":format!("Provider {p}"),"authenticated":true,"models":(0..30).map(|m|format!("model-{m:02}")).collect::<Vec<_>>()})).collect();
    let catalog =
        serde_json::from_value(json!({"provider":"p10","model":"model-15","providers":providers}))
            .unwrap();
    let mut s = open_loaded(catalog, Some("high"), SavedModels::empty());
    assert_eq!(s.view().unwrap().browse.unwrap().counts.models, 600);
    assert_eq!(s.view().unwrap().browse.unwrap().counts.providers, 20);
    for height in [9, 32] {
        s.set_list_height(height);
        press(&mut s, PickerKey::Home);
        assert_eq!(route(&selected_key(&s).unwrap()), ("p0", "model-00"));
        for _ in 0..100 {
            press(&mut s, PickerKey::PageDown);
        }
        assert_eq!(route(&selected_key(&s).unwrap()), ("p19", "model-29"));
        for _ in 0..100 {
            press(&mut s, PickerKey::PageUp);
        }
        assert_eq!(route(&selected_key(&s).unwrap()), ("p0", "model-00"));
        press(&mut s, PickerKey::End);
        assert_eq!(route(&selected_key(&s).unwrap()), ("p19", "model-29"));
    }
    type_filter(&mut s, "p19 model-29");
    assert_eq!(routes(&s).len(), 1);
}
#[path = "support/catalogue.rs"]
mod catalogue;

mod independent {
    use super::{catalogue, drive};

    use serde_json::{Value, json};
    use talaria_core::models::ListScope;
    use talaria_core::models::{
        ModelsEffect, ModelsInput, ModelsState, PickerKey, RowKey, SavedModels,
    };

    fn fixture() -> hermes_contract::schemas::ModelOptionsResult {
        catalogue::parse(
            json!({"model":"reasoner-large","provider":"example-00","providers":[
                {"slug":"example-00","name":"Example provider 00","authenticated":true,"models":["reasoner-large","chat-wide"],"capabilities":{"reasoner-large":{"fast":true,"reasoning":true},"chat-wide":{"fast":false,"reasoning":false}}},
                {"slug":"example-01","name":"Example provider 01","authenticated":true,"models":["reasoner-large"]},
                {"slug":"example-02","name":"Example provider 02","authenticated":false,"models":["cached-not-selectable"]}
            ]}),
        )
    }
    fn saved() -> SavedModels {
        let mut saved = SavedModels::empty();
        saved.favourites = vec![
            "example-01::reasoner-large".into(),
            "example-00::reasoner-large".into(),
        ];
        saved
    }
    fn open() -> ModelsState {
        drive::open_loaded(fixture(), Some("high"), saved())
    }
    fn route(key: &RowKey) -> (String, String) {
        match key {
            RowKey::Favourite { provider, model } | RowKey::Model { provider, model } => {
                (provider.clone(), model.clone())
            }
            RowKey::NotSetUp { provider } => (provider.clone(), String::new()),
        }
    }
    fn routes(state: &ModelsState) -> Vec<(String, String)> {
        state
            .view()
            .unwrap()
            .rows
            .iter()
            .filter_map(|row| row.key.as_ref().map(route))
            .collect()
    }

    #[test]
    fn all_groups_starred_routes_once_in_starring_order_and_setup_last() {
        let state = open();
        let routes = routes(&state);
        assert_eq!(
            routes,
            vec![
                ("example-01".into(), "reasoner-large".into()),
                ("example-00".into(), "reasoner-large".into()),
                ("example-00".into(), "chat-wide".into()),
                ("example-02".into(), String::new())
            ]
        );
        let view = state.view().unwrap();
        assert!(
            view.rows
                .iter()
                .any(|r| r.header && r.text == "Needs setup (1)")
        );
        assert!(
            !view
                .rows
                .iter()
                .any(|r| r.text.contains("cached-not-selectable"))
        );
        assert_eq!(
            route(&drive::selected_key(&state).unwrap()),
            ("example-00".into(), "reasoner-large".into())
        );
        assert_eq!(view.rows.iter().filter(|r| r.text.contains('●')).count(), 1);
        assert_eq!(view.rows.iter().filter(|r| r.text.contains('★')).count(), 2);
        assert_eq!(
            (
                view.browse.unwrap().counts.models,
                view.browse.unwrap().counts.providers
            ),
            (3, 3)
        );
    }

    #[test]
    fn each_scope_keeps_the_filter_and_separates_save_scope() {
        let mut state = open();
        drive::type_filter(&mut state, "reasoner");
        for (key, scope, expected) in [
            (PickerKey::ChipsNext, ListScope::Starred, 2),
            (PickerKey::ChipsNext, ListScope::Configured, 2),
            (PickerKey::ChipsNext, ListScope::SetUp, 0),
            (PickerKey::ChipsPrev, ListScope::Configured, 2),
        ] {
            drive::press(&mut state, key);
            let view = state.view().unwrap();
            assert_eq!(view.browse.unwrap().scope, scope);
            assert_eq!(view.filter, "reasoner");
            assert_eq!(routes(&state).len(), expected);
            assert_eq!(
                view.title, "Switch model · this session",
                "view chips never change the save destination"
            );
        }
        drive::press(&mut state, PickerKey::ToggleScope);
        assert_eq!(
            state.view().unwrap().browse.unwrap().scope,
            ListScope::Configured
        );
    }

    #[test]
    fn search_terms_match_name_slug_and_model_and_keep_headers() {
        let mut state = open();
        drive::type_filter(&mut state, "ExAmPlE 01 ReAsOnEr");
        assert_eq!(
            routes(&state),
            [("example-01".into(), "reasoner-large".into())]
        );
        assert!(state.view().unwrap().rows.iter().any(|r| r.header));
        drive::press(&mut state, PickerKey::ClearFilter);
        drive::type_filter(&mut state, "Example provider 00");
        assert_eq!(
            routes(&state).len(),
            2,
            "provider name retains every child, without a duplicate star"
        );
        let before = route(&drive::selected_key(&state).unwrap());
        drive::press(&mut state, PickerKey::Refresh);
        state.update(ModelsInput::OptionsLoaded {
            session_id: drive::SID.into(),
            result: fixture(),
        });
        assert_eq!(state.view().unwrap().filter, "Example provider 00");
        assert_eq!(route(&drive::selected_key(&state).unwrap()), before);
    }

    #[test]
    fn selection_follows_route_identity_and_current_marker_does_not_follow_selection() {
        let mut state = open();
        drive::press(&mut state, PickerKey::End);
        let selected = drive::selected_key(&state).unwrap();
        assert!(matches!(selected, RowKey::NotSetUp { .. }));
        let current = state
            .view()
            .unwrap()
            .rows
            .iter()
            .find(|r| r.text.contains('●'))
            .and_then(|r| r.key.clone())
            .expect("current route has its own marker");
        drive::type_filter(&mut state, "chat-wide");
        assert_eq!(
            route(&drive::selected_key(&state).unwrap()),
            ("example-00".into(), "chat-wide".into())
        );
        drive::press(&mut state, PickerKey::ClearFilter);
        assert_eq!(
            state
                .view()
                .unwrap()
                .rows
                .iter()
                .filter(|r| r.text.contains('●'))
                .count(),
            1
        );
        assert_eq!(
            route(&current),
            ("example-00".into(), "reasoner-large".into())
        );
        for key in [
            PickerKey::Home,
            PickerKey::Down,
            PickerKey::PageDown,
            PickerKey::End,
            PickerKey::PageUp,
            PickerKey::Up,
        ] {
            drive::press(&mut state, key);
            let view = state.view().unwrap();
            assert!(!view.rows[view.selected.unwrap()].header);
        }
    }

    #[test]
    fn large_catalogue_windows_and_counts() {
        let providers=(0..20).map(|p|json!({"slug":format!("example-{p:02}"),"name":format!("Example provider {p:02}"),"authenticated":true,"models":(0..30).map(|m|format!("model-{m:02}")).collect::<Vec<_>>()})).collect::<Vec<Value>>();
        let catalog = catalogue::parse(
            json!({"provider":"example-00","model":"model-00","providers":providers}),
        );
        for visible in [9, 31] {
            let mut state = drive::open_loaded(catalog.clone(), Some("high"), SavedModels::empty());
            state.set_list_height(visible);
            let view = state.view().unwrap();
            assert_eq!(
                (
                    view.browse.unwrap().counts.models,
                    view.browse.unwrap().counts.providers
                ),
                (600, 20)
            );
            assert_eq!(routes(&state).len(), 600);
            drive::press(&mut state, PickerKey::Home);
            let first = drive::selected_key(&state);
            drive::press(&mut state, PickerKey::PageDown);
            assert_ne!(drive::selected_key(&state), first);
            drive::press(&mut state, PickerKey::End);
            assert_eq!(
                route(&drive::selected_key(&state).unwrap()),
                ("example-19".into(), "model-29".into())
            );
            drive::press(&mut state, PickerKey::Home);
            assert_eq!(drive::selected_key(&state), first);
        }
    }

    #[test]
    fn badge_priority() {
        for (listed, available, limited, reasoning, expected) in [
            (false, false, true, Some(false), "not listed"),
            (true, false, true, Some(false), "unavailable"),
            (true, true, true, Some(false), "rate-limited"),
            (true, true, false, Some(false), "not offered"),
            (true, true, false, Some(true), "effort ?"),
            (true, true, false, None, "effort ?"),
        ] {
            let mut provider = json!({
                "slug": "example-00", "name": "Example provider 00", "authenticated": true,
                "models": if listed { vec!["reasoner-large"] } else { Vec::<&str>::new() },
                "unavailable_models": if available { Vec::<&str>::new() } else { vec!["reasoner-large"] },
            });
            if let Some(reasoning) = reasoning {
                provider["capabilities"] =
                    json!({"reasoner-large": {"fast": false, "reasoning": reasoning}});
            }
            if limited {
                provider["limit"] = json!({"scope": "models", "models": {"reasoner-large": "2026-10-22T18:00:00+00:00"}});
            }
            let options = catalogue::parse(
                json!({"provider": "example-00", "model": "reasoner-large", "providers": [provider]}),
            );
            let mut saved = SavedModels::empty();
            saved.favourites = vec!["example-00::reasoner-large".into()];
            let state = drive::open_loaded(options, Some("high"), saved);
            let view = state.view().unwrap();
            let row = view
                .rows
                .iter()
                .find(|row| {
                    row.key.as_ref().is_some_and(|key| {
                        route(key) == ("example-00".into(), "reasoner-large".into())
                    })
                })
                .unwrap();
            assert_eq!(row.badges.len(), 1, "only the priority badge is rendered");
            if expected == "rate-limited" {
                assert!(row.badges[0].starts_with(expected), "{:?}", row.badges);
            } else {
                assert_eq!(
                    row.badges,
                    [expected],
                    "listed={listed} available={available} limited={limited} reasoning={reasoning:?}"
                );
            }
        }
    }

    #[test]
    fn details_keep_requested_support_and_applied_distinct() {
        let mut state = open();
        let detail = state.view().unwrap().browse.unwrap().details.join("\n");
        for required in [
            "Requested: high",
            "Support: unverified for this route",
            "Applied: unknown",
            "Context: unknown",
        ] {
            assert!(detail.contains(required), "{required}: {detail}");
        }
        assert!(!detail.contains("supports high"));
        drive::type_filter(&mut state, "chat-wide");
        let view = state.view().unwrap();
        assert_eq!(view.rows[view.selected.unwrap()].badges, ["not offered"]);
        assert!(
            view.browse
                .unwrap()
                .details
                .join("\n")
                .contains("Catalogue: reasoning not advertised")
        );
        state.update(ModelsInput::EffortReadFailed {
            session_id: drive::SID.into(),
            reason: "unavailable".into(),
            unknown_method: false,
        });
        assert!(
            state
                .view()
                .unwrap()
                .browse
                .unwrap()
                .details
                .join("\n")
                .contains("Support: could not be read")
        );
    }

    #[test]
    fn details_layer_never_confirms() {
        for key in [
            PickerKey::Enter,
            PickerKey::ToggleFavourite,
            PickerKey::ToggleScope,
            PickerKey::Left,
            PickerKey::Right,
            PickerKey::Char('o'),
            PickerKey::Char('c'),
            PickerKey::Char('1'),
            PickerKey::ChipsNext,
            PickerKey::Refresh,
        ] {
            let mut state = open();
            let before = drive::selected_key(&state);
            drive::press(&mut state, PickerKey::Details);
            assert!(state.view().unwrap().browse.unwrap().details_open);
            let effects = drive::press(&mut state, key);
            assert!(!effects.iter().any(|e| matches!(
                e,
                ModelsEffect::Mutate(_)
                    | ModelsEffect::SetEffort { .. }
                    | ModelsEffect::SaveFile(_)
                    | ModelsEffect::OpenProviders { .. }
            )));
            assert_eq!(drive::selected_key(&state), before);
            assert!(state.view().unwrap().filter.is_empty());
        }
    }

    #[test]
    fn escape_closes_details_then_clears_filter_then_closes_picker() {
        let mut state = open();
        drive::type_filter(&mut state, "reasoner");
        drive::press(&mut state, PickerKey::Details);
        assert!(state.view().unwrap().browse.unwrap().details_open);
        assert!(!drive::has_close(&drive::press(
            &mut state,
            PickerKey::Escape
        )));
        assert!(!state.view().unwrap().browse.unwrap().details_open);
        assert_eq!(state.view().unwrap().filter, "reasoner");
        drive::press(&mut state, PickerKey::Escape);
        assert!(state.view().unwrap().filter.is_empty());
        assert!(drive::has_close(&drive::press(
            &mut state,
            PickerKey::Escape
        )));
    }
}
