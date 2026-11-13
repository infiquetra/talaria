//! Diff parser. Issue 221. Expected rows are written out by hand.

use talaria_core::diff::{DiffModel, RowKind, parse_diff};

/// source: contract schema, a patch result's `diff` field
const PATCH: &str = "\
--- a/calc.py
+++ b/calc.py
@@ -9,3 +9,3 @@ def average(values):
     total = sum(values)
-    return total / (len(values) + 1)
+    return total / len(values)
\x20
";

/// source: contract schema, Hermes inline diff with the colours already removed
const INLINE: &str = "\
  ┊ review diff
a/notes.txt → b/notes.txt
@@ -1,2 +1,2 @@
 alpha
-beta
+gamma
";

fn file_rows(model: &DiffModel) -> Vec<(RowKind, Option<u32>, Option<u32>, &str)> {
    model
        .files
        .iter()
        .flat_map(|file| file.hunks.iter().flat_map(|hunk| hunk.rows.iter()))
        .map(|row| (row.kind, row.old_no, row.new_no, row.text.as_str()))
        .collect()
}

#[test]
fn parses_unified_diff_with_line_numbers() {
    let model = parse_diff(PATCH);
    assert_eq!(model.display_path(), Some("calc.py"));
    assert_eq!(model.files.len(), 1);
    assert_eq!(
        file_rows(&model),
        vec![
            (
                RowKind::Context,
                Some(9),
                Some(9),
                "    total = sum(values)"
            ),
            (
                RowKind::Removed,
                Some(10),
                None,
                "    return total / (len(values) + 1)"
            ),
            (
                RowKind::Added,
                None,
                Some(10),
                "    return total / len(values)"
            ),
            (RowKind::Context, Some(11), Some(11), ""),
        ]
    );
    assert_eq!(model.row_count(), 4);
}

#[test]
fn parses_hermes_inline_form() {
    let model = parse_diff(INLINE);
    assert!(model.notes.is_empty(), "the header is dropped");
    assert_eq!(model.display_path(), Some("notes.txt"));
    assert_eq!(
        file_rows(&model),
        vec![
            (RowKind::Context, Some(1), Some(1), "alpha"),
            (RowKind::Removed, Some(2), None, "beta"),
            (RowKind::Added, None, Some(2), "gamma"),
        ]
    );
    let stats = model.stats();
    assert_eq!((stats.files, stats.added, stats.removed), (1, 1, 1));
}

#[test]
fn header_marker_is_locale_independent() {
    for header in ["  ┊ Review-Diff", "  ┊ 审查 diff"] {
        let text = format!(
            "{header}\n{}",
            INLINE.lines().skip(1).collect::<Vec<_>>().join("\n")
        );
        let model = parse_diff(&text);
        assert!(model.notes.is_empty(), "{header}");
        assert_eq!(model.display_path(), Some("notes.txt"));
    }
}

#[test]
fn multi_file_and_omitted_note() {
    // source: contract schema, Hermes file lines back to back
    let text = "\
a/x.py → b/x.py
@@ -1 +1 @@
-a
+b
a/y.py → b/y.py
@@ -5,2 +5,2 @@
 c
-d
+e
… omitted 3 diff line(s) across 1 additional file(s)/section(s)
";
    let model = parse_diff(text);
    assert_eq!(model.files.len(), 2);
    let second: Vec<_> = model.files[1]
        .hunks
        .iter()
        .flat_map(|hunk| hunk.rows.iter())
        .map(|row| (row.kind, row.old_no, row.new_no, row.text.as_str()))
        .collect();
    assert_eq!(
        second,
        vec![
            (RowKind::Context, Some(5), Some(5), "c"),
            (RowKind::Removed, Some(6), None, "d"),
            (RowKind::Added, None, Some(6), "e"),
        ]
    );
    assert_eq!(
        model.notes,
        vec!["… omitted 3 diff line(s) across 1 additional file(s)/section(s)".to_string()]
    );
}

#[test]
fn hunk_counts_keep_dash_dash_rows() {
    let text = "\
--- a/q.sql
+++ b/q.sql
@@ -1,2 +1,1 @@
--- comment
 x
";
    let model = parse_diff(text);
    assert_eq!(model.files.len(), 1);
    assert_eq!(
        file_rows(&model),
        vec![
            (RowKind::Removed, Some(1), None, "-- comment"),
            (RowKind::Context, Some(2), Some(1), "x"),
        ]
    );
}

#[test]
fn no_newline_and_binary_lines_are_notes() {
    let newline = "\
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-a
\\ No newline at end of file
";
    let model = parse_diff(newline);
    let note = model.files[0].hunks[0]
        .rows
        .iter()
        .find(|row| row.kind == RowKind::Note)
        .expect("a note row");
    assert_eq!(note.old_no, None);
    assert_eq!(note.new_no, None);
    assert_eq!(note.text, " No newline at end of file");

    let binary = parse_diff("Binary files a/x and b/x differ\n");
    assert_eq!(
        binary.notes,
        vec!["Binary files a/x and b/x differ".to_string()]
    );
}

#[test]
fn malformed_hunk_header_keeps_rows_without_numbers() {
    let model = parse_diff("@@ bogus @@\n a\n-b\n+c\n");
    assert_eq!(
        file_rows(&model),
        vec![
            (RowKind::Context, None, None, "a"),
            (RowKind::Removed, None, None, "b"),
            (RowKind::Added, None, None, "c"),
        ]
    );
}

