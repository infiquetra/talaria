# FIX-1: A grow larger than the scrollback leaves stale live-view rows on screen

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document. Read the issue for REQ-1 through REQ-5 and the three scrollback tests. This page maps the grow path measured on `main` at commit `ee5c4881` on 2026-11-11, and holds those tests in full. It does not restate the requirements.

## What exists today

`InlineTerminal::resize` (`crates/talaria-tui/src/terminal.rs:973`) follows a size change without asking the terminal where the cursor went (`terminal.rs:966`). On a grow, when `scrolled` is `None`, `pulled` is the whole growth (`terminal.rs:1020`) and `top` and `cursor` move down by that amount minus what `spilled` recovers (`terminal.rs:1022-1025`). The clear that follows starts at the new top (`terminal.rs:1054`, `clear_from_top_absolute` at `terminal.rs:1322-1334`). `park` puts the cursor at column 0 of the viewport's last row (`terminal.rs:1260-1264`). `scrolled` is `None` when the cursor started on the last row, and `Some(0)` when that row is above the last (`terminal.rs:751-754`). `spilled` counts live rows a shrink pushed above row zero (`terminal.rs:699-700`).

On a grow the terminal pulls back the smaller of the rows in history and the rows added, and moves the cursor down with that content. When history is shorter than the growth, the old live rows sit `growth - pulled` rows higher than Talaria's new top, and the clear never reaches them.

Startup with the cursor on the last row scrolls `height - 1` blank rows into history (`make_room`, `terminal.rs:1413-1425`, called from `terminal.rs:793-796`). Each committed line scrolls one more, and repainting the live rows after a commit scrolls `height` more (`insert_rows`, `terminal.rs:1456-1474`, `newline` at `terminal.rs:1337-1346`). For live height 13 and 15 committed lines that is 12 + 2 + 13 = 27 rows.

`close_overlay` leaves the alternate screen at `ALTERNATE_OFF` (`terminal.rs:1168`) and then clears from the tracked top (`terminal.rs:1157-1183`). Overlay drawing keeps its own cursor (`terminal.rs:1095`).

Clearing by absolute row from the old top down removes the stale rows and also the committed lines the terminal pulled back. Measured on 2026-11-11, that clear lost lines in 8 of 14 setups. A relative clear that omitted `spilled` left the pushed live row on screen in the shrink-then-grow setup.

## Measurements

Emulator: `Alac` in `crates/talaria-tui/tests/support/mod.rs:174-288`. Width 100 except the narrowing setup. Unless noted, the cursor started on the last row, so `scrolled` is `None`. "Pulled" is history rows before the emulator's own grow minus history rows after.

| Setup | History before | Growth | Pulled | Stale rows before the change | Lines lost by an absolute clear |
| --- | --- | --- | --- | --- | --- |
| 30 to 40, height 13, 15 lines | 27 | 10 | 10 | none | lines 6-15 |
| 30 to 40, height 13, 0 lines | 12 | 10 | 10 | none | none |
| 30 to 60, height 13, 0 lines | 12 | 30 | 12 | rows 29-41 | none |
| 30 to 40, height 2, 0 lines | 1 | 10 | 1 | rows 29-30 | none |
| 30 to 40, height 2, 3 lines | 4 | 10 | 4 | rows 32-33 | lines 1-3 |
| 30 to 40, height 2, emulator cursor at 0 | 1 | 10 | 1 | rows 29-30 | none |
| 100 shell lines, height 2, 3 lines, 30 to 40 | 75 | 10 | 10 | none | lines 1-3 |
| 34 shell lines, height 2, 30 to 40 | 6 | 10 | 6 | rows 34-35 | none |
| height 4, shrink 30 to 20, grow 20 to 40 | 15 | 20 | 15 | rows 31-34 | lines 1-2 |
| 30 to 60, height 13, 15 lines | 27 | 30 | 27 | rows 44-46 | lines 1-15 |
| shrink 30 to 12, then 12 to 60 | 45 | 48 | 45 | rows 44-46, including pushed `LIVE# 1` | lines 1-15 |
| 100 by 30 to 80 by 60 | 27 | 30 | 27 | rows 44-46 | lines 1-15 |
| 30 to 45, then 45 to 60 | 12 | 15 | 12 | rows 44-46 | lines 1-15 |
| cursor on row 1, `scrolled` `Some(0)`, 30 to 60 | 0 | 30 | 0 | none | none |

Rows pulled were the smaller of history and growth in every setup. The cursor moved down by exactly the rows pulled. Stale rows appeared only when history was shorter than the growth.

## Tests in full

```rust
#[test]
fn grow_past_known_history_leaves_no_stale_live_row_and_keeps_every_line() {
    let mut alac = Alac::new(100, 30);
    alac.feed(b"\x1b[30;1H");
    let mut rig = Rig::new(alac, (100, 30), 13, probe(false, false, Some((0, 29))));
    rig.commit_lines(tagged(15));
    let live = live_view_sections(13);
    rig.draw(&live);
    rig.resize(100, 60);
    rig.draw(&live);
    assert_tags_once_in_order(&rig.alac.history_and_screen(), 15);
    assert_live_only_in_viewport(&rig.alac.screen(), rig.term.viewport());
}

#[test]
fn grow_after_spilling_shrink_without_history_leaves_no_stale_live_row() {
    let mut alac = Alac::new(100, 30);
    alac.feed(b"\x1b[30;1H");
    let mut rig = Rig::new(alac, (100, 30), 13, probe(false, false, Some((0, 29))));
    rig.commit_lines(tagged(15));
    let live = live_view_sections(13);
    rig.draw(&live);
    rig.resize(100, 12);
    rig.draw(&live);
    rig.resize(100, 60);
    rig.draw(&live);
    assert_tags_once_in_order(&rig.alac.history_and_screen(), 15);
    assert_live_only_in_viewport(&rig.alac.screen(), rig.term.viewport());
}

#[test]
fn grow_under_overlay_past_known_history_leaves_no_stale_live_row() {
    let mut alac = Alac::new(100, 30);
    alac.feed(b"\x1b[30;1H");
    let mut rig = Rig::new(alac, (100, 30), 13, probe(false, false, Some((0, 29))));
    rig.commit_lines(tagged(15));
    let live = live_view_sections(13);
    rig.draw(&live);
    rig.open_overlay();
    rig.draw_overlay();
    rig.resize(100, 60);
    rig.draw_overlay();
    rig.close_overlay();
    rig.draw(&live);
    assert_tags_once_in_order(&rig.alac.history_and_screen(), 15);
    assert_live_only_in_viewport(&rig.alac.screen(), rig.term.viewport());
}
```

`assert_tags_once_in_order` is `crates/talaria-tui/tests/support/mod.rs:320-344`. `assert_live_only_in_viewport` is `support/mod.rs:355-366`. Before the change, each of the three fails at `support/mod.rs:360` because row 44 contains `LIVE#` while the viewport covers rows 47 to 60, and the row text is `LIVE# 1`.
