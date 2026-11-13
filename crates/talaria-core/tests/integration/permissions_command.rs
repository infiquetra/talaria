//! Test 10 (#237): every `/permissions` form parses, and bad input gives usage.

use talaria_core::permissions::{ApprovalMode, PermissionsCommand, parse_permissions_command};

const USAGE: &str = "Usage: /permissions [manual | smart | off | session off | session default]";

#[test]
fn parse_every_form_and_usage() {
    assert_eq!(parse_permissions_command(""), Ok(PermissionsCommand::Show));
    assert_eq!(
        parse_permissions_command("   "),
        Ok(PermissionsCommand::Show)
    );
    assert_eq!(
        parse_permissions_command("manual"),
        Ok(PermissionsCommand::Profile(ApprovalMode::Manual))
    );
    assert_eq!(
        parse_permissions_command("smart"),
        Ok(PermissionsCommand::Profile(ApprovalMode::Smart))
    );
    assert_eq!(
        parse_permissions_command("off"),
        Ok(PermissionsCommand::Profile(ApprovalMode::Off))
    );
    assert_eq!(
        parse_permissions_command("session off"),
        Ok(PermissionsCommand::SessionOff)
    );
    assert_eq!(
        parse_permissions_command("session default"),
        Ok(PermissionsCommand::SessionDefault)
    );

    // Words are case-insensitive and separated by any whitespace.
    assert_eq!(
        parse_permissions_command("MANUAL"),
        Ok(PermissionsCommand::Profile(ApprovalMode::Manual))
    );
    assert_eq!(
        parse_permissions_command("  Smart  "),
        Ok(PermissionsCommand::Profile(ApprovalMode::Smart))
    );
    assert_eq!(
        parse_permissions_command("OFF"),
        Ok(PermissionsCommand::Profile(ApprovalMode::Off))
    );
    assert_eq!(
        parse_permissions_command("SESSION\tOFF"),
        Ok(PermissionsCommand::SessionOff)
    );
    assert_eq!(
        parse_permissions_command("Session   Default"),
        Ok(PermissionsCommand::SessionDefault)
    );

    // Anything else is usage.
    for args in [
        "on",
        "yolo",
        "session",
        "session on",
        "session manual",
        "session default now",
        "manual smart",
        "off off",
        "manual extra words here",
        "session off please",
        "sessions off",
        "default",
    ] {
        assert_eq!(
            parse_permissions_command(args),
            Err(USAGE.to_owned()),
            "args {args:?}"
        );
    }
}
