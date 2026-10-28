use serde_json::Value;
use talaria_workflow::staffing::{Roster, RosterReport, RunBudget, StaffingTable};
use talaria_workflow::{
    roster_cli::{LocalParts, RosterErrorKind, build_envelope, drafts_dir, list_workflows},
    skill::{SkillInspection, SkillState},
};
fn local(home: &std::path::Path) -> LocalParts {
    let workflows = home.join("workflows");
    std::fs::create_dir_all(&workflows).unwrap();
    std::fs::write(workflows.join("broken.js"), "export const meta = {").unwrap();
    std::fs::write(
        workflows.join("review-branch.js"),
        "export const meta = {name: 'review-branch', description: 'Review code'};",
    )
    .unwrap();
    let (entries, warnings) = list_workflows(&workflows);
    LocalParts {
        talaria_home: Some(home.into()),
        runs_dir: Some(home.join("runs")),
        workflows_dir: Some(workflows),
        drafts_dir: Some(drafts_dir(home).unwrap()),
        workflows: entries,
        skill: Some(SkillInspection {
            state: SkillState::Current,
            installed_version: Some("1.0.0".into()),
            changed_files: vec![],
        }),
        warnings,
    }
}
fn report(problems: Vec<&str>) -> RosterReport {
    let frames: Vec<Value> = include_str!("data/skill/model-options.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let request = frames
        .iter()
        .find(|v| v["frame"]["method"] == "model.options")
        .unwrap();
    let reply = frames
        .iter()
        .find(|v| v["dir"] == "in" && v["frame"]["id"] == request["frame"]["id"])
        .unwrap();
    let mut options = reply["frame"]["result"].clone();
    if !problems.is_empty() {
        for p in options["providers"].as_array_mut().unwrap() {
            if p["slug"] == "ollama-cloud" {
                p["authenticated"] = false.into();
            }
        }
    }
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-22T12:00:00Z")
        .unwrap()
        .to_utc();
    let roster = Roster::from_model_options(&options, "fixture", now).unwrap();
    let table=StaffingTable::parse("[tiers.judge]\nmodel = 'minimax-m3.1-flash'\nprovider = 'ollama-cloud'\n[tiers.survey]\nmodel = 'minimax-m3.1-flash'\nprovider = 'ollama-cloud'\n").unwrap();
    RosterReport::build(
        &table,
        &roster,
        &RunBudget {
            tokens: None,
            money_usd: None,
        },
        now,
    )
}
#[test]
fn envelope_golden_with_fixture_roster() {
    let dir = tempfile::tempdir().unwrap();
    let parts = local(dir.path());
    let expected = report(vec![]);
    let (envelope, exit) = build_envelope("fixture", parts, Ok(expected.clone()));
    let value = serde_json::to_value(envelope).unwrap();
    assert_eq!(exit, 0);
    assert_eq!(value["version"], 1);
    assert_eq!(value["profile"], "fixture");
    assert_eq!(value["report"], serde_json::to_value(&expected).unwrap());
    assert_eq!(value["paths"]["talaria_home"], dir.path().to_str().unwrap());
    assert_eq!(value["workflows"][0]["name"], "broken");
    assert!(value["workflows"][0]["description"].is_null());
    assert_eq!(value["workflows"][1]["name"], "review-branch");
    assert_eq!(value["skill"]["state"], "current");
    assert_eq!(value["skill"]["skill_version_installed"], "1.0.0");
    assert!(!value["warnings"].as_array().unwrap().is_empty());
    assert!(value["error"].is_null());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.path().join("drafts"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}
#[test]
fn check_problems_exit_2_with_full_report() {
    let dir = tempfile::tempdir().unwrap();
    let expected = report(vec!["model is not authenticated"]);
    let (envelope, exit) = build_envelope("fixture", local(dir.path()), Ok(expected.clone()));
    let value = serde_json::to_value(envelope).unwrap();
    assert_eq!(exit, 2);
    assert_eq!(value["report"], serde_json::to_value(&expected).unwrap());
    assert!(value["error"].is_null());
}
#[test]
fn unreadable_roster_exit_1_with_error_kind() {
    for (kind, text) in [
        (RosterErrorKind::HomeUnavailable, "home_unavailable"),
        (RosterErrorKind::Filesystem, "filesystem"),
        (RosterErrorKind::StaffingInvalid, "staffing_invalid"),
        (RosterErrorKind::GatewayUnavailable, "gateway_unavailable"),
        (RosterErrorKind::ProfileUnserved, "profile_unserved"),
        (RosterErrorKind::RosterFailed, "roster_failed"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (envelope, exit) = build_envelope(
            "fixture",
            local(dir.path()),
            Err((kind, "example error".into())),
        );
        let value = serde_json::to_value(envelope).unwrap();
        assert_eq!(exit, 1);
        assert!(value["report"].is_null());
        assert_eq!(value["error"]["kind"], text);
        assert_eq!(value["error"]["message"], "example error");
        assert_eq!(value["paths"]["talaria_home"], dir.path().to_str().unwrap());
    }
}
#[test]
fn recorded_roster_builds_real_report() {
    let report = report(vec![]);
    assert!(report.providers.iter().any(|p| p.slug == "ollama-cloud"));
    assert_eq!(
        report
            .tiers
            .iter()
            .map(|tier| tier.tier.as_str())
            .collect::<Vec<_>>(),
        vec!["judge", "survey"]
    );
    assert!(report.check.is_ok());
    let unauthenticated = report_with_missing_auth();
    assert!(!unauthenticated.check.is_ok());
}
fn report_with_missing_auth() -> RosterReport {
    report(vec!["not signed in"])
}
