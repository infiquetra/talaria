use serde_json::{Value, json};
use talaria_workflow::{
    builtins::{BUILTINS, tiers_for},
    engine::parse_script,
};

#[test]
fn every_builtin_parses_with_matching_meta() {
    for builtin in BUILTINS {
        let script = parse_script(builtin.source).unwrap();
        assert!(script.warnings.is_empty());
        assert_eq!(script.meta.name, builtin.name);
        assert!(!script.meta.description.is_empty());
        assert!(
            script
                .meta
                .when_to_use
                .as_ref()
                .is_some_and(|s| !s.is_empty())
        );
        let expected = match builtin.name {
            "review-branch" => vec!["Collect", "Review", "Verify", "Synthesize"],
            "research" => vec!["Plan", "Sweep", "Read", "Synthesize"],
            "fix-failing-tests" => vec!["Find", "Fix", "Verify"],
            "ultracode" => vec!["Understand", "Design", "Choose", "Implement", "Review"],
            _ => panic!("unknown workflow"),
        };
        assert_eq!(
            script
                .meta
                .phases
                .iter()
                .map(|p| p.title.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        for title in expected {
            assert_eq!(
                builtin.source.matches(&format!("phase('{title}')")).count(),
                1,
                "{}: {title}",
                builtin.name
            );
        }
        assert_eq!(script.meta.description, builtin.summary);
    }
}
#[test]
fn sources_avoid_forbidden_constructs() {
    for builtin in BUILTINS {
        for forbidden in [
            "unattended",
            "Date.now",
            "Math.random",
            "new Date()",
            "now(",
            "random(",
            "console.",
            "import(",
            "model:",
            "provider:",
            "effort:",
            "delegation",
        ] {
            assert!(
                !builtin.source.contains(forbidden),
                "{} contains {forbidden}",
                builtin.name
            );
        }
        assert!(builtin.source.contains("checkArgs("));
        assert!(builtin.source.contains("fence("));
    }
}
#[test]
fn registry_matches_workflow_files() {
    let names = BUILTINS.iter().map(|b| b.name).collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "fix-failing-tests",
            "research",
            "review-branch",
            "ultracode"
        ]
    );
    let mut files = std::fs::read_dir("workflows")
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(
        files,
        names.iter().map(|n| format!("{n}.js")).collect::<Vec<_>>()
    );
    for b in BUILTINS {
        assert_eq!(
            b.source,
            std::fs::read_to_string(format!("workflows/{}.js", b.name)).unwrap()
        );
        let expected = match b.name {
            "review-branch" => 17,
            "research" => 13,
            "fix-failing-tests" => 11,
            "ultracode" => 23,
            _ => unreachable!(),
        };
        assert_eq!(b.max_agents_default, expected);
        assert!(expected <= 25);
    }
}
#[test]
fn registry_tier_defaults_match_the_scripts() {
    for b in BUILTINS {
        let text = b
            .source
            .split_once("const DEFAULT_TIERS = ")
            .unwrap()
            .1
            .split_once(';')
            .unwrap()
            .0;
        let actual: Value = serde_json::from_str(text).unwrap();
        let mut expected = serde_json::Map::new();
        for role in b.roles {
            let value = if role.list {
                json!(role.default_tiers)
            } else {
                json!(role.default_tiers[0])
            };
            expected.insert(role.role.into(), value);
        }
        assert_eq!(actual, Value::Object(expected), "{}", b.name);
    }
}
#[test]
fn tiers_for_lists_each_agent_slot() {
    let review = BUILTINS.iter().find(|b| b.name == "review-branch").unwrap();
    let slots = tiers_for(review, None).unwrap();
    assert_eq!(
        slots,
        vec![
            ("collector".into(), "survey".into()),
            ("reviewers[0]".into(), "reviewer-1".into()),
            ("reviewers[1]".into(), "reviewer-2".into()),
            ("reviewers[2]".into(), "reviewer-3".into()),
            ("verifier".into(), "judge".into()),
            ("synthesizer".into(), "judge".into())
        ]
    );
    let args = json!({"lenses":["correctness","security","simplicity","performance","tests"],"tiers":{"reviewers":["a","b"]}});
    let slots = tiers_for(review, Some(&args)).unwrap();
    for i in 0..5 {
        assert!(slots.contains(&(
            format!("reviewers[{i}]"),
            if i % 2 == 0 { "a" } else { "b" }.into()
        )));
    }
    let ultra = BUILTINS.iter().find(|b| b.name == "ultracode").unwrap();
    assert!(
        tiers_for(ultra, None)
            .unwrap()
            .iter()
            .any(|(k, _)| k == "review.collector")
    );
    assert!(tiers_for(review, Some(&json!({"tiers":{"reviewers":[]}}))).is_err());
}

