#![cfg(feature = "mock")]
mod common;
use common::*;
use serde_json::json;
use std::sync::Arc;
use talaria_workflow::engine::{MapResolver, ResolveError, WorkflowRef, WorkflowResolver};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn child_workflow_runs_inline() {
    let backend = default_backend();
    let child = source("return {one:await agent('one'),two:await agent('two')};");
    let resolver = Arc::new(MapResolver::new().with_name("child", &child));
    let (outcome, _) = execute(
        &source("return await workflow('child');"),
        config(),
        backend.clone(),
        resolver,
    )
    .await;
    assert_eq!(result(outcome), json!({"one":"ok","two":"ok"}));
    for request in backend.requests() {
        assert_eq!(request.workflow_path, vec!["name:child@74234e98afe7"]);
    }

    let backend = common::backend(
        r#"{"default":{"text":"ok"},"rules":[{"match":{"prompt_contains":"first 1"},"outcome":{"text":"ok"},"delay_ms":200},{"match":{"prompt_contains":"first 2"},"outcome":{"text":"ok"},"delay_ms":100}]}"#,
    );
    let child = source(
        "phase('child '+args.x); await agent('first '+args.x); await agent('second '+args.x); return args.x;",
    );
    let resolver = Arc::new(
        MapResolver::new()
            .with_name("a", &child)
            .with_name("b", &child),
    );
    let body = "phase('parent'); return await parallel([() => workflow('a',{x:1}), () => workflow('b',{x:2}), () => agent('parent')]);";
    let (outcome, _) = execute(&source(body), config(), backend.clone(), resolver).await;
    assert_eq!(result(outcome), json!([1, 2, "ok"]));
    let requests = backend.requests();
    for request in &requests {
        if request.prompt == "parent" {
            assert!(request.workflow_path.is_empty());
            assert_eq!(request.phase.as_deref(), Some("parent"));
        } else {
            let (path, number) = if request.prompt.ends_with('1') {
                ("name:a@5041bf1f713d", "1")
            } else {
                ("name:b@5e2b030a4a0f", "2")
            };
            assert_eq!(request.workflow_path, vec![path]);
            assert_eq!(
                request.phase.as_deref(),
                Some(format!("child {number}").as_str())
            );
        }
    }
    assert_eq!(requests.len(), 5);

    let resolver = Arc::new(MapResolver::new().with_name("child", &source("return NaN;")));
    let body = "let errors=[]; try {await workflow('child',{x:()=>1})} catch(e){errors.push(e.name)} try {await workflow('child')} catch(e){errors.push(e.name)} return errors;";
    let (outcome, _) = execute(&source(body), config(), default_backend(), resolver).await;
    assert_eq!(
        result(outcome),
        json!(["WorkflowOptionError", "WorkflowScriptError"])
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn nesting_errors_are_catchable() {
    let nested = source("await agent('first'); return await workflow('grandchild');");
    let nested_immediate = source("return await workflow('grandchild');");
    let parse = "export const meta = {name: BAD, description:'x'};";
    let dynamic = source("await import('x');");
    let syntax = format!("{}\nconst = ;", HEADER);
    let resolver = Arc::new(
        MapResolver::new()
            .with_name("nested", &nested)
            .with_name("nested-immediate", &nested_immediate)
            .with_name("parse", parse)
            .with_name("dynamic", &dynamic)
            .with_name("syntax", &syntax)
            .with_path("only-path", &source("return 1;")),
    );
    assert_eq!(
        resolver.resolve(&WorkflowRef::Name("only-path".into())),
        Err(ResolveError::NotFound)
    );
    assert!(
        resolver
            .resolve(&WorkflowRef::ScriptPath("only-path".into()))
            .is_ok()
    );
    let body = "const errors=[]; for(const name of ['nested','nested-immediate','missing','only-path','parse','dynamic','syntax']) {try {await workflow(name)} catch(e) {errors.push([e.name,e.message])}} return errors;";
    let (outcome, _) = execute(&source(body), config(), default_backend(), resolver).await;
    let errors = result(outcome);
    assert_eq!(errors[0][0], "NestedWorkflowError");
    assert_eq!(errors[1][0], "NestedWorkflowError");
    assert_eq!(errors[2][0], "WorkflowNotFoundError");
    assert_eq!(errors[3][0], "WorkflowNotFoundError");
    for index in 4..7 {
        assert_eq!(errors[index][0], "WorkflowScriptError");
    }
    assert!(errors[4][1].as_str().unwrap().contains("line 1"));
    assert!(errors[5][1].as_str().unwrap().contains("line 2"));
    assert!(errors[6][1].as_str().unwrap().contains("line 3"));
}
