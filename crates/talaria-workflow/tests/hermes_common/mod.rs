#![allow(dead_code)]
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use talaria_workflow::{
    engine::{AgentRequest, CallId, Effort},
    hermes::{LaunchDefaults, NewManifest, OnUnknown, ResolvedLaunch, SettledTurn},
};

pub fn data(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/hermes")
        .join(rel)
}

pub fn read_json(rel: &str) -> Value {
    let text = std::fs::read_to_string(data(rel)).unwrap();
    serde_json::from_str(&text).unwrap()
}

pub fn request(prompt: &str) -> AgentRequest {
    AgentRequest {
        call_id: CallId {
            digest: "digest".into(),
            occurrence: 0,
        },
        label: "label".into(),
        phase: None,
        prompt: prompt.into(),
        schema: None,
        tier: None,
        model: Some("scripted-model".into()),
        provider: Some("scripted".into()),
        effort: Some(Effort::Low),
        profile: Some("fixture".into()),
        toolsets: Some(vec!["file".into()]),
        skills: Some(vec!["skill".into()]),
        isolation: None,
        max_turns: Some(4),
        workflow_path: Vec::new(),
    }
}

pub fn launch() -> ResolvedLaunch {
    ResolvedLaunch {
        profile: "fixture".into(),
        model: "scripted-model".into(),
        provider: Some("scripted".into()),
        effort: Some(Effort::Low),
        toolsets: vec!["file".into()],
        skills: vec!["skill".into()],
        max_turns: Some(4),
        isolation: None,
        cwd: Some("/work/proj".into()),
    }
}

pub fn defaults(cwd: PathBuf) -> LaunchDefaults {
    LaunchDefaults {
        profile: "fixture".into(),
        cwd,
        toolsets: Some(vec!["file".into()]),
        worktrees: None,
    }
}

pub fn manifest() -> NewManifest {
    NewManifest {
        workflow_name: "fixture".into(),
        args: None,
        started_at_ms: 1,
        seed: 7,
        profile: "fixture".into(),
        on_unknown: OnUnknown::Wait,
        talaria_version: "0.7.0-dev".into(),
        hermes: json!({}),
        roster: json!({}),
        approval_policy: json!({}),
    }
}

pub fn settled(turn: u32, outcome: &str, text: Option<&str>, meta: Value) -> SettledTurn {
    SettledTurn {
        turn,
        outcome: outcome.into(),
        final_text: text.map(str::to_owned),
        result_meta: meta,
        usage: None,
    }
}

pub fn keys_of(value: &serde_json::Map<String, Value>) -> std::collections::BTreeSet<String> {
    value.keys().cloned().collect()
}
