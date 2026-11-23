use std::path::Path;
use talaria_workflow::skill::{
    COMMAND_ALLOWANCE, PROFILE_ALLOWANCE, RENDER_HEADROOM, SKILL_FILES, TOTAL_BUDGET,
    budgeted_size, file_budget, render,
};
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
    let golden = include_str!("../data/skill/front-matter.md");
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
    // The budget is counted on the embedded templates, each placeholder at a fixed allowance, so
    // it does not depend on the machine's Hermes folder path.
    let mut total = 0;
    for (name, template) in SKILL_FILES {
        let counted = budgeted_size(template);
        assert!(counted > 0);
        assert!(counted <= file_budget(name), "{name}: {counted} bytes");
        total += counted;
    }
    assert!(total <= TOTAL_BUDGET, "{total} bytes in all");
    assert_eq!(file_budget("SKILL.md"), 8192);
    assert_eq!(file_budget("references/commands.md"), 24576);
}
#[test]
fn the_budget_counts_placeholders_at_their_allowance() {
    assert_eq!(budgeted_size("abc"), 3);
    assert_eq!(
        budgeted_size("a @@TALARIA_COMMAND@@ b"),
        4 + COMMAND_ALLOWANCE
    );
    assert_eq!(
        budgeted_size("@@TALARIA_PROFILE@@\r\n@@TALARIA_PROFILE@@"),
        1 + 2 * PROFILE_ALLOWANCE
    );
}
#[test]
fn a_skill_renders_for_any_realistic_hermes_folder_path() {
    for length in [1_usize, 20, 80, 300, 600] {
        let root = format!("/{}", "h".repeat(length - 1));
        let files = render("fixture", "talaria", Path::new(&root))
            .unwrap_or_else(|e| panic!("path of {length} bytes: {e}"));
        assert!(
            files[0].bytes.len() <= 8192 + RENDER_HEADROOM,
            "SKILL.md is {} bytes",
            files[0].bytes.len()
        );
    }
}
#[test]
fn a_very_long_path_is_refused_by_naming_its_length() {
    let root = format!("/{}", "h".repeat(1000));
    let error = render("fixture", "talaria", Path::new(&root))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Hermes folder path (1001 bytes)"), "{error}");
    assert!(error.contains("over the limit of"), "{error}");
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
        "workflow result --json",
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
    let source = include_str!("../../../../docs/workflows/script-api.md");
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
#[test]
fn the_lead_plans_and_never_builds_the_deliverable_itself() {
    let all = files();
    let s = file(&all, "SKILL.md");
    for marker in [
        "Never write the deliverable in this session",
        "first phase",
        "after a background completion",
        "workflow result --json <run-id>",
    ] {
        assert!(s.contains(marker), "missing {marker}");
    }
    for (name, text) in &all {
        let lower = text.to_lowercase();
        for forbidden in ["runnable slice", "vertical slice", "scaffold first"] {
            assert!(!lower.contains(forbidden), "{name}: {forbidden}");
        }
    }
}
#[test]
fn drafts_use_an_extension_hermes_does_not_lint() {
    let all = files();
    let s = file(&all, "SKILL.md");
    assert!(
        s.contains(".workflow`"),
        "the skill names the draft extension"
    );
    for (name, text) in &all {
        for js in ["relative.js", "<script.js>"] {
            assert!(!text.contains(js), "{name} still names a .js draft");
        }
    }
}
#[test]
fn waiting_follows_the_hermes_wait_cap_and_never_loops() {
    let all = files();
    for (name, text) in &all {
        for part in text.split("timeout=").skip(1) {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            let seconds: u32 = digits.parse().unwrap_or(0);
            assert!(seconds <= 180, "{name}: wait timeout {seconds} exceeds 180");
        }
        assert!(
            !text.to_lowercase().contains("repeat the wait"),
            "{name}: still tells the lead to repeat the wait"
        );
    }
    let s = file(&all, "SKILL.md");
    assert!(s.contains("timeout=180"));
    assert!(s.contains("Wait at most once"));
    assert!(s.contains("do not wait again, the completion notice resumes you"));
}
#[test]
fn reading_a_result_never_spends() {
    // `resume` on a run with a pending restart request opens a paid execution, so every place
    // that reads a finished run's result names `workflow result`, and resume is kept for
    // continuing a run.
    let all = files();
    let s = file(&all, "SKILL.md");
    assert!(
        !s.contains("workflow resume --json"),
        "SKILL.md reads a result with resume"
    );
    for marker in [
        "read its result with `workflow result`",
        "which never spends",
        "Read a finished run's result with:",
        "Resume only to continue an interrupted run or after restart with the developer's yes",
        "with a restart request pending, resume runs agents and costs money",
    ] {
        assert!(s.contains(marker), "missing {marker}");
    }
    let commands = file(&all, "references/commands.md");
    for marker in [
        "workflow result --json <run-id>\n",
        "never opens an execution, never spends, and ignores pending restart requests",
        "On a run with a pending restart request it runs agents again and costs money",
        "never with resume",
    ] {
        assert!(commands.contains(marker), "missing {marker} in commands.md");
    }
}
#[test]
fn the_skill_describes_the_json_parked_event() {
    let all = files();
    let s = file(&all, "SKILL.md");
    for marker in [
        "{\"event\":\"parked\",...}",
        "`answer` is the `workflow resolve` command",
        "replace `<choice>` with that choice",
        "never choose one yourself",
    ] {
        assert!(s.contains(marker), "missing {marker}");
    }
    assert!(
        !s.contains("needs operator"),
        "the skill runs --json, which prints no such line"
    );
    let commands = file(&all, "references/commands.md");
    for marker in [
        "cause (unknown, lost, identity_conflict or stopped)",
        "waiting_for \"operator\", choices, answer",
        "use_reply",
        "ending in a literal `<choice>`",
        "instead of its bare `talaria`",
        "--resolution <choice>",
    ] {
        assert!(commands.contains(marker), "missing {marker} in commands.md");
    }
}
#[test]
fn reporting_keeps_worker_commits_and_separates_attention_from_facts() {
    let all = files();
    let s = file(&all, "SKILL.md");
    for marker in [
        "Workers commit on their own branches; report their commits for integration.",
        "Report the closing line's `Attention:` items as problems",
        "phases not started as facts",
        "whether the script skips it on purpose",
        "stopped by a person, Talaria or Hermes, not by a provider limit",
    ] {
        assert!(s.contains(marker), "missing {marker}");
    }
    assert!(!s.contains("by a person or by Talaria"));
    let reference = file(&all, "references/script-api.md");
    assert!(reference.contains("\"agent stopped: "));
    assert!(reference.contains("parks with cause `stopped`"));
    assert!(!reference.contains("stopped in Hermes (outcome"));
}
#[test]
fn the_skill_never_claims_to_be_locked_or_names_private_machines() {
    for (name, text) in files() {
        for forbidden in ["is locked", "owner's machines", "beside the workflows"] {
            assert!(!text.contains(forbidden), "{name}: {forbidden}");
        }
    }
    let all = files();
    let commands = file(&all, "references/commands.md");
    assert!(commands.contains("Do not edit this skill; record lessons with this command instead."));
    assert!(commands.contains("the developer may edit or prune it by hand"));
    let source = include_str!("../../../talaria-cli/src/workflow_lessons.rs");
    assert!(!source.contains("locked") && !source.contains("owner's machines"));
}
#[test]
fn the_new_project_and_worker_folder_rules_are_stated() {
    let all = files();
    let s = file(&all, "SKILL.md");
    // The worktree and new project rules live in the script reference, and SKILL.md points there.
    let reference = file(&all, "references/script-api.md");
    assert!(s.contains("worktree, new project and phase rules in references/script-api.md"));
    for marker in [
        "not inside",
        "git init",
        "first agent unisolated",
        "committed HEAD",
        "git switch -c",
        "never `cd` or `git -C`",
        "absolute path",
    ] {
        assert!(
            reference.contains(marker),
            "missing {marker} in script-api.md"
        );
    }
    for marker in [
        "`rm` on any absolute path",
        "`sh -c` or `bash -c`",
        "workflow list --json",
        "agents_finished",
        "never drop it silently",
    ] {
        assert!(s.contains(marker), "missing {marker}");
    }
}

#[test]
fn the_new_guidance_is_in_the_skill() {
    let all = files();
    let s = file(&all, "SKILL.md");
    for marker in [
        "workflow lessons",
        "unreviewed notes",
        "Leave `maxTurns` out",
        "`enum`",
        "cache_unknown_turns",
        "do not scale by agent count alone",
        "workflow worktrees list <run>",
        "never start a follow-up run that ignores them",
        "fresh explicit yes",
        "not by a provider limit",
        "Never edit this skill",
    ] {
        assert!(s.contains(marker), "missing {marker}");
    }
    let reference = file(&all, "references/script-api.md");
    for marker in [
        "## Step caps",
        "## Status words",
        "status: {enum: ['done','blocked']}",
    ] {
        assert!(
            reference.contains(marker),
            "missing {marker} in script-api.md"
        );
    }
    assert!(file(&all, "references/commands.md").contains("workflow lessons add --run"));
    // Every example that branches on a status word lists its words with enum.
    let examples = file(&all, "references/examples.md");
    assert!(examples.contains("status:{enum:['done','blocked']}"));
    assert!(examples.contains("verdict:{enum:['confirmed','rejected']}"));
    assert!(
        !examples.contains("maxTurns:"),
        "no example sets a step cap"
    );
}
