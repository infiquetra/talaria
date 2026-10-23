//! Test 21. The hand-typed pin matches PINNED, and the live-skip messages are exact.

fn pinned_fields() -> (&'static str, &'static str) {
    let mut tag = "";
    let mut commit = "";
    for line in include_str!("../PINNED").lines() {
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
