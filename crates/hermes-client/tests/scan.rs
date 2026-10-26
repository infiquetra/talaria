//! The secret scan (spec section 7).
//!
//! Every sample is built at run time from pieces, so this source file holds no string that
//! looks like a real credential and neither GitHub's push protection nor the project's own
//! scan can flag it. No failure message prints a sample; they print rule ids and line numbers.

use hermes_client::{ScanHit, scan_for_secrets, scan_literals};

/// The eleven rule ids of spec 7, in table order.
const RULES: [&str; 11] = [
    "ticket-subprotocol",
    "bearer",
    "sk-key",
    "github-token",
    "aws-access-key",
    "slack-token",
    "jwt",
    "private-key",
    "url-userinfo",
    "url-secret-query",
    "unredacted-suspicious-key",
];

fn rep(ch: char, n: usize) -> String {
    ch.to_string().repeat(n)
}

/// One sample for every rule, in `RULES` order, with extra spellings where the spec has them.
fn samples() -> Vec<(&'static str, Vec<String>)> {
    let ticket = format!(
        "{}.{}",
        ["hermes", "gateway", "ticket"].join("-"),
        rep('t', 12)
    );
    let bearer = format!("Authorization: Bearer {}", rep('b', 24));
    let sk = format!("sk-{}", rep('a', 24));
    let gh_classic = ["ghp", "gho", "ghu", "ghs", "ghr"]
        .iter()
        .map(|p| format!("{p}_{}", rep('a', 36)))
        .collect::<Vec<_>>();
    let gh_fine = format!("{}_{}", ["github", "pat"].join("_"), rep('a', 30));
    let aws = format!("{}{}", ["AK", "IA"].concat(), rep('A', 16));
    let slack = ["xoxb", "xoxa", "xoxp", "xoxr", "xoxs"]
        .iter()
        .map(|p| format!("{p}-{}", "1234567890-abcdef"))
        .collect::<Vec<_>>();
    let jwt = format!("eyJ{}.{}.{}", rep('a', 12), rep('b', 12), rep('c', 12));
    let pem = ["RSA PRIVATE", "PRIVATE", "EC PRIVATE", "OPENSSH PRIVATE"]
        .iter()
        .map(|kind| format!("-----BEGIN {kind} KEY-----"))
        .collect::<Vec<_>>();
    let userinfo = format!("wss://{}:{}@host/api/ws", "someuser", "pw12345");
    let queries = [
        "token",
        "ticket",
        "internal",
        "api_key",
        "apikey",
        "access_token",
        "password",
    ]
    .iter()
    .map(|key| format!("ws://host/api/ws?mode=1&{key}=value12345"))
    .collect::<Vec<_>>();
    let canary = format!("canary-{}", rep('z', 10));
    let suspicious = vec![
        format!(r#"{{"params":{{"password":"{canary}"}}}}"#),
        format!(r#"{{"params":{{"a":[{{"b":{{"client_secret":"{canary}"}}}}]}}}}"#),
        format!(r#"{{"openai_api_key":"{canary}","max_tokens":5}}"#),
    ];

    let mut all_gh = gh_classic;
    all_gh.push(gh_fine);
    vec![
        ("ticket-subprotocol", vec![ticket]),
        (
            "bearer",
            vec![
                bearer,
                format!("bearer {}", rep('b', 20)),
                format!("BEARER {}", rep('B', 20)),
            ],
        ),
        ("sk-key", vec![sk]),
        ("github-token", all_gh),
        ("aws-access-key", vec![aws]),
        ("slack-token", slack),
        ("jwt", vec![jwt]),
        ("private-key", pem),
        ("url-userinfo", vec![userinfo]),
        ("url-secret-query", queries),
        ("unredacted-suspicious-key", suspicious),
    ]
}

fn hits(text: &str) -> Vec<(usize, &'static str)> {
    let mut found: Vec<_> = scan_for_secrets(text)
        .iter()
        .map(|h| (h.line, h.rule))
        .collect();
    found.sort();
    found
}

#[test]
fn every_rule_fires_on_a_runtime_built_sample() {
    let mut fired = Vec::new();
    for (rule, spellings) in samples() {
        for sample in spellings {
            // The sample alone on line 1.
            let found = hits(&sample);
            assert!(
                found.contains(&(1, rule)),
                "rule {rule} did not fire on its sample (hits: {found:?})"
            );
            // The same sample on line 3, among clean lines and without a trailing newline.
            let text = format!("{{\"kind\":\"header\",\"version\":3}}\nplain line\n{sample}");
            let found = hits(&text);
            assert!(
                found.contains(&(3, rule)),
                "rule {rule} did not fire on line 3 (hits: {found:?})"
            );
            fired.push(rule);
        }
    }
    // Every id of spec 7 has been exercised, and no sample produced an unknown id.
    for rule in RULES {
        assert!(fired.contains(&rule), "no sample was built for rule {rule}");
    }
    let known: Vec<&str> = RULES.to_vec();
    for (_, spellings) in samples() {
        for sample in spellings {
            for hit in scan_for_secrets(&sample) {
                assert!(
                    known.contains(&hit.rule),
                    "unknown rule id {} emitted",
                    hit.rule
                );
            }
        }
    }
}

#[test]
fn rule_length_boundaries() {
    // Just under each minimum length is clean; the minimum is a hit (spec 7 patterns).
    let cases: Vec<(&str, String, String)> = vec![
        (
            "sk-key",
            format!("sk-{}", rep('a', 19)),
            format!("sk-{}", rep('a', 20)),
        ),
        (
            "aws-access-key",
            format!("{}{}", ["AK", "IA"].concat(), rep('A', 15)),
            format!("{}{}", ["AK", "IA"].concat(), rep('A', 16)),
        ),
        (
            "bearer",
            format!("Bearer {}", rep('b', 15)),
            format!("Bearer {}", rep('b', 16)),
        ),
        (
            "slack-token",
            format!("xoxb-{}", rep('1', 9)),
            format!("xoxb-{}", rep('1', 10)),
        ),
        (
            "github-token",
            format!("ghp_{}", rep('a', 29)),
            format!("ghp_{}", rep('a', 30)),
        ),
        (
            "github-token",
            format!("{}_{}", ["github", "pat"].join("_"), rep('a', 19)),
            format!("{}_{}", ["github", "pat"].join("_"), rep('a', 20)),
        ),
    ];
    for (rule, short, long) in cases {
        assert!(
            !hits(&short).iter().any(|h| h.1 == rule),
            "{rule} fired below its minimum"
        );
        assert!(
            hits(&long).iter().any(|h| h.1 == rule),
            "{rule} did not fire at its minimum"
        );
    }

    // The key pattern needs a word boundary before it and (for AWS) after it.
    let inside_word = format!("task-{}", rep('a', 30));
    assert!(
        hits(&inside_word).is_empty(),
        "sk- inside a word must not be a hit"
    );
    let too_long = format!("{}{}", ["AK", "IA"].concat(), rep('A', 17));
    assert!(
        !hits(&too_long).iter().any(|h| h.1 == "aws-access-key"),
        "an access key id with a 17th character is not a key id"
    );
}

#[test]
fn url_secret_query_exemptions() {
    // Withheld values are not hits, in either spelling, wherever the pair sits in the query.
    let clean = [
        "ws://host/api/ws?token=%5Bredacted%5D",
        "ws://host/api/ws?token=[redacted]",
        "ws://host/api/ws?mode=1&ticket=%5Bredacted%5D&x=2",
        "ws://host/api/ws?api_key=%5Bredacted%5D&internal=%5Bredacted%5D",
        "wss://%5Bredacted%5D@host:9119/api/ws?access_token=[redacted]&password=%5Bredacted%5D",
        "https://host/page#%5Bredacted%5D",
        "ws://host/api/ws?mode=1&max_tokens=5",
    ];
    for text in clean {
        assert!(
            hits(text).is_empty(),
            "a withheld URL was reported: {:?}",
            hits(text)
        );
    }

    // One live value next to withheld ones is still a hit, on that line only.
    let mixed = format!(
        "ws://host/api/ws?token=%5Bredacted%5D\nws://host/api/ws?mode=1&password={}\n",
        "value12345"
    );
    assert_eq!(hits(&mixed), vec![(2, "url-secret-query")]);
}

#[test]
fn clean_text_has_no_hits() {
    assert!(scan_for_secrets("").is_empty());
    assert!(scan_for_secrets("\n\n").is_empty());
    assert!(scan_for_secrets("plain prose, nothing to see").is_empty());

    // A realistic recording: header, counters, withheld values, a cleaned URL.
    let text = [
        r#"{"kind":"header","version":3,"startedAt":"2026-10-23T09:14:02.331Z","endpoint":"ws://<gateway>/api/ws","subprotocols":["hermes-gateway-v1"]}"#,
        r#"{"kind":"frame","seq":1,"tMs":0,"generation":1,"dir":"in","frame":{"jsonrpc":"2.0","method":"event","params":{"type":"usage","payload":{"input_tokens":1200,"output_tokens":340,"max_tokens":4096}}}}"#,
        r#"{"kind":"frame","seq":2,"tMs":5,"generation":1,"dir":"out","frame":{"method":"model.save_key","params":{"api_key":"[redacted]","token":"","ticket":5}},"redactions":[{"path":"params.api_key","reason":"deny-set:model.save_key"}]}"#,
        r#"{"kind":"frame","seq":3,"tMs":9,"generation":1,"dir":"in","frame":{"endpoint":"wss://%5Bredacted%5D@host:9119/api/ws?token=%5Bredacted%5D&mode=1#%5Bredacted%5D"}}"#,
        r#"{"kind":"frame","seq":4,"tMs":12,"generation":1,"dir":"in","frame":{"text":"the bearer of news said sk-short and a skip-this name"}}"#,
        r#"{"kind":"dropped","seq":5,"tMs":13,"count":2}"#,
    ]
    .join("\n");
    assert_eq!(hits(&text), Vec::<(usize, &str)>::new());

    // A line that is not JSON is skipped by the JSON-walking rule, not an error.
    let mixed = "not json at all\n{\"password\":\"[redacted]\"}\n{broken";
    assert!(scan_for_secrets(mixed).is_empty());

    // No forbidden literal is present, so the literal scan is clean too.
    assert!(scan_literals(&text, &[format!("{}-{}", "absent", "literal")]).is_empty());
    assert!(scan_literals(&text, &[]).is_empty());
}

#[test]
fn hits_carry_line_and_rule_never_the_text() {
    let sk = format!("sk-{}", rep('a', 24));
    let bearer = format!("Bearer {}", rep('b', 24));
    let canary = format!("canary-{}", rep('z', 10));
    let text = format!(
        "clean first line\nkey {sk} on line two\nclean third line\n{{\"password\":\"{canary}\"}}\n{bearer}\n"
    );

    let found = scan_for_secrets(&text);
    let mut pairs: Vec<(usize, &str)> = found.iter().map(|h: &ScanHit| (h.line, h.rule)).collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![
            (2, "sk-key"),
            (4, "unredacted-suspicious-key"),
            (5, "bearer")
        ]
    );

    // Neither the Debug form of a hit nor of the whole list holds any matched text.
    let debug = format!("{found:?}");
    for secret in [&sk, &bearer, &canary, &rep('a', 24), &rep('b', 24)] {
        assert!(
            !debug.contains(secret.as_str()),
            "a hit's Debug output holds matched text"
        );
    }

    // Two rules on one line give a hit each, both on that line.
    let both = format!("{sk} and {bearer}");
    let mut rules: Vec<&str> = scan_for_secrets(&both)
        .iter()
        .filter(|h| h.line == 1)
        .map(|h| h.rule)
        .collect();
    rules.sort();
    rules.dedup();
    assert_eq!(rules, vec!["bearer", "sk-key"]);

    // Lines are numbered from 1, whatever the line ending.
    assert_eq!(hits(&sk), vec![(1, "sk-key")]);
    assert_eq!(hits(&format!("a\r\n{sk}\r\n")), vec![(2, "sk-key")]);
}

#[test]
fn forbidden_literals_found_by_line() {
    // Built at run time; none of these is a real name.
    let host = format!("{}-{}", "fixturehost", "zq9x");
    let login = format!("{}_{}", "fixturelogin", "k4m2");
    let forbidden = vec![host.clone(), login.clone()];
    let text = format!(
        "clean line one\nthe machine {host} appears here\nclean line three\n{{\"owner\":\"{login}\"}}\n{host} and {login}\nclean last line"
    );

    let found = scan_literals(&text, &forbidden);
    assert!(!found.is_empty());
    for hit in &found {
        assert_eq!(hit.rule, "forbidden-literal");
    }
    let mut lines: Vec<usize> = found.iter().map(|h| h.line).collect();
    lines.sort();
    lines.dedup();
    assert_eq!(lines, vec![2, 4, 5]);

    // One literal alone finds only its own lines.
    let only_login: Vec<usize> = {
        let mut v: Vec<usize> = scan_literals(&text, std::slice::from_ref(&login))
            .iter()
            .map(|h| h.line)
            .collect();
        v.sort();
        v.dedup();
        v
    };
    assert_eq!(only_login, vec![4, 5]);

    // Nothing to look for, nothing found.
    assert!(scan_literals(&text, &[]).is_empty());

    // The hits never carry the literal.
    let debug = format!("{found:?}");
    assert!(
        !debug.contains(&host) && !debug.contains(&login),
        "Debug output holds a literal"
    );
}
