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
