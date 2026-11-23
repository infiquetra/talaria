//! Skill notices explain Hivecode's dependency and preserve recovery instructions.

use talaria_core::hivecode::notice;

/// The one way every notice says what Hivecode does with a prompt.
const SKILL_ROLE: &str = "It lets the agent propose a workflow and ask before starting it.";

fn explains_skill_role(text: &str) {
    assert!(
        text.contains(SKILL_ROLE),
        "notice must explain the skill's role in the shared words: {text}"
    );
    never_says_a_prompt_runs_by_itself(text);
}

/// A prompt never starts a run by itself, so no notice may say it does.
fn never_says_a_prompt_runs_by_itself(text: &str) {
    for phrase in [
        "runs as a workflow",
        "run as Talaria workflows",
        "runs substantive",
        "run the task as a workflow",
    ] {
        assert!(!text.contains(phrase), "{phrase:?}: {text}");
    }
}

#[test]
fn every_routing_notice_says_the_agent_asks_first() {
    for text in [notice::ON, notice::ON_REMOTE] {
        assert!(
            text.contains(
                "a plain prompt is now handled as a multi-agent workflow the agent proposes and asks before starting"
            ),
            "{text}"
        );
        assert!(text.ends_with("/hivecode off to stop."), "{text}");
        never_says_a_prompt_runs_by_itself(text);
    }
    never_says_a_prompt_runs_by_itself(&notice::bypassed("goal"));
}

#[test]
fn missing_skill_notice_explains_why_and_preserves_profile_install_command() {
    for profile in ["fixture", "fixture-review"] {
        let text = notice::missing(profile);
        assert!(text.contains(&format!("profile {profile}")));
        assert!(text.contains(&format!(
            "Install it with: talaria workflow skill install --profile {profile}"
        )));
        explains_skill_role(&text);
    }
}

#[test]
fn unavailable_skill_notice_explains_why_and_preserves_force_install_command() {
    let text = notice::unavailable("fixture-review");
    assert!(text.contains("could not be loaded"));
    assert!(text.contains(
        "Reinstall it with: talaria workflow skill install --force --profile fixture-review"
    ));
    explains_skill_role(&text);
}

#[test]
fn unknown_skill_notice_explains_why_and_keeps_connection_guidance() {
    let text = notice::unknown("gateway disconnected");
    assert!(text.contains("gateway disconnected"));
    assert!(text.contains("Try again once connected."));
    explains_skill_role(&text);
}

#[test]
fn hidden_skill_notice_explains_why_and_keeps_collision_guidance() {
    let text = notice::collision("fixture-review");
    assert!(text.contains("profile fixture-review"));
    assert!(text.contains("nothing was sent"));
    assert!(text.contains("Rename that command, then try again."));
    explains_skill_role(&text);
}

#[test]
fn missing_skill_offer_names_the_profile() {
    assert_eq!(
        notice::offer("fixture"),
        "Install talaria-workflows into profile fixture now? [y/N]"
    );
    let missing = notice::missing("fixture");
    assert!(missing.contains("Install it with: talaria workflow skill install --profile fixture"));
    assert!(!missing.contains("now? [y/N]"));
}

#[test]
fn remote_notices_say_runs_start_on_the_gateway_machine() {
    for text in [notice::ON_REMOTE, notice::OPENED_ON_REMOTE] {
        assert!(
            text.contains("Runs start on the gateway's machine and do not show in this window."),
            "{text}"
        );
    }
    assert_eq!(
        notice::ON_REMOTE.replace(
            " Runs start on the gateway's machine and do not show in this window.",
            ""
        ),
        notice::ON
    );
    assert_eq!(
        notice::OPENED_ON_REMOTE.replace(
            " Runs start on the gateway's machine and do not show in this window.",
            ""
        ),
        notice::OPENED_ON
    );
    assert!(notice::REMOTE_RUN_TAIL.contains("gateway's machine"));
    assert!(
        notice::missing_remote("fixture").contains(
            "on the gateway's machine. Install it there with: talaria workflow skill install --profile fixture"
        )
    );
}

