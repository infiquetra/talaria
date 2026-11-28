# TUI usability review — handoff, 2026-11-25

These observations describe the source reviewed that morning. The work list records
the completed fixes, how they were checked, and the remaining design questions.
[Validated terminal interface feedback](../../engineering-journal/QUEUED.md#terminal-interface-feedback-validated-on-november-25)

Reviewed from the input router, the draw paths, and the existing tests on 2026-11-25.
A live terminal session was not driven. Line numbers below are from that tree; if they
drift, the function names still locate the behavior.

The repeated job is composing a prompt, watching a turn, answering an approval or
clarification card, and reading finished output. Finished text goes to the terminal's
own scrollback. The live region keeps the draft, the working row, and any open card.
The optional workspace holds the fleet, workflows, and kanban. Pickers, settings, and
the other tools are overlays that take the screen until they close.

## What holds up

- Keys, paste, mouse, and resize enter through `terminal_input_actions` in
  `crates/talaria-cli/src/run.rs`. Key release is ignored. A repeated interrupt key is
  dropped. Bracketed paste inserts text and does not submit.
- An unbound character in the composer is typed. Capabilities, connectors, and kanban
  comments switch to a text mode so letters edit the field.
- An empty composer yields to a prompt card. The session picker, review, fleet launch
  dialog, and rewind overlay each take every key while open.
- Model confirm ignores `y` and Enter for 250ms after focus (`confirmation_key` in
  `crates/talaria-cli/src/run.rs`). Profiles accept only `y`. Leaving setup takes two
  Ctrl-C presses within two seconds.
- Drop and panic restore raw mode, mouse capture, bracketed paste, the alternate
  screen, the sync flag, and the cursor (`restore_sequence` and `restore_on_panic` in
  `crates/talaria-tui/src/terminal.rs`). Suspend releases the terminal and takes it
  back. If the stop signal is ignored, a notice says so.
- A non-empty `NO_COLOR` wins before `COLORTERM` (`crates/talaria-tui/src/theme/detect.rs`).
  Monochrome keeps selection with reverse and bold. Reduced motion turns off the
  activity sweep and slows redraws to one-second ticks (`crates/talaria-tui/src/activity.rs`).
- The fleet, workflow footer, command palette, and queue label shorten as the screen
  shrinks. Settings below 40 columns or 12 rows says so and keeps Esc
  (`crates/talaria-tui/src/settings.rs`).

## Findings

### Ctrl-C and Ctrl-Z die on the kanban board

Any key with Ctrl, Alt, or Super is discarded before the board sees it. Esc still
leaves the board. Interrupt and suspend do not work there, including while a comment
or filter field is open.

```rust
// crates/talaria-cli/src/run.rs, terminal_input_actions, kanban branch
if key.kind == KeyKind::Release
    || key.mods.ctrl
    || key.mods.alt
    || key.mods.super_key
    || key.mods.other
{
    return vec![];
}
```

### Ctrl-C means a different action on each overlay

| Surface | Ctrl-C |
| --- | --- |
| Session picker | Closes the overlay (`PickerInput::CtrlC`). |
| Cron, model roles, settings | Interrupts the running turn and leaves the overlay up (`UserAction::Interrupt`). |
| Setup (`configure`) | First press exits setup. A second press within two seconds quits Talaria. |
| Composer | The bound interrupt key asks for a second press (`UserAction::InterruptKey`). Ctrl-C interrupts immediately. |
| Kanban | Discarded. |

`UserAction::Interrupt` stops the turn or cuts off client voice. It does not close
the overlay (`crates/talaria-app/src/app.rs`).

### A mistyped key clears the capabilities message

`route_capabilities` returns nothing for an unbound key
(`crates/talaria-tui/src/capabilities/mod.rs`). The router turns that into
`CapInput::DismissMessage`. `DismissMessage` clears the banner unless a dialog is
open or the message is marked persistent
(`crates/talaria-core/src/capabilities/state.rs`). That banner is often the only
copy of a failure.

### The confirm delay is invisible

For 250ms after a confirm dialog focuses, `y`, Enter, `n`, and Esc are ignored
(`confirmation_key`). Nothing on screen says the key was dropped.

### The live session does not teach its keys

Settings and model roles open help with `?`. The conversation status line shows
connection, model, tokens, and notices, and it drops the lowest-priority segments
when the row is narrow (`plan` in `crates/talaria-tui/src/status/segments.rs`). It
does not show submit, interrupt, sessions, or help. The menu bar can be off. The
composer does name the send, newline, and transcript keys that fit
(`docs/terminal-ui.md`).

### Pointer input covers only some overlays

Mouse can be turned off. Hit testing exists for kanban, workflows, settings,
configure choices, and the pickers (`draw_navigation_overlay`). Cron, memory,
capabilities, connectors, model roles, providers, doctor, and MCP use `draw_overlay`
without pointer routing. A click in those screens does nothing.

### Timed notices are a single replaceable slot

`ttl` notices expire (`NoticeStore::expire` in `crates/talaria-core/src/session/notices.rs`).
A newer notice replaces the one on the status row. Scrollback cells keep many
failures. A status banner or a capabilities banner can vanish before it is read.

### Several keys ignore who has focus

Ctrl-T toggles the task list even while a card is focused (`Action::ToggleTasks` is
resolved before the card takes the key). The approval toggle is resolved again after
the card has already had a chance at the key. The priority chain in
`terminal_input_actions` is where these exceptions live, including vim normal mode,
the effort scale, and agent-back.

### There is no linear mode for a screen reader

Finished scrollback is plain text, which helps after a turn commits. The live
viewport and full-screen overlays are not a linear reading order. Theme contrast
and monochrome markers are real. They are not a screen-reader path.

## Recommendations

Do these in order. Each one should keep the behavior listed under "What holds up."

1. **Give every overlay the same three chords.** Esc closes the overlay. Ctrl-C
   interrupts the turn and leaves the overlay open. Ctrl-Z suspends. Text fields
   keep printable keys and still honor those three chords. Kanban is the first
   place this fails. Acceptance: from the kanban board, a comment field, cron,
   model roles, settings, and the session picker, those three chords do the same
   three things.

2. **Keep error text until the user dismisses it.** Clear a capabilities banner on
   Esc, or when the screen explicitly says any key dismisses it. Copy the text into
   the session notice log when it clears. Apply the same rule to status notices that
   expire or get replaced. Acceptance: an unbound letter on the capabilities list
   leaves the banner in place, and the text is still recoverable after the banner
   closes.

3. **Show the confirm gate.** Keep the 250ms delay. Put "y to confirm" in the footer
   only after the gate opens. Acceptance: a `y` in the first 250ms does not confirm,
   and the footer explains why.

4. **Add conversation help in the form settings already uses.** `?` on the live
   session lists the actions for the current focus: composer, card, queue, or vim
   normal. When the status row has room and the session is idle, show submit,
   interrupt, sessions, models, and help. Acceptance: `?` from the composer and from
   an open card shows a different list, and Esc returns to the same focus.

5. **Make pointer behavior match the keyboard on every list, or say the pointer is
   inactive.** A screen that accepts arrows and Enter accepts a click on the same
   row. Until it does, the footer says the pointer is off for that screen.
   Acceptance: cron, memory, capabilities, connectors, model roles, providers,
   doctor, and MCP either hit-test or say the pointer is off.

6. **Keep a recovery line whenever a layout becomes unusable.** Settings already
   names its minimum size and keeps Esc. Views that hide the pull request, the
   model, or the command description say so in the footer once. Acceptance: at
   80×24 and at 40 columns, each of those views still shows a way out and names
   what a wider screen would show.

7. **Lock the focus rules with a table.** Rows: composer, vim normal, prompt card,
   confirm, text field, overlay. Columns: letter, paste, Enter, Esc, Ctrl-C, Ctrl-Z.
   Test that table when `terminal_input_actions` changes. Screen snapshots do not
   catch this.

8. **Leave new motion behind the existing reduced-motion switch.** New activity
   indicators render a static glyph when motion is reduced, as the working row
   already does.

## Where to start

| Concern | File |
| --- | --- |
| Key priority | `crates/talaria-cli/src/run.rs` (`terminal_input_actions`, `confirmation_key`) |
| What interrupt does | `crates/talaria-app/src/app.rs` (`UserAction::Interrupt`) |
| Capabilities banner | `crates/talaria-tui/src/capabilities/mod.rs`, `crates/talaria-core/src/capabilities/state.rs` |
| Status row | `crates/talaria-tui/src/status/segments.rs` |
| Notices | `crates/talaria-core/src/session/notices.rs` |
| Terminal restore | `crates/talaria-tui/src/terminal.rs` |
| Motion | `crates/talaria-tui/src/activity.rs` |
| Narrow settings | `crates/talaria-tui/src/settings.rs` |
| User-facing key notes | `docs/terminal-ui.md` |
