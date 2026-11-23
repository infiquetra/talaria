use std::path::PathBuf;
use talaria_workflow::{
    script_check::{CheckOptions, CheckReport, check_script},
    staffing::StaffingTable,
};

const META: &str = "export const meta = { name: 'sample', description: 'A sample.' };\n";

fn options(staffing: &str) -> CheckOptions {
    CheckOptions {
        args: None,
        workflows_dir: PathBuf::from("/nonexistent-workflows"),
        script_dir: PathBuf::from("/nonexistent-script-dir"),
        cwd: PathBuf::from("/nonexistent-cwd"),
        staffing: StaffingTable::parse(staffing).unwrap(),
    }
}
const TIERS: &str = "[tiers.survey]\nmodel = 'm'\nprovider = 'p'\n";
async fn check(body: &str) -> CheckReport {
    check_script(&format!("{META}{body}"), &options(TIERS)).await
}
fn messages(report: &CheckReport) -> Vec<String> {
    report.errors.iter().map(|e| e.message.clone()).collect()
}

#[tokio::test]
async fn an_acceptable_script_passes_and_reaches_its_agents_without_running_them() {
    let report = check(
        "const a = await agent('one', { tier: 'survey' });\nconst b = await agent('two', { schema: { type: 'object', properties: { n: { type: 'integer' } }, required: ['n'] } });\nreturn b.n;\n",
    )
    .await;
    assert!(report.ok, "{:?}", report.errors);
    assert_eq!(report.name.as_deref(), Some("sample"));
    assert_eq!(report.probe.outcome, "completed");
    assert_eq!(report.probe.agent_calls, 2);
    assert_eq!(report.probe.tiers, ["survey"]);
}

#[tokio::test]
async fn a_top_level_return_is_not_a_syntax_error() {
    assert!(check("return 1;\n").await.ok);
}

#[tokio::test]
async fn a_syntax_error_names_its_line_and_column() {
    let report = check("const a = 1;\nconst b = ;\nreturn a;\n").await;
    assert!(!report.ok);
    let error = &report.errors[0];
    assert_eq!(error.kind, "syntax");
    assert_eq!(error.line, Some(3));
    assert!(error.column.is_some_and(|c| c >= 1), "{error:?}");
    assert!(!report.probe.ran);
}

#[tokio::test]
async fn a_syntax_error_on_the_first_body_line_has_its_column_in_the_file() {
    // The meta block is on line 1; put the error on line 2 col 11.
    let report = check("const a = ;\n").await;
    let error = &report.errors[0];
    assert_eq!((error.line, error.column), (Some(2), Some(11)), "{error:?}");
}

#[tokio::test]
async fn an_unbalanced_quote_is_a_syntax_error() {
    let report = check("const a = 'oops;\nreturn a;\n").await;
    assert_eq!(report.errors[0].kind, "syntax");
    assert_eq!(report.errors[0].line, Some(2));
}

#[tokio::test]
async fn a_missing_meta_block_is_reported_with_its_kind() {
    let report = check_script("return 1;\n", &options(TIERS)).await;
    assert!(!report.ok);
    assert_eq!(report.errors[0].kind, "meta");
    assert!(!report.probe.ran);
}

#[tokio::test]
async fn a_bad_meta_field_is_reported() {
    let report = check_script(
        "export const meta = { name: 'bad name', description: 'x' };\nreturn 1;\n",
        &options(TIERS),
    )
    .await;
    assert_eq!(report.errors[0].kind, "meta");
    assert!(report.errors[0].message.contains("name"));
}

#[tokio::test]
async fn an_unknown_meta_key_is_only_a_warning() {
    let report = check_script(
        "export const meta = { name: 'a', description: 'x', colour: 'red' };\nreturn 1;\n",
        &options(TIERS),
    )
    .await;
    assert!(report.ok);
    assert!(report.warnings[0].message.contains("colour"));
}

#[tokio::test]
async fn an_import_is_reported_with_its_position() {
    let report = check("const a = 1;\nconst fs = await import('fs');\nreturn fs;\n").await;
    assert_eq!(report.errors[0].kind, "import");
    assert_eq!(report.errors[0].line, Some(3));
    // A static import is not legal inside the script's function wrapper.
    let report = check("import fs from 'fs';\nreturn 1;\n").await;
    assert_eq!(report.errors[0].kind, "syntax");
    assert_eq!(report.errors[0].line, Some(2));
}

#[tokio::test]
async fn an_option_the_engine_refuses_is_an_error() {
    let report = check("await agent('hello', { modle: 'x' });\n").await;
    assert!(!report.ok);
    assert_eq!(report.errors[0].kind, "option");
    assert!(report.errors[0].message.contains("unknown option `modle`"));
}

