# TRU-5: Workspace footer names the transcript and search keys

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the one hint the workspace draw takes today, for the column counts behind the narrow footer, and for the tests whose row indexes move. The requirements, the tests, and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

`Workspace::draw_with_style_and_theme` (`crates/talaria-tui/src/workspace/mod.rs:141-149`) takes one `hint: Option<String>`. It passes that hint to `ChecklistSection::new` when the checklist is in the footer (`workspace/mod.rs:157-158`) and to the work panel when the panel is visible (`workspace/mod.rs:193-200`). The footer sections are the checklist, when the panel is not visible, then the borrowed live sections (`workspace/mod.rs:160-165`). Those sections are rendered at `workspace/mod.rs:204-215`. `LiveView::layout` (`live.rs:24-58`) caps the footer at `max_live_height` (`live.rs:73`), the area height minus 2. Over that cap, later sections keep their rows and the first gives way (`live.rs:21-23`). When the later sections alone are taller, the first gets no rows and the remaining rows go to the sections from the last upward. In the workspace the first footer section is the drafting line (`run.rs:4230` and `:4351-4354`). The composer is later (`run.rs:4401`). REQ-4 leaves the new row out for the width budget. When the footer sections plus the new row would exceed `max_live_height`, the new row is left out and every other section is laid out exactly as today. It never pushes another section out. `LiveView::layout` stays as it is.

The panel is visible only for `PanelVisibility::Docked` and `PanelVisibility::OverlayOpen`, and only when the area is tall enough (`workspace/mod.rs:44-68`). `Dismissed` and `Hidden` leave the checklist in the footer. REQ-4 does not measure that line. It uses the `hint` string, the separator, and the new hint.

`Workspace::draw` (`workspace/mod.rs:98-106`) calls `draw_with_style` with the checklist hint `None`. The free function `workspace::draw` (`workspace/mod.rs:232-234`) uses that method. The render tests call the free function, so those calls keep compiling when the method gains `transcript_hint`. REQ-1 makes `draw` pass the exact string while its own signature stays. `draw_with_style` (`workspace/mod.rs:108-116`) has seven parameters counting `self` and no Clippy allow. `draw_with_style_and_theme` has eight and the allow at `workspace/mod.rs:140`. REQ-1 adds the same allow to `draw_with_style`, because `transcript_hint` makes that count eight. `clippy.toml` sets no argument threshold.

`footer_matches_the_inline_footer` and `workspace_snapshots_at_80x24_and_120x40` call `workspace::draw`. `footer_names_transcript_open_and_search` is in the same file and passes `transcript_hint`. That file is one integration-test crate, so the crate fails to compile before the change and the first two tests are not run. The issue does not say they fail on an assertion in that commit.

The production call is `crates/talaria-cli/src/run.rs:4509-4518`:

```rust
let hint = self.keymap.label(Action::ToggleTasks);
let _ = self.terminal.draw_workspace(|area, buffer| {
    render_workspace.draw_with_style_and_theme(
        &workspace,
        area,
        buffer,
        &renderer,
        style,
        hint,
        &self.theme.views,
    );
});
```

`KeyMap::label` (`crates/talaria-tui/src/keys.rs:2753-2755`) returns the label of the first chord. The default chords are `ctrl+o`, `/`, and `ctrl+t` (`keys.rs:1716`, `:1727`, `:1732`). The label table in the same file expects `Ctrl+O`, `/`, and `Ctrl+T` (`keys.rs:3290`, `:3301`, `:3306`). REQ-2 keeps the checklist call and passes the settled sentence as a literal. The sentence matches those default labels. A later rebind of the chord does not rewrite the sentence.

`ChecklistSection::header_row` (`checklist.rs:156-181`) appends ` · {hint} to collapse` and, when that line is wider than the area, drops the hint and keeps the counts. The collapsed row drops the hint before it cuts the focus text (`checklist.rs:211-217`). Those tests live in `checklist_render.rs` and call `ChecklistSection` directly. REQ-5 does not change them. The new row is added by the workspace after that render, so the checklist line stays exact. REQ-3 draws that row in `ChecklistStyle.hint` of the `ChecklistStyle` the draw already holds (`checklist.rs:33-35`), the same field the hint span uses (`checklist.rs:179`).

