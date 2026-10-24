//! The frozen command contract: the document the command receives, the environment it gets,
//! how its output is read, the failure markers, the command-line parsing and the refresh
//! policy. Expected values are written by hand from the format document and the unit
//! specification, never taken from the code under test.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use talaria_tui::status::{
    CommandLimits, CommandResult, Connection, DENIED_PROVIDER_KEYS, DOCUMENT_FIELDS, Decision,
    ExternalDisplay, ExternalRow, FORWARDED_TALARIA, Failure, MIN_RERUN_GAP, ParsedOutput,
    RefreshPolicy, RunMode, SessionRef, SkipReason, StatusDocument, StatusInputs, TokenUsage, Tone,
    TurnState, build_child_env, clean_gateway_url, external_segment, interval_from_config,
    parse_command, parse_output, plan,
};

const FORMAT_DOCUMENT: &str = include_str!("../../../docs/formats/status-line.md");

const BYTE_LIMIT: usize = 16384;
const ROW_LIMIT: usize = 8;

// ---------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------

fn session_inputs() -> StatusInputs {
    StatusInputs {
        connection: Connection::Connected,
        session: Some(SessionRef {
            id: "sess-1".to_string(),
            title: None,
        }),
        ..StatusInputs::default()
    }
}

fn encoded(inputs: &StatusInputs) -> Vec<u8> {
    StatusDocument::from_inputs(inputs)
        .expect("a document for a state with a session")
        .encode()
}

fn encoded_json(inputs: &StatusInputs) -> serde_json::Value {
    let bytes = encoded(inputs);
    serde_json::from_slice(&bytes).expect("the document is JSON")
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn flags_nothing(_: &str) -> bool {
    false
}

fn env_names(env: &[(String, String)]) -> Vec<&str> {
    env.iter().map(|(k, _)| k.as_str()).collect()
}

fn env_value<'a>(env: &'a [(String, String)], name: &str) -> Option<&'a str> {
    env.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn parse(raw: &str) -> ParsedOutput {
    parse_output(raw.as_bytes(), BYTE_LIMIT, ROW_LIMIT)
}

/// The rows and the truncated flag of a parse that must have produced rows.
fn rows_of(parsed: ParsedOutput) -> (Vec<ExternalRow>, bool) {
    match parsed {
        ParsedOutput::Rows { rows, truncated } => (rows, truncated),
        ParsedOutput::Failure(failure) => panic!("expected rows, got failure {failure:?}"),
    }
}

fn texts(rows: &[ExternalRow]) -> Vec<&str> {
    rows.iter().map(|r| r.text.as_str()).collect()
}

fn is_invalid_document(parsed: &ParsedOutput) -> bool {
    matches!(parsed, ParsedOutput::Failure(Failure::InvalidDocument(_)))
}

fn row(text: &str, tone: Tone) -> ExternalRow {
    ExternalRow {
        text: text.to_string(),
        tone,
    }
}

fn rows_result(rows: Vec<ExternalRow>) -> CommandResult {
    CommandResult {
        parsed: ParsedOutput::Rows {
            rows,
            truncated: false,
        },
    }
}

fn failure_result(failure: Failure) -> CommandResult {
    CommandResult {
        parsed: ParsedOutput::Failure(failure),
    }
}

/// `n` plain rows named `r1`, `r2` and so on, each on its own line.
fn numbered_lines(n: usize) -> String {
    (1..=n).map(|i| format!("r{i}\n")).collect()
}

// ---------------------------------------------------------------------------------------
// The document the command receives
// ---------------------------------------------------------------------------------------

#[test]
fn stdin_document_has_exactly_the_frozen_field_set() {
    let expected = [
        "version",
        "mode",
        "connection",
        "session",
        "turn",
        "pending_prompts",
        "subagents",
        "usage",
    ];
    let bytes = encoded(&session_inputs());
    let text = String::from_utf8(bytes.clone()).expect("UTF-8");
    assert!(
        text.starts_with("{\"version\":1,"),
        "version is the first field: {text}"
    );

    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON");
    let object = value.as_object().expect("an object");
    let keys: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    let wanted: BTreeSet<&str> = expected.iter().copied().collect();
    assert_eq!(keys, wanted, "exactly the eight frozen fields");
    let declared: BTreeSet<&str> = DOCUMENT_FIELDS.iter().copied().collect();
    assert_eq!(keys, declared, "DOCUMENT_FIELDS names the same eight");

    // The fields appear in the frozen order in the text.
    let mut last = 0;
    for key in expected {
        let at = text
            .find(&format!("\"{key}\":"))
            .unwrap_or_else(|| panic!("{key} missing in {text}"));
        assert!(at >= last, "{key} is out of order in {text}");
        last = at;
    }

    // The documented sample, byte for byte.
    assert_eq!(
        text,
        "{\"version\":1,\"mode\":\"live\",\"connection\":\"connected\",\
         \"session\":{\"id\":\"sess-1\",\"title\":null},\"turn\":\"idle\",\
         \"pending_prompts\":0,\"subagents\":{\"active\":0,\"terminal\":0},\"usage\":null}\n"
    );
}

