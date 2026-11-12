# FLW-3: A copy key in the transcript pager

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the pager and the key map that exist today. The requirements, the tests and the acceptance checks are only in the issue. This page points at them by id.

## What exists today

`PagerAction` is scroll, search, details, Escape, and close (`crates/talaria-tui/src/pager/mod.rs:92-124`). There is no copy variant. `PagerOutcome` is `Stay` or `Close` (`pager/mod.rs:144-151`). `cell_index` is the cell at the top of the body (`pager/mod.rs:179-180`). The footer shows `self.notice` when it is set (`pager/mod.rs:720-722`). `notice` is a private field. While a search is being typed, every action except close and Escape returns `Stay { redraw: false }` (`pager/mod.rs:257-265`).

`Action::ALL` has length 206 (`crates/talaria-tui/src/keys.rs:1010`). `PagerClose` is in that array (`keys.rs:1078`). `config_name` of `PagerClose` is `pager_close` (`keys.rs:1291`). `default_chords` of `PagerClose` is `&["q"]` (`keys.rs:1731`). `KeyMap::defaults` reads `default_chords` (`keys.rs:2455-2460`). The label of `q` is `Q`. Esc and Ctrl+C cannot be bound in the pager (`keys.rs:2102-2104`). The pager defaults occupy the chords in `keys.rs:3291-3305`. `y` is not among them.

`pager_action` (`crates/talaria-cli/src/run.rs:3198-3216`) maps those actions and ends in `_ => return None`. `apply_transcript` is defined at `run.rs:3268` on `impl TuiScreen` and discards `handle_action_with_renderer` at `run.rs:3330-3335`. While the pager is typing a search, the screen feeds `handle_search_input` instead (`run.rs:3314-3321`). The app does the same split (`app.rs:8327`). The app does not store `cell_index`. `commit_lines` (`run.rs:4546`) is on `impl Screen for TuiScreen`.

`copy_mcp_text` (`run.rs:3518-3519`) calls `copy_to_clipboard` and ignores the error. `notice_lines` (`crates/talaria-app/src/app.rs:2345-2346`) commits lines prefixed with a middle dot and a space.

`ACT-3` adds `pub fn assistant_reply(cells: &[Cell], n: u32) -> Result<String, ReplyMiss>` and `pub enum ReplyMiss { Missing }` before this unit starts. They are not in this checkout. `ReplyMiss` has no other variant. `assistant_reply` counts from the newest: `1` is the newest finished reply. `Action::ALL` is declared `[Self; 206]` at `keys.rs:1010`. `PagerOutcome` is `Stay { redraw: bool }` or `Close` (`pager/mod.rs:144`). `draw_transcript` (`run.rs:3343`) draws the pager on every call. `pager_nav.rs` pulls in support with `#[path = "pager_support.rs"]`. `session/mod.rs` re-exports the cell types (`mod.rs:18-21`). REQ-4 adds `CopyBelow` beside the re-export `ACT-3` leaves.

`docs/keybindings.md:104-122` is the pager table. `docs/configuration.md:75` is `pager_close = ["q"]`. `keybinding_tables_equal_default_key_map` (`crates/talaria-cli/tests/docs.rs:161`) requires every pager action's config name and default chords to appear in that table. `doc_default_shape_loads_cleanly` (`crates/talaria-app/tests/config_doc.rs:89`) parses that document's one toml block and compares it with `default_keys` (`config_doc.rs:130`). The `pager_close` row of that list is `config_doc.rs:198`.

Two tests assert `Action::ALL.len() == 206` inside `keys.rs` (`keys.rs:2901` and `keys.rs:3508`). The same number is in `composer_vim.rs:21`, `keys_completion.rs:15`, and `model_keys.rs` (`model_keys.rs:6`, `model_keys.rs:69`, `model_keys.rs:120`, `model_keys.rs:129`). `composer_vim.rs:22` requires the last action to be `ToggleVim`.

## What this page does not restate

REQ-4 names which cell is copied and how `n` is counted. REQ-5 names the three sentences. REQ-6 names what the screen writes. This page does not copy those strings.