#[test]
fn bypass_notice_names_the_command_and_the_way_to_route() {
    assert_eq!(
        notice::bypassed("goal"),
        "Hivecode does not apply to /goal; it goes to Hermes as typed. To have the task handled as a workflow, send it as a plain prompt without /goal."
    );
    assert!(notice::bypassed("q").contains("/q;"));
}

#[test]
fn run_notices_name_key_and_workflows_command() {
    assert_eq!(
        notice::run_started("review-branch", "run-0001", Some("Alt+W")),
        "Workflow run started: review-branch (run-0001). Press Alt+W or type /workflows to see its agents."
    );
    assert_eq!(
        notice::run_started("review-branch", "run-0001", None),
        "Workflow run started: review-branch (run-0001). Type /workflows to see its agents."
    );
    assert_eq!(
        notice::run_finished("review-branch", "run-0001", "completed", Some("Alt+W")),
        "Workflow run completed: review-branch (run-0001). Press Alt+W or type /workflows to see the result."
    );
    assert_eq!(
        notice::run_finished("review-branch", "run-0001", "failed", None),
        "Workflow run failed: review-branch (run-0001). Type /workflows to see the result."
    );
    assert_eq!(notice::BRANCH_CARRIED, "Hivecode stays on in this branch.");
}

#[test]
fn the_switch_is_announced_on_open_and_after_an_interrupt() {
    assert_eq!(
        notice::OPENED_ON,
        "Hivecode is on in this session: a plain prompt is handled as a workflow. /hivecode off to stop."
    );
    assert_eq!(
        notice::STILL_ON,
        "Hivecode is still on: your next plain prompt is handled as a workflow too."
    );
    assert_eq!(notice::STILL_ON_TAIL, "Hivecode is still on.");
}

#[test]
fn a_changed_skill_names_the_files_what_it_causes_and_the_reinstall_command() {
    let changes = [
        "changed:SKILL.md".to_owned(),
        "added:references/extra.md".to_owned(),
    ];
    assert_eq!(
        notice::skill_changed("fixture", &changes),
        "Another program changed the talaria-workflows skill in profile fixture (SKILL.md, extra.md), so the agent may stop proposing workflows. /hivecode reinstall restores Talaria's copy."
    );
    assert_eq!(
        notice::skill_lock("default"),
        "To stop Hermes changing the skill again, run: hermes -p default curator lock talaria-workflows (or, in a session on that profile, type /curator lock talaria-workflows)",
        "without -p Hermes uses the profile chosen with hermes profile use"
    );
    assert_eq!(
        notice::skill_differs("fixture"),
        "The talaria-workflows skill in profile fixture differs from the copy in this Talaria, so the agent may stop proposing workflows. /hivecode reinstall replaces it."
    );
    assert!(notice::skill_outdated("fixture").contains("older than the copy in this Talaria"));
    assert!(notice::skill_outdated("fixture").ends_with("/hivecode reinstall updates it."));
    for text in [
        notice::skill_changed("fixture", &changes),
        notice::skill_differs("fixture"),
        notice::skill_outdated("fixture"),
    ] {
        assert!(
            !text.contains("[y/N]"),
            "nothing waits for an answer: {text}"
        );
    }
}

#[test]
fn a_changed_skill_names_at_most_three_files() {
    let changes: Vec<String> = (0..400)
        .map(|i| format!("added:scripts/__pycache__/helper_{i:03}.cpython-312.pyc"))
        .collect();
    let text = notice::skill_changed("fixture", &changes);
    assert!(text.contains(" and 397 more)"), "{text}");
    assert_eq!(text.matches("helper_").count(), 3, "{text}");
    assert!(rows_at_120(&text) <= 2, "{text}");
    // A file name from another program is cleaned of control characters.
    let hostile = notice::skill_changed("fixture", &["added:evil\n\u{1b}[31m.md".to_owned()]);
    assert!(!hostile.chars().any(char::is_control), "{hostile:?}");
}