#[test]
fn stdin_document_spellings_for_every_enum_value() {
    for (mode, word) in [(RunMode::Live, "live"), (RunMode::Replay, "replay")] {
        let inputs = StatusInputs {
            mode,
            ..session_inputs()
        };
        assert_eq!(encoded_json(&inputs)["mode"], word);
    }

    for (connection, word) in [
        (Connection::Disconnected, "disconnected"),
        (Connection::Connecting, "connecting"),
        (Connection::Connected, "connected"),
        (Connection::Reconnecting, "reconnecting"),
        (Connection::AuthFailed, "auth_failed"),
    ] {
        let inputs = StatusInputs {
            connection,
            ..session_inputs()
        };
        assert_eq!(encoded_json(&inputs)["connection"], word);
    }

    for (turn, word) in [
        (TurnState::Idle, "idle"),
        (TurnState::Streaming, "streaming"),
        (TurnState::Waiting, "waiting"),
        (TurnState::Cancelled, "cancelled"),
    ] {
        let inputs = StatusInputs {
            turn,
            ..session_inputs()
        };
        assert_eq!(encoded_json(&inputs)["turn"], word);
    }

    // Session and sub-agent fields carry their values.
    let inputs = StatusInputs {
        session: Some(SessionRef {
            id: "abc".to_string(),
            title: Some("a title".to_string()),
        }),
        subagents_active: 2,
        subagents_terminal: 3,
        ..session_inputs()
    };
    let value = encoded_json(&inputs);
    assert_eq!(value["session"]["id"], "abc");
    assert_eq!(value["session"]["title"], "a title");
    assert_eq!(value["subagents"]["active"], 2);
    assert_eq!(value["subagents"]["terminal"], 3);
}

#[test]
fn stdin_usage_is_null_when_absent_and_exact_when_given() {
    let absent = StatusInputs {
        usage_tokens: None,
        ..session_inputs()
    };
    let text = String::from_utf8(encoded(&absent)).expect("UTF-8");
    assert!(text.contains("\"usage\":null"), "{text}");

    let given = StatusInputs {
        usage_tokens: Some(TokenUsage {
            input_tokens: 120,
            output_tokens: 340,
        }),
        ..session_inputs()
    };
    let text = String::from_utf8(encoded(&given)).expect("UTF-8");
    assert!(
        text.contains("\"usage\":{\"input_tokens\":120,\"output_tokens\":340}"),
        "{text}"
    );

    // A supplied zero pair is encoded as zeros; the document never reinterprets it.
    let zeros = StatusInputs {
        usage_tokens: Some(TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        }),
        ..session_inputs()
    };
    let text = String::from_utf8(encoded(&zeros)).expect("UTF-8");
    assert!(
        text.contains("\"usage\":{\"input_tokens\":0,\"output_tokens\":0}"),
        "{text}"
    );
}

#[test]
fn stdin_pending_prompts_counts_the_focused_session_only() {
    // The install-wide needs-you count is 5; the focused session has one outstanding prompt.
    let inputs = StatusInputs {
        needs_you: 5,
        pending_prompts: 1,
        ..session_inputs()
    };
    assert_eq!(encoded_json(&inputs)["pending_prompts"], 1);
    let line = plan(&inputs, None, 200).text();
    assert!(
        line.contains("⚑ 5 need you"),
        "the line shows the install-wide count: {line:?}"
    );

    // And the other way round: no prompt in this session, several elsewhere.
    let inputs = StatusInputs {
        needs_you: 3,
        pending_prompts: 0,
        ..session_inputs()
    };
    assert_eq!(encoded_json(&inputs)["pending_prompts"], 0);
}

#[test]
fn stdin_no_document_without_a_session() {
    let inputs = StatusInputs {
        session: None,
        ..session_inputs()
    };
    assert!(StatusDocument::from_inputs(&inputs).is_none());
}

#[test]
fn stdin_encoded_document_is_one_utf8_line_with_trailing_newline() {
    let title = "a \"q\" \n é";
    let inputs = StatusInputs {
        session: Some(SessionRef {
            id: "sess-1".to_string(),
            title: Some(title.to_string()),
        }),
        ..session_inputs()
    };
    let bytes = encoded(&inputs);
    let newlines: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        newlines,
        vec![bytes.len() - 1],
        "exactly one newline, and it is the last byte"
    );
    let text = String::from_utf8(bytes.clone()).expect("UTF-8");
    assert!(text.ends_with("}\n"));
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON");
    assert_eq!(value["session"]["title"], title);
}

#[test]
fn stdin_document_fields_match_the_format_document() {
    // The first table whose header starts `| field` is the one under "What the command
    // receives"; the output document has a second table with the same header.
    let lines: Vec<&str> = FORMAT_DOCUMENT.lines().collect();
    let header = lines
        .iter()
        .position(|l| l.trim_start().starts_with("| field"))
        .expect("the field table");
    let mut fields = Vec::new();
    for line in &lines[header + 1..] {
        let line = line.trim_start();
        if !line.starts_with('|') {
            break;
        }
        let first = line
            .split('|')
            .nth(1)
            .expect("a first column")
            .trim()
            .replace('`', "");
        if first.chars().all(|c| c == '-' || c == ' ') {
            continue; // the separator row
        }
        fields.push(first);
    }
    assert_eq!(fields.len(), 8, "the table lists eight fields: {fields:?}");
    let declared: Vec<String> = DOCUMENT_FIELDS.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        fields, declared,
        "DOCUMENT_FIELDS follows the table in order"
    );

    // The first fenced json block of the document has the same keys as an encoded sample.
    let start = FORMAT_DOCUMENT.find("```json").expect("a json block") + "```json".len();
    let end = start + FORMAT_DOCUMENT[start..].find("```").expect("end of block");
    let sample: serde_json::Value =
        serde_json::from_str(&FORMAT_DOCUMENT[start..end]).expect("the documented sample");
    let sample_keys: BTreeSet<String> = sample
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    let encoded_value = encoded_json(&session_inputs());
    let encoded_keys: BTreeSet<String> = encoded_value
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(sample_keys, encoded_keys);
}

