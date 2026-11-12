# ACT-1: Fleet approval card footer says Ctrl+C denies and stops, but it only goes back

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that draws the approval card today, for what Ctrl+C already does in each fleet focus, and for why the overlay footer can repeat a clause. The requirements, the tests, and the acceptance checks are only in the issue. This page points at them by id.

## The card line the peek paints

`PromptStack::section` (`crates/talaria-tui/src/prompts/stack.rs:385`) returns the open card. For an approval, the last line is built at `stack.rs:607-609`:

```text
1-{} choose · Enter confirm · Esc deny · Tab note · Ctrl+C deny and stop
```

The `{}` is `choices.len()` after the card has built its displayed choices. The two-choice fixture in the screen test uses `once` and `deny`, and the painted line is `1-2`. The middle dot is the character already in that format string.

Clarify hints in the same function are different (`stack.rs:657-668`): `Ctrl+C skip and stop`. A masked card uses `Ctrl+C decline and stop` (`stack.rs:562`). After that match, `stack.rs:672-686` replaces the hint when the card is stopping (`stopping…`), when a draft is in the way, or when a choice is sending. REQ-4 rewrites a row only when it still contains the approval suffix, so those replacements stay.

`session_body` (`crates/talaria-tui/src/fleet_view/peek.rs:303`) paints the section at `peek.rs:329-335`. The rectangle is `section.render` at `peek.rs:333-334`. Below it, the same function paints the activity or outcome lines (`peek.rs:350-360`). The header is painted earlier in `render` (`peek.rs:113`). Transcript `tail_lines` run only in the `else` at `peek.rs:337-348`, when `stack.section()` is absent, so a frame that has a card has no transcript row. REQ-4 limits the rewrite to that card rectangle. `session_body` does not take `FleetViewState`. `render` (`peek.rs:78`) does, and it calls `session_body` for `Panel::Session` at `peek.rs:138-147`. REQ-8 allows the rewrite there, or a private helper in this file. It does not name the helper.

`panel_of` (`crates/talaria-tui/src/fleet_view/mod.rs:389`) returns `Panel::Opening` whenever the state's peek status is `PeekStatus::Opening` (`mod.rs:395`), before it looks at the `PeekPanel` the test passes in. `open_peek` sets that status (`crates/talaria-core/src/fleet_view/state.rs:581`). `ensure_peek` calls `open_peek` only when the row is not already the peeked row (`state.rs:590-594`). `reply_start` always calls `ensure_peek` (`state.rs:1062`) and then sets `Focus::Reply` (`state.rs:1065`). `FleetAction::Filter` only sets `Focus::Filter` (`state.rs:932-933`). Test 2 therefore sends `PeekStatus::Open` before `Reply`, and sends `Filter` with no `Peek` update. REQ-5 matches the format string above. The fleet peek is not given a correction row, and an offered choice keeps this hint.

The running loop passes a stack into every session peek (`crates/talaria-cli/src/run.rs:3742-3755`). A foreground peek uses `self.prompts`. A background peek uses `self.peek_prompts`. The card can be on screen while focus is still the list.

## What Ctrl+C does, by focus

`update` (`crates/talaria-core/src/fleet_view/state.rs:709`) matches `state.focus` at `state.rs:715-721`. `fleet_key` (`crates/talaria-cli/src/run.rs:1408`) turns the key into `FleetViewInput::CtrlC` before the card sees it, for answer focus (`run.rs:1423-1426`), for reply and filter (`run.rs:1484-1485`), and for list and confirm-stop (`run.rs:1504-1505`).

| Focus | What Ctrl+C does today | Card clause |
| --- | --- | --- |
| `List` | `list_key` calls `close_all` (`state.rs:910`, `state.rs:980`). Effects are `ClosePeek` when a peek is open, then `Close`. No `Stop`. | REQ-1: `ctrl+c close` |
| `Answer` | `answer_key` (`state.rs:1277-1282`) sets focus to `List` and returns no effects. | REQ-2: `ctrl+c back to list` |
| `ConfirmStop` | `confirm_key` (`state.rs:1225-1232`) returns to `List`. Any input other than `FleetAction::Stop` sets the notice `Stop cancelled.` and does not emit `Stop`. | REQ-3: shared hint, not rewritten |
| `Filter` | `filter_key` clears the filter and sets focus to `List` (`state.rs:1170-1173`). | REQ-3 |
| `Reply` | `reply_key` (`state.rs:1070`) on Ctrl+C keeps the draft and then leaves the editor (`state.rs:1108-1110`). The draft is written back when it is not empty (`state.rs:1145-1146`). | REQ-3 |

`focus` on `FleetViewState` is a private field (`state.rs:202`). Callers outside the module use `pub fn focus(&self) -> &Focus` (`state.rs:260`). `update` matches the field because it is in the same module (`state.rs:716`). REQ-3 leaves Filter, Reply, and ConfirmStop on the shared hint. That hint's Ctrl+C clause is not what those three keys do. The settled words name only the first two rows of this table.

The overlay footer is a different line from the card. `footer_text` (`crates/talaria-tui/src/fleet_view/mod.rs:549`) uses `list_footer` (`mod.rs:588`) for `Focus::List` and `Focus::ConfirmStop` (`mod.rs:575`). For `Focus::Answer` on a session panel it reads `card_hint` (`peek.rs:65`) and, at `mod.rs:568-570`, replaces from the capital substring `Ctrl+C` with `BACK_TO_LIST`. That constant is `ctrl+c back to list` (`mod.rs:44`). If the search misses, it appends ` · ctrl+c back to list` again.

