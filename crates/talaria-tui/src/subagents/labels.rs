//! Short row labels for goals that share most of their words.
//!
//! A delegation often fans one instruction out over many items, so every goal
//! opens and closes with the same text and differs in one word in the middle.
//! A tail cut then hides the only word that tells the rows apart. This module
//! finds the shared opening and closing words once, so a view can show them as
//! a caption and keep only the differing part on each row.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The labels to show on rows and the shared text to show once above them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GoalLabels {
    /// The shared opening and closing words joined by an ellipsis.
    pub shared: Option<String>,
    /// One label per goal, in the same order.
    pub labels: Vec<String>,
}

/// Shared text shorter than this many columns is not worth a caption.
const MIN_SHARED_COLUMNS: usize = 12;

fn is_space(grapheme: &str) -> bool {
    grapheme.chars().all(char::is_whitespace)
}

/// Find the words every goal shares at its start and end.
///
/// Goals are split on grapheme clusters, so a wide character or an emoji built
/// from several code points is never cut in two, and the shared text always
/// ends on a word boundary. The goals come back unchanged when there is one
/// goal, when all goals are identical, when the shared text is short, or when
/// dropping it would leave a row with no label.
pub(crate) fn goal_labels(goals: &[&str]) -> GoalLabels {
    let unchanged = || GoalLabels {
        shared: None,
        labels: goals.iter().map(|g| (*g).to_owned()).collect(),
    };
    if goals.len() < 2 || goals.iter().all(|g| *g == goals[0]) {
        return unchanged();
    }
    let parts: Vec<Vec<&str>> = goals.iter().map(|g| g.graphemes(true).collect()).collect();
    let shortest = parts.iter().map(Vec::len).min().unwrap_or(0);
    let mut prefix = (0..shortest)
        .take_while(|&at| parts.iter().all(|p| p[at] == parts[0][at]))
        .count();
    while prefix > 0 && !is_space(parts[0][prefix - 1]) {
        prefix -= 1;
    }
    let rests: Vec<&[&str]> = parts.iter().map(|p| &p[prefix..]).collect();
    let shortest = rests.iter().map(|r| r.len()).min().unwrap_or(0);
    let mut suffix = (1..=shortest)
        .take_while(|&back| {
            rests
                .iter()
                .all(|r| r[r.len() - back] == rests[0][rests[0].len() - back])
        })
        .count();
    while suffix > 0 && !is_space(rests[0][rests[0].len() - suffix]) {
        suffix -= 1;
    }
    let head: String = parts[0][..prefix].concat();
    let tail: String = rests[0][rests[0].len() - suffix..].concat();
    let (head, tail) = (head.trim_end(), tail.trim_start());
    if head.width() + tail.width() < MIN_SHARED_COLUMNS {
        return unchanged();
    }
    let labels: Vec<String> = rests
        .iter()
        .map(|r| r[..r.len() - suffix].concat().trim().to_owned())
        .collect();
    if labels.iter().any(String::is_empty) {
        return unchanged();
    }
    let mut caption = Vec::new();
    if !head.is_empty() {
        caption.push(head);
    }
    caption.push("…");
    if !tail.is_empty() {
        caption.push(tail);
    }
    GoalLabels {
        shared: Some(caption.join(" ")),
        labels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(goals: &[&str]) -> GoalLabels {
        goal_labels(goals)
    }

    #[test]
    fn shared_words_are_dropped_at_word_boundaries() {
        let found = labels(&[
            "Check the `alpha` sample of the demo tool, then report back.",
            "Check the `beta` sample of the demo tool, then report back.",
            "Check the `gamma` sample of the demo tool, then report back.",
        ]);
        assert_eq!(found.labels, vec!["`alpha`", "`beta`", "`gamma`"]);
        assert_eq!(
            found.shared.as_deref(),
            Some("Check the … sample of the demo tool, then report back.")
        );
    }

    #[test]
    fn a_difference_inside_a_word_keeps_the_whole_word() {
        let found = labels(&[
            "Run the alphabet test case for module one",
            "Run the alphabet test case for module only",
        ]);
        assert_eq!(found.labels, vec!["one", "only"]);
        assert_eq!(
            found.shared.as_deref(),
            Some("Run the alphabet test case for module …")
        );
    }

    #[test]
    fn short_shared_text_is_kept() {
        let goals = ["list the files", "list the logs"];
        let found = labels(&goals);
        assert_eq!(found.shared, None);
        assert_eq!(found.labels, goals);
    }

    #[test]
    fn identical_goals_keep_full_text() {
        let goal = "Run and test the demonstration of the calc CLI, then report.";
        let found = labels(&[goal, goal, goal]);
        assert_eq!(found.shared, None);
        assert_eq!(found.labels, vec![goal; 3]);
    }

    #[test]
    fn single_goal_is_unchanged() {
        let goal = "Run and test the demonstration of the calc CLI, then report.";
        let found = labels(&[goal]);
        assert_eq!(found.shared, None);
        assert_eq!(found.labels, vec![goal]);
        assert_eq!(labels(&[]).labels, Vec::<String>::new());
    }

    #[test]
    fn an_empty_label_falls_back_to_full_goals() {
        let goals = ["aaa bbb ccc ddd eee", "aaa bbb ccc  eee"];
        let found = labels(&goals);
        assert_eq!(found.shared, None);
        assert_eq!(found.labels, goals);
    }

    #[test]
    fn wide_characters_are_split_on_graphemes() {
        let found = labels(&[
            "检查 👨‍👩‍👧 样本 的 全部 报告 并 汇报",
            "检查 👨‍👦 样本 的 全部 报告 并 汇报",
        ]);
        assert_eq!(found.labels, vec!["👨‍👩‍👧", "👨‍👦"]);
        let shared = found.shared.expect("shared text");
        assert_eq!(shared, "检查 … 样本 的 全部 报告 并 汇报");
        assert!(shared.width() > 12);
    }

    #[test]
    fn rows_that_differ_in_depth_text_still_fall_back_cleanly() {
        let goals = [
            "Run and test the demonstration of the calc CLI, then report.",
            "Read one file",
        ];
        let found = labels(&goals);
        assert_eq!(found.shared, None);
        assert_eq!(found.labels, goals);
    }
}
