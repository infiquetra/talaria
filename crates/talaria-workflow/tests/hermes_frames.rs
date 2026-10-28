#[path = "hermes_common/mod.rs"]
mod common;

use proptest::prelude::*;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use talaria_workflow::{
    engine::{AgentRequest, Effort, Isolation},
    hermes::{
        CREATE_KEYS, ExplicitLaunch, FrameError, LaunchDefaults, LaunchResolver, WorktreeProvider,
        create_params, submit_params,
    },
};

fn absent(params: &Map<String, Value>, key: &str) {
    assert!(!params.contains_key(key), "{key}");
}

#[test]
fn create_params_closed_key_set() {
    let params = create_params(&common::launch(), "req-1").unwrap();
    let keys = common::keys_of(&params);
    let expected: BTreeSet<String> = CREATE_KEYS.into_iter().map(str::to_owned).collect();
    assert_eq!(keys, expected);
    assert_eq!(
        CREATE_KEYS,
        [
            "request_id",
            "source",
            "model",
            "provider",
            "reasoning",
            "cwd",
            "cwd_explicit",
            "toolsets",
            "skills",
            "max_turns",
        ]
    );
    assert_eq!(params.get("source").and_then(Value::as_str), Some("tool"));
    assert_eq!(params.get("reasoning").and_then(Value::as_str), Some("low"));
    assert_eq!(
        params.get("cwd").and_then(Value::as_str),
        Some("/work/proj")
    );
    assert_eq!(params.get("cwd_explicit"), Some(&Value::Bool(true)));
    assert_eq!(params.get("toolsets"), Some(&serde_json::json!(["file"])));
    assert_eq!(params.get("skills"), Some(&serde_json::json!(["skill"])));
    assert_eq!(params.get("max_turns"), Some(&serde_json::json!(4)));
    for key in [
        "profile",
        "title",
        "hidden",
        "yolo",
        "ignore_rules",
        "api_key",
        "base_url",
        "client_env",
        "run_budget",
        "unattended",
        "model_override",
    ] {
        absent(&params, key);
    }
}

#[test]
fn submit_params_exact_keys() {
    let params = submit_params("session", "sub-1", "hello").unwrap();
    assert_eq!(
        common::keys_of(&params),
        BTreeSet::from([
            "session_id".into(),
            "submission_id".into(),
            "text".into(),
            "finite".into(),
        ])
    );
    assert_eq!(params.get("finite"), Some(&Value::Bool(true)));
    assert_eq!(params.get("text").and_then(Value::as_str), Some("hello"));
    absent(&params, "unattended");
    for key in [
        "queued",
        "model_override",
        "api_text",
        "attachments",
        "profile",
    ] {
        absent(&params, key);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]
    #[test]
    fn create_params_are_deterministic(with_skills in proptest::bool::ANY) {
        let mut launch = common::launch();
        if !with_skills {
            launch.skills.clear();
        }
        let first = serde_json::to_vec(&create_params(&launch, "req-1").unwrap()).unwrap();
        let second = serde_json::to_vec(&create_params(&launch, "req-1").unwrap()).unwrap();
        prop_assert_eq!(first.clone(), second);
        let round: talaria_workflow::hermes::ResolvedLaunch =
            serde_json::from_slice(&serde_json::to_vec(&launch).unwrap()).unwrap();
        let third = serde_json::to_vec(&create_params(&round, "req-1").unwrap()).unwrap();
        prop_assert_eq!(first, third);
    }
}

#[test]
fn effort_maps_to_reasoning() {
    for effort in [
        Effort::None,
        Effort::Minimal,
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::Xhigh,
        Effort::Max,
        Effort::Ultra,
    ] {
        let mut launch = common::launch();
        launch.effort = Some(effort);
        let params = create_params(&launch, "req-1").unwrap();
        let name = format!("{effort:?}").to_ascii_lowercase();
        assert_eq!(
            params.get("reasoning").and_then(Value::as_str),
            Some(name.as_str())
        );
    }
    let mut launch = common::launch();
    launch.effort = None;
    let params = create_params(&launch, "req-1").unwrap();
    assert!(params.get("reasoning").is_none());
}