#[test]
fn reinstall_confirms_in_one_line() {
    assert_eq!(
        notice::reinstalled("fixture", Some("/home/fixture/.talaria/skills/backups/old")),
        "Reinstalled Talaria's talaria-workflows skill in profile fixture; the old copy is in /home/fixture/.talaria/skills/backups/old."
    );
    assert_eq!(
        notice::reinstalled("fixture", None),
        "Installed Talaria's talaria-workflows skill in profile fixture."
    );
    assert!(notice::reinstall_failed("lock timed out").ends_with(": lock timed out"));
    assert!(
        notice::reinstall_remote("fixture")
            .ends_with("talaria workflow skill install --force --profile fixture")
    );
    assert_eq!(notice::USAGE, "Usage: /hivecode [on|off|status|reinstall]");
}

#[test]
fn missing_setup_is_two_short_notices_each_ending_with_its_command() {
    assert_eq!(
        notice::setup_tiers("fixture", notice::NO_TIERS),
        "Staffing tiers: no tiers are configured, so the agent names every model itself. Add tiers to staffing.toml, then check them with: talaria workflow roster --profile fixture"
    );
    assert_eq!(
        notice::setup_builtins(&["review-branch".to_owned(), "research".to_owned()]),
        "Built-in workflows not installed (review-branch, research), so the agent writes each workflow itself. Install them with: talaria workflow builtins install"
    );
    let unreadable = notice::setup_tiers(
        "fixture",
        &format!("staffing.toml could not be read ({})", "x".repeat(300)),
    );
    assert!(rows_at_120(&unreadable) <= 2, "{unreadable}");
    assert!(unreadable.ends_with("talaria workflow roster --profile fixture"));
}

/// The rows a notice takes at 120 columns, with its `· ` prefix, wrapped at spaces.
fn rows_at_120(text: &str) -> usize {
    let mut rows = 1;
    let mut used = 0;
    for word in format!("\u{b7} {text}").split(' ') {
        let width = word.chars().count();
        let needed = if used == 0 { width } else { used + 1 + width };
        if needed <= 120 {
            used = needed;
        } else {
            rows += 1 + width.saturating_sub(1) / 120;
            used = width % 120;
        }
    }
    rows
}

#[test]
fn every_hivecode_notice_fits_in_two_rows_at_120_columns() {
    let profile = "fixture-review";
    let changes: Vec<String> = (0..9)
        .map(|i| format!("changed:references/a-long-reference-name-{i}.md"))
        .collect();
    let notices = [
        notice::ON.to_owned(),
        notice::ON_REMOTE.to_owned(),
        notice::OPENED_ON.to_owned(),
        notice::OPENED_ON_REMOTE.to_owned(),
        notice::STILL_ON.to_owned(),
        notice::BRANCH_CARRIED.to_owned(),
        notice::OFF.to_owned(),
        notice::NO_SESSION.to_owned(),
        notice::USAGE.to_owned(),
        notice::bypassed("background"),
        notice::offer(profile),
        notice::missing(profile),
        notice::missing_remote(profile),
        notice::unknown(&"gateway disconnected ".repeat(20)),
        notice::unavailable(profile),
        notice::collision(profile),
        notice::skill_changed(profile, &changes),
        notice::skill_lock(profile),
        notice::skill_differs(profile),
        notice::skill_outdated(profile),
        notice::reinstalled(
            profile,
            Some(&format!("/home/{}/backup", "deep/".repeat(40))),
        ),
        notice::reinstall_failed(&"lock timed out ".repeat(20)),
        notice::reinstall_remote(profile),
        notice::setup_tiers(profile, notice::NO_TIERS),
        notice::setup_builtins(&[
            "fix-failing-tests".to_owned(),
            "hivecode".to_owned(),
            "research".to_owned(),
            "review-branch".to_owned(),
        ]),
        notice::status(
            true,
            profile,
            &talaria_core::hivecode::SkillAvailability::Unknown {
                reason: "x ".repeat(200),
            },
        ),
    ];
    for text in notices {
        assert!(
            rows_at_120(&text) <= 2,
            "{} rows: {text}",
            rows_at_120(&text)
        );
    }
}
