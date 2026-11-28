//! The pager copies the first finished reply at or below the top cell.

use talaria_core::session::{AssistantCell, Cell, CellId, CopyBelow, UserCell, copy_below};

fn user(id: u64, text: &str) -> Cell {
    Cell::User(UserCell {
        id: CellId::new(id),
        text: text.into(),
        expanded_paste: None,
    })
}

fn reply(id: u64, text: &str, sealed: bool, interim: bool) -> Cell {
    Cell::Assistant(AssistantCell {
        id: CellId::new(id),
        text: text.into(),
        sealed,
        interim,
        transformed: false,
    })
}

fn sample() -> Vec<Cell> {
    vec![
        user(1, "ask"),
        reply(2, "draft", true, true),
        reply(3, "pong", true, false),
        user(4, "next"),
        reply(5, "later", true, false),
    ]
}

#[test]
fn copy_below_names_the_reply_at_or_below() {
    let cells = sample();

    let top = copy_below(&cells, 0);
    assert_eq!(
        top,
        CopyBelow::Ready {
            n: 2,
            text: "pong".into()
        }
    );
    assert_eq!(
        top.sentence(),
        "Copy requested: reply 2, 4 characters. Your terminal must allow clipboard access."
    );
    assert_eq!(top.copied(), Some("pong"));

    assert_eq!(
        copy_below(&cells, 3),
        CopyBelow::Ready {
            n: 1,
            text: "later".into()
        }
    );

    let miss = copy_below(&cells, 5);
    assert_eq!(miss, CopyBelow::Miss);
    assert_eq!(miss.sentence(), "No reply below this point to copy.");
    assert_eq!(miss.copied(), None);

    let long = vec![reply(1, &"a".repeat(100_001), true, false)];
    let too_long = copy_below(&long, 0);
    assert_eq!(
        too_long,
        CopyBelow::TooLong {
            n: 1,
            count: 100_001
        }
    );
    assert_eq!(
        too_long.sentence(),
        "Reply 1 is too long to copy (100001 characters). Use /export."
    );
    assert_eq!(too_long.copied(), None);

    let fence = vec![reply(1, "```code```", true, false)];
    assert_eq!(
        copy_below(&fence, 0),
        CopyBelow::Ready {
            n: 1,
            text: "```code```".into()
        }
    );
}
