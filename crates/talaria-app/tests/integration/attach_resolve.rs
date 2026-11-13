//! Pure resolution: references, shapes, tiers, candidates, the fast path. Issue #246.

use proptest::prelude::*;
use serde_json::json;
use talaria_app::attach::{
    MatchKind, ReferenceError, ReferenceShape, Resolution, candidate_lines, fast_path,
    reference_shape, resolve, validate_reference,
};
use talaria_core::directory::rows::{DirectoryRow, RowKey, RowOrigin};

const NOW: f64 = 1_792_700_000.0;

fn dir_row(profile: &str, session_id: &str, title: &str, origin: RowOrigin) -> DirectoryRow {
    DirectoryRow {
        key: RowKey {
            profile: profile.to_owned(),
            session_id: session_id.to_owned(),
        },
        title: title.to_owned(),
        source: Some("tui".to_owned()),
        started_at: Some(NOW - 90.0),
        message_count: 3,
        running: false,
        model: None,
        origin,
    }
}

fn full_id(hex8: &str) -> String {
    format!("local-{hex8}{}", "0".repeat(56))
}

#[test]
fn validate_reference_rules() {
    assert_eq!(
        validate_reference("  attach probe  ").as_deref(),
        Ok("attach probe")
    );
    assert_eq!(validate_reference(""), Err(ReferenceError::Empty));
    assert_eq!(validate_reference("   "), Err(ReferenceError::Empty));
    assert_eq!(
        validate_reference(&"a".repeat(200)).as_deref(),
        Ok("a".repeat(200).as_str())
    );
    assert_eq!(
        validate_reference(&"a".repeat(201)),
        Err(ReferenceError::TooLong { chars: 201 })
    );
    assert_eq!(
        validate_reference("ab\u{7}cd"),
        Err(ReferenceError::ControlCharacter)
    );
    assert_eq!(
        validate_reference("ab\u{9b}cd"),
        Err(ReferenceError::ControlCharacter)
    );
    assert_eq!(
        validate_reference("tab\there"),
        Err(ReferenceError::ControlCharacter)
    );
}

#[test]
fn reference_shapes() {
    assert_eq!(
        reference_shape(&full_id("1a2b3c4d")),
        ReferenceShape::FullLocal
    );
    assert_eq!(
        reference_shape("20250101_120000_abc9"),
        ReferenceShape::Stored
    );
    assert_eq!(
        reference_shape("local-1a2b3c4d5e6f7890"),
        ReferenceShape::LocalShort
    );
    assert_eq!(
        reference_shape("local-1a2b3c4d5e6f789"),
        ReferenceShape::Other
    );
    assert_eq!(
        reference_shape("local-1a2b3c4d5e6f78900"),
        ReferenceShape::Other
    );
    assert_eq!(
        reference_shape("20250101_120000_ABC9"),
        ReferenceShape::Other
    );
    assert_eq!(reference_shape("1a2b3c4d"), ReferenceShape::Other);
    assert_eq!(reference_shape("attach probe"), ReferenceShape::Other);
    assert_eq!(
        reference_shape(&full_id("1a2b3c4d").to_uppercase()),
        ReferenceShape::Other
    );
}

#[test]
fn exact_id_beats_prefix_and_title() {
    let wanted = full_id("1a2b3c4d");
    let rows = vec![
        dir_row("fixture", &wanted, "compile", RowOrigin::Live),
        dir_row("fixture", &full_id("1a2b3c4e"), "1a2b3c4d", RowOrigin::Live),
        dir_row("default", &full_id("9a2b3c4d"), &wanted, RowOrigin::Live),
    ];
    let resolved = resolve(&wanted, &rows);
    assert!(
        matches!(resolved, Resolution::One(ref candidate) if candidate.kind == MatchKind::ExactId && candidate.row.key.session_id == wanted),
        "exact id lost: {resolved:?}"
    );

    // The contract's lineage `id` is never matched, only `session_id`.
    let row = DirectoryRow::from_list_row(
        "fixture",
        RowOrigin::Stored,
        &json!({
            "session_id": wanted,
            "id": "root-lineage-1",
            "title": "stored work",
            "source": "tui",
            "started_at": NOW - 10.0,
            "message_count": 4,
            "running": false,
        }),
    )
    .expect("a list row");
    assert_eq!(
        resolve("root-lineage-1", std::slice::from_ref(&row)),
        Resolution::NoMatch
    );
    assert!(
        matches!(resolve(&wanted, &[row]), Resolution::One(ref candidate) if candidate.kind == MatchKind::ExactId),
        "stored session_id did not match exactly"
    );
}

