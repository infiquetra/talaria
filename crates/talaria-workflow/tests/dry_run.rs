#![cfg(feature = "mock")]
use serde_json::{Value, json};
use std::path::PathBuf;
use talaria_workflow::dry_run::{DryRunOptions, parse_args, run};
use tokio_util::sync::CancellationToken;

fn fixture(folder: &str, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(folder)
        .join(name)
}
fn options(script: &str, mock: &str) -> DryRunOptions {
    DryRunOptions {
        script: fixture("scripts", script),
        mock: fixture("mock", mock),
        args: None,
        max_sync_ms: None,
        seed: 0,
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn prints_events_and_result() {
    let mut opts = options("review_shape.js", "review_shape.json");
    opts.args = Some(json!({"lenses":["correctness","security","simplicity"]}));
    let mut first = Vec::new();
    assert_eq!(run(&opts, &mut first, CancellationToken::new()).await, 0);
    let mut second = Vec::new();
    assert_eq!(run(&opts, &mut second, CancellationToken::new()).await, 0);
    assert_eq!(first, second);
    let text = String::from_utf8(first).unwrap();
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(
        lines.last(),
        Some(
            &r#"{"result":{"confirmed":["F1"],"dropped":["F2"],"summary":"one confirmed finding"}}"#
        )
    );
    for line in &lines[..lines.len() - 1] {
        let event: Value = serde_json::from_str(line).unwrap();
        assert!(event.get("event").is_some());
    }

    let mut out = Vec::new();
    assert_eq!(
        run(
            &options("uses_clock.js", "default.json"),
            &mut out,
            CancellationToken::new()
        )
        .await,
        1
    );
    let text = String::from_utf8(out).unwrap();
    let failed: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(failed["failed"]["name"], "DeterminismError");
    assert_eq!(failed["failed"]["line"], 4);
    let mut out = Vec::new();
    assert_eq!(
        run(
            &options("meta_not_literal.js", "default.json"),
            &mut out,
            CancellationToken::new()
        )
        .await,
        2
    );
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 1);
    assert!(
        serde_json::from_str::<Value>(text.trim())
            .unwrap()
            .get("refused")
            .is_some()
    );

    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let opts = options("slow_three.js", "slow.json");
    let mut out = Vec::new();
    let (code, ()) = tokio::join!(run(&opts, &mut out, cancel), async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        trigger.cancel();
    });
    assert_eq!(code, 3);
    let text = String::from_utf8(out).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text.lines().last().unwrap()).unwrap(),
        json!({"aborted":"cancelled"})
    );

    for args in [
        vec![],
        vec!["script.js"],
        vec![
            "script.js",
            "--mock",
            "default.json",
            "--seed",
            "1",
            "--seed",
            "2",
        ],
    ] {
        assert!(parse_args(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err());
    }
    let mut out = Vec::new();
    assert_eq!(
        run(
            &options("uses_clock.js", "malformed.json"),
            &mut out,
            CancellationToken::new()
        )
        .await,
        64
    );
    assert!(out.is_empty());
}
