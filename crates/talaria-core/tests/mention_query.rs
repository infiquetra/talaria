//! Mention query, acceptance and list timing. Issue 224.

use std::time::{Duration, Instant};

use hermes_contract::schemas::CompletionItem;
use talaria_core::attachments::{
    Accept, MentionQuery, MentionState, accept, count_references, filter_items, format_ref_value,
    mention_at, reference_notice,
};

fn item(text: &str) -> CompletionItem {
    CompletionItem {
        text: text.to_string(),
        display: String::new(),
        meta: String::new(),
        kind: None,
    }
}

fn query(start: usize, word: &str) -> MentionQuery {
    MentionQuery {
        start,
        word: word.to_string(),
    }
}

#[test]
fn detects_query_at_caret() {
    let rows = [
        ("@ca", 3, Some((0, "@ca"))),
        ("see @ca", 7, Some((4, "@ca"))),
        ("line one\n@ca", 12, Some((9, "@ca"))),
        ("@", 1, Some((0, "@"))),
        ("@ca rest", 3, Some((0, "@ca"))),
        ("@cart", 3, None),
        ("me@host", 7, None),
        ("x/@y", 4, None),
        ("@ ca", 4, None),
        ("a @b @c", 7, Some((5, "@c"))),
        ("naïve @é", 10, Some((7, "@é"))),
        ("(@x", 3, None),
    ];
    for (draft, caret, expected) in rows {
        let found = mention_at(draft, caret).map(|query| (query.start, query.word));
        assert_eq!(
            found,
            expected.map(|(start, word)| (start, word.to_string())),
            "{draft}"
        );
    }
}

#[test]
fn filters_profile_items() {
    let items = [
        "@hermes",
        "@file:a.py",
        "@diff",
        "@jira:XYZ-1",
        "@staged",
        "@other",
    ]
    .into_iter()
    .map(item)
    .collect();
    let kept: Vec<String> = filter_items(items)
        .into_iter()
        .map(|item| item.text)
        .collect();
    assert_eq!(
        kept,
        vec![
            "@file:a.py".to_string(),
            "@diff".to_string(),
            "@jira:XYZ-1".to_string(),
            "@staged".to_string(),
        ]
    );
}

#[test]
fn quotes_like_hermes() {
    let rows = [
        ("plain.py", "plain.py"),
        ("a b.py", "`a b.py`"),
        ("a`b c", "\"a`b c\""),
        ("x(1).py", "`x(1).py`"),
        ("a`\"' b", "a`\"' b"),
    ];
    for (value, expected) in rows {
        assert_eq!(format_ref_value(value), expected, "{value}");
    }
}

#[test]
fn accept_insert_complete_and_drill() {
    let cases = [
        (
            "Summarize @calc",
            query(10, "@calc"),
            "@file:src/calc.py",
            Accept::Insert,
            "Summarize @file:src/calc.py ",
            false,
            "Summarize @file:src/calc.py ".len(),
        ),
        (
            "Summarize @calc",
            query(10, "@calc"),
            "@file:src/calc.py",
            Accept::Complete,
            "Summarize @file:src/calc.py ",
            false,
            "Summarize @file:src/calc.py ".len(),
        ),
        (
            "@",
            query(0, "@"),
            "@file:",
            Accept::Insert,
            "@file:",
            true,
            "@file:".len(),
        ),
        (
            "@sr",
            query(0, "@sr"),
            "@folder:src/",
            Accept::Complete,
            "@src/",
            true,
            "@src/".len(),
        ),
        (
            "@sr",
            query(0, "@sr"),
            "@folder:src/",
            Accept::Insert,
            "@folder:src/ ",
            false,
            "@folder:src/ ".len(),
        ),
        (
            "@my",
            query(0, "@my"),
            "@folder:my dir/",
            Accept::Complete,
            "@folder:`my dir/` ",
            false,
            "@folder:`my dir/` ".len(),
        ),
        (
            "@no",
            query(0, "@no"),
            "@file:my notes.md",
            Accept::Insert,
            "@file:`my notes.md` ",
            false,
            "@file:`my notes.md` ".len(),
        ),
    ];
    for (draft, query, text, how, expected, keep_open, caret) in cases {
        let accepted = accept(draft, &query, &item(text), how);
        assert_eq!(accepted.draft, expected, "{draft}");
        assert_eq!(accepted.keep_open, keep_open, "{draft}");
        assert_eq!(accepted.caret, caret, "{draft}");
    }
    let draft = "x @di more";
    let accepted = accept(draft, &query(2, "@di"), &item("@diff"), Accept::Insert);
    assert_eq!(accepted.draft, "x @diff more");
    assert!(!accepted.keep_open);
    assert_eq!(accepted.caret, "x @diff".len());
}