#[test]
fn prefix_matches_with_or_without_local_and_needs_four_chars() {
    let wanted = full_id("1a2b3c4d");
    let rows = vec![
        dir_row("fixture", &wanted, "compile", RowOrigin::Live),
        dir_row("fixture", &full_id("9e8d7c6b"), "migrate", RowOrigin::Live),
    ];
    for reference in [
        "1a2b3c4d",
        "local-1a2b3c4d",
        "1A2B3C4D",
        concat!("LOCAL-", "1a2b3c4d"),
    ] {
        assert!(
            matches!(resolve(reference, &rows), Resolution::One(ref candidate) if candidate.kind == MatchKind::IdPrefix && candidate.row.key.session_id == wanted),
            "{reference} did not prefix-match"
        );
    }
    for reference in ["1a2", "loca", "local", "local-", "local-1"] {
        assert_eq!(
            resolve(reference, &rows),
            Resolution::NoMatch,
            "{reference} must never use the prefix tier"
        );
    }

    let stored = vec![
        dir_row(
            "fixture",
            "20250101_120000_aaa",
            "january",
            RowOrigin::Stored,
        ),
        dir_row(
            "fixture",
            "20250202_120000_bbb",
            "february",
            RowOrigin::Stored,
        ),
        dir_row("default", "20250303_120000_ccc", "march", RowOrigin::Stored),
    ];
    match resolve("2025", &stored) {
        Resolution::Many(found) => assert_eq!(found.len(), 3),
        other => panic!("a shared year prefix must stay ambiguous, got {other:?}"),
    }
}

#[test]
fn title_exact_then_ignoring_case() {
    let rows = vec![
        dir_row(
            "fixture",
            &full_id("11111111"),
            "Build api",
            RowOrigin::Live,
        ),
        dir_row("fixture", &full_id("22222222"), "other", RowOrigin::Live),
    ];
    assert!(
        matches!(resolve("Build api", &rows), Resolution::One(ref candidate) if candidate.kind == MatchKind::Title),
        "exact title lost"
    );
    assert!(
        matches!(resolve("BUILD API", &rows), Resolution::One(ref candidate) if candidate.kind == MatchKind::TitleIgnoringCase),
        "case-folded title lost"
    );

    let untitled = vec![
        dir_row("fixture", &full_id("33333333"), "", RowOrigin::Live),
        dir_row("fixture", &full_id("44444444"), "", RowOrigin::Stored),
    ];
    assert_eq!(resolve("", &untitled), Resolution::NoMatch);
    assert_eq!(resolve("x", &untitled), Resolution::NoMatch);
}

#[test]
fn same_match_in_two_profiles_is_ambiguous() {
    let wanted = full_id("1a2b3c4d");
    let rows = vec![
        dir_row("default", &wanted, "compile", RowOrigin::Live),
        dir_row("fixture", &wanted, "compile", RowOrigin::Live),
    ];
    match resolve(&wanted, &rows) {
        Resolution::Many(found) => {
            assert_eq!(found.len(), 2);
            let profiles: Vec<&str> = found
                .iter()
                .map(|candidate| candidate.row.key.profile.as_str())
                .collect();
            assert!(profiles.contains(&"default") && profiles.contains(&"fixture"));
        }
        other => panic!("two profiles must stay ambiguous, got {other:?}"),
    }
}

/// The test's own tier check, so the uniqueness claim does not call `resolve`.
fn tier_matches(kind: MatchKind, reference: &str, row: &DirectoryRow) -> bool {
    let lower = reference.to_lowercase();
    let sid = row.key.session_id.as_str();
    match kind {
        MatchKind::ExactId => sid == reference,
        MatchKind::IdPrefix => {
            let chars_ok = lower
                .chars()
                .all(|c| matches!(c, '0'..='9' | 'a'..='z' | '_' | '-'));
            let not_local_prefix = !"local-".starts_with(&lower);
            let tail = lower.strip_prefix("local-").unwrap_or(&lower);
            if !(chars_ok && not_local_prefix && tail.len() >= 4) {
                return false;
            }
            let lsid = sid.to_lowercase();
            lsid.starts_with(&lower)
                || (lsid.starts_with("local-") && lsid[6..].starts_with(&lower))
        }
        MatchKind::Title => !row.title.is_empty() && row.title == reference,
        MatchKind::TitleIgnoringCase => !row.title.is_empty() && row.title.to_lowercase() == lower,
    }
}

