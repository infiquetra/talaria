use std::{collections::BTreeMap, path::PathBuf};
use talaria_tui::keys::{Action, KeyChord, KeyContext, KeyMap};

#[test]
fn details_defaults_are_documented_and_context_local() {
    let action = Action::from_config_name("providers_details")
        .expect("configurable provider details action");
    let map = KeyMap::defaults();
    assert_eq!(
        map.chords(action)
            .iter()
            .map(|c| c.label())
            .collect::<Vec<_>>(),
        ["F2", "Ctrl+E"]
    );
    for chord in ["f2", "ctrl+e"] {
        let key = KeyChord::parse(chord, KeyContext::Providers)
            .unwrap()
            .to_key();
        assert_eq!(map.resolve_in(&key, KeyContext::Providers), Some(action));
        assert_ne!(map.resolve_in(&key, KeyContext::Composer), Some(action));
        assert_ne!(map.resolve_in(&key, KeyContext::ModelPicker), Some(action));
    }
    for doc in ["docs/configuration.md", "docs/keybindings.md"] {
        let text = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(doc),
        )
        .unwrap();
        assert!(text.contains("providers_details"), "{doc}");
    }
}

#[test]
fn details_rebinds_unbinds_and_reports_collisions() {
    let action = Action::from_config_name("providers_details")
        .expect("configurable provider details action");
    let (map, notices) = KeyMap::from_entries(&BTreeMap::from([(
        "providers_details".into(),
        vec!["alt+d".into()],
    )]));
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(
        map.resolve_in(
            &KeyChord::parse("alt+d", KeyContext::Providers)
                .unwrap()
                .to_key(),
            KeyContext::Providers
        ),
        Some(action)
    );
    let (map, notices) =
        KeyMap::from_entries(&BTreeMap::from([("providers_details".into(), vec![])]));
    assert!(notices.is_empty());
    assert!(map.chords(action).is_empty());
    let (_, notices) = KeyMap::from_entries(&BTreeMap::from([(
        "providers_details".into(),
        vec!["ctrl+r".into()],
    )]));
    assert_eq!(notices.len(), 1, "explicit collisions are reported");
}
