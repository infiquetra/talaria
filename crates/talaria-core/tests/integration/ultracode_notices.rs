//! Skill notices explain Ultracode's dependency and preserve recovery instructions.

use talaria_core::ultracode::notice;

fn explains_skill_role(text: &str) {
    for phrase in ["substantive prompts", "Talaria workflows", "skill"] {
        assert!(
            text.contains(phrase),
            "notice must explain the skill's role ({phrase:?}): {text}"
        );
    }
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
