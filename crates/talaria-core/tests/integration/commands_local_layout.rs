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
fn bar_and_menu_are_understood() {
    assert_eq!(parse_layout_args("bar"), Ok(LayoutAction::Bar));
    assert_eq!(parse_layout_args(" menu "), Ok(LayoutAction::Menu));
}

#[test]
fn invalid_arguments_have_a_single_line_usage_notice() {
    for argument in ["", "unknown", "panel extra"] {
        let notice = parse_layout_args(argument).expect_err("unsupported layout argument");
        assert_eq!(notice.lines().count(), 1);
        assert!(
            notice.contains("Usage:")
                && notice.contains("panel")
                && notice.contains("bar")
                && notice.contains("menu"),
            "{notice}"
        );
    }
}

#[test]
fn thinking_is_a_local_command_that_reaches_hermes_never() {
    let table = LocalCommandTable::builtin();
    let command = table
        .find("/thinking")
        .expect("/thinking is a display choice and never reaches Hermes slash.exec");
    assert!(command.takes_argument);
    assert_eq!(command.tag, "talaria");
}

#[test]
fn thinking_sets_or_toggles_the_reasoning_display() {
    use talaria_core::layout::{ReasoningDisplay, parse_thinking_args};
    assert_eq!(ReasoningDisplay::default(), ReasoningDisplay::Collapsed);
    for current in [ReasoningDisplay::Collapsed, ReasoningDisplay::Full] {
        assert_eq!(
            parse_thinking_args("full", current),
            Ok(ReasoningDisplay::Full)
        );
        assert_eq!(
            parse_thinking_args(" collapsed ", current),
            Ok(ReasoningDisplay::Collapsed)
        );
    }
    assert_eq!(
        parse_thinking_args("", ReasoningDisplay::Collapsed),
        Ok(ReasoningDisplay::Full)
    );
    assert_eq!(
        parse_thinking_args("  ", ReasoningDisplay::Full),
        Ok(ReasoningDisplay::Collapsed)
    );
    assert_eq!(
        parse_thinking_args("loud", ReasoningDisplay::Full),
        Err("Usage: /thinking [collapsed|full]".to_owned())
    );
}