fn shuffled(mut rows: Vec<DirectoryRow>, seed: usize) -> Vec<DirectoryRow> {
    rows.reverse();
    let len = rows.len().max(1);
    rows.rotate_left(seed % len);
    rows
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn prop_resolution_ignores_row_order(
        tier in 0..4usize,
        suffix in "[a-d]{4,8}",
        tail in "[a-d]{0,8}",
        word in "[a-d]{3,6}",
        extra in prop::collection::vec("[a-d]{8}", 0..6),
    ) {
        let planted_id = format!("local-{suffix}{tail}");
        let planted_title = word.clone();
        let (reference, planted) = match tier {
            0 => (planted_id.clone(), dir_row("fixture", &planted_id, "title", RowOrigin::Live)),
            1 => {
                let prefix = suffix.clone();
                let row = dir_row("fixture", &planted_id, "title", RowOrigin::Live);
                (prefix, row)
            }
            2 => {
                let row = dir_row("fixture", &planted_id, &planted_title, RowOrigin::Live);
                (planted_title.clone(), row)
            }
            _ => {
                let row = dir_row("fixture", &planted_id, &planted_title.to_uppercase(), RowOrigin::Live);
                (planted_title.clone(), row)
            }
        };
        let mut rows = vec![planted.clone()];
        for (index, id_tail) in extra.iter().enumerate() {
            let sid = format!("local-{id_tail}");
            let title = format!("row-{index}");
            let row = dir_row("fixture", &sid, &title, RowOrigin::Live);
            // Higher tiers must not steal the planted tier: exact ids beat
            // prefixes, and ids beat titles.
            let stolen = match tier {
                0 => false,
                1 => tier_matches(MatchKind::ExactId, &reference, &row),
                2 | 3 => {
                    tier_matches(MatchKind::ExactId, &reference, &row)
                        || tier_matches(MatchKind::IdPrefix, &reference, &row)
                }
                _ => false,
            } || (tier == 3 && tier_matches(MatchKind::Title, &reference, &row));
            if !stolen {
                rows.push(row);
            }
        }
        // At least the planted row is in; lower tiers may still collide.
        let first = resolve(&reference, &shuffled(rows.clone(), 1));
        let second = resolve(&reference, &shuffled(rows.clone(), 3));
        prop_assert_eq!(&first, &second, "order changed the answer");
        prop_assert!(
            !matches!(first, Resolution::NoMatch),
            "the planted row never matches"
        );
        if let Resolution::One(found) = &first {
            let others = rows
                .iter()
                .filter(|row| row.key != found.row.key)
                .filter(|row| tier_matches(found.kind, &reference, row))
                .count();
            prop_assert_eq!(
                others,
                0,
                "One with a second match in its tier: {:?}",
                first
            );
        }
    }
}

#[test]
fn candidate_lines_layout() {
    use talaria_app::attach::Candidate;
    let mut rows = vec![
        dir_row(
            "fixture",
            "local-1a2b3c4d5e6f7890",
            "attach probe",
            RowOrigin::Live,
        ),
        dir_row("default", "local-abcdef1234567890", "", RowOrigin::Live),
        dir_row(
            "fixture",
            "20250101_120000_abc",
            &"0123456789".repeat(5),
            RowOrigin::Stored,
        ),
    ];
    rows[0].running = true;
    rows[0].started_at = Some(NOW - 90.0);
    rows[1].started_at = Some(NOW - 3_700.0);
    rows[2].started_at = None;
    for index in 3..12 {
        let mut row = dir_row(
            "fixture",
            &format!("local-{index:08x}"),
            &format!("extra {index}"),
            RowOrigin::Live,
        );
        row.started_at = Some(NOW - f64::from(index) - 5.0);
        rows.push(row);
    }
    let candidates: Vec<Candidate> = rows
        .into_iter()
        .map(|row| Candidate {
            row,
            kind: MatchKind::Title,
        })
        .collect();
    let text = candidate_lines(&candidates, NOW);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 11, "ten rows and the overflow line: {text:?}");
    assert_eq!(
        lines[0],
        "  fixture  local-1a2b3c4d…  \"attach probe\"  live  running  1m"
    );
    assert_eq!(lines[1], "  default  local-abcdef12…  (untitled)  live  1h");
    assert_eq!(
        lines[2],
        "  fixture  20250101_12000…  \"0123456789012345678901234567890123456789…\"  earlier  —"
    );
    assert_eq!(lines[10], "  …and 2 more");
}

#[test]
fn fast_path_needs_profile_and_full_id() {
    use talaria_app::attach::AttachRequest;
    let full = full_id("1a2b3c4d");
    let stored = "20250101_120000_abc9".to_owned();
    let with_profile = |reference: String| AttachRequest {
        reference,
        profile: Some("fixture".to_owned()),
        no_ensure: false,
    };
    assert_eq!(
        fast_path(&with_profile(full.clone())),
        Some(RowKey {
            profile: "fixture".to_owned(),
            session_id: full,
        })
    );
    assert_eq!(
        fast_path(&with_profile(stored.clone())),
        Some(RowKey {
            profile: "fixture".to_owned(),
            session_id: stored,
        })
    );
    assert_eq!(
        fast_path(&AttachRequest {
            reference: full_id("1a2b3c4d"),
            profile: None,
            no_ensure: false,
        }),
        None
    );
    assert_eq!(
        fast_path(&with_profile("local-1a2b3c4d5e6f7890".to_owned())),
        None
    );
    assert_eq!(
        fast_path(&with_profile("20250101_120000_ABC9".to_owned())),
        None
    );
    assert_eq!(fast_path(&with_profile("1a2b3c4d".to_owned())), None);
}