// ---------------------------------------------------------------------------------------
// The environment the command gets
// ---------------------------------------------------------------------------------------

#[test]
fn env_forwards_base_and_locale_names() {
    // Offered in a scrambled order: the result is sorted by name.
    let parent = pairs(&[
        ("TMPDIR", "/tmp-fixture"),
        ("LC_CTYPE", "UTF-8"),
        ("PATH", "/bin-fixture"),
        ("TERM", "xterm"),
        ("LC_ALL", "C"),
        ("SHELL", "/sh-fixture"),
        ("LANG", "en_US.UTF-8"),
        ("HOME", "/home-fixture"),
    ]);
    let env = build_child_env(&parent, &[], &flags_nothing);
    assert_eq!(
        env_names(&env),
        vec![
            "HOME", "LANG", "LC_ALL", "LC_CTYPE", "PATH", "SHELL", "TERM", "TMPDIR"
        ]
    );
    assert_eq!(env_value(&env, "PATH"), Some("/bin-fixture"));
    assert_eq!(env_value(&env, "LC_ALL"), Some("C"));
    assert_eq!(env_value(&env, "LANG"), Some("en_US.UTF-8"));
}

#[test]
fn env_lang_lookalikes_are_not_locale() {
    // The check flags nothing, so only this unit's own rule keeps the lookalikes out.
    let parent = pairs(&[
        ("LANG", "en_US.UTF-8"),
        ("LANGCHAIN_API_KEY", "canary-lang"),
        ("LANGSMITH_TRACING", "true"),
        ("LANGUAGE", "en"),
    ]);
    let env = build_child_env(&parent, &[], &flags_nothing);
    assert_eq!(env_names(&env), vec!["LANG"]);
}

#[test]
fn env_drops_unlisted_variables() {
    let parent = pairs(&[
        ("MY_CANARY", "canary-value"),
        ("PATH", "/bin-fixture"),
        ("EDITOR", "vi"),
        ("USER", "someone"),
        ("HOME", "/home-fixture"),
    ]);
    let env = build_child_env(&parent, &[], &flags_nothing);
    assert_eq!(
        env,
        pairs(&[("HOME", "/home-fixture"), ("PATH", "/bin-fixture")])
    );
    assert!(env_value(&env, "MY_CANARY").is_none());
}

#[test]
fn env_only_five_talaria_names_are_candidates() {
    let expected_five = [
        "TALARIA_CONFIG_DIR",
        "TALARIA_GATEWAY_URL",
        "TALARIA_LOG_LEVEL",
        "TALARIA_PROFILE",
        "TALARIA_STATUS_INTERVAL",
    ];
    let declared: BTreeSet<&str> = FORWARDED_TALARIA.iter().copied().collect();
    let wanted: BTreeSet<&str> = expected_five.iter().copied().collect();
    assert_eq!(declared, wanted, "FORWARDED_TALARIA names the five");

    let parent = pairs(&[
        ("TALARIA_CONFIG_DIR", "/config-fixture"),
        ("TALARIA_GATEWAY_URL", "ws://h:1/p"),
        ("TALARIA_PROFILE", "fixture"),
        ("TALARIA_LOG_LEVEL", "debug"),
        ("TALARIA_STATUS_INTERVAL", "7"),
        ("TALARIA_OTHER", "nope"),
        ("TALARIA_CREDENTIALS", "nope"),
    ]);
    let allowlist = names(&["TALARIA_OTHER", "TALARIA_CREDENTIALS"]);
    let env = build_child_env(&parent, &allowlist, &flags_nothing);
    assert_eq!(env_names(&env), expected_five.to_vec());
    assert_eq!(env_value(&env, "TALARIA_PROFILE"), Some("fixture"));
    assert_eq!(env_value(&env, "TALARIA_STATUS_INTERVAL"), Some("7"));

    // An allow-list entry equal to one of the five adds nothing second.
    let allowlist = names(&["TALARIA_PROFILE", "PATH"]);
    let env = build_child_env(&parent, &allowlist, &flags_nothing);
    let profile_entries = env.iter().filter(|(k, _)| k == "TALARIA_PROFILE").count();
    assert_eq!(profile_entries, 1);
}

