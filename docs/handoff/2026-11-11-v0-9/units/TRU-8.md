# TRU-8: Theme colours on the default inline view

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the draw path and the catalogue flag as they are today, and for why the two tests fail before the edit. The issue states what to change. This document does not restate those requirements. REQ-1 through REQ-8 are the contract.

## The inline drawer today

`ThemedCells::lines` (`crates/talaria-tui/src/theme/cells.rs:31`) matches assistant and reasoning text, returns early for a Hermes command (`cells.rs:36`), and returns early for shell, side, and background cells when `theme.views.active()` is true (`cells.rs:50-71`). Every other cell, including user and notice, hits the fallback at `cells.rs:72`:

```rust
_ => return PlainCellLines.lines(cell, width),
```

`PlainCellLines::lines` (`crates/talaria-tui/src/history.rs:21`) paints a user cell with `Style::default().add_modifier(Modifier::BOLD)` and the prefix `"> "` (`history.rs:25`). It paints a notice with `Style::default().add_modifier(Modifier::DIM)` and the prefix `"\u{b7} "` (`history.rs:29`), and it reads `notice.text` only. `prefixed` (`history.rs:55`) is private. `split_lines` (`history.rs:70`) is `pub(crate)`: it splits on `\n` and drops a `\r` before that `\n`. REQ-1 calls it from `cells.rs` and writes the prefix there. REQ-2 does not draw `NoticeCell.detail`.

`CapturedCells` (`crates/talaria-cli/src/run.rs:933`) sends a cell that is not a tool and not background through `ThemedCells` (`run.rs:949-952`). REQ-3 leaves `themed_cell_id` (`cells.rs:95`) as it is: only assistant and reasoning cells have a captured theme id. A user cell and a notice cell use the current theme.

The pager does not use `ThemedCells`. It holds a `PlainCellLines` (`crates/talaria-tui/src/pager/mod.rs:18`). Its own user prefix is `"\u{203a} "` (`pager/render.rs:88`) and its notice prefix is `"! "` (`pager/render.rs:256`). An error notice in the pager uses `self.theme.tool_failed` (`pager/render.rs:248-249`). `ViewStyles::pager_theme` binds that field with `style(tokens::TOOL_FAILED, ...)` (`crates/talaria-tui/src/theme/legacy.rs:389`), not with `alert.error`. Those paths stay.

## Where a loaded colour already lives

`ViewStyles::inline` (`surfaces.rs:410`) looks up `style` (`surfaces.rs:406`) and, when `active()` is true (`surfaces.rs:376`, elements non-empty), returns `Style { bg: None, ..style }`. A theme file inserts every catalogue name (`file.rs:126`), so `active()` stays true when depth is none. The stored style replaces the fallback, and a catalogue attribute of none does not keep the bold or dim that was passed in. `ViewStyles::new` is `pub(super)` (`surfaces.rs:362`), so a test outside the module cannot call it. `resolve_with_theme_file` (`crates/talaria-tui/src/theme/mod.rs:18`) fills one element per catalogue name and then sets `theme.views` from those elements (`mod.rs:25-30`). That is the loader the new test uses.

`apply` (`surfaces.rs:710`) writes `SURFACE_BINDINGS` onto `Theme` fields and clears each bound background (`surfaces.rs:721`). The list has 42 pairs (`surfaces.rs:581`). Pair `("reasoning", "transcript.reasoning")` is at `surfaces.rs:607`. There is no pair for user text, notice, the reasoning label, or `alert.error`. `Theme` (`mod.rs:92`) has `pub reasoning: Style` (`mod.rs:102`) and `pub views: ViewStyles` (`mod.rs:94`). It has no user, notice, label, or error field. REQ-4 adds none, and it does not edit `surfaces.rs`.

With no theme file, `resolve` sets `theme.reasoning` from the skin role `"thinking"` only when a skin is applied (`mod.rs:234`). The tests in `reasoning_label.rs` call `resolve` with `skin: None`, so `views` is empty and `active()` is false, then they assign `theme.reasoning` a foreground (`reasoning_label.rs:15-26`). `assert_label` (`reasoning_label.rs:60`) requires dim and that same foreground. The heading those tests see is `Line::styled("∴ thinking", base_style.add_modifier(Modifier::DIM))` (`cells.rs:84-88`). `is_label` accepts `"thinking"` or `"∴ thinking"` (`reasoning_label.rs:47-49`). REQ-3 keeps that heading text and keeps this fallback when `active()` is false.

