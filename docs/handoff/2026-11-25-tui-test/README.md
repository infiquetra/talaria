# TUI bugs found by driving Talaria, 2026-11-25

These observations describe the build tested that morning. The work list records
the completed fixes, how they were checked, and which findings remain open.
[Validated terminal interface feedback](../../engineering-journal/QUEUED.md#terminal-interface-feedback-validated-on-november-25)

## How this was run

`tui-test` 0.1.1 drove `target/debug/talaria` from this checkout. That binary was
built at 09:21 on 2026-11-25, so it can lag uncommitted source edits made after
that. The session was 100×36 unless a resize is named. Talaria attached to the
local Hermes gateway (doctor reported Hermes 0.23.4; this Talaria was tested
against v0.23.1). No prompt was sent to a model. No setting was saved, no cron
job was created, and no session was deleted.

One empty conversation was opened. `/thinking` with no argument switched reasoning
to full; `/thinking collapsed` put it back. `/layout menu` turned the menu bar on;
`/layout bar` turned it off. `/quit` then left that empty conversation running in
Hermes.

The earlier code review is
[docs/handoff/2026-11-25-tui-usability/README.md](../2026-11-25-tui-usability/README.md).
This file is only what the live session showed.

## Bugs

### 1. The slash palette lists the wrong commands for `/j`

Typing `/j` showed `/journey` three times, then `/save`, `/review`, `/indicator`,
`/memory`, and `/init`. `/save`, `/review`, `/indicator`, and `/memory` do not
contain `j`. Local filtering keeps a name only when it contains the query
(`tier` in `crates/talaria-core/src/commands/palette.rs`). `merged_rows` then
appends the Hermes completion reply without that same check, and the three
`/journey` rows were still drawn.

### 2. PageDown and End do not move the slash palette

On `/`, Down moved the highlight from `/quit` to `/btw`, and 25 Downs reached
`/mcp`. PageDown and End left the highlight on `/quit`.

### 3. Escape closes the palette and leaves the slash in the composer

After `/`, Escape removed the list and restored the composer hints, and the draft
was still `/`. A second Escape left it there. Typing `t` afterwards produced `/t`
and opened the list again.

### 4. A `/plan` collision notice sits under every palette

Opening `/` always added this row above the status line:

`Hermes: slash command /plan unavailable — name taken by built-in; use /skill plan`

It does not name the built-in that took the name. It is gone once the palette
closes.

### 5. `/theme` dumps a long token report into the conversation

`/theme` wrote a source line for every theme element, then a "Contrast unknown"
list that wrapped in the middle of token names (`pager.footer` split across two
lines). The report stayed in the transcript above the composer. This matches the
queued note from 2026-11-24.

### 6. The session picker writes its truncation notice into the conversation

Opening `/resume` added this line to the transcript, and a second open added it
again:

`default: only the newest 100 earlier sessions are listed.`

The same sentence is the picker's own footer. In the transcript it has no subject
other than the profile name.

### 7. `/workflows` opens an empty frame

The frame had blank rows and one footer: `↑↓ select · enter open · esc close`.
There was no empty-state sentence and no list.

### 8. `/thinking` with no argument changes the view

The command hint is `collapsed|full`. Enter on `/thinking` alone switched the view
to full and printed:

`Thinking now shows in full in this view. Set [ui] reasoning = "full" in ~/.talaria/config.toml to keep it.`

`/thinking collapsed` switched it back. The first message stays in the transcript.

### 9. The menu bar paints on top of existing cells

`/layout menu` drew the bar over the transcript. At 100 columns the last heading
read `Helps.selection`: `Help` plus the theme-token text that was already on that
row. The open dropdown did the same. Conversation lines read through the menu, for
example `/context` on the same row as `Sessions` and `Fleet`. At 40×16 the bar
read `Talaria Menuf`. After a later full redraw the bar was clean. The dropdown,
while open, was not.

### 10. Kanban ignores Ctrl+C, Ctrl+Z, and a click on its close hint

On the board, Ctrl+C left the board up. Ctrl+Z left it up. Clicking the footer
text `Esc close` left it up. Escape closed it. The footer is
`h/l columns · j/k cards · Enter details · / filter · b boards · Esc close`.
Above that footer the only status was `live:25534`, with no label for what the
number counts. The board did say `Columns 1–4 of 8` and named the off-screen
columns.

### 11. Prose wraps in the middle of words

Seen while a screen was narrower than the line:

- The capabilities detail split `background` into `b` and `ackground`.
- The processes empty state split `output` into `o` and `utput`.
- With the menu bar narrowing the transcript, `charizard` split into `chariz`
  and `ard`.
- At 40 columns, `Skill` and `view` split the same way.

### 12. Action footers are cut off at 100 columns

The close key was past the end of the row, replaced by `…`, on MCP, cron, the
subagent list, and the changes review. Escape still closed each of those screens.
The MCP empty state also says `ctrl+n` while the cron footer says `Ctrl+N`.

### 13. The subagent title is the raw session id

`/agents` headed the screen with `Subagents` and the full `local-…` id. The id
used the width the title needed for a name.

### 14. Two settings rows do not say what is wrong

In `/config`, `Updates › pre update backup` displayed `false (not in the list)`.
Under the model value, `may need a restart` had no subject.

### 15. `/copy` with nothing to copy names reply 1

On a conversation with no assistant reply, `/copy` printed `No reply 1 to copy.`

### 16. Connectors does not say whether this profile is signed in

`/connectors` said that if the profile has no Nous sign-in, Hermes creates a free
guest identity, and offered `Enter continue`. It did not say whether this profile
was signed in. The providers screen, opened in the same session, described Nous
Portal as signed in.

### 17. At 40×16 the status row drops the model

The status line was only `approvals: manual`. The model name, which is on the
status row at 80 and 100 columns, was gone. Words in the transcript were split
as in bug 11. The composer still accepted typing.

## What opened and closed

Escape returned to the composer from the model picker, the effort scale (the scale
was visible; Escape was not rechecked after the menu stole a later attempt), the
session picker, doctor, profiles, settings, providers, model roles, MCP,
capabilities, memory, cron, connectors, fleet, subagents, processes, the changes
review, diff, and kanban.

Doctor reported 7 ok, 0 warnings, 0 failures, and attached in 59 ms. Ctrl+Z on the
composer, while no overlay was open, printed that this terminal has no job control
and named `/quit`.

## Not exercised

No model prompt, no settings save, no cron create, no destructive review action,
no vim mode, no theme change, and no second profile. Mouse was checked only by
clicking the kanban close hint. Confirm dialogs were not opened, so the 250 ms
confirm gate from the usability review was not seen live.
