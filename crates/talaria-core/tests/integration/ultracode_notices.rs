//! Skill notices explain Ultracode's dependency and preserve recovery instructions.

use talaria_core::ultracode::notice;

/// The one way every notice says what Ultracode does with a prompt.
const SKILL_ROLE: &str = "With Ultracode on, a plain prompt is handled as a workflow: the agent proposes a workflow and asks before starting it.";

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
        assert!(text.contains("is handled as a workflow"), "{text}");
        assert!(
            text.contains("the agent proposes a workflow and asks before starting it"),
            "{text}"
        );
        never_says_a_prompt_runs_by_itself(text);
    }
    for text in [notice::ROUTED, notice::ROUTED_REMOTE, notice::ONE_SHOT] {
        assert!(
            text.contains("handling this prompt as a workflow"),
            "{text}"
        );
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
fn enabled_notice_already_explains_the_skill_role() {
    explains_skill_role(notice::ON);
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
fn on_notice_says_slash_commands_are_not_routed_and_where_runs_appear() {
    for phrase in [
        "Slash commands such as /goal still go to Hermes as typed",
        "A started run shows above the composer",
        "/workflows lists every run",
        "stays on in a /branch copy",
        "other sessions keep their own setting",
        "/ultracode off to stop",
    ] {
        assert!(notice::ON.contains(phrase), "{phrase:?}: {}", notice::ON);
    }
}

#[test]
fn remote_on_notice_says_runs_start_on_the_gateway_machine() {
    explains_skill_role(notice::ON_REMOTE);
    for phrase in [
        "Slash commands such as /goal still go to Hermes as typed",
        "Runs start on the gateway's machine, so this window cannot show them",
        "talaria workflow list there lists them",
        "stays on in a /branch copy",
        "/ultracode off to stop",
    ] {
        assert!(
            notice::ON_REMOTE.contains(phrase),
            "{phrase:?}: {}",
            notice::ON_REMOTE
        );
    }
    assert!(!notice::ON_REMOTE.contains("above the composer"));
    assert!(!notice::ON_REMOTE.contains("/workflows lists"));
}

#[test]
fn routed_notice_is_one_short_line() {
    assert_eq!(
        notice::ROUTED,
        "Ultracode: handling this prompt as a workflow."
    );
    assert_eq!(
        notice::ROUTED_REMOTE,
        "Ultracode: handling this prompt as a workflow on the gateway's machine."
    );
    assert_eq!(
        notice::ONE_SHOT,
        "Ultracode (keyword ultracode): handling this prompt as a workflow."
    );
}

#[test]
fn bypass_notice_names_the_command_and_the_way_to_route() {
    assert_eq!(
        notice::bypassed("goal"),
        "Ultracode does not apply to /goal; it goes to Hermes as typed. To have the task handled as a workflow, send it as a plain prompt without /goal."
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
    assert_eq!(notice::BRANCH_CARRIED, "Ultracode stays on in this branch.");
}