#[tokio::test]
async fn a_provider_without_a_model_is_an_error() {
    let report = check("await agent('hello', { provider: 'p' });\n").await;
    assert!(
        messages(&report)[0].contains("needs a model"),
        "{:?}",
        report.errors
    );
}

#[tokio::test]
async fn a_tier_missing_from_the_staffing_table_is_an_error() {
    let report = check("await agent('hello', { tier: 'builder' });\n").await;
    let text = messages(&report).join("\n");
    assert!(
        text.contains("tier 'builder' is not in the staffing table"),
        "{text}"
    );
    assert!(text.contains("survey"));
}

#[tokio::test]
async fn a_tier_with_no_staffing_table_says_so() {
    let report = check_script(
        &format!("{META}await agent('x', {{ tier: 'builder' }});\n"),
        &options(""),
    )
    .await;
    assert!(messages(&report)[0].contains("needs a staffing table"));
}

#[tokio::test]
async fn a_model_with_a_tier_is_left_to_the_run() {
    let report = check("await agent('x', { tier: 'builder', model: 'm', provider: 'p' });\n").await;
    assert!(report.ok, "{:?}", report.errors);
}

#[tokio::test]
async fn a_script_that_trips_over_a_stand_in_answer_gets_a_warning_not_an_error() {
    let report = check("const r = await agent('x');\nreturn r.missing.deeper;\n").await;
    assert!(report.ok, "{:?}", report.errors);
    assert_eq!(report.probe.outcome, "failed");
    assert_eq!(report.warnings[0].kind, "probe");
}

#[tokio::test]
async fn nondeterministic_calls_are_errors() {
    let report = check("log(String(Math.random()));\n").await;
    assert!(!report.ok);
    assert!(report.errors[0].message.contains("Math.random()"));
}

#[tokio::test]
async fn a_nested_workflow_that_does_not_compile_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("child.js"),
        "export const meta = { name: 'child', description: 'x' };\nconst a = ;\n",
    )
    .unwrap();
    let mut opts = options(TIERS);
    opts.workflows_dir = dir.path().into();
    let report = check_script(&format!("{META}await workflow('child');\n"), &opts).await;
    assert!(!report.ok);
    assert!(
        report.errors[0].message.contains("child"),
        "{:?}",
        report.errors
    );
}

#[tokio::test]
async fn args_reach_the_probe() {
    let mut opts = options(TIERS);
    opts.args = Some(serde_json::json!({"lens": "x"}));
    let report = check_script(
        &format!("{META}await agent('p', {{ label: args.lens }});\n"),
        &opts,
    )
    .await;
    assert!(report.ok, "{:?}", report.errors);
    assert_eq!(report.probe.agent_calls, 1);
}

#[tokio::test]
async fn literal_model_pairs_are_listed_but_not_judged() {
    let report = check("await agent('x', { model: 'm1', provider: 'p1' });\n").await;
    assert!(report.ok);
    assert_eq!(report.named_routes.len(), 1);
    assert_eq!(report.named_routes[0].model, "m1");
}

#[tokio::test]
async fn a_syntax_error_on_the_meta_line_has_its_column_in_the_file() {
    let source = "export const meta = { name: 'a', description: 'x' }; const b = ;\nreturn 1;\n";
    let report = check_script(source, &options(TIERS)).await;
    let error = &report.errors[0];
    assert_eq!(error.kind, "syntax");
    assert_eq!(error.line, Some(1));
    assert_eq!(
        error.column,
        Some(u32::try_from(source.find(" ;").unwrap() + 2).unwrap()),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_body_that_closes_the_wrapper_early_cannot_hang_the_check() {
    let started = std::time::Instant::now();
    let report = check("});\nfor (;;) {}\n(async function () {\n").await;
    assert!(!report.ok);
    assert_eq!(report.errors[0].kind, "syntax");
    assert!(
        report.errors[0]
            .message
            .contains("while it was being compiled"),
        "{:?}",
        report.errors
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
}

#[tokio::test]
async fn a_nested_script_path_is_found_through_the_cwd_option() {
    let cwd = tempfile::tempdir().unwrap();
    std::fs::write(
        cwd.path().join("child.js"),
        "export const meta = { name: 'child', description: 'x' };\nreturn 1;\n",
    )
    .unwrap();
    let source = format!("{META}await workflow({{ scriptPath: 'child.js' }});\n");
    let without = check_script(&source, &options(TIERS)).await;
    assert!(!without.ok);
    let mut with = options(TIERS);
    with.cwd = cwd.path().into();
    assert!(check_script(&source, &with).await.ok);
}
