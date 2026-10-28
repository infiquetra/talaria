#![cfg(feature = "mock")]
mod builtins_common;
use builtins_common::{completed, run};
use std::collections::HashSet;
use talaria_workflow::engine::Isolation;
#[tokio::test(flavor = "current_thread")]
async fn every_request_follows_the_tool_rules() {
    let (out, _, backend) = run(
        "review-branch",
        None,
        include_str!("data/builtins/review-branch/happy.mock.json"),
        true,
    )
    .await;
    let _ = completed(out);
    let mut seen = HashSet::new();
    for r in backend.requests() {
        assert!(r.tier.is_some());
        assert!(r.model.is_none() && r.provider.is_none() && r.effort.is_none());
        assert!(r.label.len() <= 100);
        assert!(seen.insert((r.workflow_path.clone(), r.label.clone())));
        let tools = r.toolsets.unwrap();
        let names: Vec<_> = tools.iter().map(String::as_str).collect();
        assert!(
            [
                vec!["file"],
                vec!["file", "terminal"],
                vec!["web"],
                vec!["todo"]
            ]
            .contains(&names)
        );
        if names.contains(&"file") || names.contains(&"terminal") {
            assert_eq!(r.isolation, Some(Isolation::Worktree))
        } else {
            assert_eq!(r.isolation, None)
        }
    }
}

fn assert_schema_properties_unmasked(value: &serde_json::Value) {
    const MASKED: &[&str] = &[
        "api_key",
        "apikey",
        "token",
        "secret",
        "password",
        "access_token",
        "refresh_token",
        "auth_token",
        "bearer",
        "secret_value",
        "raw_secret",
        "secret_input",
        "key_material",
    ];
    match value {
        serde_json::Value::Object(map) => {
            if let Some(properties) = map.get("properties").and_then(|v| v.as_object()) {
                for key in properties.keys() {
                    assert!(
                        !MASKED.contains(&key.to_lowercase().as_str()),
                        "masked schema property: {key}"
                    );
                }
            }
            for child in map.values() {
                assert_schema_properties_unmasked(child);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                assert_schema_properties_unmasked(child);
            }
        }
        _ => {}
    }
}

