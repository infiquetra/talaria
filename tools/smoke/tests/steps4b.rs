//! Scripted checks of step results, exact failures, terminal inputs, and idle boundaries.
#[path = "support/scripts4b.rs"]
mod scripts4b;
mod support;

use scripts4b::{ALL_STEPS, Exec4b, SCREEN_STEPS, SECRET_LINE, exec_4b, script_4b};
use smoke::{ReadSource, StepOutcome};
use std::time::Duration;
use support::Call;

type Case<'a> = (&'a str, &'a str, &'a str, &'a str);

fn opener(step: &str) -> &'static str {
    match step {
        "mcp-browse" => "/mcp",
        "capabilities-browse" => "/capabilities",
        "memory-open" => "/memory-settings",
        "memory-commands" => "/memory pending",
        "cron-browse" => "/cron",
        "messaging-browse" => "/messaging",
        "connectors-consent" => "/connectors",
        "configure-secret-refused" => SECRET_LINE,
        _ => "/quit",
    }
}

fn inputs(calls: &[support::Recorded]) -> Vec<String> {
    calls
        .iter()
        .flat_map(|entry| match &entry.call {
            Call::SendText { text, .. } => vec![format!("text:{text}")],
            Call::SendKeys { keys, .. } => keys.iter().map(|key| format!("key:{key}")).collect(),
            _ => Vec::new(),
        })
        .collect()
}

fn span(exec: &Exec4b, step: &str) -> (usize, usize) {
    let calls = exec.term.calls();
    let start = calls
        .iter()
        .position(
            |entry| matches!(&entry.call, Call::SendText { text, .. } if text == opener(step)),
        )
        .expect("step opener");
    let next = SCREEN_STEPS
        .iter()
        .position(|name| *name == step)
        .and_then(|index| SCREEN_STEPS.get(index + 1))
        .copied();
    let next = next.map(opener).unwrap_or("/quit");
    let end = calls[start + 1..]
        .iter()
        .position(|entry| {
            matches!(&entry.call, Call::SendText { text, .. } if text == next)
                || matches!(&entry.call, Call::Run { text, .. } if text == next)
        })
        .map(|offset| start + 1 + offset)
        .unwrap_or(calls.len());
    (start, end)
}