#[test]
fn env_gateway_url_is_cleaned_by_name_on_every_path() {
    // The function on its own.
    assert_eq!(
        clean_gateway_url("ws://h:1/p?ticket=x#f").as_deref(),
        Some("ws://h:1/p")
    );
    assert_eq!(
        clean_gateway_url("ws://u:pw@h:1/p").as_deref(),
        Some("ws://h:1/p")
    );
    assert_eq!(
        clean_gateway_url("ws://[::1]:2/p?token=x").as_deref(),
        Some("ws://[::1]:2/p")
    );
    assert_eq!(clean_gateway_url("not a url"), None);

    // Through the environment, with and without the name on the allow-list: the allow-list
    // cannot bring the raw value back.
    let cases = [
        ("ws://h:1/p?ticket=x#f", Some("ws://h:1/p")),
        ("ws://u:pw@h:1/p", Some("ws://h:1/p")),
        ("ws://[::1]:2/p?token=x", Some("ws://[::1]:2/p")),
        ("not a url", None),
    ];
    for allowlist in [vec![], names(&["TALARIA_GATEWAY_URL"])] {
        for (raw, expected) in cases {
            let parent = pairs(&[("TALARIA_GATEWAY_URL", raw), ("PATH", "/bin-fixture")]);
            let env = build_child_env(&parent, &allowlist, &flags_nothing);
            assert_eq!(
                env_value(&env, "TALARIA_GATEWAY_URL"),
                expected,
                "value {raw:?} with allow-list {allowlist:?}"
            );
            let entries = env
                .iter()
                .filter(|(k, _)| k == "TALARIA_GATEWAY_URL")
                .count();
            assert!(entries <= 1, "the name appears at most once");
            for (_, value) in &env {
                assert!(!value.contains("ticket"), "no ticket anywhere: {value}");
                assert!(
                    !value.contains("pw"),
                    "no user information anywhere: {value}"
                );
            }
        }
    }
}

#[test]
fn env_allowlist_forwards_named_but_cannot_rescue_credentials() {
    let is_credential = |name: &str| name == "SERVICE_TOKEN" || name == "APP_SECRET";
    let parent = pairs(&[
        ("SERVICE_TOKEN", "canary-token"),
        ("APP_SECRET", "canary-secret"),
        ("BENIGN_NAME", "benign-value"),
        ("PATH", "/bin-fixture"),
    ]);
    let allowlist = names(&["SERVICE_TOKEN", "APP_SECRET", "BENIGN_NAME", "ABSENT_NAME"]);
    let env = build_child_env(&parent, &allowlist, &is_credential);
    assert_eq!(
        env,
        pairs(&[("BENIGN_NAME", "benign-value"), ("PATH", "/bin-fixture")])
    );

    // Without the allow-list the benign name is not forwarded at all.
    let env = build_child_env(&parent, &[], &is_credential);
    assert_eq!(env, pairs(&[("PATH", "/bin-fixture")]));

    // A flagged name stays out even when it is one of the five Talaria names.
    let flag_profile = |name: &str| name == "TALARIA_PROFILE";
    let parent = pairs(&[("TALARIA_PROFILE", "fixture"), ("PATH", "/bin-fixture")]);
    let env = build_child_env(&parent, &names(&["TALARIA_PROFILE"]), &flag_profile);
    assert_eq!(env, pairs(&[("PATH", "/bin-fixture")]));
}

#[test]
fn env_provider_key_names_never_forward() {
    let denied_upper: BTreeSet<String> = DENIED_PROVIDER_KEYS
        .iter()
        .map(|s| s.to_uppercase())
        .collect();
    let wanted: BTreeSet<String> = [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "AWS_ACCESS_KEY_ID",
        "GITHUB_PAT",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    assert_eq!(denied_upper, wanted, "DENIED_PROVIDER_KEYS names the four");

    // Upper, lower and mixed case, in the parent and on the allow-list, with a check that
    // flags nothing: the fixed list alone removes them.
    let mut parent = pairs(&[("PATH", "/bin-fixture")]);
    let mut allowlist = Vec::new();
    for upper in wanted {
        let lower = upper.to_lowercase();
        let mixed: String = upper
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i % 2 == 0 {
                    c
                } else {
                    c.to_ascii_lowercase()
                }
            })
            .collect();
        for name in [upper, lower, mixed] {
            parent.push((name.clone(), "canary-key".to_string()));
            allowlist.push(name);
        }
    }
    let env = build_child_env(&parent, &allowlist, &flags_nothing);
    assert_eq!(env, pairs(&[("PATH", "/bin-fixture")]));
}

#[test]
fn env_real_shared_check_denies_the_documented_shapes() {
    let check = hermes_client::redact::is_suspicious_key;
    let credential_names = [
        "SERVICE_TOKEN",
        "APP_SECRET",
        "DB_PASSWORD",
        "X_API_KEY",
        "TALARIA_SESSION_TOKEN",
    ];
    for name in credential_names {
        assert!(check(name), "the shared check flags {name}");
    }

    let mut parent = pairs(&[("PATH", "/bin-fixture"), ("BENIGN_NAME", "benign-value")]);
    for name in credential_names {
        parent.push((name.to_string(), "canary-credential".to_string()));
    }
    let mut allowlist = names(&["BENIGN_NAME"]);
    allowlist.extend(credential_names.iter().map(|s| (*s).to_string()));
    let env = build_child_env(&parent, &allowlist, &check);
    assert_eq!(
        env,
        pairs(&[("BENIGN_NAME", "benign-value"), ("PATH", "/bin-fixture")])
    );
}

// ---------------------------------------------------------------------------------------
// Reading what the command printed
// ---------------------------------------------------------------------------------------

