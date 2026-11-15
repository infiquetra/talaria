//! Test 21. The hand-typed pin matches PINNED, and the live-skip messages are exact.

fn pinned_fields() -> (&'static str, &'static str) {
    let mut tag = "";
    let mut commit = "";
    for line in include_str!("../../PINNED").lines() {
        if let Some(value) = line.strip_prefix("tag=") {
            tag = value;
        } else if let Some(value) = line.strip_prefix("commit=") {
            commit = value;
        }
    }
    (tag, commit)
}

#[test]
fn pinned_parses_tag_and_commit() {
    let (tag, commit) = pinned_fields();
    assert!(!tag.is_empty());
    assert_eq!(commit.len(), 40);
    assert!(
        commit
            .chars()
            .all(|char| char.is_ascii_hexdigit() && !char.is_ascii_uppercase())
    );
    assert_eq!(hermes_contract::pinned::TAG, tag);
    assert_eq!(hermes_contract::pinned::COMMIT, commit);
}

#[test]
fn live_skip_reason_table() {
    let (tag, commit) = pinned_fields();
    assert_eq!(
        hermes_contract::pinned::live_skip_reason(Some(commit)),
        None,
        "a full commit matches"
    );
    let seven: String = commit
        .chars()
        .take(7)
        .collect::<String>()
        .to_ascii_uppercase();
    assert_eq!(seven.len(), 7);
    assert_eq!(
        hermes_contract::pinned::live_skip_reason(Some(&seven)),
        None,
        "seven matching characters match regardless of case"
    );

    let six: String = commit.chars().take(6).collect();
    let short =
        hermes_contract::pinned::live_skip_reason(Some(&six)).expect("six characters do not match");
    assert_eq!(
        short,
        format!(
            "gateway code_sha \"{six}\" does not match the pinned Hermes commit {commit} ({tag}); live tests are skipped"
        )
    );

    let other = "0123456789abcdef0123456789abcdef01234567";
    assert_ne!(other, commit);
    let mismatched = hermes_contract::pinned::live_skip_reason(Some(other))
        .expect("a different sha does not match");
    assert_eq!(
        mismatched,
        format!(
            "gateway code_sha \"{other}\" does not match the pinned Hermes commit {commit} ({tag}); live tests are skipped"
        )
    );

    let missing = hermes_contract::pinned::live_skip_reason(None).expect("absence is a skip");
    assert_eq!(
        missing,
        format!(
            "the gateway reported no code_sha; live tests need Hermes {tag} (commit {commit}) and are skipped without it"
        )
    );
}

#[test]
fn version_floor_skips_below_oldest() {
    let (tag, commit) = pinned_fields();
    assert_eq!(hermes_contract::pinned::OLDEST_SUPPORTED, (0, 23, 0));

    let missing_msg = "the gateway reported no code_version; live tests need Hermes 0.23.0 or later and are skipped without it";
    for input in [None, Some(""), Some("   ")] {
        let reason = hermes_contract::pinned::version_skip_reason(input, (0, 23, 0))
            .expect("missing or whitespace-only version must skip");
        assert_eq!(reason, missing_msg, "input {input:?}");
        assert!(!reason.contains(commit));
        assert!(!reason.contains(tag));
    }

    for raw in ["0.22.4", "unknown", "  0.22.4", "0.23"] {
        let reason = hermes_contract::pinned::version_skip_reason(Some(raw), (0, 23, 0))
            .expect("older or unparseable version must skip");
        assert_eq!(
            reason,
            format!(
                "gateway code_version \"{raw}\" is older than the oldest supported Hermes release 0.23.0; live tests are skipped"
            ),
            "raw {raw:?}"
        );
        assert!(!reason.contains(commit));
        assert!(!reason.contains(tag));
    }

    for ok in [
        "0.23.0",
        "0.23.1",
        "v0.23.0",
        "V0.23.0",
        "0.23.0+12.gabcdef",
    ] {
        assert_eq!(
            hermes_contract::pinned::version_skip_reason(Some(ok), (0, 23, 0)),
            None,
            "supported version {ok:?} must not skip"
        );
    }
}

