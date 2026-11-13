//! Fleet notification scopes in the configuration file.

use talaria_app::config::parse;
use talaria_core::fleet_attention::FleetScope;

const NEEDS_BAD: &str =
    "[notifications] fleet_needs_you must be \"all\", \"watched\" or \"off\"; using \"all\".";
const FINISHED_BAD: &str =
    "[notifications] fleet_finished must be \"all\", \"watched\" or \"off\"; using \"watched\".";

#[test]
fn fleet_notification_keys_defaults_and_bad_values() {
    let loaded = parse("");
    assert_eq!(loaded.config.notifications.fleet_needs_you, FleetScope::All);
    assert_eq!(
        loaded.config.notifications.fleet_finished,
        FleetScope::Watched
    );
    assert!(
        loaded.config.notifications.speech.is_empty(),
        "spoken alerts default to off"
    );
    assert!(loaded.notices.is_empty(), "{:?}", loaded.notices);

    let loaded = parse("[notifications]\nfleet_needs_you = \"sometimes\"\n");
    assert_eq!(loaded.config.notifications.fleet_needs_you, FleetScope::All);
    assert_eq!(loaded.notices, vec![NEEDS_BAD.to_owned()]);

    let loaded = parse("[notifications]\nfleet_finished = \"sometimes\"\n");
    assert_eq!(
        loaded.config.notifications.fleet_finished,
        FleetScope::Watched
    );
    assert_eq!(loaded.notices, vec![FINISHED_BAD.to_owned()]);

    let loaded = parse("[notifications]\nfleet_needs_you = 3\n");
    assert_eq!(loaded.config.notifications.fleet_needs_you, FleetScope::All);
    assert_eq!(loaded.notices, vec![NEEDS_BAD.to_owned()]);

    let loaded = parse("[notifications]\nfleet_finished = true\n");
    assert_eq!(
        loaded.config.notifications.fleet_finished,
        FleetScope::Watched
    );
    assert_eq!(loaded.notices, vec![FINISHED_BAD.to_owned()]);
}

#[test]
fn speech_list_parses_the_same_kinds_as_bell_and_desktop() {
    use talaria_core::attention::AlertKind;

    let loaded = parse("[notifications]\nspeech = [\"needs_you\", \"done\", \"failed\"]\n");
    assert!(
        loaded.notices.is_empty(),
        "speech is a notification channel: {:?}",
        loaded.notices
    );
    assert_eq!(
        loaded.config.notifications.speech,
        vec![AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed]
    );
}

#[test]
fn speech_must_be_a_list_of_notification_kinds() {
    let loaded = parse("[notifications]\nspeech = \"needs_you\"\n");
    assert!(loaded.config.notifications.speech.is_empty());
    assert_eq!(
        loaded.notices,
        vec![
            "[notifications] speech must be a list of notification kinds; using the default."
                .to_owned(),
        ]
    );

    let loaded = parse("[notifications]\nspeech = [\"nope\"]\n");
    assert!(loaded.config.notifications.speech.is_empty());
    assert_eq!(
        loaded.notices,
        vec![
            "Unknown notification kind \"nope\" in [notifications] speech; it is ignored."
                .to_owned(),
        ]
    );
}
