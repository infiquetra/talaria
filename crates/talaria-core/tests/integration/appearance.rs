//! The skin as domain state: sources, precedence, and change detection.
//!
//! Behavior contract: issue 227.

use serde_json::{Value, json};
use talaria_core::appearance::{
    AppearanceChange, AppearanceInput, AppearanceState, Skin, SkinMap, SkinSource, SkippedValue,
};

fn payload(name: &str, colors: Value) -> Value {
    json!({
        "name": name,
        "colors": colors,
        "light_colors": {},
        "dark_colors": {},
    })
}

fn ares() -> Value {
    payload("ares", json!({"prompt": "#F1E6CF"}))
}

fn default() -> Value {
    payload("default", json!({"prompt": "#FFF8DC"}))
}

fn handshake(ready: Option<Value>, describe: Option<Value>) -> AppearanceInput {
    AppearanceInput::Handshake { ready, describe }
}

#[test]
fn empty_skin_payload_means_no_skin() {
    let (skin, skipped) = Skin::from_value(&json!({}));
    assert_eq!(skin, None);
    assert!(skipped.is_empty());
    let (skin, skipped) = Skin::from_value(&Value::Null);
    assert_eq!(skin, None);
    assert!(skipped.is_empty());

    let mut state = AppearanceState::default();
    let change = state.update(handshake(None, Some(json!({}))));
    assert_eq!(change, AppearanceChange::Unchanged);
    assert_eq!(state.skin(), None);
    assert_eq!(state.source(), SkinSource::None);

    let mixed = json!({
        "name": "mixed",
        "colors": {"ui_accent": null, "prompt": "#FFF8DC"},
        "light_colors": {"ui_ok": 7},
        "dark_colors": {},
    });
    let (skin, skipped) = Skin::from_value(&mixed);
    let skin = skin.expect("a name and a string colour give a skin");
    assert_eq!(skin.name, "mixed");
    assert_eq!(
        skipped,
        vec![
            SkippedValue {
                map: SkinMap::Colors,
                key: "ui_accent".to_owned(),
            },
            SkippedValue {
                map: SkinMap::LightColors,
                key: "ui_ok".to_owned(),
            },
        ]
    );
    let change = state.update(handshake(None, Some(mixed)));
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(state.skin().map(|skin| skin.name.as_str()), Some("mixed"));
    assert_eq!(
        state.skipped(),
        [
            SkippedValue {
                map: SkinMap::Colors,
                key: "ui_accent".to_owned(),
            },
            SkippedValue {
                map: SkinMap::LightColors,
                key: "ui_ok".to_owned(),
            },
        ]
    );
}

#[test]
fn describe_skin_wins_over_ready_skin() {
    let mut state = AppearanceState::default();
    let change = state.update(handshake(Some(default()), Some(ares())));
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(state.skin().map(|skin| skin.name.as_str()), Some("ares"));
    assert_eq!(state.source(), SkinSource::RuntimeDescribe);

    let mut state = AppearanceState::default();
    let change = state.update(handshake(Some(ares()), Some(json!({}))));
    assert_eq!(change, AppearanceChange::Unchanged);
    assert_eq!(state.skin(), None, "an empty describe beats a ready skin");
    assert_eq!(state.source(), SkinSource::None);
}

#[test]
fn missing_describe_skin_uses_ready_then_keeps_current() {
    let mut state = AppearanceState::default();
    let change = state.update(handshake(Some(default()), None));
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(state.skin().map(|skin| skin.name.as_str()), Some("default"));
    assert_eq!(state.source(), SkinSource::GatewayReady);

    let mut state = AppearanceState::default();
    let change = state.update(handshake(None, None));
    assert_eq!(change, AppearanceChange::Unchanged);
    assert_eq!(state.skin(), None);

    let mut state = AppearanceState::default();
    state.update(handshake(None, Some(ares())));
    let change = state.update(handshake(None, None));
    assert_eq!(change, AppearanceChange::Unchanged);
    assert_eq!(state.skin().map(|skin| skin.name.as_str()), Some("ares"));
    assert_eq!(state.source(), SkinSource::RuntimeDescribe);
}

#[test]
fn identical_skin_changed_is_unchanged() {
    let mut state = AppearanceState::default();
    let change = state.update(AppearanceInput::SkinChanged {
        payload: ares(),
        replayed: false,
    });
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(state.source(), SkinSource::Changed);

    let change = state.update(AppearanceInput::SkinChanged {
        payload: ares(),
        replayed: false,
    });
    assert_eq!(change, AppearanceChange::Unchanged);

    let change = state.update(AppearanceInput::SkinChanged {
        payload: default(),
        replayed: true,
    });
    assert_eq!(change, AppearanceChange::Unchanged);
    assert_eq!(
        state.skin().map(|skin| skin.name.as_str()),
        Some("ares"),
        "a replayed event keeps the skin"
    );
}

#[test]
fn unknown_skin_only_when_hermes_fell_back_to_default() {
    let mut state = AppearanceState::default();
    state.update(handshake(None, Some(ares())));
    let change = state.update(AppearanceInput::Described {
        payload: default(),
        requested: Some("nosuch".to_owned()),
    });
    assert_eq!(
        change,
        AppearanceChange::UnknownSkin {
            requested: "nosuch".to_owned(),
            using: "default".to_owned(),
            redraw: true,
        }
    );
    assert_eq!(state.skin().map(|skin| skin.name.as_str()), Some("default"));

    let mut state = AppearanceState::default();
    state.update(handshake(None, Some(default())));
    let change = state.update(AppearanceInput::Described {
        payload: default(),
        requested: Some("nosuch".to_owned()),
    });
    assert_eq!(
        change,
        AppearanceChange::UnknownSkin {
            requested: "nosuch".to_owned(),
            using: "default".to_owned(),
            redraw: false,
        }
    );

    let mut state = AppearanceState::default();
    state.update(handshake(None, Some(ares())));
    let change = state.update(AppearanceInput::Described {
        payload: payload("Nord Dark", json!({"prompt": "#F1E6CF"})),
        requested: Some("nord".to_owned()),
    });
    assert_eq!(change, AppearanceChange::Changed);

    let mut state = AppearanceState::default();
    state.update(handshake(None, Some(ares())));
    let change = state.update(AppearanceInput::Described {
        payload: default(),
        requested: Some("default".to_owned()),
    });
    assert_eq!(change, AppearanceChange::Changed);

    let mut state = AppearanceState::default();
    state.update(handshake(None, Some(ares())));
    let change = state.update(AppearanceInput::SkinChanged {
        payload: default(),
        replayed: false,
    });
    assert_eq!(
        change,
        AppearanceChange::Changed,
        "a plain change never yields an unknown skin"
    );
}

#[test]
fn new_handshake_replaces_skin_after_reconnect() {
    let mut state = AppearanceState::default();
    state.update(handshake(None, Some(ares())));
    let change = state.update(handshake(None, Some(default())));
    assert_eq!(change, AppearanceChange::Changed);
    assert_eq!(state.skin().map(|skin| skin.name.as_str()), Some("default"));
    assert_eq!(state.source(), SkinSource::RuntimeDescribe);
}
