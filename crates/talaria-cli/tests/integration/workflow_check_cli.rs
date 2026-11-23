use clap::CommandFactory;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const GOOD: &str = "export const meta = { name: 'sample', description: 'A sample.' };\nconst r = await agent('say hi', { schema: { type: 'object', properties: { ok: { type: 'boolean' } } } });\nreturn r;\n";

fn talaria(args: &[&str], home: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(args)
        .env("TALARIA_CONFIG_DIR", home.join("cfg"))
        .env("HERMES_HOME", home.join("no-hermes"))
        .output()
        .unwrap()
}

#[test]
fn every_workflow_subcommand_has_a_help_line() {
    let command = talaria_cli::workflow::WorkflowArgs::command();
    for sub in command.get_subcommands().filter(|s| s.get_name() != "help") {
        let about = sub.get_about().map(ToString::to_string).unwrap_or_default();
        assert!(
            !about.trim().is_empty(),
            "`workflow {}` has no description",
            sub.get_name()
        );
    }
}

#[test]
fn the_printed_help_gives_every_command_a_description() {
    let t = tempfile::tempdir().unwrap();
    let out = talaria(&["workflow", "--help"], t.path());
    let text = String::from_utf8(out.stdout).unwrap();
    let commands: Vec<&str> = text
        .lines()
        .skip_while(|l| *l != "Commands:")
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .collect();
    assert!(commands.len() >= 10, "{text}");
    for line in commands {
        let mut words = line.split_whitespace();
        let name = words.next().unwrap();
        assert!(
            words.next().is_some(),
            "`{name}` is listed without a description"
        );
    }
}

#[test]
fn compile_is_described_as_the_kanban_compiler_and_check_is_named() {
    let t = tempfile::tempdir().unwrap();
    let out = talaria(&["workflow", "--help"], t.path());
    let text = String::from_utf8(out.stdout).unwrap();
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("compile"))
        .unwrap();
    assert!(line.contains("kanban"), "{line}");
    assert!(line.contains("check"), "{line}");
}

#[test]
fn an_acceptable_script_exits_zero_and_json_is_stable() {
    let t = tempfile::tempdir().unwrap();
    let script = t.path().join("good.workflow");
    fs::write(&script, GOOD).unwrap();
    let out = talaria(
        &["workflow", "check", script.to_str().unwrap(), "--json"],
        t.path(),
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["version"], 1);
    assert_eq!(v["ok"], true);
    assert_eq!(v["name"], "sample");
    assert_eq!(v["errors"], serde_json::json!([]));
    assert_eq!(v["probe"]["outcome"], "completed");
    assert_eq!(v["probe"]["agent_calls"], 1);
    // Nothing was run: no run folder appears.
    assert!(!t.path().join("cfg").join("runs").exists());
}

#[test]
fn a_syntax_error_exits_two_with_line_and_column() {
    let t = tempfile::tempdir().unwrap();
    let script = t.path().join("bad.workflow");
    fs::write(
        &script,
        "export const meta = { name: 'bad', description: 'x' };\nconst a = 'oops;\nreturn a;\n",
    )
    .unwrap();
    let out = talaria(&["workflow", "check", script.to_str().unwrap()], t.path());
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("error (syntax) at line 2, column 11"),
        "{text}"
    );
    assert!(text.contains("not acceptable"), "{text}");
    let json = talaria(
        &["workflow", "check", script.to_str().unwrap(), "--json"],
        t.path(),
    );
    assert_eq!(json.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["errors"][0]["kind"], "syntax");
    assert_eq!(v["errors"][0]["line"], 2);
}

#[test]
fn an_unreadable_script_and_bad_arguments_are_reported_as_input_problems() {
    let t = tempfile::tempdir().unwrap();
    let missing = t.path().join("missing.workflow");
    let out = talaria(
        &["workflow", "check", missing.to_str().unwrap(), "--json"],
        t.path(),
    );
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["errors"][0]["kind"], "input");
    let script = t.path().join("good.workflow");
    fs::write(&script, GOOD).unwrap();
    let out = talaria(
        &[
            "workflow",
            "check",
            script.to_str().unwrap(),
            "--args",
            "{nope",
            "--json",
        ],
        t.path(),
    );
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("arguments JSON")
    );
}

#[test]
fn a_tier_the_staffing_table_lacks_fails_the_check() {
    let t = tempfile::tempdir().unwrap();
    let script = t.path().join("tiered.workflow");
    fs::write(&script, "export const meta = { name: 'tiered', description: 'x' };\nawait agent('go', { tier: 'builder' });\n").unwrap();
    let table = t.path().join("staffing.toml");
    fs::write(&table, "[tiers.survey]\nmodel = 'm'\nprovider = 'p'\n").unwrap();
    let out = talaria(
        &[
            "workflow",
            "check",
            script.to_str().unwrap(),
            "--staffing",
            table.to_str().unwrap(),
        ],
        t.path(),
    );
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("tier 'builder' is not in the staffing table"),
        "{text}"
    );
}
