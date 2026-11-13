//! Routing raw drafts to shell, empty shell, or prompt.

use talaria_core::session::Event;
use talaria_core::side::{Submission, btw_command, classify};

#[test]
fn bang_prefix_is_a_shell_command() {
    assert_eq!(
        classify("!git status"),
        Submission::Shell("git status".to_string())
    );
    assert_eq!(
        classify("!  ls -la  "),
        Submission::Shell("ls -la".to_string())
    );
}

#[test]
fn bang_not_first_is_a_prompt() {
    assert_eq!(classify(" !ls"), Submission::Prompt);
    assert_eq!(classify("please run !ls"), Submission::Prompt);
    assert_eq!(classify("/btw x"), Submission::Prompt);
}

#[test]
fn blank_bang_is_empty() {
    assert_eq!(classify("!"), Submission::ShellEmpty);
    assert_eq!(classify("!   "), Submission::ShellEmpty);
}

#[test]
fn multiline_command_kept_whole() {
    assert_eq!(
        classify("!echo a\necho b"),
        Submission::Shell("echo a\necho b".to_string())
    );
}

#[test]
fn btw_command_trims_and_blank_means_usage() {
    assert_eq!(
        btw_command("  why? ", 7),
        Event::SideAsked {
            question: "why?".to_string(),
            started_ms: 7,
        }
    );
    assert_eq!(btw_command("   ", 7), Event::SideUsage);
}
