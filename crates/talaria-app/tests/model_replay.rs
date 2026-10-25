//! Issue 230, scenario "recorded switch reduces to the expected notices".

use std::path::Path;

use hermes_client::ReplaySource;
use serde_json::Value;
use talaria_core::models::{
    Effort, EffortCommand, ModelCommand, ModelCommandOrEffort, ModelsEffect, ModelsInput,
    ModelsState, SavedModels,
};
use talaria_core::mutation::MutationOutcome;

const MODEL_SWITCH: &str = "Model: muse-spark-open on ollama-cloud.";
const EFFORT_LOW: &str = "Effort: low for this session.";
const MODEL_BACK: &str = "Model: minimax-m3.1-flash on ollama-cloud.";

struct Step {
    operation: String,
    payload: Value,
    result: Value,
    events: Vec<(String, Value)>,
}

fn effort_variant(word: &str) -> Effort {
    match word {
        "none" => Effort::None,
        "minimal" => Effort::Minimal,
        "low" => Effort::Low,
        "medium" => Effort::Medium,
        "high" => Effort::High,
        "xhigh" => Effort::Xhigh,
        "max" => Effort::Max,
        "ultra" => Effort::Ultra,
        other => panic!("the recording names an effort this scenario does not: {other}"),
    }
}

fn command_for(step: &Step, session_id: &str) -> ModelsInput {
    let command = match step.operation.as_str() {
        "model" => ModelCommandOrEffort::Model(ModelCommand::Switch {
            model: step.payload["model"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            provider: step.payload["provider"].as_str().map(str::to_string),
            effort: None,
            save_default: false,
        }),
        "reasoning" => {
            let level = step.payload["effort"].as_str().map(effort_variant);
            ModelCommandOrEffort::Effort(EffortCommand::Set {
                level,
                save_default: false,
            })
        }
        other => panic!("the recording uses operation {other}"),
    };
    ModelsInput::Command {
        session_id: Some(session_id.to_string()),
        profile: "fixture".to_string(),
        command: Ok(command),
    }
}

fn notice_text(effects: &[ModelsEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            ModelsEffect::Notice { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn steps_of(source: &ReplaySource) -> (String, i64, i64, Vec<Step>) {
    let mut frames = Vec::new();
    for frame in source.outbound() {
        frames.push((frame.seq, false, &frame.frame));
    }
    for frame in source.inbound() {
        frames.push((frame.seq, true, &frame.frame));
    }
    frames.sort_by_key(|(seq, _, _)| *seq);

    let mut create_id = None;
    let mut create_result = None;
    let mut waiting: Option<(Value, Value)> = None;
    let mut steps: Vec<Step> = Vec::new();
    let mut collecting = false;

    for (_, inbound, frame) in frames {
        let method = frame.get("method").and_then(Value::as_str);
        if !inbound && method == Some("session.create") {
            create_id = frame.get("id").cloned();
            collecting = false;
            continue;
        }
        if inbound && create_result.is_none() && frame.get("id") == create_id.as_ref() {
            create_result = frame.get("result").cloned();
            continue;
        }
        if !inbound && method == Some("session.mutate") {
            waiting = Some((
                frame.get("id").cloned().unwrap_or(Value::Null),
                frame.get("params").cloned().unwrap_or(Value::Null),
            ));
            collecting = false;
            continue;
        }
        if let Some((id, params)) = waiting.clone()
            && inbound
            && frame.get("id") == Some(&id)
        {
            let result = frame.get("result").cloned().unwrap_or(Value::Null);
            steps.push(Step {
                operation: params["operation"].as_str().unwrap_or_default().to_string(),
                payload: params.get("payload").cloned().unwrap_or(Value::Null),
                result,
                events: Vec::new(),
            });
            waiting = None;
            collecting = true;
            continue;
        }
        if collecting
            && inbound
            && method == Some("event")
            && let Some(step) = steps.last_mut()
        {
            let params = frame.get("params").cloned().unwrap_or(Value::Null);
            let type_name = params["type"].as_str().unwrap_or_default().to_string();
            let payload = params.get("payload").cloned().unwrap_or(Value::Null);
            step.events.push((type_name, payload));
        }
    }

    let created = create_result.expect("the recording creates a session");
    let session_id = created["session_id"]
        .as_str()
        .expect("create result names a session")
        .to_string();
    let revision = created["revision"].as_i64().expect("create revision");
    let generation = created["execution_generation"]
        .as_i64()
        .expect("create generation");
    (session_id, revision, generation, steps)
}

/// Issue 230, scenario "recorded switch reduces to the expected notices".
#[test]
fn recorded_switch_reduces_to_expected_notices() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/frames/model-switch.jsonl");
    let source = ReplaySource::open(&path).expect("the model switch recording opens");
    assert!(!source.truncated(), "the recording ends on a complete line");
    let (session_id, revision, generation, steps) = steps_of(&source);
    assert_eq!(steps.len(), 3, "the recording has three session changes");
    assert_eq!(steps[0].result["model"].as_str(), Some("muse-spark-open"));
    assert!(steps[0].result.get("effort").is_none());
    assert_eq!(steps[0].result["provider"].as_str(), Some("ollama-cloud"));
    assert_eq!(steps[1].result["effort"].as_str(), Some("low"));
    assert_eq!(
        steps[2].result["model"].as_str(),
        Some("minimax-m3.1-flash")
    );
    assert_eq!(steps[2].result["provider"].as_str(), Some("ollama-cloud"));

    let mut state = ModelsState::new("record-switch", Ok(SavedModels::empty()));
    let mut notices = notice_text(&state.update(ModelsInput::SessionOpened {
        session_id: session_id.clone(),
        revision: Some(revision),
        execution_generation: Some(generation),
    }));

    for step in &steps {
        let effects = state.update(command_for(step, &session_id));
        notices.extend(notice_text(&effects));
        let request = effects
            .iter()
            .find_map(|effect| match effect {
                ModelsEffect::Mutate(request) => Some(request.clone()),
                _ => None,
            })
            .expect("the command builds one mutate");
        let effects = state.update(ModelsInput::MutationReturned {
            request,
            outcome: MutationOutcome::Committed(step.result.clone()),
        });
        notices.extend(notice_text(&effects));
        for (type_name, payload) in &step.events {
            let effects = state.update(ModelsInput::SessionEvent {
                session_id: session_id.clone(),
                type_name: type_name.clone(),
                payload: payload.clone(),
            });
            notices.extend(notice_text(&effects));
        }
    }

    assert_eq!(
        notices,
        vec![
            MODEL_SWITCH.to_string(),
            EFFORT_LOW.to_string(),
            MODEL_BACK.to_string(),
        ]
    );
    let fences = state.fences(&session_id);
    assert_eq!(fences.revision, Some(3));
    assert_eq!(fences.execution_generation, Some(3));
}
