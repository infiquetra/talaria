use std::{
    fs,
    process::{Command, Output},
};
fn run(args: &[&str], home: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_talaria"))
        .args(args)
        .env("TALARIA_CONFIG_DIR", home.join("cfg"))
        .env("HERMES_HOME", home.join("empty-hermes"))
        .output()
        .unwrap()
}
#[test]
fn list_json_has_four_entries() {
    let t = tempfile::tempdir().unwrap();
    let o = run(&["workflow", "builtins", "list", "--json"], t.path());
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 4);
    assert!(
        v.as_array()
            .unwrap()
            .iter()
            .all(|x| x["installed"] == "not-installed")
    );
}
#[test]
fn show_prints_exact_source() {
    let t = tempfile::tempdir().unwrap();
    let o = run(&["workflow", "builtins", "show", "review-branch"], t.path());
    assert!(o.status.success());
    assert_eq!(
        String::from_utf8(o.stdout).unwrap(),
        include_str!("../../talaria-workflow/workflows/review-branch.js")
    );
}
#[test]
fn install_prints_one_line_each() {
    let t = tempfile::tempdir().unwrap();
    let o = run(&["workflow", "builtins", "install"], t.path());
    assert!(o.status.success());
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(text.lines().next().unwrap().starts_with("created folder"));
    assert_eq!(
        text.lines().filter(|s| s.starts_with("created ")).count(),
        5
    );
}
#[test]
fn unknown_name_exits_2() {
    let t = tempfile::tempdir().unwrap();
    let o = run(
        &["workflow", "builtins", "show", "no-such-workflow"],
        t.path(),
    );
    assert_eq!(o.status.code(), Some(2));
}
#[test]
fn check_reports_missing_tiers_with_exit_2() {
    let t = tempfile::tempdir().unwrap();
    let file = t.path().join("staffing.toml");
    fs::write(
        &file,
        "[tiers.survey]\nmodel='m'\nprovider='p'\n[tiers.judge]\nmodel='m'\nprovider='p'\n",
    )
    .unwrap();
    let o = run(
        &[
            "workflow",
            "builtins",
            "check",
            "review-branch",
            "--staffing",
            file.to_str().unwrap(),
        ],
        t.path(),
    );
    assert_eq!(o.status.code(), Some(2));
    let text = String::from_utf8(o.stdout).unwrap();
    for i in 0..3 {
        assert!(text.contains(&format!("reviewers[{i}]: tier 'reviewer-{}'", i + 1)));
    }
    assert!(text.contains("missing tiers:"));
}
#[test]
fn check_ok_with_fixture_staffing() {
    let t = tempfile::tempdir().unwrap();
    let file = t.path().join("staffing.toml");
    let data = ["survey", "judge", "reviewer-1", "reviewer-2", "reviewer-3"]
        .iter()
        .map(|n| format!("[tiers.{n}]\nmodel='m'\nprovider='p'\n"))
        .collect::<String>();
    fs::write(&file, data).unwrap();
    let o = run(
        &[
            "workflow",
            "builtins",
            "check",
            "review-branch",
            "--staffing",
            file.to_str().unwrap(),
        ],
        t.path(),
    );
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("ok"));
}
#[test]
fn run_installs_missing_builtins_first() {
    let t = tempfile::tempdir().unwrap();
    let o = run(&["workflow", "run", "review-branch"], t.path());
    assert!(String::from_utf8_lossy(&o.stderr).contains("installed built-in workflows into"));
    for n in [
        "review-branch",
        "research",
        "fix-failing-tests",
        "ultracode",
    ] {
        assert!(
            t.path()
                .join("cfg/workflows")
                .join(format!("{n}.js"))
                .is_file()
        );
    }
}
