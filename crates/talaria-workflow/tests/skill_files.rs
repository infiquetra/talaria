use std::path::Path;
use talaria_workflow::skill::render;
fn files() -> Vec<(String, String)> {
    render("fixture", "talaria", Path::new("/h"))
        .unwrap()
        .into_iter()
        .map(|f| (f.path.to_owned(), String::from_utf8(f.bytes).unwrap()))
        .collect()
}
fn file<'a>(files: &'a [(String, String)], name: &str) -> &'a str {
    &files.iter().find(|(path, _)| path == name).unwrap().1
}
#[test]
fn front_matter_matches_golden() {
    let all = files();
    let golden = include_str!("data/skill/front-matter.md");
    assert!(file(&all, "SKILL.md").starts_with(golden));
}
#[test]
fn no_preprocessing_or_injection_tokens() {
    for (name, text) in files() {
        for forbidden in [
            "!`",
            "${HERMES_",
            "@@",
            "ignore previous instructions",
            "ignore all previous instructions",
            "system prompt",
        ] {
            assert!(
                !text.to_lowercase().contains(&forbidden.to_lowercase()),
                "{name}: {forbidden}"
            );
        }
    }
}
#[test]
fn size_budgets() {
    let all = files();
    let mut total = 0;
    for (name, text) in all {
        assert!(!text.is_empty());
        assert!(text.len() <= if name == "SKILL.md" { 8192 } else { 24576 });
        total += text.len();
    }
    assert!(total <= 65536);
}
#[test]
fn statements_s1_to_s12_present() {
    let all = files();
    let s = file(&all, "SKILL.md");
    for marker in [
        "answer directly",
        "workflow roster --profile",
        "HERMES_HOME=",
        "never invent",
        "drafts_dir",
        "ask",
        "estimate",
        "explicit yes",
        "background=true",
        "session_id=",
        "--detach",
        "\"event\":\"run_finished\"",
        "workflow resume --json",
        "workflow resolve",
        "never choose",
        "workflow stop",
        "--budget-usd",
        "isolation: 'worktree'",
        "can write files",
        "secrets",
        "approval settings",
    ] {
        assert!(s.contains(marker), "missing {marker}");
    }
    for forbidden in ["unless the developer", "not to ask", "skip the question"] {
        assert!(!s.contains(forbidden));
    }
}
#[test]
fn no_bypass_words() {
    for (_, text) in files() {
        for word in ["unattended", "yolo", "approve all"] {
            assert!(!text.to_lowercase().contains(word));
        }
    }
}
#[test]
fn script_options_match_engine() {
    let all = files();
    let reference = file(&all, "references/script-api.md");
    let names: std::collections::BTreeSet<_> = reference
        .lines()
        .filter_map(|line| line.trim().strip_prefix("| `opts."))
        .filter_map(|line| line.split('`').next())
        .collect();
    let source = include_str!("../../../docs/workflows/script-api.md");
    let table = source
        .split("| Option | Value |")
        .nth(1)
        .unwrap()
        .split("\n\n")
        .next()
        .unwrap();
    let expected: std::collections::BTreeSet<_> = table
        .lines()
        .filter_map(|line| line.trim().strip_prefix("| "))
        .flat_map(|line| {
            line.split('|')
                .next()
                .unwrap_or("")
                .split('`')
                .skip(1)
                .step_by(2)
        })
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(names, expected);
}
#[cfg(feature = "mock")]
#[tokio::test(flavor = "current_thread")]
async fn examples_parse_and_complete_on_mock() {
    use std::sync::Arc;
    use talaria_workflow::{
        engine::{self, EngineConfig, Isolation, MapResolver, RunOutcome},
        mock::MockBackend,
    };
    use tokio_util::sync::CancellationToken;
    let all = files();
    let text = file(&all, "references/examples.md");
    let scripts: Vec<_> = text
        .split("```js\n")
        .skip(1)
        .map(|s| s.split("```").next().unwrap())
        .collect();
    assert_eq!(scripts.len(), 3);
    let cases = [
        (
            vec![
                (
                    "review:",
                    serde_json::json!({"findings": [{"id":"one","file":"src/lib.rs","line":1,"title":"t"},{"id":"two","file":"src/lib.rs","line":2,"title":"t"}]}),
                ),
                (
                    "verify:",
                    serde_json::json!({"verdict":"confirmed","reason":"r"}),
                ),
                ("synthesize", serde_json::json!({"summary":"s"})),
            ],
            10,
        ),
        (
            vec![
                (
                    "search:",
                    serde_json::json!({"leads":[{"ref":"a","why":"w"},{"ref":"b","why":"w"}]}),
                ),
                ("read:", serde_json::json!({"summary":"s"})),
                ("synthesize", serde_json::json!({"summary":"s"})),
            ],
            6,
        ),
        (
            vec![
                (
                    "plan",
                    serde_json::json!({"tasks":[{"id":"t1","goal":"g"},{"id":"t2","goal":"g"}]}),
                ),
                (
                    "build:",
                    serde_json::json!({"status":"done","commit":"0000000"}),
                ),
                ("review", serde_json::json!({"summary":"s"})),
            ],
            4,
        ),
    ];
    for (source, (rules, expected)) in scripts.into_iter().zip(cases) {
        let parsed = engine::parse_script(source).unwrap();
        for use_specific in [false, true] {
            let mut entries = Vec::new();
            if use_specific {
                let labels = [
                    "review:correctness",
                    "review:security",
                    "review:simplicity",
                    "verify:correctness-1",
                    "verify:correctness-2",
                    "verify:security-1",
                    "verify:security-2",
                    "verify:simplicity-1",
                    "verify:simplicity-2",
                    "search:1",
                    "search:2",
                    "search:3",
                    "read:a",
                    "read:b",
                    "plan",
                    "build:t1",
                    "build:t2",
                    "synthesize",
                    "review",
                ];
                for label in labels {
                    if let Some((_, object)) =
                        rules.iter().find(|(prefix, _)| label.starts_with(prefix))
                    {
                        let reply = if let Some(lens) = label.strip_prefix("review:") {
                            serde_json::json!({"findings": [{"id":format!("{lens}-1"),"file":"src/lib.rs","line":1,"title":"t"},{"id":format!("{lens}-2"),"file":"src/lib.rs","line":2,"title":"t"}]})
                        } else {
                            object.clone()
                        };
                        entries.push(
                            serde_json::json!({"match":{"label":label},"outcome":{"object":reply}}),
                        );
                    }
                }
            }
            let mock = Arc::new(
                MockBackend::from_json(
                    &serde_json::json!({"default":{"object":{}},"rules":entries}).to_string(),
                )
                .unwrap(),
            );
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();
            let outcome = engine::run(
                parsed.clone(),
                EngineConfig::new("fixture", 0, 0),
                mock.clone(),
                Arc::new(MapResolver::new()),
                tx,
                CancellationToken::new(),
            )
            .await;
            assert!(
                matches!(outcome, RunOutcome::Completed { .. }),
                "{outcome:?}"
            );
            if use_specific {
                assert_eq!(mock.requests().len(), expected);
            }
            for request in mock.requests() {
                let tools = request.toolsets.unwrap();
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
                }
            }
        }
    }
}
#[test]
fn render_is_deterministic_and_substitutes() {
    let first = files();
    assert_eq!(first, files());
    let combined = first.iter().map(|(_, s)| s.as_str()).collect::<String>();
    assert!(combined.contains("HERMES_HOME='/h' talaria"));
    assert!(!combined.contains("@@"));
    for command in ["", "some command", "$(whoami)", "../relative"] {
        assert!(render("fixture", command, Path::new("/h")).is_err());
    }
    for root in ["relative", "/bad'root", "/bad\nroot"] {
        assert!(render("fixture", "talaria", Path::new(root)).is_err());
    }
}
