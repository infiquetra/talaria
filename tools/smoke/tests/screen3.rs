//! Pure screen readers for the phase 3 steps, on the recorded screens.
//!
//! Each reader runs on its recorded screen. All fail until the stubs in `src/screen3.rs`
//! are replaced.

use smoke::{
    LaunchResult, deny_digit, fleet_headers, fleet_row_state, has_here_row, launch_result,
    launched_id, needs_you_ids, needs_you_segment, opened_notice, worktree_line,
};

fn screens(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/screens/phase3")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("cannot read {name}: {error}"))
}

#[test]
fn fleet_headers_reads_names_and_counts() {
    assert!(
        fleet_headers(&screens("fleet-quiet-empty.txt")).is_empty(),
        "a quiet fleet has no group header"
    );
    assert_eq!(
        fleet_headers(&screens("fleet-quiet-busy.txt")),
        vec![
            ("NEEDS YOU".to_owned(), 1),
            ("WORKING".to_owned(), 2),
            ("DONE".to_owned(), 3),
            ("IDLE".to_owned(), 4),
        ]
    );
}

#[test]
fn fleet_headers_ignores_other_lines() {
    let text = "see NEEDS YOU\nWORKING\nNEEDS YOU (1)\n<title>  local-a1b2c3d4e5f67890\n";
    assert_eq!(fleet_headers(text), vec![("NEEDS YOU".to_owned(), 1)]);
}

#[test]
fn here_row_found_by_its_marker() {
    assert!(has_here_row(&screens("fleet-view-here.screen.txt")));
}

#[test]
fn here_row_absent_when_the_overlay_is_closed() {
    assert!(!has_here_row(&screens("fleet-view-no-match.screen.txt")));
    assert!(!has_here_row(&screens("needs-you-cleared.screen.txt")));
}

#[test]
fn needs_you_segment_reads_the_count() {
    assert_eq!(
        needs_you_segment(&screens("needs-you-notice.screen.txt")),
        Some(1)
    );
}

#[test]
fn needs_you_segment_absent_without_the_mark() {
    assert_eq!(needs_you_segment(&screens("fleet-quiet-empty.txt")), None);
    assert_eq!(needs_you_segment("1 background session needs you\n"), None);
}

#[test]
fn launch_result_reads_launched() {
    assert_eq!(
        launch_result(&screens("bg-launch-ok.json.txt")),
        Some(LaunchResult {
            status: "launched".to_owned(),
            session_id: "local-a1b2c3d4e5f67890".to_owned(),
        })
    );
}

#[test]
fn launch_result_refuses_two_lines() {
    let text = concat!(
        "{\"status\":\"launched\",\"session_id\":\"local-a1b2c3d4e5f67890\"}\n",
        "{\"status\":\"launched\",\"session_id\":\"local-b2c3d4e5f6789012\"}\n"
    );
    assert_eq!(launch_result(text), None);
}

#[test]
fn launch_result_reads_not_started() {
    assert_eq!(
        launch_result("{\"status\":\"not_started\",\"session_id\":\"local-0000000000000000\"}\n"),
        Some(LaunchResult {
            status: "not_started".to_owned(),
            session_id: "local-0000000000000000".to_owned(),
        })
    );
}

#[test]
fn fleet_row_state_finds_the_id() {
    assert_eq!(
        fleet_row_state(&screens("fleet-list-rows.json"), "local-a1b2c3d4e5f67890"),
        Some("done".to_owned())
    );
}

#[test]
fn fleet_row_state_is_none_for_another_id() {
    assert_eq!(
        fleet_row_state(&screens("fleet-list-rows.json"), "local-c3d4e5f678901234"),
        None
    );
}

#[test]
fn needs_you_ids_lists_the_queue() {
    assert_eq!(
        needs_you_ids(&screens("fleet-list-needs-you.json")),
        vec!["local-c3d4e5f678901234".to_owned()]
    );
}

#[test]
fn needs_you_ids_empty_queue() {
    assert!(needs_you_ids(&screens("fleet-list-rows.json")).is_empty());
}

#[test]
fn deny_digit_reads_the_digit_beside_deny() {
    assert_eq!(deny_digit(&screens("needs-you-card.screen.txt")), Some('1'));
}

#[test]
fn deny_digit_none_without_a_card() {
    assert_eq!(deny_digit(&screens("fleet-view-here.screen.txt")), None);
}

#[test]
fn opened_notice_reads_the_title_for_herdr() {
    assert_eq!(
        opened_notice(&screens("open-in-pane-notice.screen.txt")),
        Some("<title>".to_owned())
    );
}

#[test]
fn opened_notice_refuses_the_tmux_text() {
    assert_eq!(
        opened_notice("Opened \"<title>\" in a new tmux pane.\n"),
        None
    );
    assert_eq!(
        opened_notice(&screens("open-in-pane-not-in-herdr.screen.txt")),
        None
    );
}

#[test]
fn worktree_line_reads_branch_and_commit() {
    assert_eq!(
        worktree_line(&screens("worktree-launch.out.txt"), "smoke-abcd1234"),
        Some("abcdef123456".to_owned())
    );
}

#[test]
fn worktree_line_absent() {
    assert_eq!(
        worktree_line(&screens("worktree-launch.out.txt"), "smoke-other"),
        None
    );
    assert_eq!(
        worktree_line(&screens("bg-launch-refusal.err.txt"), "smoke-abcd1234"),
        None
    );
}

#[test]
fn launched_id_reads_the_plain_text_line() {
    assert_eq!(
        launched_id(&screens("worktree-launch.out.txt")),
        Some("local-f678901234567890".to_owned())
    );
}