`study_minimal_and_full_examples_reach_real_draw_paths` (`theme_file_surfaces.rs:287`) draws a reasoning cell whose text is `Reasoning prose` through `ThemedCells` after loading Ember (`theme_file_surfaces.rs:210-216`). It asserts that phrase's foreground, not the heading's (`theme_file_surfaces.rs:308`). The body must stay on `theme.reasoning` so that assertion stays green. `all_42_concrete_fields_have_one_binding_and_obey_their_token` (`theme_file_surfaces.rs:131`) asserts `SURFACE_BINDINGS.len() == 42` and does not draw a user cell.

## The token constants

The constants are `TRANSCRIPT_USER_LABEL` (`surfaces.rs:17`), `TRANSCRIPT_USER_TEXT` (`surfaces.rs:18`), `TRANSCRIPT_ASSISTANT_LABEL` (`surfaces.rs:19`), `TRANSCRIPT_SYSTEM` (`surfaces.rs:20`), `TRANSCRIPT_REASONING` (`surfaces.rs:22`), `TRANSCRIPT_REASONING_LABEL` (`surfaces.rs:23`), `ALERT_ERROR` (`surfaces.rs:218`), and `TRANSCRIPT_NOTICE` (`surfaces.rs:244`).

`NoticeKind` (`crates/talaria-core/src/session/cell.rs:146`) is `Interrupted`, `Error`, `Warning`, `Automation`, `Diagnostic`, and `System`. `NoticeCell` (`cell.rs:163`) has `id`, `kind`, `text`, and `detail`. REQ-2 uses `Error` for `alert.error` and the other five kinds for `transcript.notice`.

`catalogue()` (`crates/talaria-app/src/theme_file/catalogue.rs:47`) reads `catalogue.txt`. The comment at `catalogue.rs:55` says 323 records. `reserved` is `token.starts_with("picker.")` (`catalogue.rs:62`). `catalogue_is_complete_and_self_consistent` asserts length 323, reserved count 30, and `entry.reserved == entry.token.starts_with("picker.")` (`crates/talaria-app/tests/theme_file_catalogue.rs:7-23`). REQ-5 changes the predicate. The test commit changes the count to 33 and the equality to the picker prefix plus the three names. That fails on today's `main` before `catalogue.rs` changes. `every_token_resolves` (`crates/talaria-tui/tests/theme_builtin.rs:201`) asserts the same 323 names for the terminal, ember, and paper files. It stays.

A file that sets a reserved name already gets a warning (`crates/talaria-tui/src/theme/file.rs:105-109`). Marking the three names reserved makes that warning apply to them. No test asserts those three names are free of the warning.

`documentation_lists_exactly_the_catalogue_names` (`theme_file_catalogue.rs:65`) compares token names taken from `docs/theme-tokens.md` lines that start with `| \``. It does not read the status column or the introduction. REQ-6 still edits both, because the introduction currently says only the 30 `picker.*` names are reserved (`docs/theme-tokens.md:4-5`) and the three rows are `defined` (`theme-tokens.md:12`, `:14`, `:15`). Picker rows are already `reserved` (`theme-tokens.md:96`). `transcript.user.text` (`theme-tokens.md:13`), `transcript.reasoning.label` (`theme-tokens.md:18`), `alert.error` (`theme-tokens.md:213`), and `transcript.notice` (`theme-tokens.md:239`) stay `defined`.

## The rewind exception

`rewind_palette_follows_the_legacy_no_color_rule` (`crates/talaria-tui/tests/view_colours_default.rs:114`) loops `no_color` over `None`, `Some("")`, and `Some("1")`, and calls `resolved.rewind_theme(no_color.is_none())` (`view_colours_default.rs:116-118`). For the empty string and for `"1"`, warning stays bold and accent stays `Style::default()` (`view_colours_default.rs:128-132`). REQ-7 does not edit this test. The new colour test does not read or write an environment variable, so it cannot move this result.

`every_default_view_matches_its_recorded_cells` (`view_colours_default.rs:8`) compares recorded fixtures. Those fixtures do not draw a user cell through `ThemedCells`. REQ-7 does not update recordings.

## Why the new colour test fails first

`command_reply_respects_the_resolved_color_policy_and_palette` is already in `crates/talaria-tui/tests/theme_color.rs:6`. It draws a Hermes command. The new function is not in that file yet. It compiles against today's `ThemedCells` and fails on the assertion, because user and notice lines never consult the elements map, and the heading adds dim to `theme.reasoning` (`cells.rs:87`) instead of reading `transcript.reasoning.label`.

The test builds the file the same way `empty()` does in `theme_file_surfaces.rs:33-43`, then overrides seven elements. It compares painted styles with `line.style.patch(span.style)`, the same patch `reasoning_label.rs:56` uses, so a style on the line or on the span both count. It reads the expected foreground from `ThemeFileResolution.elements` because `resolve_with_theme_file` runs `constrain` before it stores an element (`file.rs:125-126`).
