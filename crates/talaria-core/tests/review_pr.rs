//! Pull-request query, parse and label red tests. Issue #245.

use std::path::PathBuf;

use talaria_core::review::{
    Checks, LabelTone, PrState, PrStatus, ReviewDecision, parse_pr_reply, pr_chunks, pr_label,
    pr_query,
};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/gh")
}

/// 120 branches give 50, 50 and 20; 350 keep the first 300; duplicates go
/// first; the query JSON-escapes branch names.
#[test]
fn query_escapes_and_chunks() {
    let branches: Vec<String> = (0..120)
        .map(|index| format!("talaria/work-{index}"))
        .collect();
    let chunks = pr_chunks(&branches);
    assert_eq!(chunks.len(), 3, "120 branches give 3 chunks");
    assert_eq!(
        chunks.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![50, 50, 20]
    );
    assert_eq!(chunks[0][0], "talaria/work-0");
    assert_eq!(chunks[2][19], "talaria/work-119");

    let many: Vec<String> = (0..350)
        .map(|index| format!("talaria/work-{index}"))
        .collect();
    let chunks = pr_chunks(&many);
    assert_eq!(chunks.len(), 6, "350 branches keep 300 in 6 chunks");
    assert!(chunks.iter().all(|chunk| chunk.len() == 50));
    assert_eq!(chunks[5][49], "talaria/work-299");

    let dupes = vec![
        "talaria/a".to_owned(),
        "talaria/b".to_owned(),
        "talaria/a".to_owned(),
    ];
    assert_eq!(
        pr_chunks(&dupes),
        vec![vec!["talaria/a".to_owned(), "talaria/b".to_owned()]],
        "duplicates are dropped first"
    );

    let query = pr_query(
        "fixture-owner",
        "fixture-repo",
        &["plain".to_owned(), "we\"ird\\name".to_owned()],
    );
    assert!(
        query.contains("repository(owner: \"fixture-owner\", name: \"fixture-repo\")"),
        "the query names the repository: {query}"
    );
    assert!(
        query.contains("b0: pullRequests"),
        "aliases start at b0: {query}"
    );
    assert!(
        query.contains("b1: pullRequests"),
        "aliases count up: {query}"
    );
    assert!(
        query.contains("headRefName: \"we\\\"ird\\\\name\""),
        "quotes and backslashes are JSON-escaped: {query}"
    );
    assert!(
        query.contains("first: 5, orderBy: {field: CREATED_AT, direction: DESC}"),
        "newest first: {query}"
    );
}