#[test]
fn list_state_timers_scroll_and_reopen() {
    let mut state = MentionState::new();
    let start = Instant::now();
    state.on_edit("@ca", 3, start);
    assert!(state.tick(start + Duration::from_millis(59)).is_none());
    let request = state
        .tick(start + Duration::from_millis(60))
        .expect("one request");
    assert_eq!(request.word, "@ca");
    assert_eq!(request.generation, 1);
    let requested = start + Duration::from_millis(60);
    assert!(
        state
            .view(requested + Duration::from_millis(149))
            .status
            .is_none()
    );
    assert_eq!(
        state
            .view(requested + Duration::from_millis(150))
            .status
            .as_deref(),
        Some("Searching…")
    );

    state.on_edit("@cal", 4, requested + Duration::from_millis(30));
    state.on_items(1, vec![item("@file:stale.py")]);
    assert!(
        state
            .view(requested + Duration::from_millis(40))
            .rows
            .is_empty()
    );
    let second = state
        .tick(requested + Duration::from_millis(90))
        .expect("generation 2");
    assert_eq!(second.generation, 2);
    assert_eq!(second.word, "@cal");

    let mut files: Vec<CompletionItem> = (0..12)
        .map(|index| item(&format!("@file:f{index}.py")))
        .collect();
    state.on_items(2, files.clone());
    for _ in 0..9 {
        state.next();
    }
    let window = state.view(requested + Duration::from_millis(200));
    assert_eq!(window.rows.len(), 8);
    assert_eq!(window.selected, Some(7));
    assert_eq!(window.rows[0].0, "@file:f2.py");
    assert_eq!(window.rows[7].0, "@file:f9.py");
    for _ in 0..9 {
        state.prev();
    }
    state.prev();
    let wrapped = state.view(requested + Duration::from_millis(200));
    assert_eq!(
        wrapped.rows.last().map(|(text, _)| text.as_str()),
        Some("@file:f11.py")
    );
    assert_eq!(wrapped.selected, Some(7));

    state.dismiss();
    state.on_edit("@cal", 4, requested + Duration::from_millis(300));
    assert!(!state.is_open());
    state.on_edit("@calc", 5, requested + Duration::from_millis(301));
    assert!(state.is_open());
    let current = state
        .tick(requested + Duration::from_millis(361))
        .expect("reopened request");

    state.on_failed(
        current.generation,
        "Hermes did not answer in time.".to_string(),
    );
    assert_eq!(
        state
            .view(requested + Duration::from_millis(400))
            .status
            .as_deref(),
        Some("Completion unavailable: Hermes did not answer in time.")
    );

    files = (0..5)
        .map(|index| item(&format!("@profile{index}")))
        .collect();
    files.extend((0..30).map(|index| item(&format!("@file:n{index}.py"))));
    state.on_items(current.generation, files);
    let listed = state.view(requested + Duration::from_millis(500));
    assert_eq!(listed.rows.len(), 8);
    for _ in 0..29 {
        state.next();
    }
    let end = state.view(requested + Duration::from_millis(500));
    assert_eq!(
        end.rows.last().map(|(text, _)| text.as_str()),
        Some("@file:n29.py")
    );
}

#[test]
fn counts_references_like_hermes() {
    let rows = [
        ("@file:a.py and @diff", 2),
        ("me@file:a.py", 0),
        ("@file:`a b.py` @staged", 2),
        ("@folder:src/ @url:https://x.test @git:3", 3),
        ("[Pasted text #1 +3 lines]", 0),
        ("@diff", 1),
        ("@file:`a b.py`:3-5 and @file:\"c.py\":7", 2),
    ];
    for (text, expected) in rows {
        assert_eq!(count_references(text), expected, "{text}");
    }
    let many = (0..17).map(|_| "@file:a.py").collect::<Vec<_>>().join(" ");
    assert_eq!(count_references(&many), 17);
    assert_eq!(
        reference_notice(17),
        Some("Hermes expands at most 16 @ references per message; the rest arrive as plain text.")
    );
    assert_eq!(reference_notice(16), None);
}