#[test]
fn feature_skip_names_the_feature() {
    assert_eq!(
        hermes_contract::pinned::feature_skip_reason(Some("0.23.0"), "sudo prompt", "0.24.0"),
        Some("needs sudo prompt (Hermes 0.24.0 or later); this gateway runs 0.23.0".to_owned())
    );
    assert_eq!(
        hermes_contract::pinned::feature_skip_reason(Some("0.24.0"), "sudo prompt", "0.24.0"),
        None
    );
    assert_eq!(
        hermes_contract::pinned::feature_skip_reason(Some("0.24.1"), "sudo prompt", "0.24.0"),
        None
    );
    assert_eq!(
        hermes_contract::pinned::feature_skip_reason(None, "sudo prompt", "0.24.0"),
        Some("needs sudo prompt (Hermes 0.24.0 or later); this gateway runs unknown".to_owned())
    );
    assert_eq!(
        hermes_contract::pinned::feature_skip_reason(Some("   "), "sudo prompt", "0.24.0"),
        Some("needs sudo prompt (Hermes 0.24.0 or later); this gateway runs unknown".to_owned())
    );
    assert_eq!(
        hermes_contract::pinned::feature_skip_reason(Some("0.23.0"), "sudo prompt", "later"),
        Some("needs sudo prompt (Hermes later or later); this gateway runs 0.23.0".to_owned())
    );
}

#[test]
fn live_test_skip_uses_the_floor_unless_exact() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    assert_eq!(
        hermes_contract::pinned::live_test_skip(Some("0.23.1"), Some(sha), false),
        None
    );
    assert_eq!(
        hermes_contract::pinned::live_test_skip(Some("0.23.1"), Some(sha), true),
        hermes_contract::pinned::live_skip_reason(Some(sha))
    );
    assert!(!hermes_contract::pinned::live_exact_from(None));
    assert!(hermes_contract::pinned::live_exact_from(Some("1")));
    assert!(!hermes_contract::pinned::live_exact_from(Some("0")));
}

#[test]
fn gateway_version_line_names_both_fields() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    assert_eq!(
        hermes_contract::pinned::gateway_version_line(Some("0.23.1"), Some(sha)),
        format!("gateway code_version=0.23.1 code_sha={sha}")
    );
    assert_eq!(
        hermes_contract::pinned::gateway_version_line(None, Some("")),
        "gateway code_version=unknown code_sha=unknown"
    );
    assert_eq!(
        hermes_contract::pinned::gateway_version_line(Some("   "), Some("  ab ")),
        "gateway code_version=unknown code_sha=ab"
    );
    assert_eq!(
        hermes_contract::pinned::gateway_version_line(None, Some(sha)),
        format!("gateway code_version=unknown code_sha={sha}")
    );
}

#[test]
fn live_profile_defaults_to_fixture() {
    assert_eq!(hermes_contract::pinned::live_profile_from(None), "fixture");
    assert_eq!(
        hermes_contract::pinned::live_profile_from(Some("   ")),
        "fixture"
    );
    assert_eq!(
        hermes_contract::pinned::live_profile_from(Some("  other  ")),
        "other"
    );
}

#[test]
fn current_matches_pin_and_floor() {
    let current = hermes_contract::compat::CURRENT;
    assert_eq!(current.pinned_tag, hermes_contract::pinned::TAG);
    assert_eq!(current.pinned_commit, hermes_contract::pinned::COMMIT);
    let floor_str = current
        .floor
        .strip_prefix('v')
        .expect("floor starts with v");
    let parts: Vec<u64> = floor_str
        .split('.')
        .map(|part| part.parse::<u64>().expect("numeric version component"))
        .collect();
    assert_eq!(
        parts.len(),
        3,
        "three-part floor version: {}",
        current.floor
    );
    assert_eq!(
        (parts[0], parts[1], parts[2]),
        hermes_contract::pinned::OLDEST_SUPPORTED
    );
}