#[test]
fn git_metadata_is_not_notes() {
    let text = "\
diff --git a/old.txt b/new.txt
similarity index 100%
rename from old.txt
rename to new.txt
diff --git a/z b/z
index 1..2 100644
--- a/z
+++ b/z
@@ -1 +1 @@
-1
+2
";
    let model = parse_diff(text);
    assert!(model.notes.is_empty());
    assert!(!model.is_empty());
    assert_eq!(model.files[0].old_path.as_deref(), Some("old.txt"));
    assert_eq!(model.files[0].new_path.as_deref(), Some("new.txt"));
    assert!(model.files[0].hunks.is_empty());
    assert_eq!(model.files[1].hunks.len(), 1);
    let stats = model.stats();
    assert_eq!((stats.files, stats.added, stats.removed), (2, 1, 1));
}

#[test]
fn file_line_with_spaces_and_dev_null() {
    let spaced = parse_diff("a/my notes.txt → b/my notes.txt\n@@ -1 +1 @@\n-a\n+b\n");
    assert_eq!(spaced.display_path(), Some("my notes.txt"));
    let created = parse_diff("/dev/null → b/x\n@@ -0,0 +1 @@\n+b\n");
    assert_eq!(created.files[0].old_path, None);
    assert_eq!(created.display_path(), Some("x"));
}

#[test]
fn stats_ignore_file_header_lines() {
    let stats = parse_diff(PATCH).stats();
    assert_eq!((stats.added, stats.removed), (1, 1));
}

#[test]
fn parse_never_panics_and_numbers_never_decrease() {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, TestRunner};

    let mut runner = TestRunner::new(Config::with_cases(32));
    runner
        .run(&any::<String>(), |text| {
            let model = parse_diff(&text);
            for file in &model.files {
                for hunk in &file.hunks {
                    let mut previous_old = None;
                    let mut previous_new = None;
                    for row in &hunk.rows {
                        if let Some(number) = row.old_no {
                            if let Some(previous) = previous_old {
                                prop_assert!(number >= previous);
                            }
                            previous_old = Some(number);
                        }
                        if let Some(number) = row.new_no {
                            if let Some(previous) = previous_new {
                                prop_assert!(number >= previous);
                            }
                            previous_new = Some(number);
                        }
                    }
                }
            }
            Ok(())
        })
        .unwrap();

    let generated = prop::collection::vec(
        (
            "[a-z]{1,6}",
            prop::collection::vec(
                prop::collection::vec(
                    (
                        prop_oneof![
                            Just(RowKind::Added),
                            Just(RowKind::Removed),
                            Just(RowKind::Context)
                        ],
                        prop_oneof![
                            Just("-- comment".to_string()),
                            Just("++ added".to_string()),
                            "[ -~]{0,12}"
                        ],
                    ),
                    1..5usize,
                ),
                1..4usize,
            ),
        ),
        1..3usize,
    );
    runner
        .run(&generated, |files| {
            let (text, expected) = render_generated(&files);
            let model = parse_diff(&text);
            prop_assert_eq!(model.files.len(), expected.len());
            for (file, expect) in model.files.iter().zip(expected.iter()) {
                prop_assert_eq!(file.hunks.len(), expect.len());
                for (hunk, rows) in file.hunks.iter().zip(expect.iter()) {
                    let got: Vec<_> = hunk
                        .rows
                        .iter()
                        .map(|row| (row.kind, row.text.as_str()))
                        .collect();
                    prop_assert_eq!(
                        got,
                        rows.iter()
                            .map(|(kind, text)| (*kind, text.as_str()))
                            .collect::<Vec<_>>()
                    );
                }
            }
            Ok(())
        })
        .unwrap();
}

/// Rows of one generated hunk.
type GeneratedHunk = Vec<(RowKind, String)>;
/// A generated file name and its hunks.
type GeneratedFile = (String, Vec<GeneratedHunk>);

/// synthetic: a unified diff whose hunk counts are computed from the rows
fn render_generated(files: &[GeneratedFile]) -> (String, Vec<Vec<GeneratedHunk>>) {
    let mut text = String::new();
    let mut expected = Vec::new();
    for (index, (name, hunks)) in files.iter().enumerate() {
        text.push_str(&format!("--- a/f{index}{name}\n+++ b/f{index}{name}\n"));
        let mut file_rows = Vec::new();
        let mut old_start = 1u32;
        let mut new_start = 1u32;
        for hunk in hunks {
            let old_count = hunk
                .iter()
                .filter(|(kind, _)| matches!(kind, RowKind::Removed | RowKind::Context))
                .count();
            let new_count = hunk
                .iter()
                .filter(|(kind, _)| matches!(kind, RowKind::Added | RowKind::Context))
                .count();
            text.push_str(&format!(
                "@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"
            ));
            let mut rows = Vec::new();
            for (kind, body) in hunk {
                let body = body.replace('\n', "");
                let (sign, stored) = match kind {
                    RowKind::Added => ("+", body.trim_start_matches('+').to_string()),
                    RowKind::Removed => ("-", body.trim_start_matches('-').to_string()),
                    RowKind::Context => (" ", body.clone()),
                    RowKind::Note => continue,
                };
                // A removed `-- comment` is the line `--- comment`: sign plus `-- comment`.
                let stored = if (*kind == RowKind::Removed && body.starts_with("-- "))
                    || (*kind == RowKind::Added && body.starts_with("++ "))
                {
                    body.clone()
                } else {
                    stored
                };
                text.push(sign.chars().next().unwrap());
                text.push_str(&stored);
                text.push('\n');
                rows.push((*kind, stored));
            }
            old_start = old_start.saturating_add(old_count as u32);
            new_start = new_start.saturating_add(new_count as u32);
            file_rows.push(rows);
        }
        expected.push(file_rows);
    }
    (text, expected)
}