REQ-7 exists because of that search. The settled clause is lowercase `ctrl+c`. If the rewrite changes the string `card_hint` reads, `find("Ctrl+C")` misses and the overlay footer shows the clause twice. The issue therefore rewrites only the painted peek row, after `session_body`, and leaves the stack `card_hint` reads unchanged. `footer_text` and `list_footer` stay. The list footer already ends in `esc close` (`mod.rs:606`).

## Why the main session sentence stays

The same format string feeds the main session card. While a turn is running, an interrupt of that session denies open approvals before `session.interrupt`. `interrupt_sequence` (`crates/talaria-app/src/app.rs:10111`) calls `PromptService::deny_before_interrupt` (`crates/talaria-app/src/prompts.rs:329`) when an approval is open. That method calls `approval.respond` with choice `deny` (`prompts.rs:346`). The effect comment at `app.rs:2214` names the order: deny open approvals, then `session.interrupt`.

A fleet stop is a different path, and it is not Ctrl+C. The first `FleetAction::Stop` calls `stop_start` (`state.rs:1190`) and sets `Focus::ConfirmStop`. The second calls `confirm_key`, which pushes `FleetViewEffect::Stop` (`state.rs:1241`). `stop_asks_and_refuses_idle_rows` (`crates/talaria-core/tests/fleet_view_keys.rs:210`) locks that pair. For a background row, `fleet_actions` handles `FleetViewEffect::Stop` by calling `PeekService::stop` (`crates/talaria-app/src/fleet_actions.rs:229`). `PeekService::stop` (`crates/talaria-app/src/peek.rs:679`) denies, then calls `session.interrupt` (`peek.rs:697-720`). For the foreground row, `app.rs:14301` routes `Stop` to `ForegroundJob::Stop`, which emits `Event::Interrupt` (`app.rs:14408-14418`) and then uses the same deny-then-interrupt sequence. The hint does not need to say `x`. The default binding for fleet stop is `x` in the keybinding reference. REQ-6 leaves this path alone.

## What the screen test actually locks

`peek_panel_tail_card_and_outcome` (`crates/talaria-tui/tests/fleet_view_screens.rs:617`) paints three peeks. Only the middle one has a card. Its expected card line is at `fleet_view_screens.rs:659` and includes `Ctrl+C deny and stop`. `hold` (`fleet_view_screens.rs:422`) calls `FleetViewState::new(None, NOW)` and `update` with a snapshot at version 1 and page 8. It does not set answer focus, so the state is `Focus::List`. `finished_session` (`fleet_view_screens.rs:558`) sets `running` false. The card is the `PromptStack` passed to `paint`, not a turn inside that session. Paint in that test is 80 by 24.

`Expect.answer` (`fleet_view_screens.rs:244`) is not that focus. `expect_screen` uses it only to pick a footer (`fleet_view_screens.rs:330`). `FOOTER_ANSWER` (`fleet_view_screens.rs:32`) is the list footer that contains `a answer`. The card half passes `answer: true` (`fleet_view_screens.rs:670`) because a visible card adds `a answer` to the list footer. Test 1 keeps that flag and changes the card line to the list clause. The tail with no card, and the interrupted tail, stay as they are.

Test 2 has to enter `Focus::Answer` through `update`. `answer_start` (`state.rs:1247`) requires a peek whose status is `PeekStatus::Open { has_open_prompt: true, .. }`. A row whose backend is `FleetBackend::Herdr` is sent to `reply_start` instead (`state.rs:1262-1267`). A foreground row returns `FleetViewEffect::Close` (`state.rs:1269-1271`). `is_foreground` is true when the state's foreground key matches or `row.foreground` is set (`state.rs:505-506`). The fixture row from `peek_row` uses `row`, which sets `foreground: false` and `backend: FleetBackend::Hermes` (`fleet_view_screens.rs:39-65`). `hold` passes `None` as the foreground key. `answer_focus_rules` (`fleet_view_keys.rs:323`) is the existing walk of that same input, including Ctrl+C returning to `Focus::List` at `fleet_view_keys.rs:358-361`. `esc_unwinds_and_ctrl_c_never_stops` (`fleet_view_keys.rs:279`) locks list-focus Ctrl+C as `ClosePeek` then `Close` at `fleet_view_keys.rs:310-318`.

`card_hint` renders the stack into its own buffer and reads the last row (`peek.rs:65-75`). It does not read the peek panel after `session_body`. A test that only asserts the overlay footer therefore passes before any draw change. Test 2's failing assertion is the card row inside the painted peek.

## Tests that must stay green

`approval_card_snapshot_80_and_160_cols` (`crates/talaria-tui/tests/prompt_cards.rs:363`) and `approval_keys_matrix` (`prompt_cards.rs:452`) render `PromptStack` directly. They fail if REQ-5 is broken. `clarify_single_other_and_free_text` asserts `Ctrl+C skip and stop` at `prompt_cards.rs:807`. `approval_keys_matrix` also fails if `PromptStack::handle_key` (`stack.rs:227`) moves. None of those files are in the file list except as files this issue must not touch.

The width in the screen test is 80. The two-choice hint, with either settled clause, is one row at that width, and that row is the card's last line. `add` (`stack.rs:696-702`) passes the hint through `wrap_lines`, so a narrower peek can split `Ctrl+C deny and stop` across two painted rows. REQ-1 and REQ-2 name the row that holds the whole suffix. REQ-4 leaves a split suffix as `PromptStack` drew it. The issue adds no narrower test.

The implementation commit also adds one entry at the top of `docs/engineering-journal/LEARNINGS.md` and one at the top of `docs/engineering-journal/DECISIONS.md`. The issue's Notes section says what each entry contains. The test commit does not touch them.