#[test]
fn output_plain_rows_are_literal_and_trailing_newline_is_not_a_row() {
    let (rows, truncated) = rows_of(parse("alpha\nbeta\n"));
    assert_eq!(texts(&rows), vec!["alpha", "beta"]);
    assert!(rows.iter().all(|r| r.tone == Tone::Muted));
    assert!(!truncated);

    // No trailing newline: the same rows.
    let (rows, _) = rows_of(parse("alpha\nbeta"));
    assert_eq!(texts(&rows), vec!["alpha", "beta"]);

    // Only one trailing newline is dropped; a blank row before it stays a row.
    let (rows, _) = rows_of(parse("alpha\n\n"));
    assert_eq!(texts(&rows), vec!["alpha", ""]);
    let (rows, _) = rows_of(parse("a\n\nb\n"));
    assert_eq!(texts(&rows), vec!["a", "", "b"]);

    // Carriage returns before newlines, and at the very end, are removed.
    let (rows, _) = rows_of(parse("a\r\nb\r\n"));
    assert_eq!(texts(&rows), vec!["a", "b"]);
    let (rows, _) = rows_of(parse("a\r\nb\r"));
    assert_eq!(texts(&rows), vec!["a", "b"]);

    // Colour codes are not interpreted: they stay as characters of the row.
    let (rows, _) = rows_of(parse("\u{1b}[31mred\u{1b}[0m text\n"));
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].text.contains("[31mred") && rows[0].text.contains("[0m text"),
        "the colour codes are kept as characters: {:?}",
        rows[0].text
    );
    assert_eq!(rows[0].tone, Tone::Muted);
}

#[test]
fn output_more_than_eight_rows_ends_with_a_visible_marker() {
    // Exactly eight rows: nothing is cut.
    let (rows, truncated) = rows_of(parse(&numbered_lines(8)));
    assert_eq!(rows.len(), 8);
    assert_eq!(rows[7].text, "r8");
    assert!(!truncated);

    // Nine rows: the first seven, then the marker.
    let (rows, truncated) = rows_of(parse(&numbered_lines(9)));
    assert_eq!(rows.len(), 8);
    assert_eq!(
        texts(&rows),
        vec![
            "r1",
            "r2",
            "r3",
            "r4",
            "r5",
            "r6",
            "r7",
            "… truncated (9 rows)"
        ]
    );
    assert!(truncated);

    // Twelve rows.
    let (rows, truncated) = rows_of(parse(&numbered_lines(12)));
    assert_eq!(rows.len(), 8);
    assert_eq!(rows[6].text, "r7");
    assert_eq!(rows[7].text, "… truncated (12 rows)");
    assert!(truncated);

    // A version two document has the same bound; its marker row has the text tone.
    let entries: Vec<String> = (1..=9).map(|i| format!("\"d{i}\"")).collect();
    let document = format!("{{\"version\":2,\"rows\":[{}]}}", entries.join(","));
    let (rows, truncated) = rows_of(parse(&document));
    assert_eq!(rows.len(), 8);
    assert_eq!(rows[6].text, "d7");
    assert_eq!(rows[7], row("… truncated (9 rows)", Tone::Text));
    assert!(truncated);
}

#[test]
fn output_past_16_kib_is_cut_marked_and_split_characters_are_tolerated() {
    // Exactly 16384 bytes is not past the cap.
    let exact = "a".repeat(16384);
    let (rows, truncated) = rows_of(parse(&exact));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].text.len(), 16384);
    assert!(!truncated, "the cap applies from 16385 bytes");

    // One byte more is cut and flagged.
    let over = "a".repeat(16385);
    let (rows, truncated) = rows_of(parse(&over));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].text.len(), 16384);
    assert!(truncated);

    // A three byte character starting at byte 16383 is split by the cap: tolerated, and the
    // text keeps everything before it.
    let mut split = vec![b'a'; 16383];
    split.extend_from_slice("€".as_bytes());
    assert_eq!(split.len(), 16386);
    let (rows, truncated) = rows_of(parse_output(&split, BYTE_LIMIT, ROW_LIMIT));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].text, "a".repeat(16383));
    assert!(truncated);

    // A genuinely bad byte well before the cap fails, cut or not.
    let mut bad = vec![b'a'; 20000];
    bad[10] = 0xff;
    assert_eq!(
        parse_output(&bad, BYTE_LIMIT, ROW_LIMIT),
        ParsedOutput::Failure(Failure::InvalidUtf8)
    );
    let mut bad_short = vec![b'a'; 100];
    bad_short[10] = 0xff;
    assert_eq!(
        parse_output(&bad_short, BYTE_LIMIT, ROW_LIMIT),
        ParsedOutput::Failure(Failure::InvalidUtf8)
    );
}

#[test]
fn output_empty_and_invalid_utf8_are_failures() {
    assert_eq!(
        parse_output(b"", BYTE_LIMIT, ROW_LIMIT),
        ParsedOutput::Failure(Failure::NoOutput)
    );
    assert_eq!(
        parse_output(&[0xff], BYTE_LIMIT, ROW_LIMIT),
        ParsedOutput::Failure(Failure::InvalidUtf8)
    );
    assert_eq!(
        parse_output(b"ab\xffcd\n", BYTE_LIMIT, ROW_LIMIT),
        ParsedOutput::Failure(Failure::InvalidUtf8)
    );

    // Only a newline is not empty: it is one blank row, with no failure.
    let (rows, truncated) = rows_of(parse("\n"));
    assert_eq!(texts(&rows), vec![""]);
    assert!(!truncated);
    assert!(matches!(parse("  \n\n"), ParsedOutput::Rows { .. }));
}

