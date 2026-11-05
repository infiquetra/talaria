use std::collections::BTreeMap;

use talaria_tui::keys::{Action, KeyChord, KeyContext, KeyMap, ModelAction};

#[test]
fn fixed_picker_keys_and_filter_text_cannot_be_shadowed() {
    for chord in [
        "o",
        "shift+c",
        "4",
        "ctrl+u",
        "ctrl+c",
        "enter",
        "esc",
        "backspace",
        "up",
        "pagedown",
    ] {
        let (map, notices) = KeyMap::from_entries(&BTreeMap::from([(
            "model_details".into(),
            vec![chord.into()],
        )]));
        assert_eq!(notices.len(), 1, "{chord}: {notices:?}");
        let f2 = KeyChord::parse("f2", KeyContext::ModelPicker)
            .unwrap()
            .to_key();
        assert_eq!(
            map.resolve_model(KeyContext::ModelPicker, &f2),
            Some(ModelAction::ModelDetails)
        );
        let reserved = KeyChord::parse(chord, KeyContext::ModelPicker)
            .unwrap()
            .to_key();
        assert_eq!(
            map.resolve_model(KeyContext::ModelPicker, &reserved),
            None,
            "{chord}"
        );
    }
}

#[test]
fn new_actions_resolve_only_inside_the_picker() {
    let docs = include_str!("../../../docs/configuration.md");
    for name in ["model_chips_next", "model_chips_prev", "model_details"] {
        assert!(docs.contains(name), "missing documented action {name}");
    }
    let map = KeyMap::defaults();
    for (chord, action) in [
        ("tab", ModelAction::ModelChipsNext),
        ("shift+tab", ModelAction::ModelChipsPrev),
        ("f2", ModelAction::ModelDetails),
        ("ctrl+e", ModelAction::ModelDetails),
    ] {
        let key = KeyChord::parse(chord, KeyContext::ModelPicker)
            .unwrap()
            .to_key();
        assert_eq!(
            map.resolve_model(KeyContext::ModelPicker, &key),
            Some(action)
        );
        assert_eq!(map.resolve_model(KeyContext::Composer, &key), None);
    }
    assert_eq!(Action::ALL.len(), 194);
}

#[test]
fn new_actions_can_be_rebound_and_conflicts_produce_a_notice() {
    let entries = BTreeMap::from([
        ("model_chips_next".into(), vec!["ctrl+n".into()]),
        ("model_chips_prev".into(), vec!["ctrl+p".into()]),
        ("model_details".into(), vec!["ctrl+d".into()]),
    ]);
    let (map, notices) = KeyMap::from_entries(&entries);
    assert!(notices.is_empty(), "{notices:?}");
    for (chord, action) in [
        ("ctrl+n", ModelAction::ModelChipsNext),
        ("ctrl+p", ModelAction::ModelChipsPrev),
        ("ctrl+d", ModelAction::ModelDetails),
    ] {
        let key = KeyChord::parse(chord, KeyContext::ModelPicker)
            .unwrap()
            .to_key();
        assert_eq!(
            map.resolve_model(KeyContext::ModelPicker, &key),
            Some(action)
        );
    }
    let (_, notices) = KeyMap::from_entries(&BTreeMap::from([(
        "model_details".into(),
        vec!["ctrl+f".into()],
    )]));
    assert!(!notices.is_empty(), "a clash must be reported");
}

#[test]
fn ordinary_letters_and_digits_are_not_model_actions() {
    let map = KeyMap::defaults();
    for chord in ["o", "c", "0", "1", "9", "g", "r", "f"] {
        let key = KeyChord::parse(chord, KeyContext::ModelPicker)
            .unwrap()
            .to_key();
        assert_eq!(
            map.resolve_model(KeyContext::ModelPicker, &key),
            None,
            "{chord}"
        );
    }
}
mod independent {
    use std::{collections::BTreeMap, path::PathBuf};
    use talaria_tui::keys::{Action, KeyChord, KeyContext, KeyMap, ModelAction};

    #[test]
    fn chips_and_details_defaults_are_local_and_documented() {
        let map = KeyMap::defaults();
        assert_eq!(
            Action::ALL.len(),
            194,
            "model actions must not change the general action count"
        );
        let doc = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/configuration.md"),
        )
        .unwrap();
        for (name, chords) in [
            ("model_chips_next", vec!["tab"]),
            ("model_chips_prev", vec!["shift+tab"]),
            ("model_details", vec!["f2", "ctrl+e"]),
        ] {
            let action = ModelAction::from_config_name(name).expect("new picker action");
            assert!(doc.contains(name), "{name}");
            for chord in chords {
                let key = KeyChord::parse(chord, KeyContext::ModelPicker)
                    .unwrap()
                    .to_key();
                assert_eq!(
                    map.resolve_model(KeyContext::ModelPicker, &key),
                    Some(action)
                );
                assert_eq!(map.resolve_model(KeyContext::Composer, &key), None);
            }
        }
    }

    #[test]
    fn details_rebinds_and_collision_is_reported() {
        let action = ModelAction::from_config_name("model_details").expect("new picker action");
        let (map, notices) = KeyMap::from_entries(&BTreeMap::from([(
            "model_details".into(),
            vec!["alt+d".into()],
        )]));
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(
            map.resolve_model(
                KeyContext::ModelPicker,
                &KeyChord::parse("alt+d", KeyContext::ModelPicker)
                    .unwrap()
                    .to_key()
            ),
            Some(action)
        );
        let (_, notices) = KeyMap::from_entries(&BTreeMap::from([(
            "model_details".into(),
            vec!["ctrl+r".into()],
        )]));
        assert_eq!(notices.len(), 1);
    }
}
