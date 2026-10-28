#![cfg(feature = "mock")]
mod common;
use common::*;
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use talaria_workflow::engine::{self, MapResolver, RunOutcome, ScriptError};
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn host_apis_are_absent() {
    let names = [
        "require",
        "process",
        "fetch",
        "XMLHttpRequest",
        "WebSocket",
        "setTimeout",
        "setInterval",
        "setImmediate",
        "std",
        "os",
        "Deno",
        "Bun",
        "crypto",
        "performance",
        "Intl",
        "WeakRef",
        "FinalizationRegistry",
        "navigator",
        "console",
    ];
    let expression = names
        .iter()
        .map(|name| format!("typeof {name}"))
        .collect::<Vec<_>>()
        .join(",");
    let (outcome, _) = run(&format!("return [{expression}];"), default_backend()).await;
    assert_eq!(result(outcome), json!(vec!["undefined"; names.len()]));
    let (outcome, _) = run("console.log('x');", default_backend()).await;
    assert_eq!(failure(outcome).name, "ReferenceError");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn import_is_refused_before_running() {
    let script = source("try { await import('fs') } catch (e) {}\nawait agent('x');");
    let expected = script.lines().nth(1).unwrap().find("import").unwrap() as u32 + 1;
    assert_eq!(
        engine::parse_script(&script).unwrap_err(),
        ScriptError::ImportNotAllowed {
            line: 2,
            column: expected
        }
    );
    for body in [
        "const x = { 'import': n => n }; return x.import(1);",
        "const s = 'import('; // import('x')\n return s;",
        "return /import\\(/.test('x');",
    ] {
        assert!(engine::parse_script(&source(body)).is_ok(), "{body}");
    }
    let backend = default_backend();
    let (outcome, _) = run("import x from 'x';\nawait agent('never');", backend.clone()).await;
    let error = failure(outcome);
    assert_eq!((error.name.as_str(), error.line), ("SyntaxError", Some(2)));
    let (outcome, _) = run(
        "try { await new Function(\"return import('fs')\")() } catch (e) {}\nawait agent('never');",
        backend.clone(),
    )
    .await;
    assert_eq!(failure(outcome).name, "ImportNotAllowed");
    assert!(backend.requests().is_empty());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn clock_and_randomness_throw() {
    for (call, hint) in [
        ("Date.now()", "now()"),
        ("Date()", "args"),
        ("new Date()", "now()"),
        ("Math.random()", "random(key)"),
    ] {
        let body =
            format!("try {{ {call}; return null; }} catch (e) {{ return [e.name, e.message]; }}");
        let (outcome, _) = run(&body, default_backend()).await;
        let value = result(outcome);
        assert_eq!(value[0], "DeterminismError");
        assert!(value[1].as_str().unwrap().contains(call));
        assert!(value[1].as_str().unwrap().contains(hint));
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn dates_with_arguments_still_work() {
    let (outcome, _) = run("return [new Date(0).toISOString(), Date.UTC(2026, 9, 22), Date.parse('2026-10-22T00:00:00Z'), new Date(0) instanceof Date];", default_backend()).await;
    assert_eq!(
        result(outcome),
        json!([
            "1970-01-01T00:00:00.000Z",
            1792627200000_i64,
            1792627200000_i64,
            true
        ])
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn date_escape_hatches_are_closed() {
    for expr in [
        "new (new Date(0).constructor)()",
        "Reflect.construct(Date, [])",
    ] {
        let (outcome, _) = run(&format!("return {expr};"), default_backend()).await;
        assert_eq!(failure(outcome).name, "DeterminismError");
    }
    for expr in ["Date = function() {}", "Math.random = () => 1"] {
        let (outcome, _) = run(&format!("{expr};"), default_backend()).await;
        assert_eq!(failure(outcome).name, "TypeError");
    }
    let (outcome, _) = run(
        "return [typeof performance, typeof Intl, typeof WeakRef];",
        default_backend(),
    )
    .await;
    assert_eq!(
        result(outcome),
        json!(["undefined", "undefined", "undefined"])
    );
}

#[tokio::test(flavor = "current_thread")]
async fn runaway_loop_is_stopped() {
    for body in ["while (true) {}", "for (;;) await null;"] {
        let mut cfg = config();
        cfg.max_sync_slice = Duration::from_millis(200);
        let started = Instant::now();
        let (outcome, _) = execute(
            &source(body),
            cfg,
            default_backend(),
            Arc::new(MapResolver::new()),
        )
        .await;
        assert!(failure(outcome).message.contains("without waiting"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    let backend = backend(
        r#"{"rules":[{"match":{"prompt_equals":"slow"},"outcome":{"text":"ok"},"delay_ms":600}]}"#,
    );
    let mut cfg = config();
    cfg.max_sync_slice = Duration::from_millis(200);
    let (outcome, _) = execute(
        &source(
            "await agent('slow'); let n = 0; for (let i=0; i<1000000; i++) n += i; return 'ok';",
        ),
        cfg,
        backend,
        Arc::new(MapResolver::new()),
    )
    .await;
    assert_eq!(result(outcome), json!("ok"));
    for body in ["while (true) {}", "for (;;) await null;"] {
        let parsed = engine::parse_script(&source(body)).unwrap();
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            trigger.cancel();
        });
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let outcome = engine::run(
            parsed,
            config(),
            default_backend(),
            Arc::new(MapResolver::new()),
            tx,
            cancel,
        )
        .await;
        thread.join().unwrap();
        assert_eq!(
            outcome,
            RunOutcome::Aborted {
                reason: "cancelled".into()
            }
        );
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn memory_and_stack_limits_hold() {
    let mut cfg = config();
    cfg.memory_limit_bytes = 8 * 1024 * 1024;
    let (outcome, _) = execute(
        &source("let a=[]; for(let i=0;;i++) a.push('x'.repeat(1024*1024) + i);"),
        cfg,
        default_backend(),
        Arc::new(MapResolver::new()),
    )
    .await;
    assert!(
        failure(outcome)
            .message
            .to_lowercase()
            .contains("out of memory")
    );
    let (outcome, _) = run("function f() { return f(); } f();", default_backend()).await;
    assert!(
        failure(outcome)
            .message
            .to_lowercase()
            .contains("stack overflow")
    );
    let (outcome, _) = run("return 1;", default_backend()).await;
    assert_eq!(result(outcome), json!(1));
}