#[test]
fn output_version_two_document_yields_rows_with_tones() {
    let document = r#"{"version":2,"rows":["tests: 296",{"text":"wip: 2","color":"warning"},{"text":"plain"},{"text":"odd","color":"neon"},{"text":"fine","color":"success"}]}"#;
    let (rows, truncated) = rows_of(parse(document));
    assert_eq!(
        rows,
        vec![
            row("tests: 296", Tone::Text),
            row("wip: 2", Tone::Warning),
            row("plain", Tone::Text),
            row("odd", Tone::Text),
            row("fine", Tone::Success),
        ]
    );
    assert!(!truncated);

    // Every one of the seven tone names is understood.
    let document = r#"{"version":2,"rows":[
        {"text":"a","color":"text"},{"text":"b","color":"muted"},
        {"text":"c","color":"separator"},{"text":"d","color":"success"},
        {"text":"e","color":"warning"},{"text":"f","color":"error"},
        {"text":"g","color":"attention"}]}"#;
    let (rows, _) = rows_of(parse(document));
    let tones: Vec<Tone> = rows.iter().map(|r| r.tone).collect();
    assert_eq!(
        tones,
        vec![
            Tone::Text,
            Tone::Muted,
            Tone::Separator,
            Tone::Success,
            Tone::Warning,
            Tone::Error,
            Tone::Attention,
        ]
    );

    // An empty rows list is a valid document with no rows.
    let (rows, truncated) = rows_of(parse(r#"{"version":2,"rows":[]}"#));
    assert!(rows.is_empty());
    assert!(!truncated);
}

#[test]
fn output_malformed_documents_are_rejected_not_rendered() {
    let bodies = [
        r#"{"version":1,"rows":[]}"#,
        r#"{"version":3,"rows":[]}"#,
        r#"{"version":"2","rows":[]}"#,
        r#"{"rows":[]}"#,
        r#"{"version":2}"#,
        r#"{"version":2,"rows":"x"}"#,
        r#"{"version":2,"rows":[null]}"#,
        r#"{"version":2,"rows":[{"text":1}]}"#,
        r#"{"version":2,"rows":[{"text":"a","color":3}]}"#,
        r#"{"version":2,"rows":["a"],"extra":1}"#,
        r#"{"version":2,"rows":[{"text":"a","x":1}]}"#,
    ];
    for body in bodies {
        let parsed = parse(body);
        assert!(
            is_invalid_document(&parsed),
            "{body} should be an invalid document, got {parsed:?}"
        );
        let parsed_with_newline = parse(&format!("{body}\n"));
        assert!(
            is_invalid_document(&parsed_with_newline),
            "{body} with a newline should be an invalid document"
        );
    }
}

#[test]
fn output_scalars_arrays_and_pretty_documents() {
    // Scalars and arrays are plain text, never documents.
    for body in ["123", "null", "[1]", "\"text\"", "true"] {
        let (rows, truncated) = rows_of(parse(&format!("{body}\n")));
        assert_eq!(texts(&rows), vec![body], "{body} stays a plain row");
        assert_eq!(rows[0].tone, Tone::Muted);
        assert!(!truncated);
    }

    // A pretty-printed document spans lines and is still a document.
    let pretty = "{\n  \"version\": 2,\n  \"rows\": [\n    \"a\",\n    {\"text\": \"b\", \"color\": \"error\"}\n  ]\n}\n";
    let (rows, truncated) = rows_of(parse(pretty));
    assert_eq!(rows, vec![row("a", Tone::Text), row("b", Tone::Error)]);
    assert!(!truncated);

    // Output cut at the cap is never parsed as a document, even when what is left happens
    // to be a complete document followed by blanks.
    let mut raw = String::from(r#"{"version":2,"rows":["a"]}"#);
    raw.push_str(&" ".repeat(100));
    let (rows, truncated) = rows_of(parse_output(raw.as_bytes(), 30, ROW_LIMIT));
    assert!(truncated);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tone, Tone::Muted, "plain text, not a document row");
    assert!(
        rows[0].text.starts_with(r#"{"version":2"#),
        "the document text is shown as plain text: {:?}",
        rows[0].text
    );

    // A version one document is not a plain row either: it is rejected.
    assert!(is_invalid_document(&parse(
        r#"{"version":1,"mode":"live"}"#
    )));
}

#[test]
fn failure_markers_have_the_documented_text() {
    assert_eq!(Failure::NoOutput.marker(), "status: no output");
    assert_eq!(
        Failure::InvalidUtf8.marker(),
        "status: invalid output (not UTF-8)"
    );
    assert_eq!(
        Failure::InvalidDocument("version must be 2".to_string()).marker(),
        "status: invalid script document"
    );
    assert_eq!(
        Failure::NonzeroExit {
            code: Some(3),
            stderr_line: None
        }
        .marker(),
        "status: command failed (exit 3)"
    );
    assert_eq!(
        Failure::NonzeroExit {
            code: Some(3),
            stderr_line: Some("oops".to_string())
        }
        .marker(),
        "status: command failed (exit 3): oops"
    );
    assert_eq!(
        Failure::NonzeroExit {
            code: None,
            stderr_line: None
        }
        .marker(),
        "status: command failed (signal)"
    );
    assert_eq!(
        Failure::NonzeroExit {
            code: None,
            stderr_line: Some("oops".to_string())
        }
        .marker(),
        "status: command failed (signal): oops"
    );
    assert_eq!(Failure::TimedOut.marker(), "status: command timed out");
    assert_eq!(Failure::NotFound.marker(), "status: command not found");

    // Reasons kept for the log are never shown in the row.
    let spawn = Failure::SpawnFailed("permission denied for fixture-detail".to_string());
    assert_eq!(spawn.marker(), "status: could not start command");
    let document = Failure::InvalidDocument("fixture-reason".to_string());
    assert!(!document.marker().contains("fixture-reason"));
}

#[test]
fn parse_command_splits_posix_quoting_without_a_shell() {
    assert_eq!(
        parse_command("git status --short"),
        Ok(names(&["git", "status", "--short"]))
    );
    assert_eq!(
        parse_command("echo \"a b\" 'c d'"),
        Ok(names(&["echo", "a b", "c d"]))
    );
    assert_eq!(parse_command("echo a\\ b"), Ok(names(&["echo", "a b"])));
    assert_eq!(
        parse_command("  spaced   out  "),
        Ok(names(&["spaced", "out"]))
    );

    // Shell syntax is plain words: nothing expands, nothing pipes.
    assert_eq!(
        parse_command("one | two && three $HOME * ;"),
        Ok(names(&[
            "one", "|", "two", "&&", "three", "$HOME", "*", ";"
        ]))
    );

    let empty = "the status command must not be empty; it is disabled";
    assert_eq!(parse_command(""), Err(empty.to_string()));
    assert_eq!(parse_command("   \t "), Err(empty.to_string()));

    // A bad quote gives a fixed reason that never repeats the command.
    let invalid = "the status command has invalid quoting; it is disabled";
    assert_eq!(
        parse_command("echo \"unterminated fixture-secret"),
        Err(invalid.to_string())
    );
    assert_eq!(
        parse_command("echo 'unterminated fixture-secret"),
        Err(invalid.to_string())
    );
}

#[test]
fn interval_from_config_clamps_to_one_through_3600() {
    assert_eq!(interval_from_config(0), Duration::from_secs(1));
    assert_eq!(interval_from_config(1), Duration::from_secs(1));
    assert_eq!(interval_from_config(5), Duration::from_secs(5));
    assert_eq!(interval_from_config(3600), Duration::from_secs(3600));
    assert_eq!(interval_from_config(3601), Duration::from_secs(3600));
    assert_eq!(interval_from_config(u64::MAX), Duration::from_secs(3600));
}

#[test]
fn limits_default_to_the_documented_values() {
    // The shortest gap between two runs when the document changed: one second.
    assert_eq!(MIN_RERUN_GAP, Duration::from_secs(1));

    let limits = CommandLimits::default();
    assert_eq!(limits.timeout, Duration::from_secs(2));
    assert_eq!(limits.stdout_bytes, 16384);
    assert_eq!(limits.stderr_bytes, 4096);
    assert_eq!(limits.rows, 8);
}

// ---------------------------------------------------------------------------------------
// When the command runs
// ---------------------------------------------------------------------------------------

fn at(base: Instant, millis: u64) -> Instant {
    base + Duration::from_millis(millis)
}

#[test]
fn refresh_first_tick_runs() {
    let base = Instant::now();
    let mut policy = RefreshPolicy::new(Duration::from_secs(5));
    assert_eq!(
        policy.next_wake(base),
        Some(base),
        "a policy that never ran wakes now"
    );
    assert_eq!(policy.decide(base, b"doc-a", false), Decision::Run);
}

#[test]
fn refresh_unchanged_document_waits_for_the_interval() {
    let base = Instant::now();
    let mut policy = RefreshPolicy::new(Duration::from_secs(5));
    assert_eq!(policy.decide(base, b"doc-a", false), Decision::Run);

    assert_eq!(
        policy.next_wake(at(base, 10)),
        Some(at(base, 5000)),
        "nothing pending: the next wake is the interval"
    );
    assert_eq!(
        policy.decide(at(base, 2000), b"doc-a", false),
        Decision::Skip(SkipReason::Unchanged)
    );
    assert_eq!(
        policy.decide(at(base, 4999), b"doc-a", false),
        Decision::Skip(SkipReason::Unchanged)
    );
    assert_eq!(
        policy.decide(at(base, 5000), b"doc-a", false),
        Decision::Run
    );
    assert_eq!(
        policy.next_wake(at(base, 5010)),
        Some(at(base, 10000)),
        "the interval counts from the latest start"
    );
    assert_eq!(
        policy.decide(at(base, 9999), b"doc-a", false),
        Decision::Skip(SkipReason::Unchanged)
    );
    assert_eq!(
        policy.decide(at(base, 10000), b"doc-a", false),
        Decision::Run
    );

    // A change that is undone before any run leaves nothing pending.
    let mut policy = RefreshPolicy::new(Duration::from_secs(5));
    assert_eq!(policy.decide(base, b"doc-a", false), Decision::Run);
    assert_eq!(
        policy.decide(at(base, 500), b"doc-b", false),
        Decision::Skip(SkipReason::TooSoon)
    );
    assert_eq!(
        policy.next_wake(at(base, 500)),
        Some(at(base, 1000)),
        "a change is pending: wake when the minimum gap has passed"
    );
    assert_eq!(
        policy.decide(at(base, 600), b"doc-a", false),
        Decision::Skip(SkipReason::Unchanged)
    );
    assert_eq!(
        policy.next_wake(at(base, 600)),
        Some(at(base, 5000)),
        "the change was undone: the next wake is the interval, not one second"
    );
}

#[test]
fn refresh_changed_document_runs_after_the_minimum_gap() {
    let base = Instant::now();
    let mut policy = RefreshPolicy::new(Duration::from_secs(5));
    assert_eq!(policy.decide(base, b"doc-a", false), Decision::Run);
    assert_eq!(
        policy.decide(at(base, 200), b"doc-b", false),
        Decision::Skip(SkipReason::TooSoon)
    );
    assert_eq!(
        policy.decide(at(base, 999), b"doc-b", false),
        Decision::Skip(SkipReason::TooSoon)
    );
    assert_eq!(
        policy.decide(at(base, 1000), b"doc-b", false),
        Decision::Run
    );

    // The caller calls when the document changes and at each instant `next_wake` reports.
    let mut policy = RefreshPolicy::new(Duration::from_secs(5));
    let mut runs = Vec::new();
    if policy.decide(base, b"doc-a", false) == Decision::Run {
        runs.push(0u64);
    }
    // The document changes at 0.5 s: the caller offers it at once.
    let change = at(base, 500);
    assert_eq!(
        policy.decide(change, b"doc-b", false),
        Decision::Skip(SkipReason::TooSoon)
    );
    let mut now = change;
    for _ in 0..20 {
        let Some(wake) = policy.next_wake(now) else {
            break;
        };
        let offset = u64::try_from(wake.duration_since(base).as_millis()).expect("fits");
        if offset > 7000 {
            break;
        }
        now = wake;
        if policy.decide(now, b"doc-b", false) == Decision::Run {
            runs.push(offset);
        }
    }
    assert_eq!(runs, vec![0, 1000, 6000]);
}

#[test]
fn refresh_never_overlaps_a_running_command() {
    let base = Instant::now();

    // Even on the very first tick, a running command wins; nothing is consumed by the skip.
    let mut policy = RefreshPolicy::new(Duration::from_secs(5));
    assert_eq!(
        policy.decide(base, b"doc-a", true),
        Decision::Skip(SkipReason::Running)
    );
    assert_eq!(policy.decide(at(base, 10), b"doc-a", false), Decision::Run);

    // Changed and overdue: still a skip while running.
    assert_eq!(
        policy.decide(at(base, 60_000), b"doc-b", true),
        Decision::Skip(SkipReason::Running)
    );
    assert_eq!(
        policy.decide(at(base, 61_000), b"doc-c", true),
        Decision::Skip(SkipReason::Running)
    );
    // When the command has ended the next tick runs.
    assert_eq!(
        policy.decide(at(base, 62_000), b"doc-c", false),
        Decision::Run
    );
}

// ---------------------------------------------------------------------------------------
// What is shown beside a failure
// ---------------------------------------------------------------------------------------

#[test]
fn external_display_keeps_last_good_beside_a_failure() {
    let mut display = ExternalDisplay::default();
    assert!(
        display.segment().is_none(),
        "nothing before the first result"
    );

    display.apply(&rows_result(vec![row("ok", Tone::Muted)]));
    let shown = display.segment().expect("a segment");
    assert_eq!(shown.text, "ok");
    assert_eq!(shown.tone, Tone::Muted);

    display.apply(&failure_result(Failure::NoOutput));
    let shown = display.segment().expect("a segment");
    assert_eq!(shown.text, "ok · status: no output");
    assert_eq!(shown.tone, Tone::Warning);

    // A second failure replaces the marker, not the good text.
    display.apply(&failure_result(Failure::TimedOut));
    let shown = display.segment().expect("a segment");
    assert_eq!(shown.text, "ok · status: command timed out");
    assert_eq!(shown.tone, Tone::Warning);

    // The next good result replaces both.
    display.apply(&rows_result(vec![row("fine", Tone::Muted)]));
    let shown = display.segment().expect("a segment");
    assert_eq!(shown.text, "fine");
    assert_eq!(shown.tone, Tone::Muted);

    // A failure with no earlier good row shows the marker alone.
    let mut display = ExternalDisplay::default();
    display.apply(&failure_result(Failure::NotFound));
    let shown = display.segment().expect("a segment");
    assert_eq!(shown.text, "status: command not found");
    assert_eq!(shown.tone, Tone::Warning);

    // An explicit empty list clears the segment, and the earlier text is gone for good.
    let mut display = ExternalDisplay::default();
    display.apply(&rows_result(vec![row("ok", Tone::Muted)]));
    display.apply(&rows_result(Vec::new()));
    assert!(display.segment().is_none());
    display.apply(&failure_result(Failure::NoOutput));
    assert_eq!(
        display.segment().expect("a segment").text,
        "status: no output"
    );

    // Only blank rows clear it too; the first non-blank row is the one shown.
    let mut display = ExternalDisplay::default();
    display.apply(&rows_result(vec![row("ok", Tone::Muted)]));
    display.apply(&rows_result(vec![
        row("", Tone::Muted),
        row("  ", Tone::Muted),
    ]));
    assert!(display.segment().is_none());
    display.apply(&rows_result(vec![
        row("", Tone::Muted),
        row("second", Tone::Success),
        row("third", Tone::Muted),
    ]));
    let shown = display.segment().expect("a segment");
    assert_eq!(shown.text, "second");
    assert_eq!(shown.tone, Tone::Success);

    // The stateless form: a failure gives its marker alone; the first non-blank row wins.
    let stateless = external_segment(&failure_result(Failure::TimedOut)).expect("a marker");
    assert_eq!(stateless.text, "status: command timed out");
    assert_eq!(stateless.tone, Tone::Warning);
    assert!(external_segment(&rows_result(vec![row("", Tone::Muted)])).is_none());
    let stateless = external_segment(&rows_result(vec![
        row("", Tone::Muted),
        row("shown", Tone::Error),
    ]))
    .expect("a row");
    assert_eq!(stateless.text, "shown");
    assert_eq!(stateless.tone, Tone::Error);
}