#[cfg(feature = "mock")]
#[tokio::test(flavor = "current_thread")]
async fn argument_errors_start_no_agent() {
    use std::sync::Arc;
    use talaria_workflow::{
        engine::{self, EngineConfig, MapResolver, RunOutcome},
        mock::MockBackend,
    };
    use tokio_util::sync::CancellationToken;
    let cases = [
        (
            "review-branch",
            json!({"unknown":1}),
            "review-branch: unknown argument 'unknown'; allowed: base, focus, head, lenses, maxDiffChars, maxFindings, tiers",
        ),
        (
            "review-branch",
            json!({"lenses":[]}),
            "review-branch: 'lenses' must be 1 to 5 distinct values from correctness, security, simplicity, performance, tests, got []",
        ),
        (
            "review-branch",
            json!({"lenses":["speed"]}),
            "review-branch: 'lenses' must be 1 to 5 distinct values from correctness, security, simplicity, performance, tests, got [\"speed\"]",
        ),
        (
            "review-branch",
            json!({"focus":""}),
            "review-branch: 'focus' must be a non-blank string of at most 500 characters",
        ),
        (
            "review-branch",
            json!({"tiers":{"reviewers":[]}}),
            "review-branch: 'tiers.reviewers' must be an array of 1 to 5 tier names, got []",
        ),
        (
            "review-branch",
            json!({"tiers":{"reviewers":["Bad"]}}),
            "review-branch: 'tiers.reviewers[0]' must be a tier name, got \"Bad\"",
        ),
        (
            "review-branch",
            json!({"tiers":{"bogus":"x"}}),
            "review-branch: unknown argument 'tiers.bogus'; allowed: collector, reviewers, synthesizer, verifier",
        ),
        (
            "ultracode",
            json!({"task":"x","review":3}),
            "ultracode: 'review' must be an object, got 3",
        ),
        (
            "ultracode",
            json!({"task":"x","review":{"base":"main"}}),
            "ultracode: review.base and review.head are set by ultracode",
        ),
        (
            "review-branch",
            json!({"base":"-rf"}),
            "review-branch: 'base' must be a git ref such as main or HEAD~2, got \"-rf\"",
        ),
        (
            "review-branch",
            json!({"maxFindings":0}),
            "review-branch: 'maxFindings' must be an integer from 1 to 20, got 0",
        ),
        (
            "research",
            json!({}),
            "research: 'question' is required and must be a non-blank string of at most 2000 characters",
        ),
        (
            "fix-failing-tests",
            json!({"testCommand":"one\ntwo"}),
            "fix-failing-tests: 'testCommand' must be one line of at most 500 characters",
        ),
        (
            "ultracode",
            json!({"task":"x","review":{"maxFindings":0}}),
            "ultracode: 'review.maxFindings' must be an integer from 1 to 20, got 0",
        ),
    ];
    for (name, args, expected) in cases {
        let script = BUILTINS.iter().find(|b| b.name == name).unwrap();
        let mut cfg = EngineConfig::new("test-run", 0, 0);
        cfg.args = Some(args);
        let backend = Arc::new(
            MockBackend::from_json(r#"{"default":{"failed":"unexpected"},"rules":[]}"#).unwrap(),
        );
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let outcome = engine::run(
            parse_script(script.source).unwrap(),
            cfg,
            backend.clone(),
            Arc::new(MapResolver::new()),
            tx,
            CancellationToken::new(),
        )
        .await;
        match outcome {
            RunOutcome::Failed { error } => assert_eq!(error.message, expected),
            other => panic!("expected failure: {other:?}"),
        }
        assert!(backend.requests().is_empty());
    }
}
