//! Checkpoint matching. Issue 233. Checkpoint rewind and diff.

use talaria_core::rewind::{
    CheckpointList, NoMatch, RewindTurn, checkpoint_for_turn, clock_text, parse_checkpoint_time,
};

fn turn(start: Option<i64>, end: Option<i64>, edits: bool) -> RewindTurn {
    RewindTurn {
        row_id: 5,
        ordinal: 1,
        preview: "prompt".to_string(),
        prompt_text: Some("prompt".to_string()),
        text_parts: "prompt".to_string(),
        attachments: false,
        timestamp: start.map(|seconds| seconds as f64 + 0.7),
        window_start: start,
        window_end: end,
        edits_files: edits,
        later_count: 1,
    }
}

fn rows(times: &[&str]) -> CheckpointList {
    let entries = times
        .iter()
        .map(|time| {
            (
                format!("abcdef0123456789{time}"),
                (*time).to_string(),
                "before patch".to_string(),
            )
        })
        .collect();
    CheckpointList::from_result(true, entries)
}

#[test]
fn list_result_off_empty_and_numbered() {
    assert!(matches!(
        CheckpointList::from_result(false, vec![]),
        CheckpointList::Off
    ));
    assert!(matches!(
        CheckpointList::from_result(true, vec![]),
        CheckpointList::Empty
    ));
    let list = CheckpointList::from_result(
        true,
        vec![
            (
                "abcdef0123456789".into(),
                "2026-10-22T14:02:11+01:00".into(),
                "before patch".into(),
            ),
            (
                "1234567890abcdef".into(),
                "2026-10-22T14:00:00+01:00".into(),
                "before write_file".into(),
            ),
            (
                "short1".into(),
                "2026-10-22T13:00:00Z".into(),
                "before terminal: echo".into(),
            ),
        ],
    );
    let CheckpointList::Rows(rows) = list else {
        panic!("three rows");
    };
    assert_eq!(
        rows.iter().map(|row| row.index).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(rows[0].short, "abcdef01");
    assert_eq!(rows[2].short, "short1");
}

#[test]
fn parses_git_iso_times() {
    assert_eq!(
        parse_checkpoint_time("2026-10-22T14:02:11+01:00"),
        Some(1_792_674_131)
    );
    assert_eq!(
        parse_checkpoint_time("2026-10-22T13:02:11Z"),
        Some(1_792_674_131)
    );
    let east = parse_checkpoint_time("2026-10-22T14:02:11+01:00").unwrap_or(0);
    let west = parse_checkpoint_time("2026-10-22T07:32:11-05:30").unwrap_or(0);
    assert_eq!(east, west);
    assert_eq!(parse_checkpoint_time("2026-10-22T14:02+01:00"), None);
    assert_eq!(parse_checkpoint_time("2026-10-22t14:02:11Z"), None);
    assert_eq!(parse_checkpoint_time("2026-10-22T14:02:11Z extra"), None);
    assert_eq!(clock_text("2026-10-22T14:02:11+01:00"), "14:02");
    assert_eq!(clock_text("not a time"), "");
}

#[test]
fn checkpoint_for_turn_picks_oldest_at_or_after_prompt() {
    let prompt = turn(Some(1000), None, false);
    let list = CheckpointList::Rows(vec![
        row(1, "11111111aaaaaaaa", 1005),
        row(2, "22222222bbbbbbbb", 1000),
        row(3, "33333333cccccccc", 990),
    ]);
    let matched = checkpoint_for_turn(&prompt, &list).expect("a checkpoint at the prompt");
    assert_eq!(matched.index, 2);
    assert!(!matched.oldest_kept);
    let without_older = CheckpointList::Rows(vec![
        row(1, "11111111aaaaaaaa", 1005),
        row(2, "22222222bbbbbbbb", 1000),
    ]);
    let matched = checkpoint_for_turn(&prompt, &without_older).expect("still a match");
    assert!(matched.oldest_kept);
    let only_before = CheckpointList::Rows(vec![row(1, "33333333cccccccc", 990)]);
    assert_eq!(
        checkpoint_for_turn(&prompt, &only_before),
        Err(NoMatch::NoChanges)
    );
    let tied = CheckpointList::Rows(vec![
        row(1, "11111111aaaaaaaa", 1000),
        row(2, "22222222bbbbbbbb", 1000),
    ]);
    assert_eq!(checkpoint_for_turn(&prompt, &tied).expect("tie").index, 2);
}

fn row(index: usize, hash: &str, time: i64) -> talaria_core::rewind::Checkpoint {
    talaria_core::rewind::Checkpoint {
        index,
        hash: hash.to_string(),
        short: hash.chars().take(8).collect(),
        message: "before patch".to_string(),
        raw_time: time.to_string(),
        time: Some(time),
    }
}

#[test]
fn no_mapping_without_both_times() {
    let prompt = turn(None, None, false);
    let list = CheckpointList::Rows(vec![row(1, "11111111aaaaaaaa", 1000)]);
    assert_eq!(checkpoint_for_turn(&prompt, &list), Err(NoMatch::NoTime));
    let unparsed = talaria_core::rewind::Checkpoint {
        time: None,
        raw_time: "yesterday".to_string(),
        ..row(1, "11111111aaaaaaaa", 1000)
    };
    let skipped = CheckpointList::Rows(vec![unparsed, row(2, "22222222bbbbbbbb", 1000)]);
    let prompt = turn(Some(1000), None, false);
    assert_eq!(
        checkpoint_for_turn(&prompt, &skipped)
            .expect("parsed row")
            .index,
        2
    );
    assert_eq!(
        checkpoint_for_turn(&prompt, &CheckpointList::Unavailable("down".into())),
        Err(NoMatch::Unknown)
    );
}

#[test]
fn no_match_when_the_turn_edited_files_without_a_checkpoint() {
    let prompt = turn(Some(1000), Some(1010), true);
    let after = CheckpointList::Rows(vec![
        row(1, "11111111aaaaaaaa", 1020),
        row(2, "22222222bbbbbbbb", 1012),
    ]);
    assert_eq!(
        checkpoint_for_turn(&prompt, &after),
        Err(NoMatch::NoCheckpointForEdits)
    );
    let inside = CheckpointList::Rows(vec![
        row(1, "22222222bbbbbbbb", 1012),
        row(2, "33333333cccccccc", 1004),
    ]);
    assert_eq!(
        checkpoint_for_turn(&prompt, &inside).expect("inside").index,
        2
    );
    let no_edit = turn(Some(1000), Some(1010), false);
    assert_eq!(
        checkpoint_for_turn(&no_edit, &after)
            .expect("later is fine")
            .index,
        2
    );
    let newest = turn(Some(1000), None, true);
    let later = CheckpointList::Rows(vec![row(1, "22222222bbbbbbbb", 1012)]);
    assert_eq!(
        checkpoint_for_turn(&newest, &later)
            .expect("no upper bound")
            .index,
        1
    );
    let _ = rows(&[]);
}
