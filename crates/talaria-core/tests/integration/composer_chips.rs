//! Paste labels, image numbers and expansion. Issue 224.

use proptest::prelude::*;
use talaria_core::attachments::{
    Attachment, ChipError, ChipStore, CollapseRule, ExpandError, Expanded,
};
use talaria_core::session::SessionId;

fn session() -> SessionId {
    SessionId::new("local-session-1")
}

fn store() -> ChipStore {
    ChipStore::new(16 * 1024 * 1024)
}

#[test]
fn collapse_thresholds() {
    let rule = CollapseRule::HERMES_DEFAULT;
    assert!(!rule.should_collapse("a\nb\nc\nd"));
    assert!(rule.should_collapse("a\nb\nc\nd\ne"));
    assert!(!rule.should_collapse(&"é".repeat(1999)));
    assert!(rule.should_collapse(&"é".repeat(2000)));
    assert!(rule.should_collapse("a\r\nb\r\nc\r\nd\r\ne"));
    assert!(!rule.should_collapse(&"a".repeat(1999)));
    assert!(rule.should_collapse(&"a".repeat(2000)));
    let off = CollapseRule { lines: 0, chars: 0 };
    assert!(!off.should_collapse("a\nb\nc\nd\ne\nf"));
    assert!(!off.should_collapse(&"é".repeat(4000)));
}

#[test]
fn labels_and_numbering() {
    let session = session();
    let mut chips = store();
    let (_id, label) = chips.begin_image("", &session).unwrap();
    assert_eq!(label, "[Image #1]");

    let mut chips = store();
    let (_first, first) = chips.begin_image("", &session).unwrap();
    let draft = format!("{first} [Image #2]");
    chips.reconcile(&draft);
    let (_id, label) = chips.begin_image("", &session).unwrap();
    assert_eq!(label, "[Image #3]");

    let mut chips = store();
    let (id, label) = chips.begin_image("", &session).unwrap();
    chips.image_ready(
        id,
        "<tmp>/cache/red.png".to_string(),
        "image/png".to_string(),
    );
    let expanded = chips.expand(&label).unwrap();
    chips.mark_sent(&expanded);
    let (_id, again) = chips.begin_image("", &session).unwrap();
    assert_eq!(again, "[Image #1]");

    let mut chips = store();
    let (_id, label) = chips.begin_image("[Image #4]", &session).unwrap();
    assert_eq!(label, "[Image #5]");

    let mut chips = store();
    let file = chips.add_file("", "notes.txt", "@file:notes.txt".to_string(), &session);
    assert_eq!(file, "[File #1: notes.txt]");

    let mut chips = store();
    let first = chips.collapse_paste("one");
    let second = chips.collapse_paste("two");
    assert!(first.contains("#1"));
    assert!(second.contains("#2"));

    let mut chips = store();
    let file = chips.add_file("", "a\u{1b}b.txt", "@file:secret".to_string(), &session);
    assert_eq!(file, "[File #1: a\u{241b}b.txt]");
}

#[test]
fn expand_text_display_attachments() {
    let session = session();
    let mut chips = store();
    let paste = chips.collapse_paste("a\nb\nc");
    let draft = format!("{paste} fix it");
    let expanded = chips.expand(&draft).unwrap();
    assert_eq!(expanded.text, "a\nb\nc fix it");
    assert_eq!(expanded.display, draft);
    assert!(expanded.attachments.is_empty());

    let mut chips = store();
    let (first, left) = chips.begin_image("", &session).unwrap();
    let (second, right) = chips.begin_image(&left, &session).unwrap();
    chips.image_ready(first, "<tmp>/cache/one.png".into(), "image/png".into());
    chips.image_ready(second, "<tmp>/cache/two.png".into(), "image/png".into());
    let draft = format!("look {left} and {right}");
    let expanded = chips.expand(&draft).unwrap();
    assert_eq!(expanded.text, draft);
    assert_eq!(
        expanded.attachments,
        vec![
            Attachment {
                path: "<tmp>/cache/one.png".into(),
                mime: "image/png".into()
            },
            Attachment {
                path: "<tmp>/cache/two.png".into(),
                mime: "image/png".into()
            },
        ]
    );

    let reversed = format!("{right} then {left}");
    let expanded = chips.expand(&reversed).unwrap();
    assert_eq!(expanded.attachments[0].path, "<tmp>/cache/two.png");
    assert_eq!(expanded.attachments[1].path, "<tmp>/cache/one.png");

    let mut chips = store();
    let file = chips.add_file("", "notes.txt", "@file:notes.txt".into(), &session);
    let draft = format!("{file} summarise");
    let expanded = chips.expand(&draft).unwrap();
    assert_eq!(expanded.text, "@file:notes.txt summarise");
    assert!(expanded.attachments.is_empty());

    let mut chips = store();
    let (id, label) = chips.begin_image("", &session).unwrap();
    chips.image_ready(id, "<tmp>/cache/one.png".into(), "image/png".into());
    let expanded = chips.expand(&label).unwrap();
    assert_eq!(expanded.text, "[Image #1]");
    assert_eq!(expanded.attachments.len(), 1);

    let duplicate = format!("{label} and {label}");
    let expanded = chips.expand(&duplicate).unwrap();
    assert_eq!(expanded.attachments.len(), 1);
    assert!(!expanded.text.is_empty());
}