#[tokio::test(flavor = "current_thread")]
async fn mock_objects_validate_against_request_schemas() {
    use talaria_workflow::engine::AgentOutcome;
    let cases: &[(&str, &str, Option<&str>)] = &[
        (
            "review-branch",
            include_str!("data/builtins/review-branch/happy.mock.json"),
            None,
        ),
        (
            "review-branch",
            include_str!("data/builtins/review-branch/empty.mock.json"),
            None,
        ),
        (
            "review-branch",
            include_str!("data/builtins/review-branch/reviewer-fails.mock.json"),
            None,
        ),
        (
            "review-branch",
            include_str!("data/builtins/review-branch/many.mock.json"),
            None,
        ),
        (
            "review-branch",
            include_str!("data/builtins/review-branch/synth-ids.mock.json"),
            None,
        ),
        (
            "review-branch",
            include_str!("data/builtins/review-branch/synth-null.mock.json"),
            None,
        ),
        (
            "review-branch",
            include_str!("data/builtins/review-branch/truncate.mock.json"),
            None,
        ),
        (
            "review-branch",
            include_str!("data/builtins/review-branch/other-revision.mock.json"),
            Some(include_str!(
                "data/builtins/review-branch/other-revision.args.json"
            )),
        ),
        (
            "research",
            include_str!("data/builtins/research/happy.mock.json"),
            Some(include_str!("data/builtins/research/happy.args.json")),
        ),
        (
            "research",
            include_str!("data/builtins/research/no-web.mock.json"),
            Some(include_str!("data/builtins/research/no-web.args.json")),
        ),
        (
            "research",
            include_str!("data/builtins/research/web-off.mock.json"),
            Some(include_str!("data/builtins/research/web-off.args.json")),
        ),
        (
            "research",
            include_str!("data/builtins/research/code-off.mock.json"),
            Some(include_str!("data/builtins/research/code-off.args.json")),
        ),
        (
            "research",
            include_str!("data/builtins/research/nothing-readable.mock.json"),
            Some(include_str!(
                "data/builtins/research/nothing-readable.args.json"
            )),
        ),
        (
            "research",
            include_str!("data/builtins/research/synth-fails.mock.json"),
            Some(include_str!("data/builtins/research/synth-fails.args.json")),
        ),
        (
            "fix-failing-tests",
            include_str!("data/builtins/fix-failing-tests/happy.mock.json"),
            None,
        ),
        (
            "fix-failing-tests",
            include_str!("data/builtins/fix-failing-tests/no-failures.mock.json"),
            None,
        ),
        (
            "fix-failing-tests",
            include_str!("data/builtins/fix-failing-tests/unparsed.mock.json"),
            None,
        ),
        (
            "fix-failing-tests",
            include_str!("data/builtins/fix-failing-tests/cap.mock.json"),
            None,
        ),
        (
            "fix-failing-tests",
            include_str!("data/builtins/fix-failing-tests/bad-commit.mock.json"),
            None,
        ),
        (
            "fix-failing-tests",
            include_str!("data/builtins/fix-failing-tests/ff-fails.mock.json"),
            None,
        ),
        (
            "fix-failing-tests",
            include_str!("data/builtins/fix-failing-tests/suite-error.mock.json"),
            None,
        ),
        (
            "fix-failing-tests",
            include_str!("data/builtins/fix-failing-tests/long-ids.mock.json"),
            None,
        ),
        (
            "ultracode",
            include_str!("data/builtins/ultracode/happy.mock.json"),
            Some(include_str!("data/builtins/ultracode/happy.args.json")),
        ),
        (
            "ultracode",
            include_str!("data/builtins/ultracode/blocked.mock.json"),
            Some(include_str!("data/builtins/ultracode/happy.args.json")),
        ),
        (
            "ultracode",
            include_str!("data/builtins/ultracode/bad-choice.mock.json"),
            Some(include_str!("data/builtins/ultracode/happy.args.json")),
        ),
    ];
    for (name, mock, args) in cases {
        let args = args.map(|a| serde_json::from_str(a).unwrap());
        let (_, _, backend) = run(name, args, mock, true).await;
        let mut labels = HashSet::new();
        for request in backend.requests() {
            assert!(request.tier.is_some());
            assert!(
                request.model.is_none() && request.provider.is_none() && request.effort.is_none()
            );
            assert!(request.label.chars().count() <= 100);
            assert!(labels.insert((request.workflow_path.clone(), request.label.clone())));
            let tools = request.toolsets.as_ref().expect("toolsets");
            let names: Vec<_> = tools.iter().map(String::as_str).collect();
            assert!(
                [
                    vec!["file"],
                    vec!["file", "terminal"],
                    vec!["web"],
                    vec!["todo"]
                ]
                .contains(&names)
            );
            if names.contains(&"file") || names.contains(&"terminal") {
                assert_eq!(request.isolation, Some(Isolation::Worktree));
            } else {
                assert_eq!(request.isolation, None);
            }
        }
        let mut completed = 0;
        for (request, outcome) in backend.calls() {
            if let AgentOutcome::Completed(object) = outcome {
                completed += 1;
                let schema = request
                    .schema
                    .as_ref()
                    .expect("completed call must have a schema");
                assert_schema_properties_unmasked(schema);
                let validator = jsonschema::options()
                    .with_draft(jsonschema::Draft::Draft202012)
                    .build(schema)
                    .expect("valid request schema");
                assert!(
                    validator.is_valid(&object),
                    "fixture {name}, call {}: {}",
                    request.label,
                    validator
                        .iter_errors(&object)
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                );
            }
        }
        assert!(completed > 0, "fixture {name} produced no completed calls");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn generated_review_bounds_hold_for_64_cases() {
    use serde_json::json;
    let lenses = [
        "correctness",
        "security",
        "simplicity",
        "performance",
        "tests",
    ];
    let sample: serde_json::Value =
        serde_json::from_str(include_str!("data/builtins/review-branch/happy.mock.json")).unwrap();
    let collector = sample["rules"][0].clone();
    let mut seed = 0x1234_5678_u32;
    for case in 0..64 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let count = if case == 0 {
            3
        } else {
            (seed as usize % 5) + 1
        };
        let max = if case == 0 {
            12
        } else {
            ((seed >> 8) % 20) + 1
        };
        let mut rules = vec![collector.clone()];
        for lens in &lenses[..count] {
            let findings: Vec<_> = (0..10).map(|i| json!({"file":format!("file-{i}.py"),"severity":"high","title":format!("Finding {i}"),"detail":"Example"})).collect();
            rules.push(json!({"match":{"label":format!("review: {lens}")},"outcome":{"object":{"findings":findings}}}));
            for n in 1..=10 {
                rules.push(json!({"match":{"label":format!("verify: {lens}-{n}")},"outcome":{"object":{"verdict":"confirmed","reason":"confirmed"}}}));
            }
        }
        rules.push(json!({"match":{"label":"synthesize"},"outcome":{"object":{"summary":"done","ranked":[],"themes":[]}}}));
        let args = json!({"lenses": &lenses[..count], "maxFindings": max});
        let (out, _, backend) = run(
            "review-branch",
            Some(args),
            &json!({"rules":rules}).to_string(),
            true,
        )
        .await;
        let _ = completed(out);
        let actual = backend.requests().len();
        assert_eq!(actual, 2 + count + (max as usize).min(10 * count));
        assert!(actual <= 2 + count + max as usize);
        if case == 0 {
            assert_eq!(actual, 17);
        }
    }
}
