#![cfg(feature = "mock")]
mod common;
use common::*;
use serde_json::json;
use std::sync::Arc;
use talaria_workflow::engine::{Effort, EngineEvent, Isolation, MapResolver, OutcomeKind};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn returns_text_and_objects() {
    let backend = backend(
        r#"{"rules":[{"match":{"prompt_equals":"text"},"outcome":{"text":"hello"}},{"match":{"prompt_equals":"object"},"outcome":{"object":{"n":2}}}]}"#,
    );
    let (outcome, _) = run("return [await agent('text'), await agent('object', {schema:{type:'object',properties:{n:{type:'number'}}}})];", backend).await;
    assert_eq!(result(outcome), json!(["hello", {"n":2}]));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn failed_and_skipped_return_null() {
    let backend = backend(
        r#"{"rules":[{"match":{"prompt_equals":"bad"},"outcome":{"failed":"provider error"}},{"match":{"prompt_equals":"skip"},"outcome":"skipped"}]}"#,
    );
    let (outcome, events) = run("return [await agent('bad'), await agent('skip')];", backend).await;
    assert_eq!(result(outcome), json!([null, null]));
    let kinds = events
        .into_iter()
        .filter_map(|event| match event {
            EngineEvent::AgentFinished { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            OutcomeKind::Failed {
                reason: "provider error".into()
            },
            OutcomeKind::Skipped
        ]
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn request_carries_every_option() {
    let backend = default_backend();
    let body = "await agent(' full prompt ', {label:' label ', phase:'phase', schema:{type:'object',properties:{x:{type:'string'}},required:['x']}, tier:'tier', model:'model', provider:'provider', effort:'xhigh', profile:'fixture', toolsets:['Web','web'], skills:['skill'], isolation:'worktree', maxTurns:5, stallMs:1000}); await agent('same', null); await agent('same', {}); await agent('same', {stallMs:1000}); return 1;";
    let (outcome, events) = run(body, backend.clone()).await;
    assert_eq!(result(outcome), json!(1));
    let requests = backend.requests();
    let first = &requests[0];
    assert_eq!(first.prompt, " full prompt ");
    assert_eq!(first.label, " label ");
    assert_eq!(first.phase.as_deref(), Some("phase"));
    assert_eq!(
        first.schema,
        Some(json!({"type":"object","properties":{"x":{"type":"string"}},"required":["x"]}))
    );
    assert_eq!(first.tier.as_deref(), Some("tier"));
    assert_eq!(first.model.as_deref(), Some("model"));
    assert_eq!(first.provider.as_deref(), Some("provider"));
    assert_eq!(first.effort, Some(Effort::Xhigh));
    assert_eq!(first.profile.as_deref(), Some("fixture"));
    assert_eq!(first.toolsets, Some(vec!["Web".into(), "web".into()]));
    assert_eq!(first.skills, Some(vec!["skill".into()]));
    assert_eq!(first.isolation, Some(Isolation::Worktree));
    assert_eq!(first.max_turns, Some(5));
    assert!(first.workflow_path.is_empty());
    assert!(
        serde_json::to_value(first)
            .unwrap()
            .get("unattended")
            .is_none()
    );
    assert!(
        serde_json::to_value(first)
            .unwrap()
            .get("stallMs")
            .is_none()
    );
    for request in &requests[1..] {
        assert_eq!(request.schema, None);
        assert_eq!(request.phase, None);
        assert_eq!(request.toolsets, None);
        assert_eq!(request.skills, None);
        assert_eq!(
            (
                request.tier.as_ref(),
                request.model.as_ref(),
                request.provider.as_ref(),
                request.profile.as_ref()
            ),
            (None, None, None, None)
        );
        assert_eq!(
            (request.effort, request.isolation, request.max_turns),
            (None, None, None)
        );
        assert_eq!(request.call_id.digest, requests[1].call_id.digest);
    }
    assert_eq!(events.iter().filter(|e| matches!(e, EngineEvent::Log {message, ..} if message.contains("stallMs is ignored"))).count(), 1);
    for (effort, spelling) in [
        (Effort::None, "none"),
        (Effort::Minimal, "minimal"),
        (Effort::Low, "low"),
        (Effort::Medium, "medium"),
        (Effort::High, "high"),
        (Effort::Xhigh, "xhigh"),
        (Effort::Max, "max"),
        (Effort::Ultra, "ultra"),
    ] {
        assert_eq!(serde_json::to_value(effort).unwrap(), json!(spelling));
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn invalid_options_reject() {
    let bad = [
        "agent('')",
        "agent(1)",
        "agent('x', [])",
        "agent('x', 'bad')",
        "agent('x', {modle:'x'})",
        "agent('x', {agentType:'code-reviewer'})",
        "agent('x', {effort:'extreme'})",
        "agent('x', {effort:'HIGH'})",
        "agent('x', {isolation:'copy'})",
        "agent('x', {maxTurns:0})",
        "agent('x', {maxTurns:1.5})",
        "agent('x', {toolsets:[]})",
        "agent('x', {toolsets:['a','a']})",
        "agent('x', {skills:[' ']})",
        "agent('x', {model:null})",
        "agent('x', {label:''})",
        "agent('x', {label:'   '})",
        "agent('x', {tier:'   '})",
        "agent('x', {label:'a'.repeat(121)})",
        "agent('x', {stallMs:-1})",
    ];
    let backend = default_backend();
    for call in bad {
        let body = format!(
            "try {{ await {call}; return 'accepted'; }} catch(e) {{ return [e.name, e.message]; }}"
        );
        let (outcome, _) = run(&body, backend.clone()).await;
        let value = result(outcome);
        assert_eq!(value[0], "WorkflowOptionError", "{call}");
        if call.contains("agentType") {
            let message = value[1].as_str().unwrap();
            for hint in ["tier", "toolsets", "skills"] {
                assert!(message.contains(hint));
            }
        }
    }
    assert!(backend.requests().is_empty());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn schema_well_formedness() {
    let invalid = [
        "{type:'array',properties:{}}",
        "{type:'object'}",
        "{type:'object',properties:{a:{}},required:'a'}",
        "{type:'object',properties:{a:{}},required:['missing']}",
        "{type:'object',properties:{a:{default:()=>1}}}",
        "{type:'object',properties:{a:{default:undefined}}}",
        "{type:'object',properties:{a:{default:new Date(0)}}}",
        "{type:'object',properties:{a:{toJSON(){return 1}}}}",
    ];
    let backend = default_backend();
    for schema in invalid {
        let body = format!(
            "try {{ await agent('x', {{schema:{schema}}}); return 'accepted'; }} catch(e) {{ return e.name; }}"
        );
        let (outcome, _) = run(&body, backend.clone()).await;
        assert_eq!(result(outcome), json!("SchemaError"), "{schema}");
    }
    assert!(backend.requests().is_empty());
    let (outcome, _) = run("return [await agent('a',{schema:{type:'object',properties:{x:{type:'string'}}}}), await agent('b',{schema:{type:'object',properties:{},required:[]}})];", backend.clone()).await;
    assert_eq!(result(outcome), json!(["ok", "ok"]));
    assert_eq!(
        backend.requests()[0].schema,
        Some(json!({"type":"object","properties":{"x":{"type":"string"}}}))
    );
    assert_eq!(
        backend.requests()[1].schema,
        Some(json!({"type":"object","properties":{},"required":[]}))
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn default_label_and_phase() {
    let backend = default_backend();
    let body = "await agent('\\n  first  \\nmore'); await agent('a'.repeat(70)); await agent('é'.repeat(70)); await agent('\\r\\n  Title \\r\\nmore'); phase('A'); await agent('one', {phase:'B'}); await agent('two');";
    let (outcome, _) = run(body, backend.clone()).await;
    assert!(matches!(
        outcome,
        talaria_workflow::engine::RunOutcome::Completed { .. }
    ));
    let requests = backend.requests();
    assert_eq!(requests[0].label, "first");
    assert_eq!(requests[1].label, format!("{}…", "a".repeat(60)));
    assert_eq!(requests[2].label, format!("{}…", "é".repeat(60)));
    assert_eq!(requests[3].label, "Title");
    assert_eq!(requests[4].phase.as_deref(), Some("B"));
    assert_eq!(requests[5].phase.as_deref(), Some("A"));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn agent_limit() {
    let backend = default_backend();
    let mut cfg = config();
    cfg.max_agents = 3;
    let child =
        source("await agent('child'); try { await agent('fourth'); } catch (e) { return e.name; }");
    let resolver = Arc::new(MapResolver::new().with_name("child", &child));
    let (outcome, _) = execute(
        &source("await agent('one'); await agent('two'); return await workflow('child');"),
        cfg,
        backend.clone(),
        resolver,
    )
    .await;
    assert_eq!(result(outcome), json!("AgentLimitError"));
    assert_eq!(backend.requests().len(), 3);
}
