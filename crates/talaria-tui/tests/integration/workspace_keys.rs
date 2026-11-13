use std::collections::BTreeMap;
use talaria_tui::keys::{Action, KeyChord, KeyContext, KeyMap};

#[test]
fn alt_d_default_and_rebinding() {
    let action =
        Action::from_config_name("layout_panel").expect("configurable layout_panel action");
    let key = KeyChord::parse("alt+d", KeyContext::Composer)
        .unwrap()
        .to_key();
    assert_eq!(
        KeyMap::defaults().resolve(&key, KeyContext::Composer),
        Some(action)
    );
    let (map, notices) = KeyMap::from_entries(&BTreeMap::from([(
        "layout_panel".into(),
        vec!["alt+w".into()],
    )]));
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(map.resolve(&key, KeyContext::Composer), None);
    assert_eq!(
        map.resolve(
            &KeyChord::parse("alt+w", KeyContext::Composer)
                .unwrap()
                .to_key(),
            KeyContext::Composer
        ),
        Some(action)
    );
}

#[test]
fn panel_can_be_unbound() {
    let action =
        Action::from_config_name("layout_panel").expect("configurable layout_panel action");
    let (map, notices) = KeyMap::from_entries(&BTreeMap::from([("layout_panel".into(), vec![])]));
    assert!(notices.is_empty(), "{notices:?}");
    assert!(map.chords(action).is_empty());
}

#[test]
fn explicit_user_binding_collision_produces_a_notice() {
    Action::from_config_name("layout_panel").expect("configurable layout_panel action");
    let (map, notices) = KeyMap::from_entries(&BTreeMap::from([(
        "agents_panel".into(),
        vec!["alt+d".into()],
    )]));
    assert!(
        !notices.is_empty(),
        "default panel chord must not silently shadow an explicit binding"
    );
    let key = KeyChord::parse("alt+d", KeyContext::Composer)
        .unwrap()
        .to_key();
    assert_eq!(
        map.resolve(&key, KeyContext::Composer),
        Some(Action::AgentsPanel)
    );
}

#[test]
fn alt_i_default_and_rebinding() {
    assert!(
        Action::from_config_name("toggle_menu_bar").is_some(),
        "configurable toggle_menu_bar action"
    );
    let action = Action::from_config_name("toggle_menu_bar").unwrap();
    let key = KeyChord::parse("alt+i", KeyContext::Composer)
        .unwrap()
        .to_key();
    assert_eq!(
        KeyMap::defaults().resolve(&key, KeyContext::Composer),
        Some(action)
    );
    let (map, notices) = KeyMap::from_entries(&BTreeMap::from([(
        "toggle_menu_bar".into(),
        vec!["alt+w".into()],
    )]));
    assert!(notices.is_empty(), "{notices:?}");
    assert_eq!(map.resolve(&key, KeyContext::Composer), None);
    assert_eq!(
        map.resolve(
            &KeyChord::parse("alt+w", KeyContext::Composer)
                .unwrap()
                .to_key(),
            KeyContext::Composer
        ),
        Some(action)
    );
}

#[test]
fn menu_bar_can_be_unbound() {
    assert!(
        Action::from_config_name("toggle_menu_bar").is_some(),
        "configurable toggle_menu_bar action"
    );
    let action = Action::from_config_name("toggle_menu_bar").unwrap();
    let (map, notices) =
        KeyMap::from_entries(&BTreeMap::from([("toggle_menu_bar".into(), vec![])]));
    assert!(notices.is_empty(), "{notices:?}");
    assert!(map.chords(action).is_empty());
}

#[test]
fn explicit_alt_i_binding_collision_produces_a_notice() {
    assert!(
        Action::from_config_name("toggle_menu_bar").is_some(),
        "configurable toggle_menu_bar action"
    );
    let (map, notices) = KeyMap::from_entries(&BTreeMap::from([(
        "agents_panel".into(),
        vec!["alt+i".into()],
    )]));
    assert!(
        !notices.is_empty(),
        "default menu chord must not silently shadow an explicit binding"
    );
    let key = KeyChord::parse("alt+i", KeyContext::Composer)
        .unwrap()
        .to_key();
    assert_eq!(
        map.resolve(&key, KeyContext::Composer),
        Some(Action::AgentsPanel)
    );
}
