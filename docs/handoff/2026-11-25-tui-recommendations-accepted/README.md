# Accepted TUI recommendations — 2026-11-25

Status: `accepted`. Jeff accepted all 19 recommendations from the afternoon review.

The observations and the live session are in
[the morning review](../2026-11-25-tui-usability/README.md) and
[the terminal session](../2026-11-25-tui-test/README.md). What already landed, and
how it was checked, is in
[the November 25 validation record](../../engineering-journal/QUEUED.md#terminal-interface-feedback-validated-on-november-25).

## Accepted

| # | Accepted behavior |
| --- | --- |
| 1 | Esc closes the overlay. Ctrl+C interrupts the turn and leaves the overlay open. Ctrl+Z suspends. All three work from every overlay, including a text field. |
| 2 | Hermes palette rows use the same name filter as local commands, and a command appears once. Home, End, Page Up, and Page Down move that list. |
| 3 | Escape closes the palette. A draft that is only `/` is cleared. A longer draft stays, and the list stays closed until the draft changes. |
| 4 | A `/plan` name collision is one transcript notice that names both commands. It is not pinned under every palette. |
| 5 | `/theme` opens the picker. `/theme report` prints the token and contrast list. |
| 6 | "Only the newest 100 sessions" stays inside the session picker and names sessions. It is not copied into the conversation. |
| 7 | The menu bar and each dropdown row clear the full width they cover before drawing. |
| 8 | Transcript and detail text wrap on word boundaries. |
| 9 | A footer that does not fit drops extra hints and keeps Esc. Chords use one capitalization. |
| 10 | The `/thinking` hint says that no argument toggles collapsed and full. The notice says which way it went. |
| 11 | `/copy` with no reply says there is no reply to copy. |
| 12 | A settings value missing from its list, and a restart warning, name the setting. |
| 13 | Connectors states whether this profile is signed in before mentioning a guest identity. |
| 14 | The subagent screen titles the session by name. The raw id is on a detail row. |
| 15 | The kanban live counter is labeled. A click does the same thing as the key it sits on when mouse navigation is on. |
| 16 | `?` is not the help key on the live session, because it is typed into the draft. Help is a separate key and lists the actions for the current focus: composer, card, queue, or vim normal. An idle status row with room shows submit, interrupt, sessions, models, and help. |
| 17 | The 250 ms confirm delay stays. "y to confirm" appears only after the delay. |
| 18 | A capabilities banner clears on Esc, or when the screen says any key dismisses it. The text is copied into the session notice log. |
| 19 | At 40 columns the status row keeps the model name, or names what was hidden. |

## Verification in the updated client

The local project suite passed 5,659 tests with 83.93% line coverage and
skipped 151. The status-command shutdown test again reported an open output
pipe after its assertions passed. In a terminal connected to Hermes, Ctrl+C
preserved a session search, Ctrl+Z followed by `fg` restored it, and the live
command catalogue produced one notice naming both ways to invoke `plan`.

| Items | Behavior exercised | Regression coverage |
| --- | --- | --- |
| 1 | Interrupt a running session while editing a session search, then keep typing. Setup can interrupt its own configuration agent and remain open. | [Terminal and gateway test](../../../crates/talaria-cli/tests/integration/working_row.rs) · [Setup lifecycle](../../../crates/talaria-core/tests/integration/configure_state.rs) |
| 2–4 | Filter and page through commands, escape a bare slash or a longer query, refresh a catalogue that reports a collision, then switch sessions. | [Palette transitions](../../../crates/talaria-core/tests/integration/commands_palette.rs) · [Transcript and draft effects](../../../crates/talaria-app/tests/integration/palette_routing.rs) |
| 5–7 | Open the theme selector and request its report explicitly. Close the session picker and check the transcript. Paint the menu over occupied cells. | [Theme commands](../../../crates/talaria-app/tests/integration/theme_command.rs) · [Sessions](../../../crates/talaria-app/tests/integration/session_picker_keys.rs) · [Menu](../../../crates/talaria-tui/tests/integration/menu_bar.rs) |
| 8–9 | Wrap prose at different widths and retain Escape as competing footer actions drop away. | [Text wrapping](../../../crates/talaria-tui/tests/integration/history_text.rs) · [Settings](../../../crates/talaria-tui/tests/integration/settings_render.rs) · [Scheduled jobs](../../../crates/talaria-tui/tests/integration/cron_render.rs) · [Changes review](../../../crates/talaria-tui/tests/integration/review_render.rs) |
| 10–13 | Toggle reasoning display, copy from an empty conversation, inspect missing values and restart warnings, and read the connector consent for each sign-in state. | [Reasoning](../../../crates/talaria-core/tests/integration/commands_local_layout.rs) · [Copy](../../../crates/talaria-app/tests/copy_command.rs) · [Settings](../../../crates/talaria-core/tests/integration/settings_state.rs) · [Connectors](../../../crates/talaria-core/tests/integration/connectors_rows.rs) |
| 14–15 | Read a session's name and ID in the subagent view. Click the displayed board and detail keys, including individual directions. In a terminal, click Filter, type a query, cancel and close the board. | [Subagent details](../../../crates/talaria-tui/tests/integration/subagent_view.rs) · [Kanban pointer targets](../../../crates/talaria-tui/tests/integration/kanban_view.rs) · [Terminal mouse test](../../../crates/talaria-cli/tests/integration/cli_process.rs) |
| 16–17 | Type a question mark, open contextual help and restore focus. Check the model confirmation before and after its 250 ms guard. | [Focus and key routing](../../../crates/talaria-cli/src/run.rs) · [Question pane](../../../crates/talaria-cli/tests/integration/effort_scale_card.rs) · [Confirmation rendering](../../../crates/talaria-tui/tests/integration/model_picker_render.rs) |
| 18–19 | Dismiss capabilities feedback and recover its text from the conversation. Display an active model with warnings and a notice at 40 columns. | [Capabilities state](../../../crates/talaria-core/src/capabilities/state.rs) · [Capabilities session notices](../../../crates/talaria-app/src/app.rs) · [Compact status](../../../crates/talaria-tui/tests/integration/status_inline.rs) |

## Leave as they are

Doctor, the model picker, profiles, fleet, cron, memory, and the effort scale.
Ctrl+Z on the composer with no overlay open explains a terminal with no job
control. `NO_COLOR`, monochrome selection, reduced motion, paste that does not
submit, and scrollback for finished text stay.