fn passes(step: &str, expected: &[&str], limit: u64) {
    let exec = exec_4b(&script_4b(&[]), false, false, 1.0, 900);
    let elapsed = exec
        .report
        .outcomes
        .iter()
        .find_map(|outcome| match outcome {
            StepOutcome::Pass { step: got, elapsed } if *got == step => Some(*elapsed),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{step} did not pass: {}", exec.lines()));
    assert!(elapsed <= Duration::from_secs(limit), "{step}: {elapsed:?}");
    let calls = exec.term.calls();
    if SCREEN_STEPS.contains(&step) {
        let (start, end) = span(&exec, step);
        assert_eq!(
            inputs(&calls[start..end]),
            expected,
            "{step} keys must equal its complete source sequence"
        );
        let before = exec
            .reads
            .iter()
            .rfind(|read| read.index < start && read.source == ReadSource::Visible)
            .expect("idle read before opening");
        assert!(smoke::screen4b::idle_screen(&before.text));
        let after = exec
            .reads
            .iter()
            .rfind(|read| {
                read.index < end && read.index > start && read.source == ReadSource::Visible
            })
            .expect("idle read after teardown");
        assert!(
            smoke::screen4b::idle_screen(&after.text),
            "{step}: {}",
            after.text
        );
        if step == "mcp-browse" {
            assert_eq!(before.text, after.text);
        }
    } else {
        let runs: Vec<_> = calls
            .iter()
            .filter_map(|entry| match &entry.call {
                Call::Run { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        match step {
            "startup" => {
                assert!(
                    runs.iter()
                        .any(|line| line.contains("--no-ensure --profile fixture")
                            && line.contains("--model minimax-m3.1-flash")
                            && line.contains("--provider ollama-cloud"))
                )
            }
            "plain-answer" => assert_eq!(
                exec.term.memory().len(),
                2,
                "one initial token and one fresh resumed token"
            ),
            "quit" | "resume" => {
                assert!(calls.iter().any(|entry| matches!(
                    &entry.call,
                    Call::Read {
                        source: ReadSource::RecentUnwrapped,
                        lines: Some(3000),
                        ..
                    }
                )));
                if step == "resume" {
                    assert!(
                        runs.iter()
                            .any(|line| line.contains("--no-ensure -c --profile fixture"))
                    );
                    let tokens = exec.term.memory();
                    assert_ne!(tokens[0], tokens[1]);
                }
                let repeated = exec_4b(
                    &script_4b(&[("@many-committed-token", "", "")]),
                    false,
                    false,
                    1.0,
                    900,
                );
                assert!(
                    repeated.passed(step),
                    "multiple committed copies are valid: {}",
                    repeated.lines()
                );
                assert_eq!(repeated.report.exit_code(), 0);
            }
            "capabilities-cli" => {
                for part in [
                    "capabilities skills --profile fixture",
                    "capabilities toolsets --json --profile fixture",
                    "capabilities plugins --json --profile fixture",
                ] {
                    assert!(
                        runs.iter().any(|line| line.contains(part)),
                        "missing shell command {part}"
                    );
                }
            }
            "no-gateway" => {
                for part in [
                    "HERMES_HOME=",
                    "capabilities skills",
                    "capabilities nothing",
                    "ls -A",
                ] {
                    assert!(
                        runs.iter().any(|line| line.contains(part)),
                        "missing shell command {part}"
                    );
                }
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(exec.report.exit_code(), 0);
    assert_eq!(exec.close_tabs(), 1);
}

fn check_failure(step: &str, changes: &[(&str, &str, &str)], expected: &str) {
    let exec = exec_4b(&script_4b(changes), false, false, 1.0, 900);
    match exec.report.outcomes.last() {
        Some(StepOutcome::Fail {
            step: got, reason, ..
        }) => {
            assert_eq!(*got, step);
            assert_eq!(reason, expected);
        }
        _ => panic!("{step} did not fail: {}", exec.lines()),
    }
    assert_eq!(exec.report.exit_code(), 1);
    assert_eq!(exec.close_tabs(), 1);
    assert!(std::fs::read_dir(&exec.root).unwrap().next().is_none());
    assert!(
        std::fs::read_dir(&exec.empty_home_parent)
            .unwrap()
            .next()
            .is_none()
    );
    if matches!(step, "memory-commands" | "configure-secret-refused") {
        let calls = exec.term.calls();
        if let Some(start) = calls
            .iter()
            .position(|entry| matches!(&entry.call,Call::SendText{text,..} if text==opener(step)))
        {
            assert!(!calls[start..].iter().any(|entry|matches!(&entry.call,Call::SendKeys{keys,..} if keys.iter().any(|key|key=="esc"))));
            if expected == "an overlay opened for a secret in a /configure request" {
                assert!(!calls[start..].iter().any(|entry|matches!(&entry.call,Call::SendKeys{keys,..} if keys.iter().any(|key|key=="up" || key=="ctrl+c"))));
            }
        }
    }
}

fn fails(step: &str, cases: &[Case<'_>]) {
    for &(file, before, after, reason) in cases {
        check_failure(step, &[(file, before, after)], reason);
    }
    // Simultaneous adjacent violations establish the specified check order, as well as
    // proving each predicate independently above. Flags represent whole terminal paths.
    for pair in cases.windows(2) {
        let (a, b, c, reason) = pair[0];
        let (x, y, z, _) = pair[1];
        if !a.starts_with('@')
            && !x.starts_with('@')
            && (a != x || (!b.contains(y) && !y.contains(b)))
        {
            check_failure(step, &[(a, b, c), (x, y, z)], reason);
        }
    }
    if SCREEN_STEPS.contains(&step) {
        let r0 = format!("{step}-r0.txt");
        check_failure(
            step,
            &[(
                &r0,
                "# synthetic\n",
                "# synthetic\nMCP servers · profile fixture\nSaved.\n",
            )],
            "the step did not start from an idle composer",
        );
        let r1 = format!("{step}-r1.txt");
        check_failure(
            step,
            &[(&r1, "# synthetic\n", "# synthetic\nSaved.\n")],
            "a write banner appeared; the suite must never change anything",
        );
        let screen = match step {
            "mcp-browse" => Some(("MCP servers", 2)),
            "capabilities-browse" => Some(("capabilities", 1)),
            "memory-open" => Some(("memory", 1)),
            "cron-browse" => Some(("scheduled jobs", 1)),
            "messaging-browse" => Some(("messaging", 0)),
            "connectors-consent" => Some(("connectors", 0)),
            _ => None,
        };
        if let Some((screen, source_escapes)) = screen {
            let flag = format!("@stuck-{step}");
            let reason = format!("the {screen} overlay did not close after 3 Esc");
            let exec = exec_4b(&script_4b(&[(&flag, "", "")]), false, false, 1.0, 900);
            match exec.report.outcomes.last() {
                Some(StepOutcome::Fail {
                    step: got,
                    reason: got_reason,
                    ..
                }) => {
                    assert_eq!(*got, step);
                    assert_eq!(got_reason, &reason);
                }
                _ => panic!("stuck overlay did not fail: {}", exec.lines()),
            }
            let calls = exec.term.calls();
            let start = calls
                .iter()
                .position(
                    |entry| matches!(&entry.call,Call::SendText{text,..} if text==opener(step)),
                )
                .unwrap();
            let escapes:Vec<_>=calls[start..].iter().enumerate().filter_map(|(index,entry)|matches!(&entry.call,Call::SendKeys{keys,..} if keys.iter().any(|key|key=="esc")).then_some(start+index)).collect();
            assert_eq!(
                escapes.len(),
                source_escapes + 3,
                "bounded close attempts for {step}"
            );
            for (position, index) in escapes.iter().enumerate() {
                let end = escapes.get(position + 1).copied().unwrap_or(calls.len());
                let read = exec
                    .reads
                    .iter()
                    .find(|read| {
                        read.index > *index
                            && read.index < end
                            && read.source == ReadSource::Visible
                    })
                    .expect("visible read after each Esc");
                assert!(
                    calls[read.index].at.saturating_sub(calls[*index].at)
                        >= Duration::from_millis(400)
                );
            }
            assert_eq!(exec.close_tabs(), 1);
        }
    }
}

macro_rules! pair {
    ($pass:ident,$fail:ident,$step:literal,$limit:literal,[$($key:expr),* $(,)?],[$($case:expr),* $(,)?])=>{
        #[test] fn $pass(){passes($step,&[$($key),*],$limit);}
        #[test] fn $fail(){fails($step,&[$($case),*]);}
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
        ),
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
    mcp_browse_passes_on_the_recorded_screen,
    mcp_browse_fails_with_its_exact_reason,
    "mcp-browse",
    30,
    [
        "text:/mcp",
        "key:enter",
        "key:tab",
        "text:deepwiki",
        "key:esc",
        "key:esc"
    ],
    [
        (
            "mcp-browse-r1.txt",
            "MCP servers · profile fixture",
            "Unavailable",
            "MCP servers did not show \"MCP servers · profile fixture\" within 30 s"
        ),
        (
            "mcp-browse-r1.txt",
            "No MCP servers in profile fixture yet.",
            "",
            "the server list shows neither rows nor the empty text"
        ),
        (
            "mcp-browse-r2.txt",
            "remote",
            "local",
            "the Catalogue tab shows 0 remote rows, not at least 10"
        ),
        (
            "mcp-browse-r2.txt",
            "# synthetic\n",
            "# synthetic\nTransportSpec(\n",
            "the Catalogue tab shows \"TransportSpec(\""
        ),
        (
            "mcp-browse-r3.txt",
            " > deepwiki                  remote          Read public project documentation\n",
            "",
            "the filter \"deepwiki\" shows 0 rows, not 1"
        ),
        (
            "mcp-browse-teardown.txt",
            ">",
            "> changed",
            "the screen after closing MCP servers differs from the screen before"
        ),
    ]
);
pair!(
    capabilities_browse_passes_on_the_recorded_screen,
    capabilities_browse_fails_with_its_exact_reason,
    "capabilities-browse",
    40,
    [
        "text:/capabilities",
        "key:enter",
        "text:3",
        "text:4",
        "text:a",
        "key:esc"
    ],
    [
        (
            "capabilities-browse-r1.txt",
            "Capabilities · profile fixture",
            "Unavailable",
            "capabilities did not show \"Capabilities · profile fixture\" within 40 s"
        ),
        (
            "capabilities-browse-r1.txt",
            "[",
            "(",
            "the Skills tab shows no category header or no [x] or [ ] row"
        ),
        (
            "capabilities-browse-r2.txt",
            " tools",
            " widgets",
            "the Toolsets tab shows no \"<n> tools\" row"
        ),
        (
            "capabilities-browse-r3.txt",
            "You have not installed any plugins in profile fixture. Press a to show the 2 that ship with Hermes, or i to install one.",
            "",
            "the Plugins tab shows neither rows nor the empty text"
        ),
        (
            "capabilities-browse-r4.txt",
            "bundled",
            "unknown",
            "the plugin filter \"a\" shows no bundled row"
        ),
        (
            "capabilities-browse-r5.txt",
            ">",
            "> occupied",
            "the composer is not idle after closing capabilities"
        ),
    ]
);
pair!(
    memory_open_passes_on_the_recorded_screen,
    memory_open_fails_with_its_exact_reason,
    "memory-open",
    30,
    ["text:/memory-settings", "key:enter", "key:esc"],
    [
        (
            "memory-open-r1.txt",
            "Memory · profile fixture",
            "Unavailable",
            "memory did not show \"Memory · profile fixture\" within 30 s"
        ),
        (
            "memory-open-r1.txt",
            "MEMORY PROVIDER",
            "MISSING",
            "memory section MEMORY PROVIDER missing"
        ),
        (
            "memory-open-r1.txt",
            "BUILT-IN MEMORY",
            "MISSING",
            "memory section BUILT-IN MEMORY missing"
        ),
        (
            "memory-open-r1.txt",
            "SKILL CURATOR",
            "MISSING",
            "memory section SKILL CURATOR missing"
        ),
        (
            "memory-open-r1.txt",
            "built-in only",
            "unknown",
            "the memory provider row lacks \"built-in only\""
        ),
        (
            "memory-open-r1.txt",
            "1.2 KB",
            "unknown",
            "a built-in memory file row shows neither a size nor \"empty\""
        ),
        (
            "memory-open-r1.txt",
            "active",
            "unknown",
            "the curator row shows none of active, paused, off"
        ),
        (
            "memory-open-r2.txt",
            "# synthetic\n",
            "# synthetic\nMemory · profile fixture\n",
            "the memory title is still shown after Esc"
        ),
    ]
);
pair!(
    memory_commands_passes_on_the_recorded_screen,
    memory_commands_fails_with_its_exact_reason,
    "memory-commands",
    40,
    [
        "text:/memory pending",
        "key:enter",
        "text:/curator status",
        "key:enter"
    ],
    [
        (
            "memory-commands-r1.txt",
            "No pending memory writes.",
            "No reply.",
            "\"/memory pending\" printed neither \"No pending memory writes.\" nor \"Pending memory writes (\""
        ),
        (
            "memory-commands-r2.txt",
            "curator: ENABLED",
            "curator: UNKNOWN",
            "\"/curator status\" printed no curator state"
        ),
        (
            "memory-commands-r2.txt",
            "# synthetic\n",
            "# synthetic\nMemory · profile fixture\n",
            "a Talaria memory overlay or an unknown-command text answered a Hermes command"
        ),
    ]
);
pair!(
    cron_browse_passes_on_the_recorded_screen,
    cron_browse_fails_with_its_exact_reason,
    "cron-browse",
    30,
    ["text:/cron", "key:enter", "key:esc"],
    [
        (
            "cron-browse-r1.txt",
            "Scheduled jobs · profile fixture",
            "Unavailable",
            "scheduled jobs did not show \"Scheduled jobs · profile fixture\" within 30 s"
        ),
        (
            "cron-browse-r1.txt",
            "fixture-daily                0 8 * * *              next in 3h 12m\nfixture-weekly               0 9 * * 1              paused\n",
            "",
            "the job list shows neither rows with a state nor the empty text"
        ),
        (
            "cron-browse-r2.txt",
            ">",
            "> occupied",
            "the composer is not idle after closing scheduled jobs"
        ),
    ]
);
pair!(
    messaging_browse_passes_on_the_recorded_screen,
    messaging_browse_fails_with_its_exact_reason,
    "messaging-browse",
    40,
    [
        "text:/messaging",
        "key:enter",
        "text:/signal",
        "key:enter",
        "key:esc",
        "key:esc",
        "key:esc"
    ],
    [
        (
            "messaging-browse-r1.txt",
            "Connectors and messaging · profile fixture",
            "Unavailable",
            "messaging did not show \"Connectors and messaging · profile fixture\" within 40 s"
        ),
        (
            "messaging-browse-r1.txt",
            " > Telegram     needs setup\n",
            "",
            "the platform list shows 9 rows, not at least 10"
        ),
        (
            "messaging-browse-r2.txt",
            " > Signal       off\n",
            "",
            "the filter \"/signal\" shows 0 Signal rows, not 1"
        ),
        (
            "messaging-browse-r3.txt",
            "SIGNAL_HTTP_URL",
            "OTHER_FIELD",
            "SIGNAL_HTTP_URL is missing from the Signal card"
        ),
        (
            "messaging-browse-r3.txt",
            "not set",
            "unknown",
            "SIGNAL_HTTP_URL shows neither \"set\" nor \"not set\""
        ),
        (
            "messaging-browse-r3.txt",
            "not set",
            "set stored-value",
            "SIGNAL_HTTP_URL shows stored characters"
        ),
    ]
);
pair!(
    connectors_consent_passes_on_the_recorded_screen,
    connectors_consent_fails_with_its_exact_reason,
    "connectors-consent",
    20,
    ["text:/connectors", "key:enter", "key:esc"],
    [
        (
            "connectors-consent-r1.txt",
            "If profile fixture has no Nous sign-in",
            "If profile default has no Nous sign-in",
            "the connectors consent text is missing or does not name profile fixture"
        ),
        (
            "connectors-consent-r1.txt",
            "enter continue · esc back",
            "",
            "the connectors consent text lacks \"enter continue · esc back\""
        ),
        (
            "connectors-consent-teardown.txt",
            ">",
            "> occupied",
            "the composer is not idle after leaving the consent text"
        ),
    ]
);
pair!(
    configure_secret_refused_passes_on_the_recorded_screen,
    configure_secret_refused_fails_with_its_exact_reason,
    "configure-secret-refused",
    20,
    [
        format!("text:{SECRET_LINE}").as_str(),
        "key:enter",
        "key:up",
        "key:ctrl+c"
    ],
    [
        (
            "configure-secret-refused-r1.txt",
            "That request contains something that looks like a key or token. Talaria never sends secrets to a model. Add keys with /providers.",
            "Nothing happened.",
            "the refusal text of a secret in a /configure request is missing"
        ),
        (
            "configure-secret-refused-r1.txt",
            "# synthetic\n",
            "# synthetic\nMemory · profile fixture\n",
            "an overlay opened for a secret in a /configure request"
        ),
        (
            "configure-secret-refused-r2.txt",
            ">",
            SECRET_LINE,
            "Up brought the secret line back"
        ),
        (
            "configure-secret-refused-r3.txt",
            ">",
            "> occupied",
            "the composer is not empty after Ctrl+C"
        ),
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
        ),
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
            "@resume-shell",
            "",
            "",
            "the shell did not become ready within 45 s"
        ),
        (
            "@resume-exit",
            "",
            "",
            "resumed talaria exited with code 7 before showing its screen"
        ),
        (
            "@resume-unstable",
            "",
            "",
            "resumed talaria did not show a stable screen within 30 s"
        ),
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
    capabilities_cli_passes_on_the_recorded_screen,
    capabilities_cli_fails_with_its_exact_reason,
    "capabilities-cli",
    45,
    [],
    [
        (
            "capabilities-cli-r1.txt",
            "NAME  STATE  SOURCE  CATEGORY",
            "",
            "capabilities skills printed no header"
        ),
        (
            "capabilities-cli-r1.txt",
            "exit=0",
            "exit=4",
            "capabilities skills exit was 4, not 0"
        ),
        (
            "@capabilities-json",
            "",
            "",
            "capabilities --json is not valid JSON"
        ),
        (
            "@capabilities-profile",
            "",
            "",
            "capabilities --json .profile is not fixture"
        ),
        (
            "@capabilities-key",
            "",
            "",
            "capabilities --json holds a forbidden key"
        ),
        (
            "@capabilities-home",
            "",
            "",
            "capabilities --json holds the home directory"
        ),
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
            "@no-gateway",
            "",
            "",
            "capabilities without a gateway did not fail with a next step and exit 1"
        ),
        (
            "@unknown-list",
            "",
            "",
            "the unknown list text or exit 2 is missing"
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
    let exec = exec_4b(&script_4b(&[]), false, false, 1.0, 900);
    assert_eq!(
        exec.report
            .outcomes
            .iter()
            .map(|outcome| match outcome {
                StepOutcome::Pass { step, .. } => *step,
                _ => panic!("{}", exec.lines()),
            })
            .collect::<Vec<_>>(),
        ALL_STEPS
    );
    let calls = exec.term.calls();
    for step in SCREEN_STEPS {
        let (start, end) = span(&exec, step);
        let before = exec
            .reads
            .iter()
            .rfind(|read| read.index < start && read.source == ReadSource::Visible)
            .unwrap();
        let after = exec
            .reads
            .iter()
            .rfind(|read| {
                read.index > start && read.index < end && read.source == ReadSource::Visible
            })
            .unwrap();
        assert!(
            smoke::screen4b::idle_screen(&before.text),
            "before {step}: {}",
            before.text
        );
        assert!(
            smoke::screen4b::idle_screen(&after.text),
            "after {step}: {}",
            after.text
        );
        assert!(!after.text.contains("working..."));
        assert!(
            !calls[after.index + 1..end]
                .iter()
                .any(|entry| matches!(entry.call, Call::SendText { .. } | Call::SendKeys { .. })),
            "no input between idle teardown and the next step"
        );
    }
    assert_eq!(exec.report.exit_code(), 0);
}

#[test]
fn a_failing_step_stops_the_run_and_cleans_up() {
    let exec = exec_4b(
        &script_4b(&[("memory-open-r1.txt", "built-in only", "unknown")]),
        false,
        false,
        1.0,
        900,
    );
    assert_eq!(exec.report.outcomes.len(), 5);
    assert!(matches!(
        exec.report.outcomes.last(),
        Some(StepOutcome::Fail {
            step: "memory-open",
            ..
        })
    ));
    assert!(
        !exec
            .term
            .everything_typed()
            .iter()
            .any(|line| line == "/memory pending" || line == "/cron" || line == "/quit")
    );
    assert_eq!(exec.close_tabs(), 1);
    assert!(std::fs::read_dir(&exec.root).unwrap().next().is_none());
    assert!(
        std::fs::read_dir(&exec.empty_home_parent)
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn deadline_names_the_step() {
    let exec = exec_4b(&script_4b(&[("@reply", "", "")]), false, false, 1.0, 5);
    assert!(
        exec.lines()
            .contains("deadline exceeded in step plain-answer")
    );
    assert!(matches!(
        exec.report.outcomes.last(),
        Some(StepOutcome::Fail {
            step: "plain-answer",
            ..
        })
    ));
    assert_eq!(exec.report.exit_code(), 1);
    assert_eq!(exec.close_tabs(), 1);
}

#[test]
fn a_missing_precondition_is_a_fail_not_a_skip() {
    for (fault, step) in [
        ("@missing-gateway", "startup"),
        ("@missing-fixture", "startup"),
        ("@missing-jq", "capabilities-cli"),
    ] {
        let exec = exec_4b(&script_4b(&[(fault, "", "")]), false, false, 1.0, 900);
        assert!(
            matches!(exec.report.outcomes.last(),Some(StepOutcome::Fail{step:got,..}) if *got==step)
        );
        assert!(!exec.lines().contains("SKIP"));
        assert_eq!(exec.report.exit_code(), 1);
        assert_eq!(exec.close_tabs(), 1);
    }
}

#[test]
fn live_require_does_not_change_a_phase4b_result() {
    for changes in [
        vec![],
        vec![("@missing-gateway", "", "")],
        vec![("@missing-fixture", "", "")],
        vec![("@missing-jq", "", "")],
        vec![("@home-not-empty", "", "")],
    ] {
        let ordinary = exec_4b(&script_4b(&changes), false, false, 1.0, 900);
        let required = exec_4b(&script_4b(&changes), true, false, 1.0, 900);
        assert_eq!(ordinary.lines(), required.lines());
        assert_eq!(ordinary.report.exit_code(), required.report.exit_code());
        assert_ne!(ordinary.report.exit_code(), 3);
    }
}

#[test]
fn step_scale_leaves_quiet_periods() {
    let exec = exec_4b(&script_4b(&[]), false, false, 0.5, 900);
    assert_eq!(exec.report.exit_code(), 0);
    let calls = exec.term.calls();
    let mut escapes = 0;
    let mut submits = 0;
    for (index, entry) in calls.iter().enumerate() {
        if matches!(&entry.call,Call::SendKeys{keys,..} if keys.iter().any(|key|key=="esc")) {
            let read = exec
                .reads
                .iter()
                .find(|read| read.index > index && read.source == ReadSource::Visible)
                .expect("read after Esc");
            assert!(calls[read.index].at.saturating_sub(entry.at) >= Duration::from_millis(400));
            escapes += 1;
        }
        if let Call::SendText { text, .. } = &entry.call
            && (text.starts_with('/') && text != "/signal" || text.contains("join these two words"))
        {
            let enter = calls[index + 1..]
                .iter()
                .find(|entry| matches!(entry.call, Call::SendKeys { .. } | Call::SendText { .. }))
                .expect("Enter after submitted text");
            assert!(matches!(&enter.call,Call::SendKeys{keys,..} if keys==&["enter"]));
            assert!(enter.at.saturating_sub(entry.at) >= Duration::from_millis(300));
            submits += 1;
        }
        if matches!(&entry.call,Call::SendKeys{keys,..} if keys.iter().any(|key|key=="ctrl+c")) {
            let read = exec
                .reads
                .iter()
                .find(|read| read.index > index && read.source == ReadSource::Visible)
                .unwrap();
            assert!(calls[read.index].at.saturating_sub(entry.at) >= Duration::from_millis(2500));
        }
    }
    assert_eq!(escapes, 9);
    assert!(submits >= 9);
}