#[test]
fn create_never_carries_profile() {
    let mut launch = common::launch();
    launch.profile = "other-profile".into();
    let params = create_params(&launch, "req-1").unwrap();
    absent(&params, "profile");
    assert_eq!(params.get("source").and_then(Value::as_str), Some("tool"));
}

struct Folder(std::path::PathBuf);

impl WorktreeProvider for Folder {
    fn worktree_for(&self, _request: &AgentRequest) -> Result<std::path::PathBuf, String> {
        Ok(self.0.clone())
    }
}

#[test]
fn explicit_launch_requires_model_provider_toolsets() {
    let dir = tempfile::tempdir().unwrap();
    let launch = ExplicitLaunch::new(LaunchDefaults {
        profile: "fixture".into(),
        cwd: dir.path().to_path_buf(),
        toolsets: None,
        worktrees: None,
    });
    let mut tier_only = common::request("hi");
    tier_only.model = None;
    tier_only.provider = None;
    tier_only.tier = Some("high".into());
    let tier_err = launch.resolve(&tier_only).unwrap_err();
    assert_eq!(
        tier_err,
        "tier 'high' needs the staffing table; name model and provider, or run with staffing"
    );
    let mut no_model = common::request("hi");
    no_model.model = None;
    no_model.provider = None;
    no_model.tier = None;
    let no_model_err = launch.resolve(&no_model).unwrap_err();
    assert_eq!(
        no_model_err,
        "no model: name model and provider, or run with staffing"
    );
    let mut no_provider = common::request("hi");
    no_provider.provider = None;
    let provider_err = launch.resolve(&no_provider).unwrap_err();
    assert_eq!(provider_err, "model 'scripted-model' needs a provider");
    let mut no_tools = common::request("hi");
    no_tools.toolsets = None;
    let tools_err = launch.resolve(&no_tools).unwrap_err();
    assert_eq!(
        tools_err,
        "name toolsets for this agent, or set a run default"
    );
    let mut bad_schema = common::request("hi");
    bad_schema.schema = Some(serde_json::json!({"type": "no-such-type"}));
    let schema_err = launch
        .complete(
            &bad_schema,
            "scripted-model".into(),
            "scripted".into(),
            Some(Effort::Low),
        )
        .unwrap_err();
    assert!(
        schema_err.starts_with("schema does not compile: "),
        "{schema_err}"
    );
}

#[test]
fn explicit_launch_worktree_needs_provider() {
    let bare = ExplicitLaunch::new(common::defaults(std::path::PathBuf::from("/work/proj")));
    let mut request = common::request("hi");
    request.isolation = Some(Isolation::Worktree);
    let err = bare
        .complete(&request, "scripted-model".into(), "scripted".into(), None)
        .unwrap_err();
    assert_eq!(err, "worktree isolation needs a worktree provider (P5-07)");
    let dir = tempfile::tempdir().unwrap();
    let provided = ExplicitLaunch::new(LaunchDefaults {
        worktrees: Some(std::sync::Arc::new(Folder(dir.path().to_path_buf()))),
        ..common::defaults(std::path::PathBuf::from("/work/proj"))
    });
    let resolved = provided
        .complete(&request, "scripted-model".into(), "scripted".into(), None)
        .unwrap();
    assert_eq!(resolved.cwd.as_deref(), dir.path().to_str());
    let relative = ExplicitLaunch::new(common::defaults(std::path::PathBuf::from("relative")));
    let mut plain = common::request("hi");
    plain.isolation = None;
    let cwd_err = relative
        .complete(&plain, "scripted-model".into(), "scripted".into(), None)
        .unwrap_err();
    assert_eq!(
        cwd_err,
        "cwd must be an absolute path to an existing directory"
    );
}

#[test]
fn over_long_ids_refused() {
    let request_id = "r".repeat(257);
    let request_err = create_params(&common::launch(), &request_id).unwrap_err();
    assert_eq!(request_err, FrameError::RequestIdLength(257));
    let submission_id = "s".repeat(1025);
    let submission_err = submit_params("session", &submission_id, "text").unwrap_err();
    assert_eq!(submission_err, FrameError::SubmissionIdLength(1025));
}
