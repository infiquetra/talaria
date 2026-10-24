//! The benchmark workflow file: triggers, job name, permissions, timeout and the action
//! pins it shares with the main check workflow.

mod support;

use std::fs;

fn read_workflow(name: &str) -> String {
    let path = support::workspace_root()
        .join(".github/workflows")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("cannot read workflow {name}: {error}"))
}

/// Trimmed lines that are neither blank nor comments.
fn active_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// The value of a `uses:` line, without any trailing comment: `owner/action@hash`.
fn uses_value(line: &str) -> Option<String> {
    let (_, rest) = line.split_once("uses:")?;
    rest.split_whitespace().next().map(str::to_string)
}

/// Every `uses:` value in the file that starts with `action@`.
fn uses_of(text: &str, action: &str) -> Vec<String> {
    let prefix = format!("{action}@");
    active_lines(text)
        .into_iter()
        .filter_map(uses_value)
        .filter(|value| value.starts_with(&prefix))
        .collect()
}

fn has_key(lines: &[&str], key: &str) -> bool {
    lines.iter().any(|line| {
        line.strip_prefix(key)
            .is_some_and(|rest| rest.starts_with(':'))
    })
}

fn assert_full_hash(value: &str) {
    let hash = value.split_once('@').map(|(_, hash)| hash).unwrap_or("");
    assert!(
        hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()),
        "{value} is not pinned to a full commit hash"
    );
}

/// The one distinct value the main workflow uses for `action`.
fn main_workflow_pin(ci: &str, action: &str) -> String {
    let mut values = uses_of(ci, action);
    values.sort();
    values.dedup();
    assert_eq!(
        values.len(),
        1,
        "the main workflow should pin {action} exactly once, found {values:?}"
    );
    values.remove(0)
}

/// The action of the step that uploads the artifact named `lcov` in the main workflow.
fn lcov_upload_pin(ci: &str) -> String {
    let lines = active_lines(ci);
    let at = lines
        .iter()
        .position(|line| *line == "name: lcov")
        .expect("the main workflow uploads an artifact named lcov");
    let value = lines[..at]
        .iter()
        .rev()
        .take_while(|line| !line.starts_with("- name:"))
        .find_map(|line| uses_value(line))
        .expect("the lcov artifact is uploaded by an action step");
    assert!(
        value.starts_with("actions/upload-artifact@"),
        "the lcov artifact is uploaded by {value}"
    );
    assert_full_hash(&value);
    value
}

#[test]
fn bench_workflow_has_triggers_job_name_and_pins() {
    let ci = read_workflow("ci.yml");
    let bench = read_workflow("bench.yml");
    let lines = active_lines(&bench);

    assert!(
        has_key(&lines, "pull_request"),
        "bench.yml must run on pull_request"
    );
    assert!(
        has_key(&lines, "workflow_dispatch"),
        "bench.yml must run on workflow_dispatch"
    );
    assert!(
        lines.contains(&"name: bench"),
        "bench.yml must define a job named exactly bench"
    );
    assert!(
        lines.iter().any(|line| line.contains("macos-14")),
        "bench.yml must offer macos-14 as a runner"
    );
    assert!(
        lines.contains(&"contents: read"),
        "bench.yml must grant only contents: read"
    );
    assert!(
        lines.contains(&"timeout-minutes: 30"),
        "bench.yml must set timeout-minutes: 30"
    );

    let checkout = main_workflow_pin(&ci, "actions/checkout");
    assert_full_hash(&checkout);
    let checkouts = uses_of(&bench, "actions/checkout");
    assert!(
        !checkouts.is_empty(),
        "bench.yml must check the repository out with {checkout}"
    );
    for value in &checkouts {
        assert_eq!(
            value, &checkout,
            "bench.yml must use the checkout pin the main workflow uses"
        );
    }

    let upload = lcov_upload_pin(&ci);
    let uploads = uses_of(&bench, "actions/upload-artifact");
    assert!(
        !uploads.is_empty(),
        "bench.yml must upload its report with {upload}"
    );
    for value in &uploads {
        assert_eq!(
            value, &upload,
            "bench.yml must use the upload pin the main workflow uses for its lcov artifact"
        );
    }
}