#[test]
fn broken_or_retyped_label_is_plain_text() {
    let mut chips = store();
    chips.reconcile("[Pasted text #1 +3 line]");
    let edited = "[Pasted text #1 +3 line]";
    assert!(chips.live(edited).is_empty());
    let retyped = "[Pasted text #1 +3 lines]";
    chips.reconcile(retyped);
    assert!(chips.live(retyped).is_empty());
    let twice = format!("{retyped} {retyped}");
    chips.reconcile(&twice);
    assert!(chips.live(&twice).len() <= 1);
}

#[test]
fn refusals_block_submit() {
    let session = session();
    let mut chips = store();
    let (id, label) = chips.begin_image("", &session).unwrap();
    let _ = id;
    assert!(matches!(
        chips.expand(&label),
        Err(ExpandError::Pending { .. })
    ));

    let mut chips = ChipStore::new(1024);
    let first = chips.collapse_paste(&"a".repeat(400));
    let _second = chips.collapse_paste(&"b".repeat(400));
    let _third = chips.collapse_paste(&"c".repeat(400));
    assert!(matches!(
        chips.expand(&first),
        Err(ExpandError::MissingPaste { .. })
    ));

    let mut chips = store();
    let mut draft = String::new();
    for _ in 0..10 {
        let (_id, label) = chips.begin_image(&draft, &session).unwrap();
        draft = if draft.is_empty() {
            label
        } else {
            format!("{draft} {label}")
        };
    }
    assert!(matches!(
        chips.begin_image(&draft, &session),
        Err(ChipError::TooManyImages)
    ));
    assert!(matches!(
        chips.expand(&draft),
        Err(ExpandError::Pending { .. })
    ));
}

#[test]
fn paste_budget_evicts_oldest() {
    let mut chips = ChipStore::new(1024);
    let first = chips.collapse_paste(&"a".repeat(400));
    let second = chips.collapse_paste(&"b".repeat(400));
    let third = chips.collapse_paste(&"c".repeat(400));
    assert!(matches!(
        chips.expand(&first),
        Err(ExpandError::MissingPaste { .. })
    ));
    assert_eq!(chips.expand(&second).unwrap().text, "b".repeat(400));
    assert_eq!(chips.expand(&third).unwrap().text, "c".repeat(400));

    let mut chips = ChipStore::new(1024);
    let only = chips.collapse_paste(&"d".repeat(2000));
    assert_eq!(chips.expand(&only).unwrap().text, "d".repeat(2000));
}

#[test]
fn sent_image_label_is_plain_text() {
    let session = session();
    let mut chips = store();
    let sent = Expanded {
        text: "[Image #1]".into(),
        display: "[Image #1]".into(),
        attachments: vec![Attachment {
            path: "<tmp>/cache/red.png".into(),
            mime: "image/png".into(),
        }],
        reference_count: 0,
    };
    chips.mark_sent(&sent);
    let after = chips.expand("[Image #1]").unwrap();
    assert!(after.attachments.is_empty());

    let mut chips = store();
    let (id, label) = chips.begin_image("", &session).unwrap();
    chips.image_ready(id, "<tmp>/cache/red.png".into(), "image/png".into());
    let ready = chips.expand(&label).unwrap();
    assert_eq!(ready.attachments.len(), 1);
}

proptest! {
    #[test]
    fn expansion_property(
        text in "[a-z \n]{0,200}",
        pastes in proptest::collection::vec(("[a-z]{1,20}", 0usize..10_000), 0..=5),
    ) {
        let mut chips = store();
        let mut labels = Vec::new();
        for (paste, _) in &pastes {
            labels.push(chips.collapse_paste(paste));
        }
        let bounds: Vec<usize> = text
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(text.len()))
            .collect();
        let mut placed: Vec<(usize, &str, &str)> = pastes
            .iter()
            .zip(&labels)
            .map(|((paste, slot), label)| (bounds[slot % bounds.len()], paste.as_str(), label.as_str()))
            .collect();
        placed.sort_by_key(|left| std::cmp::Reverse(left.0));
        let mut draft = text.clone();
        let mut expected = text.clone();
        for (at, paste, label) in placed {
            draft.insert_str(at, label);
            expected.insert_str(at, paste);
        }
        let expanded = chips.expand(&draft).unwrap();
        prop_assert_eq!(expanded.text, expected);
        prop_assert!(expanded.attachments.is_empty());
    }
}