The existing hint `Ctrl+T` is 6 columns. The separator ` · ` at `checklist.rs:165` is 3. `Ctrl+O transcript, / search` is 27 ASCII columns. The sum is 36. Width 35 leaves the new row out. Width 36 shows the new row. The narrow draw still contains `Ctrl+T to collapse` because `ChecklistSection` draws that line unchanged. A rule that only compares 27 with the width would show the new row at 35, and the narrow draw would fail.

`footer_matches_the_inline_footer` (`crates/talaria-tui/tests/workspace_render.rs:293-322`) draws 120 by 40 with two live sections and no checklist. The footer is the last two rows: row 38 `composer draft`, row 39 `model · listening · read-aloud`. One added row moves those to 37 and 38 and puts the hint on 39. The test's `take(38)` must become `take(37)` so the body check stays above the footer.

`workspace_snapshots_at_80x24_and_120x40` (`workspace_render.rs:67-175`) asserts the last four rows of one four-line section and reads the panel column for `height - 4` rows. The snapshot files are 19 lines and end with `Alt+S details / Alt+A next`. A fifth footer row makes the panel body `height - 5`. At 80 by 24 that body is 19 rows, equal to the snapshot. At 120 by 40 it is larger. The snapshot files stay. The last asserted row changes from `status voice` to the hint.

`menu_bar.rs::row_has_no_voice_or_model_text` also calls `workspace::draw`. It checks the menu row and that the frame still contains `listening`. It does not assert the last row. It is not in the test list. If that assertion fails, the status row was lost, which REQ-3 does not allow.

`keymap_doc.rs::doc_keys_equal_defaults` (`crates/talaria-tui/tests/keymap_doc.rs:60`) fails when a default chord and the key bindings page disagree. REQ-5 changes no chord, so the test stays green. AC-4 runs it.

`Terminal::draw_live` (`crates/talaria-tui/src/terminal.rs:898`) draws a `LiveView` and is the inline path. Test 3 renders the same sections through `LiveView::layout` (`live.rs:24`) and does not call the workspace. The hint row is not a live section, so that buffer does not contain it.

## Edge cases

| Footer | Width | New row | Checklist line |
| --- | --- | --- | --- |
| No checklist, `transcript_hint` is the exact string | 120 | Last row, exact string | No checklist line |
| Checklist in the footer, hint `Ctrl+T`, expanded 1/3 | 35 | Absent | `Ctrl+T to collapse` stays |
| Same checklist | 36 or 120 | Present, its own row | Unchanged from `ChecklistSection` |
| `transcript_hint` is `None` | Any | Absent | Unchanged |
| Width below 27, no checklist | Below 27 | Absent | No checklist line |
| Inline `LiveView` of the same sections | 120 | Absent | Not the workspace |
| Sections plus the new row exceed `max_live_height` | Any width that passes REQ-4 | Absent | Other sections laid out as today |
| No other footer sections, `transcript_hint` is the exact string | 120 | The only footer row | No checklist line |

The panel snapshot at 80 by 24 is the case that looks like a clip and is not one. The body loses one row and still holds every snapshot line. Test 2 locks that. A failure that asks for a shorter snapshot is a stop condition.

`every_default_view_matches_its_recorded_cells` compares `crates/talaria-tui/tests/snapshots/view_colours/workspace.json` cell by cell at 80 by 24. The fixture calls `ThemeFileResolution::draw_workspace`, which calls `workspace::draw_with_theme` (`theme/mod.rs:750-757`). That path passes the new hint, so the recording gains the footer row in `ChecklistStyle.hint` of the theme in use. The test commit replaces the file with those cells. The test compiles and fails on the comparison until the draw changes. The test function stays.