/// A fork's pull request never labels the branch; the first own node wins;
/// check states map. Every expectation is read from the recording itself, so
/// the test pins the mapping rules whatever the throwaway repository held.
#[test]
fn parse_recorded_reply_skips_forks_and_maps_checks() {
    let path = fixtures_dir().join("api-graphql-status.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        panic!("BLOCKED: fixtures/gh/api-graphql-status.json not recorded");
    };
    let reply: serde_json::Value = serde_json::from_str(&text).expect("reply json");
    let stdout = reply["stdout"].as_str().expect("stdout");
    let body: serde_json::Value = serde_json::from_str(stdout).expect("reply stdout json");
    let repository = &body["data"]["repository"];

    // The branches the recording queried, in alias order: b0, b1, ... An alias
    // with no nodes keeps a placeholder, so positions still line up.
    let mut branches: Vec<String> = Vec::new();
    let mut with_nodes = 0;
    let mut index = 0;
    while repository.get(format!("b{index}")).is_some() {
        let nodes = &repository[format!("b{index}")]["nodes"];
        let branch = nodes
            .as_array()
            .and_then(|nodes| nodes.first())
            .and_then(|node| node["headRefName"].as_str())
            .unwrap_or_default();
        if !branch.is_empty() {
            with_nodes += 1;
        }
        branches.push(branch.to_owned());
        index += 1;
    }
    assert!(!branches.is_empty(), "the recording holds aliases");

    let statuses = parse_pr_reply(stdout, &branches).expect("the reply parses");
    assert_eq!(
        statuses.len(),
        with_nodes,
        "one status per alias with nodes: {statuses:?}"
    );

    for (alias, branch) in branches.iter().enumerate() {
        let nodes = repository[format!("b{alias}")]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if nodes.is_empty() {
            continue;
        }
        let status = statuses
            .iter()
            .find(|status| &status.branch == branch)
            .expect("a status for the queried branch");
        // The first node that is not from a fork wins, whatever its state.
        let winner = nodes
            .iter()
            .find(|node| node["isCrossRepository"] != true)
            .expect("an own node");
        assert_eq!(
            status.number,
            winner["number"].as_u64().expect("number"),
            "the first own node wins for {branch}"
        );
        for node in &nodes {
            if node["isCrossRepository"] == true {
                assert_ne!(
                    status.number,
                    node["number"].as_u64().unwrap_or(0),
                    "a fork's pull request never labels the branch"
                );
            }
        }
        // States and checks map one to one.
        let state = match winner["state"].as_str().expect("state") {
            "OPEN" => PrState::Open,
            "CLOSED" => PrState::Closed,
            "MERGED" => PrState::Merged,
            other => panic!("a known state: {other}"),
        };
        assert_eq!(status.state, state, "the state maps for {branch}");
        assert_eq!(
            status.draft,
            winner["isDraft"].as_bool().unwrap_or(false),
            "the draft flag maps for {branch}"
        );
        let rollup = winner["commits"]["nodes"]
            .as_array()
            .and_then(|nodes| nodes.first())
            .and_then(|node| node["commit"]["statusCheckRollup"]["state"].as_str());
        let checks = match rollup {
            Some("SUCCESS") => Checks::Passing,
            Some("FAILURE" | "ERROR") => Checks::Failing,
            Some("PENDING" | "EXPECTED") => Checks::Pending,
            _ => Checks::None,
        };
        assert_eq!(status.checks, checks, "the check state maps for {branch}");
        let review = match winner["reviewDecision"].as_str() {
            Some("APPROVED") => Some(ReviewDecision::Approved),
            Some("CHANGES_REQUESTED") => Some(ReviewDecision::ChangesRequested),
            Some("REVIEW_REQUIRED") => Some(ReviewDecision::ReviewRequired),
            _ => None,
        };
        assert_eq!(
            status.review, review,
            "the review decision maps for {branch}"
        );
    }
}

/// The label table: text and tone per status, muted when stale.
#[test]
fn label_text_and_tone_table() {
    let status = |state: PrState, draft: bool, checks: Checks| PrStatus {
        number: 12,
        state,
        draft,
        checks,
        review: None,
        url: "https://github.com/fixture-owner/fixture-repo/pull/12".to_owned(),
        branch: "talaria/work".to_owned(),
    };
    let table: Vec<(PrStatus, &str, LabelTone)> = vec![
        (
            status(PrState::Open, true, Checks::Passing),
            "#12 draft ✓",
            LabelTone::Success,
        ),
        (
            status(PrState::Open, true, Checks::Failing),
            "#12 draft ✗",
            LabelTone::Error,
        ),
        (
            status(PrState::Open, true, Checks::Pending),
            "#12 draft ◐",
            LabelTone::Warning,
        ),
        (
            status(PrState::Open, true, Checks::None),
            "#12 draft",
            LabelTone::Muted,
        ),
        (
            status(PrState::Open, false, Checks::Passing),
            "#12 open ✓",
            LabelTone::Success,
        ),
        (
            status(PrState::Open, false, Checks::None),
            "#12 open",
            LabelTone::Muted,
        ),
        (
            status(PrState::Merged, false, Checks::None),
            "#12 merged",
            LabelTone::Accent,
        ),
        (
            status(PrState::Closed, false, Checks::Failing),
            "#12 closed",
            LabelTone::Muted,
        ),
    ];
    for (status, text, tone) in &table {
        let label = pr_label(status, false);
        assert_eq!(label.text, *text, "{status:?}");
        assert_eq!(label.tone, *tone, "{status:?}");
    }

    // Stale labels keep the text in muted.
    for (status, text, _) in &table {
        let label = pr_label(status, true);
        assert_eq!(label.text, *text, "stale keeps the text: {status:?}");
        assert_eq!(label.tone, LabelTone::Muted, "stale is muted: {status:?}");
    }
}
