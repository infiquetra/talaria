//! The workflow suite types only its prescribed read-only commands.
#[path = "support/scripts5.rs"]
mod scripts5;
mod support;

use scripts5::{exec_5, script_5};
use std::time::Duration;
use support::Call;

fn run() -> scripts5::Exec5 {
    let exec = exec_5(&script_5(&[]), false, false, 1.0, 600);
    assert_eq!(exec.report.exit_code(), 0, "{}", exec.lines());
    exec
}

#[test]
fn nothing_typed_contains_unattended() {
    let exec = run();
    assert!(
        exec.term
            .everything_typed()
            .iter()
            .all(|line| !line.to_ascii_lowercase().contains("unattended"))
    );
}

#[test]
fn slash_lines_are_the_allowed_list() {
    let exec = run();
    let lines: Vec<_> = exec
        .term
        .everything_typed()
        .into_iter()
        .filter(|line| line.starts_with('/'))
        .collect();
    assert_eq!(lines, ["/workflows", "/ultracode status", "/quit", "/quit"]);
}

#[test]
fn submits_wait_at_least_300_milliseconds_before_enter() {
    let exec = run();
    let calls = exec.term.calls();
    for (index, entry) in calls.iter().enumerate() {
        let Call::SendText { pane, text } = &entry.call else {
            continue;
        };
        if !(text.starts_with('/') || text.starts_with("join these two words")) {
            continue;
        }
        let enter = calls[index + 1..].iter().find(|next| matches!(&next.call, Call::SendKeys { pane: target, keys } if target == pane && keys.iter().any(|key| key == "enter"))).expect("submit Enter");
        assert!(
            enter.at.saturating_sub(entry.at) >= Duration::from_millis(300),
            "{text}"
        );
    }
}

#[test]
fn shell_commands_are_scoped_and_use_the_first_pane_after_resume() {
    let exec = run();
    let calls = exec.term.calls();
    let second_quit = calls.iter().rposition(|entry| matches!(&entry.call, Call::Run { text, .. } | Call::SendText { text, .. } if text == "/quit")).expect("resumed quit");
    let shell_read = exec
        .reads
        .iter()
        .find(|read| {
            read.index > second_quit
                && read.pane == support::second_pane().0
                && read.text.contains("$ ")
        })
        .expect("shell read");
    let commands: Vec<_> = calls
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| match &entry.call {
            Call::Run { pane, text } | Call::SendText { pane, text }
                if text.contains("workflow ") && !text.starts_with('/') =>
            {
                Some((index, pane, text))
            }
            _ => None,
        })
        .collect();
    assert!(commands.len() >= 8);
    for (index, pane, line) in commands {
        assert!(index > shell_read.index);
        assert_eq!(pane, &support::first_pane().0);
        assert!(line.contains("TALARIA_CONFIG_DIR="), "{line}");
        assert!(line.contains("TALARIA_RUNS_DIR="), "{line}");
        assert!(
            line.contains(exec.report.scratch.to_string_lossy().as_ref()),
            "{line}"
        );
        for variable in ["TALARIA_CONFIG_DIR", "TALARIA_RUNS_DIR"] {
            let pattern =
                regex::Regex::new(&format!(r#"{variable}=(?:'([^']*)'|"([^"]*)"|([^\s;]+))"#))
                    .unwrap();
            let captures = pattern
                .captures(line)
                .unwrap_or_else(|| panic!("missing {variable}: {line}"));
            let value = (1..=3)
                .find_map(|index| captures.get(index))
                .unwrap()
                .as_str();
            let path = std::path::Path::new(value);
            assert!(
                path.starts_with(&exec.report.scratch)
                    && !path
                        .components()
                        .any(|part| part == std::path::Component::ParentDir),
                "{variable} escapes scratch: {value}"
            );
        }
        assert!(line.contains("exit=$?"), "{line}");
        if line.contains("HERMES_HOME=") {
            assert!(line.contains("workflow run"), "{line}");
        }
    }
}

#[test]
fn no_restart_or_confirmation_key_is_sent() {
    let exec = run();
    assert!(
        exec.term
            .everything_typed()
            .iter()
            .all(|line| !line.contains("--restart"))
    );
    assert!(!exec.term.calls().iter().any(|entry| matches!(&entry.call, Call::SendKeys { keys, .. } if keys.iter().any(|key| ["y", "ctrl+y", "shift+x"].contains(&key.as_str())))));
}

#[test]
fn no_line_runs_hermes_update_or_config_set() {
    let exec = run();
    assert!(
        exec.term
            .everything_typed()
            .iter()
            .all(|line| !line.contains("hermes update") && !line.contains("config set"))
    );
}
