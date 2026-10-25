//! Test 1 (#237): the approval-mode words parse, and nothing else does.

use talaria_core::permissions::ApprovalMode;

#[test]
fn parse_hermes_words_only() {
    assert_eq!(ApprovalMode::parse("manual"), Some(ApprovalMode::Manual));
    assert_eq!(ApprovalMode::parse("smart"), Some(ApprovalMode::Smart));
    assert_eq!(ApprovalMode::parse("off"), Some(ApprovalMode::Off));
    assert_eq!(ApprovalMode::Manual.as_wire(), "manual");
    assert_eq!(ApprovalMode::Smart.as_wire(), "smart");
    assert_eq!(ApprovalMode::Off.as_wire(), "off");

    for word in [
        "",
        "Manual",
        "MANUAL",
        " manual",
        "manual ",
        "on",
        "yolo",
        "auto",
        "1",
        "0",
        "true",
        "false",
        "manual,smart",
        "smart ",
        " off",
        "approve",
        "deny",
    ] {
        assert_eq!(ApprovalMode::parse(word), None, "word {word:?}");
    }
}
