//! Workflow screen predicates against specification-derived fixtures.
use smoke::screen5::*;
use smoke::wording5::*;

fn screen(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/screens/phase5")
            .join(name),
    )
    .expect("synthetic screen")
}

#[test]
fn overlay_title_found_with_profile() {
    let text = "Workflows · profile fixture\n".to_owned();
    assert!(overlay_title_found(
        &text,
        "Workflows · profile <name>",
        "fixture"
    ));
    assert!(!overlay_title_found(
        &text,
        "Workflows · profile <name>",
        "default"
    ));
    assert!(!overlay_title_found(
        &screen("idle.txt"),
        "Workflows · profile <name>",
        "fixture"
    ));
    assert!(!overlay_title_found(
        "notice: Workflows · profile fixture",
        "Workflows · profile <name>",
        "fixture"
    ));
}

#[test]
fn workflows_empty() {
    assert!(overlay_title_found(
        &screen("workflows-empty.txt"),
        "Workflows",
        "fixture"
    ));
    assert_eq!(
        WORKFLOWS_EMPTY,
        "no runs yet — talaria workflow run starts one"
    );
    assert!(smoke::screen5::workflows_empty(&screen(
        "workflows-empty.txt"
    )));
    assert!(!smoke::screen5::workflows_empty(&screen("idle.txt")));
    assert!(!smoke::screen5::workflows_empty("no runs yet"));
}

#[test]
fn ultracode_status() {
    assert_eq!(
        ULTRACODE_STATUS,
        "Ultracode is off for this session. Skill: not found in profile fixture."
    );
    assert!(smoke::screen5::ultracode_status(&screen(
        "ultracode-status.txt"
    )));
    assert!(!smoke::screen5::ultracode_status(&screen("idle.txt")));
    assert!(!smoke::screen5::ultracode_status(
        "Ultracode is on for this session. Skill: not found in profile fixture."
    ));
}

#[test]
fn created_folder() {
    assert_eq!(CREATED_FOLDER, "created folder ");
    assert!(smoke::screen5::created_folder(&screen(
        "builtins-install.txt"
    )));
    assert!(!smoke::screen5::created_folder(&screen(
        "builtins-install-again.txt"
    )));
}

#[test]
fn created() {
    assert_eq!(CREATED, "created ");
    assert!(smoke::screen5::created(&screen("builtins-install.txt")));
    assert!(!smoke::screen5::created(&screen(
        "builtins-install-again.txt"
    )));
}

#[test]
fn unchanged() {
    assert_eq!(UNCHANGED, "unchanged ");
    assert!(smoke::screen5::unchanged(&screen(
        "builtins-install-again.txt"
    )));
    assert!(!smoke::screen5::unchanged(&screen("builtins-install.txt")));
}

#[test]
fn installed() {
    assert_eq!(INSTALLED, "installed");
    assert!(smoke::screen5::installed(&screen("builtins-list.txt")));
    assert!(!smoke::screen5::installed(&screen("builtins-install.txt")));
}

#[test]
fn missing_tiers() {
    assert_eq!(MISSING_TIERS, "missing tiers:");
    assert!(smoke::screen5::missing_tiers(&screen("builtins-check.txt")));
    assert!(!smoke::screen5::missing_tiers(&screen("builtins-list.txt")));
}

#[test]
fn empty_list() {
    assert_eq!(EMPTY_LIST, "No workflow runs found.");
    assert!(smoke::screen5::empty_list(&screen("list.txt")));
    assert!(!smoke::screen5::empty_list(&screen("list-json.txt")));
    assert!(!smoke::screen5::empty_list("No workflow runs"));
}

#[test]
fn idle_screen() {
    assert!(smoke::screen5::idle_screen(&screen("idle.txt")));
    assert!(smoke::screen5::idle_screen(&screen("ultracode-status.txt")));
    assert!(!smoke::screen5::idle_screen(&screen("workflows-empty.txt")));
    assert!(!smoke::screen5::idle_screen(&screen("busy.txt")));
    assert!(!smoke::screen5::idle_screen(
        "Talaria · profile fixture\n> unsent draft"
    ));
}

#[test]
fn recorded_composer_hints_and_active_status() {
    assert!(smoke::screen5::idle_screen(
        "> Type a message. Enter sends, Alt+Enter adds a line."
    ));
    assert!(smoke::screen5::idle_screen(
        "> Type a message. Enter sends, Shift+Enter adds a line."
    ));
    assert!(!smoke::screen5::idle_screen("> unsent draft"));
    assert!(!smoke::screen5::idle_screen(
        "> Type a message. Enter sends, Alt+Enter adds a line.\n model │ 1 working"
    ));
}
