use talaria_core::commands::LocalCommandTable;
use talaria_core::layout::{LayoutAction, parse_layout_args};

#[test]
fn layout_is_a_local_command_accepting_panel() {
    let table = LocalCommandTable::builtin();
    let command = table
        .find("/layout")
        .expect("/layout must never reach Hermes slash.exec");
    assert!(command.takes_argument);
    assert_eq!(parse_layout_args("panel"), Ok(LayoutAction::Panel));
    assert_eq!(parse_layout_args(" panel "), Ok(LayoutAction::Panel));
}

#[test]
fn invalid_arguments_have_a_single_line_usage_notice() {
    for argument in ["", "unknown", "panel extra"] {
        let notice = parse_layout_args(argument).expect_err("unsupported layout argument");
        assert_eq!(notice.lines().count(), 1);
        assert!(
            notice.contains("Usage:") && notice.contains("/layout panel"),
            "{notice}"
        );
    }
}
