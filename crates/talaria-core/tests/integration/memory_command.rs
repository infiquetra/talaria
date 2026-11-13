use talaria_core::{
    commands::local::{LocalAction, LocalCommandTable},
    settings::SETTINGS_LINKS,
};

#[test]
fn memory_settings_does_not_shadow_hermes() {
    let table = LocalCommandTable::builtin();
    let command = table
        .find("memory-settings")
        .expect("memory settings command");
    assert!(!command.takes_argument);
    assert_eq!(command.action, LocalAction::MemorySettings);
    assert!(table.find("memory").is_none());
    assert!(table.find("curator").is_none());
    assert!(
        SETTINGS_LINKS
            .iter()
            .any(|link| link.label == "Memory and curator" && link.action == command.action)
    );
}
