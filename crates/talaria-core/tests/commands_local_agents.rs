//! Red-run test 18 (#242): the `/agents` command forms.
//!
//! Red: `parse_agents_arg` panics in `todo!()`.

use talaria_core::commands::local::{LocalAction, LocalCommandTable};
use talaria_core::subagents::{AgentsArg, parse_agents_arg};

#[test]
fn agents_command_forms() {
    let table = LocalCommandTable::builtin();
    let entry = table.find("agents").expect("/agents");
    assert_eq!(entry.action, LocalAction::Agents);
    assert!(entry.takes_argument);
    assert_eq!(
        table
            .find("/AGENTS")
            .expect("case and slash do not matter")
            .action,
        LocalAction::Agents
    );
    // Issue #242 asks for a `tasks` alias here, but `/tasks` already toggles
    // the checklist (issue #226) and the table forbids a repeated name. Until
    // the phase planner resolves which command owns `/tasks`, the alias stays
    // off and `/tasks` keeps toggling.
    assert_eq!(
        table.find("tasks").expect("/tasks").action,
        LocalAction::Tasks
    );

    for open in ["", "   "] {
        assert_eq!(parse_agents_arg(open), AgentsArg::Open, "{open:?}");
    }
    for pause in ["pause", " pause ", "PAUSE"] {
        assert_eq!(parse_agents_arg(pause), AgentsArg::Pause, "{pause:?}");
    }
    for resume in ["resume", "RESUME"] {
        assert_eq!(parse_agents_arg(resume), AgentsArg::Resume, "{resume:?}");
    }
    assert_eq!(
        parse_agents_arg("frobnicate"),
        AgentsArg::Unknown("frobnicate".to_owned())
    );
    assert_eq!(
        parse_agents_arg("  pause now  "),
        AgentsArg::Unknown("pause now".to_owned())
    );
    assert_eq!(
        AgentsArg::Unknown("frobnicate".to_owned()).unknown_text(),
        Some(
            "Unknown /agents option: frobnicate. Use /agents, /agents pause or /agents resume."
                .to_owned()
        )
    );
    let long = "x".repeat(60);
    let text = AgentsArg::Unknown(long).unknown_text().expect("text");
    assert_eq!(
        text,
        format!(
            "Unknown /agents option: {}. Use /agents, /agents pause or /agents resume.",
            "x".repeat(40)
        )
    );
    assert_eq!(AgentsArg::Open.unknown_text(), None);
}
