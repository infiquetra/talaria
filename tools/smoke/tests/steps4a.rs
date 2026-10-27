//! Synthetic scripted checks of step outcomes, inputs, and failure messages.
#[path = "support/scripts4a.rs"]
mod scripts4a;
mod support;
use scripts4a::{exec_4a, script_4a};
use smoke::StepOutcome;
use std::time::Duration;
use support::Call;

fn passes(step: &str, expected: &[&str], max_secs: u64) {
    let exec = exec_4a(&script_4a(&[]), false, false, 1.0, 900);
    let elapsed = exec
        .report
        .outcomes
        .iter()
        .find_map(|outcome| match outcome {
            StepOutcome::Pass { step: got, elapsed } if *got == step => Some(*elapsed),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{step} did not pass: {}", exec.lines()));
    assert!(elapsed <= Duration::from_secs(max_secs));
    let inputs: Vec<String> = exec
        .term
        .calls()
        .into_iter()
        .flat_map(|call| match call.call {
            Call::SendText { text, .. } => vec![format!("text:{text}")],
            Call::SendKeys { keys, .. } => {
                keys.into_iter().map(|key| format!("key:{key}")).collect()
            }
            _ => vec![],
        })
        .collect();
    if !expected.is_empty() {
        assert!(
            inputs.windows(expected.len()).any(|window| window
                .iter()
                .map(String::as_str)
                .eq(expected.iter().copied())),
            "{step}: {inputs:?}"
        );
    }
    let runs: Vec<String> = exec
        .term
        .calls()
        .into_iter()
        .filter_map(|call| match call.call {
            Call::Run { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    match step {
        "startup" => assert!(runs.iter().any(
            |line| line.contains("--no-ensure --profile fixture")
                && line.contains("--provider ollama-cloud")
                && line.contains("--model minimax-m3.1-flash")
        )),
        "plain-answer" => assert!(
            runs.iter()
                .any(|line| line.contains("join these two words"))
        ),
        "providers-key-masked" => {
            let junk = format!("text:sk-or-v1-{}", "0".repeat(64));
            let sequence = [
                "text:/providers",
                "key:enter",
                "text:openrouter",
                "key:enter",
                junk.as_str(),
                "key:esc",
                "key:esc",
            ];
            assert!(inputs.windows(sequence.len()).any(|window| {
                window
                    .iter()
                    .map(String::as_str)
                    .eq(sequence.iter().copied())
            }));
        }
        "quit" => assert!(
            runs.iter().any(|line| line == "/quit")
                || inputs.iter().any(|line| line == "text:/quit")
        ),
        "resume" => assert!(
            runs.iter()
                .any(|line| line.contains("--no-ensure -c --profile fixture"))
        ),
        "profiles-cli" => {
            for fragment in [
                "profiles fixture default",
                "profiles --json fixture",
                "profiles nosuchprofile",
            ] {
                assert!(runs.iter().any(|line| line.contains(fragment)));
            }
        }
        "doctor-cli" => {
            for fragment in [
                "doctor --profile fixture",
                "doctor --json --profile fixture",
                "doctor --profile nosuchprofile",
                "doctor --profile",
            ] {
                assert!(runs.iter().any(|line| line.contains(fragment)));
            }
        }
        "no-gateway" => {
            for fragment in ["HERMES_HOME=", "doctor", "profiles", "ls -A"] {
                assert!(runs.iter().any(|line| line.contains(fragment)));
            }
        }
        _ => {}
    }
    assert_eq!(exec.report.exit_code(), 0);
    assert_eq!(exec.close_tabs(), 1);
}
fn fails(step: &str, cases: &[(&str, &str, &str, &str)]) {
    for &(file, before, after, expected) in cases {
        let script = script_4a(&[(file, before, after)]);
        let exec = exec_4a(&script, false, false, 1.0, 900);
        match exec.report.outcomes.last() {
            Some(StepOutcome::Fail {
                step: got, reason, ..
            }) => {
                assert_eq!(*got, step);
                assert_eq!(reason, expected);
            }
            _ => panic!("{step} did not fail: {}", exec.lines()),
        }
        if file == "@doctor-close-once" || file.starts_with("@stuck-") {
            let calls = exec.term.calls();
            let open = match step {
                "config-browse" | "config-invalid-input" => "/config",
                "providers-tabs" | "providers-key-masked" => "/providers",
                "models-page" | "models-bad-preset" => "/models",
                "profiles-list" | "profiles-bad-names" => "/profiles",
                "doctor-overlay" => "/doctor",
                _ => unreachable!(),
            };
            let start = calls
                .iter()
                .rposition(
                    |entry| matches!(&entry.call, Call::SendText { text, .. } if text == open),
                )
                .expect("screen opener");
            let after = &calls[start..];
            let escapes: Vec<usize> = after.iter().enumerate().filter_map(|(i, entry)| matches!(&entry.call, Call::SendKeys { keys, .. } if keys.iter().any(|key| key == "esc")).then_some(i)).collect();
            let extra = if file == "@doctor-close-once" { 1 } else { 3 };
            let count = if file == "@doctor-close-once" {
                2
            } else if matches!(step, "providers-tabs" | "models-page" | "models-bad-preset") {
                3
            } else {
                4
            };
            assert!(
                escapes.len() >= count,
                "expected source and extra close keys"
            );
            for position in (escapes.len() - extra)..escapes.len() {
                let start = escapes[position] + 1;
                let end = escapes.get(position + 1).copied().unwrap_or(after.len());
                let read = after[start..end]
                    .iter()
                    .find(|entry| {
                        matches!(
                            entry.call,
                            Call::Read {
                                source: smoke::ReadSource::Visible,
                                ..
                            }
                        )
                    })
                    .expect("each extra close key is followed by a visible read");
                assert!(
                    read.at.saturating_sub(after[escapes[position]].at)
                        >= Duration::from_millis(400)
                );
            }
        }
        assert_eq!(exec.close_tabs(), 1);
    }
}
fn screen_guards(step: &str) {
    let screen = match step {
        "config-browse" | "config-invalid-input" => Some("settings"),
        "providers-tabs" | "providers-key-masked" => Some("providers"),
        "models-page" | "models-bad-preset" => Some("model roles"),
        "profiles-list" | "profiles-bad-names" => Some("profiles"),
        "doctor-overlay" => Some("doctor"),
        "config-secret-refused" => Some("settings"),
        _ => None,
    };
    if let Some(name) = screen {
        let file = format!("{step}-r0.txt");
        let title = match name {
            "settings" => "Settings · profile fixture",
            "providers" => "Providers · profile fixture",
            "model roles" => "Model roles - profile fixture",
            "profiles" => "Profiles (2)",
            _ => "Doctor: 7 ok, 0 warnings, 0 failures",
        };
        let injected = format!("# recorded\n{title}\n");
        fails(
            step,
            &[(
                &file,
                "# recorded\n",
                &injected,
                "the step did not start from an idle composer",
            )],
        );
        if step != "config-secret-refused" {
            let marker = format!("@stuck-{step}");
            let reason = format!("the {name} overlay did not close after 3 Esc");
            fails(step, &[(&marker, "", "", &reason)]);
        }
    }
}
macro_rules! pair {
    ($pass:ident, $fail:ident, $step:expr, $limit:expr, [$($input:expr),* $(,)?], [$($case:expr),* $(,)?]) => {
        #[test] fn $pass() { passes($step, &[$($input),*], $limit); }
        #[test] fn $fail() { fails($step, &[$($case),*]); screen_guards($step); }
    };
}
pair!(
    startup_passes_on_the_recorded_screen,
    startup_fails_with_its_exact_reason,
    "startup",
    75,
    [],
    [
        (
            "@startup",
            "",
            "",
            "the shell did not become ready within 45 s"
        ),
        (
            "@startup-exit",
            "",
            "",
            "talaria exited with code 7 before showing its screen"
        ),
        (
            "@startup-unstable",
            "",
            "",
            "startup did not show a stable screen within 30 s"
        )
    ]
);
pair!(
    plain_answer_passes_on_the_recorded_screen,
    plain_answer_fails_with_its_exact_reason,
    "plain-answer",
    60,
    [],
    [(
        "@reply",
        "",
        "",
        "the reply token did not appear and settle within 60 s"
    ),]
);
pair!(
    config_browse_passes_on_the_recorded_screen,
    config_browse_fails_with_its_exact_reason,
    "config-browse",
    30,
    [
        "text:/config",
        "key:enter",
        "key:right",
        "key:right",
        "text:/",
        "text:compact",
        "key:esc",
        "text:/",
        "text:api_key",
        "key:esc",
        "key:esc"
    ],
    [
        (
            "config-browse-r1.txt",
            "Settings · profile fixture",
            "Unavailable",
            "settings did not show \"Settings · profile fixture\" within 3 s"
        ),
        (
            "config-browse-r1.txt",
            "general",
            "",
            "settings category general missing"
        ),
        (
            "config-browse-r2.txt",
            "display.compact",
            "missing",
            "display.compact row missing after the filter \"compact\""
        ),
        (
            "config-browse-r2.txt",
            "may need a restart",
            "",
            "display.compact row lacks a value, set or default, or \"may need a restart\""
        ),
        (
            "config-browse-r3.txt",
            "delegation.api_key",
            "missing",
            "delegation.api_key missing after the filter \"api_key\""
        ),
        (
            "config-browse-r3.txt",
            "•••••••• (r reveals once)",
            "plain",
            "delegation.api_key shows plain text, not (not set) or bullets"
        ),
        (
            "config-browse-r4.txt",
            ">",
            "> changed",
            "the screen after closing settings differs from the screen before"
        ),
    ]
);
pair!(
    config_invalid_input_passes_on_the_recorded_screen,
    config_invalid_input_fails_with_its_exact_reason,
    "config-invalid-input",
    20,
    [
        "text:/config",
        "key:enter",
        "text:/",
        "text:resume_exchanges",
        "key:enter",
        "key:enter",
        "text:abc",
        "key:enter",
        "key:esc",
        "text:a",
        "key:esc"
    ],
    [
        (
            "config-invalid-input-r1.txt",
            "Enter a whole number.",
            "",
            "no whole-number refusal after \"abc\""
        ),
        (
            "config-invalid-input-r1.txt",
            "display.resume_exchanges",
            "",
            "the number editor closed on invalid input"
        ),
        (
            "config-invalid-input-r2.txt",
            "display.resume_exchanges",
            "display.resume_exchanges *",
            "a \"*\" mark remained after Esc"
        ),
        (
            "config-invalid-input-r3.txt",
            "No changes to review.",
            "",
            "\"No changes to review.\" missing after the refused edit"
        ),
    ]
);
pair!(
    config_secret_refused_passes_on_the_recorded_screen,
    config_secret_refused_fails_with_its_exact_reason,
    "config-secret-refused",
    20,
    [
        "text:/config delegation.api_key=not-a-real-key",
        "key:enter",
        "key:up",
        "key:ctrl+c"
    ],
    [
        (
            "config-secret-refused-r1.txt",
            "Nothing was sent.",
            "",
            "the refusal text of a secret on the command line is missing"
        ),
        (
            "config-secret-refused-r1.txt",
            "# recorded\n",
            "# recorded\nSettings · profile fixture\n",
            "a settings overlay opened for a secret"
        ),
        (
            "config-secret-refused-r2.txt",
            ">",
            "> not-a-real-key",
            "Up brought the secret line back"
        ),
        (
            "config-secret-refused-r3.txt",
            ">",
            "> occupied",
            "the composer is not empty after Ctrl+C"
        ),
    ]
);
pair!(
    providers_tabs_passes_on_the_recorded_screen,
    providers_tabs_fails_with_its_exact_reason,
    "providers-tabs",
    30,
    [
        "text:/providers",
        "key:enter",
        "key:tab",
        "key:tab",
        "key:esc"
    ],
    [
        (
            "providers-tabs-r1.txt",
            "Providers · profile fixture",
            "Unavailable",
            "providers did not show \"Providers · profile fixture\" within 30 s"
        ),
        (
            "providers-tabs-r1.txt",
            "ready ·",
            "offline ·",
            "Ollama Cloud is not ready"
        ),
        (
            "providers-tabs-r1.txt",
            "key …abcd",
            "key",
            "Ollama Cloud shows no key tail \"key …\" plus four characters"
        ),
        (
            "providers-tabs-r2.txt",
            "not signed in",
            "unknown",
            "the Accounts tab shows no sign-in state on row Nous"
        ),
        (
            "providers-tabs-r3.txt",
            "+ Add an endpoint",
            "",
            "the Endpoints tab has no \"+ Add an endpoint\" row"
        ),
    ]
);
pair!(
    providers_key_masked_passes_on_the_recorded_screen,
    providers_key_masked_fails_with_its_exact_reason,
    "providers-key-masked",
    30,
    [],
    [
        (
            "providers-key-masked-r1.txt",
            "OpenRouter",
            "Missing",
            "the OpenRouter row is missing after the filter \"openrouter\""
        ),
        (
            "providers-key-masked-r2.txt",
            "API key for OpenRouter",
            "",
            "the key card for OpenRouter did not open"
        ),
        (
            "providers-key-masked-r3.txt",
            "API key for OpenRouter",
            "API key for OpenRouter\nsk-or",
            "the key card showed the typed key"
        ),
        (
            "providers-key-masked-r3.txt",
            "…",
            "",
            "the key card did not show 40 bullets and \"…\""
        ),
        (
            "providers-key-masked-r4.txt",
            "OpenRouter · OPENROUTER_API_KEY · no key",
            "API key for OpenRouter",
            "the key card did not close on Esc"
        ),
        (
            "providers-key-masked-r4.txt",
            "no key",
            "ready",
            "the OpenRouter row changed after a cancelled key card"
        ),
    ]
);
pair!(
    models_page_passes_on_the_recorded_screen,
    models_page_fails_with_its_exact_reason,
    "models-page",
    40,
    [
        "text:/models",
        "key:enter",
        "text:2",
        "text:3",
        "text:4",
        "text:5",
        "key:esc"
    ],
    [
        (
            "models-page-r1.txt",
            "Model roles - profile fixture",
            "",
            "model roles did not show its title within 40 s"
        ),
        (
            "models-page-r3.txt",
            "Vision",
            "Failed",
            "section Helpers says Failed"
        ),
        (
            "models-page-r1.txt",
            "minimax-m3.1-flash",
            "",
            "Main shows no model"
        ),
        (
            "models-page-r3.txt",
            "Curator",
            "",
            "Helpers does not list Vision first and Curator twelfth"
        ),
        (
            "models-page-r4.txt",
            "Route: follows main model",
            "",
            "Subagents shows no route or no chain mode"
        ),
        (
            "models-page-r5.txt",
            include_str!("fixtures/screens/phase4a/models-page-r5.txt"),
            "Model roles - profile fixture\n",
            "Mixture of Agents shows neither a preset nor the notice"
        ),
    ]
);
pair!(
    models_bad_preset_passes_on_the_recorded_screen,
    models_bad_preset_fails_with_its_exact_reason,
    "models-bad-preset",
    30,
    [
        "text:/models",
        "key:enter",
        "text:5",
        "key:left",
        "text:a",
        "text:uat-test",
        "key:enter",
        "key:right",
        "text:a",
        "key:esc",
        "key:ctrl+s",
        "key:esc",
        "text:d",
        "text:/models",
        "key:enter",
        "text:5",
        "key:esc"
    ],
    [
        (
            "models-bad-preset-r1.txt",
            "preset 'uat-test' reference 1: provider and model are required",
            "",
            "the incomplete-preset refusal is missing"
        ),
        (
            "models-bad-preset-r1.txt",
            "# recorded\n",
            "# recorded\nReview changes to Mixture of Agents\n",
            "a diff was offered for an incomplete preset"
        ),
        (
            "models-bad-preset-r2.txt",
            "You have unapplied changes",
            "Changes",
            "Esc did not ask to apply, discard or stay"
        ),
        (
            "models-bad-preset-r3.txt",
            ">",
            "Model roles - profile fixture\n> ",
            "d did not close the model roles page"
        ),
        (
            "models-bad-preset-r4.txt",
            "No presets",
            "uat-test",
            "a preset \"uat-test\" exists after discard"
        ),
    ]
);
pair!(
    profiles_list_passes_on_the_recorded_screen,
    profiles_list_fails_with_its_exact_reason,
    "profiles-list",
    30,
    [
        "text:/profiles",
        "key:enter",
        "text:/fixture",
        "key:enter",
        "key:esc",
        "key:esc"
    ],
    [
        (
            "profiles-list-r1.txt",
            "Profiles (2 on disk · 2 served)",
            "",
            "profiles header missing"
        ),
        (
            "profiles-list-r1.txt",
            "served",
            "stopped",
            "profile fixture is not served"
        ),
        (
            "profiles-list-r1.txt",
            "minimax-m3.1-flash · ollama-cloud",
            "",
            "the fixture row lacks \"minimax-m3.1-flash · ollama-cloud\""
        ),
        (
            "profiles-list-r2.txt",
            "talaria --profile fixture",
            "",
            "the detail of fixture lacks \"talaria --profile fixture\""
        ),
        (
            "profiles-list-r2.txt",
            "Skills:",
            "",
            "the detail of fixture lacks a \"Skills:\" line"
        ),
        (
            "profiles-list-r3.txt",
            ">",
            "> changed",
            "the list changed after Esc"
        ),
    ]
);
pair!(
    profiles_bad_names_passes_on_the_recorded_screen,
    profiles_bad_names_fails_with_its_exact_reason,
    "profiles-bad-names",
    20,
    [
        "text:/profiles",
        "key:enter",
        "text:n",
        "text:Bad Name!",
        "key:enter",
        "key:ctrl+u",
        "text:default",
        "key:enter",
        "key:ctrl+u",
        "text: Fixture ",
        "key:enter",
        "key:esc",
        "key:esc"
    ],
    [
        (
            "profiles-bad-names-r1.txt",
            "Use lower-case letters, digits, - or _, starting with a letter or digit, up to 64 characters.",
            "",
            "name refusal 1 of 3 missing or out of order"
        ),
        (
            "profiles-bad-names-r2.txt",
            "default is reserved by Hermes. Choose another name.",
            "",
            "name refusal 2 of 3 missing or out of order"
        ),
        (
            "profiles-bad-names-r3.txt",
            "A profile named fixture already exists.",
            "",
            "name refusal 3 of 3 missing or out of order"
        ),
        (
            "profiles-bad-names-r1.txt",
            "Name: Bad Name!",
            "Other field: Bad Name!",
            "focus left the Name field"
        ),
        (
            "profiles-bad-names-r4.txt",
            "fixture  served",
            "fixture  stopped",
            "the profile list changed after the refused names"
        ),
    ]
);
pair!(
    doctor_overlay_passes_on_the_recorded_screen,
    doctor_overlay_fails_with_its_exact_reason,
    "doctor-overlay",
    30,
    [
        "text:/doctor",
        "key:enter",
        "key:down",
        "key:down",
        "key:enter",
        "key:esc"
    ],
    [
        (
            "doctor-overlay-r1.txt",
            "Doctor: 7 ok, 0 warnings, 0 failures",
            "",
            "doctor summary line missing within 30 s"
        ),
        (
            "doctor-overlay-r1.txt",
            "ok    Runtime health  healthy · 0 agents working\n",
            "",
            "doctor shows 6 rows, not 7"
        ),
        (
            "doctor-overlay-r1.txt",
            "↑↓ select · Enter details · r check again · R restart gateway · Esc close",
            "",
            "the key hint line is not the last overlay line"
        ),
        (
            "doctor-overlay-r1.txt",
            "running: 0.22.4 at 4d043038",
            "",
            "\"running:\" detail missing"
        ),
        (
            "doctor-overlay-r1.txt",
            "built for: v0.22.4 (4d043038)",
            "",
            "\"built for:\" detail missing"
        ),
        ("@doctor-close-once", "", "", "doctor did not close on Esc"),
    ]
);
pair!(
    quit_passes_on_the_recorded_screen,
    quit_fails_with_its_exact_reason,
    "quit",
    30,
    [],
    [
        (
            "@lost-token",
            "",
            "",
            "committed output vanished on exit (token A missing)"
        ),
        (
            "@quit-exit",
            "",
            "",
            "/quit ended talaria with exit code 7, expected 0"
        ),
        (
            "@quit-never",
            "",
            "",
            "/quit did not end talaria within 15 s"
        ),
        (
            "@quit-shell",
            "",
            "",
            "the shell did not return to the foreground after /quit"
        )
    ]
);
pair!(
    resume_passes_on_the_recorded_screen,
    resume_fails_with_its_exact_reason,
    "resume",
    150,
    [],
    [
        (
            "@missing-token",
            "",
            "",
            "token count after resume: A 0 (first pane 1); it must be at least 1 and at most the first pane's count"
        ),
        (
            "@duplicate-token",
            "",
            "",
            "token count after resume: A 2 (first pane 1); it must be at least 1 and at most the first pane's count"
        ),
        (
            "@resume-exit",
            "",
            "",
            "resumed talaria exited with code 7 before showing its screen"
        ),
        (
            "@resume-shell",
            "",
            "",
            "the shell did not become ready within 45 s"
        ),
        (
            "@resume-unstable",
            "",
            "",
            "resumed talaria did not show a stable screen within 30 s"
        ),
        (
            "@resume-silent",
            "",
            "",
            "the reply to the token C prompt did not appear and stay stable within 60 s"
        ),
        (
            "@resume-quit",
            "",
            "",
            "/quit in the resumed pane did not end talaria with exit code 0 within 15 s"
        ),
    ]
);
pair!(
    profiles_cli_passes_on_the_recorded_screen,
    profiles_cli_fails_with_its_exact_reason,
    "profiles-cli",
    30,
    [],
    [
        (
            "profiles-cli-r1.txt",
            "NAME     STATE   MODEL                              NEXT STEP\n",
            "",
            "profiles command printed no header"
        ),
        (
            "profiles-cli-r1.txt",
            "fixture  served",
            "",
            "profiles command printed no row for fixture"
        ),
        (
            "profiles-cli-r1.txt",
            "default  served",
            "",
            "profiles command printed no row for default"
        ),
        (
            "@profiles-json",
            "",
            "",
            "profiles --json is not valid JSON"
        ),
        (
            "@profiles-unknown",
            "",
            "",
            "the unknown-profile text is missing"
        ),
        ("@profiles-exit", "", "", "unknown profile did not exit 2"),
    ]
);
pair!(
    doctor_cli_passes_on_the_recorded_screen,
    doctor_cli_fails_with_its_exact_reason,
    "doctor-cli",
    45,
    [],
    [
        (
            "doctor-cli-r1.txt",
            "ok    Runtime health  healthy · 0 agents working\n",
            "",
            "doctor printed 6 rows, not 7"
        ),
        (
            "doctor-cli-r1.txt",
            "ok    Runtime health  healthy · 0 agents working",
            "FAIL Runtime health",
            "doctor row Runtime health is not ok or warn"
        ),
        (
            "doctor-cli-r1.txt",
            "Result:",
            "",
            "doctor printed no Result: line"
        ),
        (
            "doctor-cli-r1.txt",
            "exit=0",
            "exit=4",
            "doctor exit was 4, not 0"
        ),
        (
            "@doctor-home",
            "",
            "",
            "doctor output contains the home directory"
        ),
        (
            "@doctor-json",
            "",
            "",
            "doctor --json format is not talaria-doctor-v1 with 7 checks"
        ),
        (
            "@doctor-unknown",
            "",
            "",
            "unknown profile did not give FAIL Profiles and exit 1"
        ),
        ("@doctor-blank", "", "", "blank profile did not exit 2"),
    ]
);
pair!(
    no_gateway_passes_on_the_recorded_screen,
    no_gateway_fails_with_its_exact_reason,
    "no-gateway",
    20,
    [],
    [
        (
            "@no-gateway-doctor",
            "",
            "",
            "doctor without a gateway did not fail with the documented fix"
        ),
        (
            "@no-gateway-profiles",
            "",
            "",
            "profiles without a gateway did not exit 1 with a next step"
        ),
        (
            "@home-not-empty",
            "",
            "",
            "the empty home is not empty (1 entries)"
        ),
    ]
);
#[test]
fn a_full_run_returns_to_idle_between_steps() {
    let exec = exec_4a(&script_4a(&[]), false, false, 1.0, 900);
    assert_eq!(exec.report.outcomes.len(), 17);
    assert_eq!(exec.report.exit_code(), 0);
    assert!(
        exec.report
            .outcomes
            .iter()
            .all(|outcome| matches!(outcome, StepOutcome::Pass { .. }))
    );
    let calls = exec.term.calls();
    let mut last = 0;
    for command in [
        "/config",
        "/config",
        "/config delegation.api_key=not-a-real-key",
        "/providers",
        "/providers",
        "/models",
        "/models",
        "/profiles",
        "/profiles",
        "/doctor",
    ] {
        let offset = calls[last..]
            .iter()
            .position(|entry| matches!(&entry.call, Call::SendText { text, .. } if text == command))
            .expect("screen opening input");
        let index = last + offset;
        assert!(
            calls[last..index].iter().any(|entry| matches!(
                entry.call,
                Call::Read {
                    source: smoke::ReadSource::Visible,
                    ..
                }
            )),
            "visible idle read before {command}"
        );
        last = index + 1;
    }
}
#[test]
fn a_failing_step_stops_the_run_and_cleans_up() {
    let exec = exec_4a(
        &script_4a(&[("config-browse-r2.txt", "display.compact", "absent")]),
        false,
        false,
        1.0,
        900,
    );
    assert!(matches!(
        exec.report.outcomes.last(),
        Some(StepOutcome::Fail {
            step: "config-browse",
            ..
        })
    ));
    assert_eq!(exec.close_tabs(), 1);
}
#[test]
fn deadline_names_the_step() {
    let exec = exec_4a(&script_4a(&[("@reply", "", "")]), false, false, 1.0, 5);
    assert!(
        exec.lines()
            .contains("deadline exceeded in step plain-answer")
    );
    assert_eq!(exec.close_tabs(), 1);
}
#[test]
fn a_skip_is_never_a_pass() {
    let exec = exec_4a(
        &script_4a(&[(
            "providers-tabs-r1.txt",
            "OpenRouter · OPENROUTER_API_KEY · no key",
            "OpenRouter · OPENROUTER_API_KEY · ready · key …abcd",
        )]),
        false,
        false,
        1.0,
        900,
    );
    assert!(exec.report.outcomes.iter().any(|outcome| matches!(outcome, StepOutcome::Skip { step: "providers-tabs", reason } if reason == "OpenRouter already has a key on profile fixture (operator precondition: no OpenRouter key)")));
    assert_eq!(exec.report.exit_code(), 3);
}
#[test]
fn live_require_turns_a_skip_into_a_failure() {
    let exec = exec_4a(
        &script_4a(&[(
            "providers-tabs-r1.txt",
            "OpenRouter · OPENROUTER_API_KEY · no key",
            "OpenRouter · OPENROUTER_API_KEY · ready · key …abcd",
        )]),
        true,
        false,
        1.0,
        900,
    );
    assert!(exec.lines().contains("skipped: OpenRouter already has a key on profile fixture (operator precondition: no OpenRouter key); TALARIA_LIVE_REQUIRE=1"));
    assert_eq!(exec.report.exit_code(), 1);
}
#[test]
fn step_scale_leaves_quiet_periods() {
    let exec = exec_4a(&script_4a(&[]), false, false, 0.5, 900);
    let calls = exec.term.calls();
    let mut checked = 0;
    for (i, call) in calls.iter().enumerate() {
        if matches!(&call.call, Call::SendKeys { keys, .. } if keys.iter().any(|k| k == "esc")) {
            let next = calls[i + 1..]
                .iter()
                .find(|next| {
                    matches!(
                        next.call,
                        Call::Read { .. } | Call::SendKeys { .. } | Call::SendText { .. }
                    )
                })
                .expect("an operation after Esc");
            assert!(next.at.saturating_sub(call.at) >= Duration::from_millis(400));
            checked += 1;
        }
    }
    assert!(checked > 0);
}
