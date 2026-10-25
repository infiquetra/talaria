//! The comfort configuration tables: defaults and bad values.
//!
//! Behavior contract: issue 228.

use talaria_app::config::{DEFAULT_DONE_AFTER_SECONDS, parse};
use talaria_core::attention::AlertKind;

#[test]
fn new_tables_defaults_and_bad_values() {
    let loaded = parse("");
    assert!(loaded.notices.is_empty());
    assert_eq!(loaded.config.editor.command, "");
    assert!(loaded.config.history.save);
    assert_eq!(
        loaded.config.notifications.bell,
        [AlertKind::NeedsYou, AlertKind::Failed]
    );
    assert_eq!(
        loaded.config.notifications.desktop,
        [AlertKind::NeedsYou, AlertKind::Done, AlertKind::Failed]
    );
    assert_eq!(
        loaded.config.notifications.done_after_seconds,
        DEFAULT_DONE_AFTER_SECONDS
    );
    assert_eq!(loaded.config.notifications.done_after_seconds, 20);
    assert!(loaded.config.herdr.report_state);

    let loaded = parse("[notifications]\nbell = [\"needs_you\", \"bogus\"]\n");
    assert_eq!(
        loaded.notices,
        ["Unknown notification kind \"bogus\" in [notifications] bell; it is ignored."]
    );
    assert_eq!(loaded.config.notifications.bell, [AlertKind::NeedsYou]);

    let loaded = parse("[notifications]\ndone_after_seconds = -1\n");
    assert_eq!(
        loaded.notices,
        ["[notifications] done_after_seconds must be a whole number from 0 to 86400; using 20."]
    );
    assert_eq!(loaded.config.notifications.done_after_seconds, 20);

    let loaded = parse("[editor]\ncommand = 7\n");
    assert_eq!(
        loaded.notices,
        ["editor.command in ~/.talaria/config.toml must be a text string; using the default."]
    );
    assert_eq!(loaded.config.editor.command, "");

    let loaded = parse("[history]\nsave = \"yes\"\n");
    assert_eq!(
        loaded.notices,
        ["history.save in ~/.talaria/config.toml must be true or false; using true."]
    );
    assert!(loaded.config.history.save);

    let loaded = parse("[herdr]\nreport_state = 1\n");
    assert_eq!(
        loaded.notices,
        ["herdr.report_state in ~/.talaria/config.toml must be true or false; using true."]
    );
    assert!(loaded.config.herdr.report_state);
}
