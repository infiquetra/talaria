---
date: 2026-08-02
topic: hermes-tui-feature-inventory
scope: what the shipping Hermes terminal UI does, feature by feature, with a proposed verdict for each
status: draft — verdicts are proposals for the operator to confirm
source: Hermes Agent 7f4d15515 (2026-08-01)
---

# Hermes terminal UI feature inventory

[ADR-0003](../../platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md)
requires that Talaria's feature set be a decision the project makes rather than a residue of what was
convenient to translate. This is the input to that decision: what the shipping Hermes terminal UI
actually contains, organized by surface, with a proposed verdict on each.

**Status: the verdicts are proposals, not decisions.** They are one reader's recommendation and exist
to be argued with. A verdict becomes real when the operator confirms it.

## Method, and what this is not

Derived from Hermes Agent
[`7f4d15515`](https://github.com/NousResearch/hermes-agent/tree/7f4d155159e2a5d4098bb2f27d3fccb01ff84c3d)
(2026-08-01), the revision that is installed and running, by mapping the component tree and reading
the layout composition, the state stores, the command registry, and selected components in full.

**This is not a complete read.** `ui-tui/src` is 58,581 lines across 277 files. The structure, the
state shape, the command vocabulary, and the layout composition were read directly; most individual
components were sized and classified rather than read line by line. A feature listed here as small
may hide a hard problem, and the estimate that survives contact with the code is usually smaller than
the one that motivated the reading — that was the lesson of the last file this project measured.

Where the count matters, it is stated. Where it is a judgment, it says so.

## The shape of the screen

```text
┌──────────────────────────────────────────────────────────────┐
│  status rule (optional position: top)                        │
├────────┬────────────────────────────────────────────┬────────┤
│ ambient│  transcript  |  agents overlay  |  journey │ ambient│
│  rail  │  (one of the three occupies the pane)      │  rail  │
│ (left) │                                            │ (right)│
├────────┴────────────────────────────────────────────┴────────┤
│  prompt zone — composer, approval, clarify, progress         │
├──────────────────────────────────────────────────────────────┤
│  status rule (default position: bottom)                      │
└──────────────────────────────────────────────────────────────┘
```

Two structural facts worth carrying forward. The main pane is **exclusive** — the agents overlay and
the journey view replace the transcript rather than splitting with it, and the ambient rails hide
when either is open. And the whole shell runs in the alternate screen **except** in an inline mode
that deliberately skips it so the host terminal's own scrollback captures rows that scroll off the
top. That inline/alternate fork is a real design decision, not an implementation detail.

## A. Frame and chrome

| #   | Feature                                                                                                                                                                                                                                                                            | Size                          | Proposed verdict                                                                                                                                                                                  |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A1  | **Status rule.** Positionable top or bottom, carrying 20 fields: model, fast/priority tier, reasoning effort, session and turn timers, busy state, background-task count, live-session count (clickable), working-directory label, battery, usage, voice label, notice, focus view | part of `appLayout` 591       | **Keep, redesign.** Named by the operator as a target. The field list is the useful artifact — it is a survey of what a running agent's operator actually wants visible, arrived at by iteration. |
| A2  | **Busy indicator.** Four styles (kaomoji, emoji, ascii, braille), rotating verb, duration clock, per-style cadence, pre-computed frame widths so a style switch cannot shift layout                                                                                                | `appChrome` 859               | **Keep the mechanic, drop the default.** The reusable knowledge is the width pre-computation and the bounded-cadence rule; the four personalities are Hermes product character.                   |
| A3  | **Alternate screen versus inline mode.** Inline skips the alternate screen so host scrollback keeps history                                                                                                                                                                        | `appLayout`                   | **Keep as an explicit decision.** This interacts directly with transcript virtualization and must be settled before the transcript is built, not after.                                           |
| A4  | **Ambient rails, ambient dock, active widget slot.** Side panels fed by a widget SDK under `sdk/host`                                                                                                                                                                              | `sdk/` 671 + `widgetGrid` 267 | **Drop for the first version.** A plugin surface before there is a product to plug into. Revisit once the fleet views exist.                                                                      |
| A5  | **Branding / splash**                                                                                                                                                                                                                                                              | `branding` 562                | **Drop.** 562 lines of identity for a different product.                                                                                                                                          |

## B. Transcript and message rendering

| #   | Feature                                                                                                   | Size                                      | Proposed verdict                                                                                                                                                           |
| --- | --------------------------------------------------------------------------------------------------------- | ----------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| B1  | **Markdown rendering**                                                                                    | `markdown` 1178 + `streamingMarkdown` 166 | **Keep.** Check first against what the selected framework already provides — this is the single largest candidate for being replaced by a built-in rather than re-encoded. |
| B2  | **Thinking and reasoning display.** Separate from assistant text; `showReasoning` toggles it              | `thinking` 1237                           | **Keep.** The second-largest component in the tree, which is itself the finding: displaying reasoning well is a substantial problem, not a formatting detail.              |
| B3  | **Streaming assistant text**                                                                              | `streamingAssistant` 118                  | **Keep.** Small only because `turnController` holds the buffering.                                                                                                         |
| B4  | **Message line, density, sections, details mode.** Compact mode, per-section collapse, a details override | `messageLine` 311 + `accordion` 58        | **Keep.** Density control is what makes a long transcript usable and it is cheap.                                                                                          |
| B5  | **Inline diffs**                                                                                          | `uiStore.inlineDiffs`                     | **Keep.**                                                                                                                                                                  |
| B6  | **Paste collapse.** Large pastes fold to a line count, thresholds configurable in lines and characters    | `uiStore` + `paste.collapse` RPC          | **Keep.** Very high value per line, and the gateway already has the method.                                                                                                |
| B7  | **Queued messages.** Input submitted while busy, shown pending                                            | `queuedMessages` 64                       | **Keep.**                                                                                                                                                                  |
| B8  | **Todo panel**                                                                                            | `todoPanel` 93                            | **Keep with changes.** Overlaps the Kanban surface Talaria wants; do not build both.                                                                                       |
| B9  | **Loaders, help hints, spinners**                                                                         | `loaders` 172 + `helpHint` 68             | **Keep.**                                                                                                                                                                  |

## C. Composer and input

| #   | Feature                                                                                                                                                            | Size                   | Proposed verdict                                                                                                                                                                                                                                                               |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| C1  | **Text input.** History cycling with arrow-key fall-through, input-method and dead-key handling for Telex and comparable input methods, bracketed paste, multiline | `textInput` 1555       | **Keep the behaviors; expect this to be the hardest re-encode in the project.** The largest single component in the tree. Its comments record input-method correctness problems that are invisible until someone types Vietnamese into your prompt. Budget for it accordingly. |
| C2  | **Masked prompt** for secret entry                                                                                                                                 | `maskedPrompt` 41      | **Keep. Required.** The protocol's `secret.request` and `sudo.request` are unanswerable without it, and ignoring them hangs the agent rather than degrading the display.                                                                                                       |
| C3  | **Keybinding and input routing**                                                                                                                                   | `useInputHandlers` 694 | **Keep the routing shape, re-derive the bindings.** Talaria's command set differs enough that inheriting the map is not useful.                                                                                                                                                |
| C4  | **Mouse tracking, focus view**                                                                                                                                     | `uiStore`              | **Keep.**                                                                                                                                                                                                                                                                      |

## D. Overlays and blocking prompts

The overlay store holds **18 keys**. They divide cleanly into three groups.

**Required by the protocol — ignoring them hangs the agent.**

| #   | Overlay                                 | Proposed verdict                                                                                                                                                 |
| --- | --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1  | `approval`, `clarify`, `secret`, `sudo` | **Keep, all four.** These are round trips split across directions; the inbound half is a request with an empty payload and the outbound half carries the answer. |
| D2  | `confirm`                               | **Keep.**                                                                                                                                                        |

**Talaria's stated differentiator — keep and extend well past what Hermes does.**

| #   | Overlay                                                       | Size                        | Proposed verdict                                                                                                                                                                                                                      |
| --- | ------------------------------------------------------------- | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D3  | **Agents overlay** — sub-agent visibility, with history index | `agentsOverlay` 976         | **Keep and extend.** Sub-agent visibility is the project's stated reason to exist, six protocol event types already carry it, and Hermes renders it as a modal that replaces the transcript. Talaria wants it alongside, not instead. |
| D4  | **Active session switcher**                                   | `activeSessionSwitcher` 917 | **Keep and extend.** Closest thing Hermes has to a fleet view. Constrained by being a single-profile process — see below.                                                                                                             |
| D5  | **Model picker**                                              | `modelPicker` 710           | **Keep.**                                                                                                                                                                                                                             |
| D6  | **Pager**                                                     | —                           | **Keep.**                                                                                                                                                                                                                             |

**Hermes product surfaces Talaria has no reason to carry.**

| #   | Overlay                                                       | Size                                                                                                     | Proposed verdict                                                                                                             |
| --- | ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| D7  | `billing`, `subscription`, `topup`                            | 950 + 1024 = **1,974 lines** of overlay, plus 598 more in the `topup` and `subscription` command modules | **Drop.** Commercial surfaces for a product Talaria is not.                                                                  |
| D8  | `petPicker`, pet sprite, pet flash                            | 187 + 93 + 28 + `usePet` 313                                                                             | **Drop.**                                                                                                                    |
| D9  | `widget`, grid test overlay, grid streams demo, FPS overlay   | 318 + 364 + 30                                                                                           | **Drop.** Development and demonstration surfaces.                                                                            |
| D10 | **Journey** — a star-map visualization over `learning.frames` | `journey` 595 + starmap palette                                                                          | **Defer, do not drop.** Genuinely novel and it reads a real gateway method. It is also not what a fleet console needs first. |
| D11 | `skillsHub`, `pluginsHub`                                     | 301 + 241                                                                                                | **Defer.** Read-only versions are cheap later; management surfaces are not first-version work.                               |

## E. Command surface

Hermes registers **64 slash commands** across eight modules (`core`, `ops`, `session`, `topup`,
`subscription`, `wake`, `debug`, `setup` — 3,135 lines in total):

```
agents background battery branch browser busy clear compress copy density details
fast focus fortune heapdump help history image indicator journey logs mem model
mouse paste personality pet plugins prompt queue quit reasoning redraw reload
reload-mcp reload-skills replay replay-diff retry rollback save sessions setup
skin skills status statusbar steer stop subscription terminal-setup theme
theme-info title tools topup undo update usage verbose voice wake widgets-reload
yolo
```

**Proposed verdict: keep roughly a quarter.** A first version needs the ones that change what you can
see or what the agent is doing — `agents`, `sessions`, `model`, `status`, `statusbar`, `theme`,
`density`, `details`, `reasoning`, `steer`, `stop`, `retry`, `queue`, `clear`, `help`, `quit` — and can
defer or drop the rest. Note that `replay` and `replay-diff` already exist upstream, which is worth a
look before Talaria builds its own replay command, and that `fortune`, `personality`, and `pet` are
character rather than capability.

## F. Theme and colour

| #   | Feature                                                                                                                                                                                                                                            | Size                                                        | Proposed verdict                                                                                                                                                                                                                                                                                                                                 |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| F1  | **Terminal background detection.** An OSC-11 probe; the finding that xterm.js-based hosts and tmux both answer `#000000` regardless of the real background; an OSC-10 foreground tiebreaker for exactly that case; a platform-specific last resort | ~310 lines inside `createGatewayEventHandler` + `color` 325 | **Keep the knowledge; check the framework first.** This is the accumulated bug history of one genuinely hard question, and it is the highest-value read in the Hermes tree for anyone building a themed terminal application. It may also be partly solved by whichever framework Talaria selects — establish that before re-encoding any of it. |
| F2  | **Boot theme cache.** Persists the resolved theme and seeds the environment on the next start so the first frame is not wrong                                                                                                                      | `themeBoot` 209                                             | **Keep.** Small, and it fixes a flash that is otherwise unavoidable.                                                                                                                                                                                                                                                                             |
| F3  | **Truecolor forcing**                                                                                                                                                                                                                              | `forceTruecolor` 60                                         | **Keep.**                                                                                                                                                                                                                                                                                                                                        |
| F4  | **Skins**, pushed from the gateway in `gateway.ready`                                                                                                                                                                                              | —                                                           | **Keep the seam, re-derive the skins.** The handshake already carries a skin, so a client that ignores it is discarding state the server took the trouble to send.                                                                                                                                                                               |

## What Hermes's terminal UI does not do, that Talaria intends to

These are the divergences. They are not gaps in Hermes — they follow from Hermes's terminal UI being
one operator at one session, which is not what Talaria is for.

1. **It is a single-profile process.** Hermes's own client says so in a comment and acts on it: a
   wake-word event enrolled by another profile is refused and the user is told to switch, rather than
   routed. Talaria wants to see across profiles at once, which
   [ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md)
   expects to mean one gateway connection per profile.
2. **The agents view is modal.** Sub-agent visibility replaces the transcript rather than living
   beside it. For a project whose stated purpose is sub-agent visibility, that is the first thing to
   change.
3. **There is no always-visible exception signal.** Errors are notices and transcript lines. The
   product ideation asks for a strip that is always present.
4. **There is no merged multi-source view.** One session's events, one pane.
5. **Nothing drives a pane manager.** Hermes's terminal UI occupies the terminal it was started in.

## The one recommendation this inventory produces

**Choose the validation gate's vertical slice from this list rather than from a generic renderer
stress list.** The gate as currently written in the engineering journal exercises streaming, resize,
Unicode, and malformed events — all necessary, none of them a product. If the slice is instead
"status rule plus transcript plus composer plus one blocking prompt, under a replayed session," it
proves the same renderer properties **and** leaves a prototype standing. Otherwise the gate gets built,
measured, and thrown away, and the prototype starts from nothing.

The status rule is the natural first surface for a second reason: it is the operator's named target,
it is small, and its 20 fields are a ready-made list of everything the domain state has to expose —
which makes it a test of the framework boundary in
[ADR-0002](../../platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md)
as well as of the renderer.

## Verdicts re-checked at the pinned checkout revision `7095e23eb` (2026-08-17, v0.4 unit U1)

The v0.4 fleet turn re-pinned the protocol read to `7095e23eb` and drove the affected surfaces
live ([evidence](2026-08-17-v0-4-topology-verification.md)). The rows the fleet turn touches,
re-checked against that revision:

| Row | Verdict at `7095e23eb` | What changed since `7f4d15515` |
| --- | --- | --- |
| D1 (`approval`, `clarify`, `secret`, `sudo`) | **Keep — verdict stands, scope grows.** | The blocking-bridge family is now eight request kinds (`terminal.read`, `preview.read`, `window.read`, `mcp.setup` beside the four here). Talaria cards the original four (R14); the GUI-only four are named on the fleet registry row and not queued — a kind Talaria cannot render anywhere is not a resolvable queue item (KTD2). Approvals gained a synthesized `request_id` and an aimed `approval.respond`; the payloads carry no start stamp, so every wait age is an observation floor (KTD12). |
| D2 (`confirm`) | **Keep — now load-bearing.** | Operator ruling OP2 makes a confirm dialog the gate on activating any live session Talaria does not drive, because activation — and `prompt.submit` itself — silently rebinds the session's event stream (verified live; the displaced client gets nothing, mid-turn included). |
| D4 (active session switcher) | **Keep and extend — "extend" is now specified.** | `session.active_list` is confirmed live: rows carry `{current, id, last_active, message_count, model, preview, session_key, started_at, status, title}`, statuses `waiting`/`starting`/`working`/`idle`, no kind on a waiting row, no transport-identity field (hence OP2's confirm-before-steal). The v0.4 registry + needs-you queue is the extension; Hermes's modal switcher remains the shape to surpass, not to copy. One inherited caveat: `waiting` never covers approval-blocked sessions (they report `working`) — a switcher trusting `waiting` as "needs you" under-reports exactly the kind that matters most. |
| D10/D11 territory — `approval.pending`, `approval.received` | **New methods, adopt `approval.pending` with its guard.** | New at `7095e23eb`; `approval.pending` warms the target session's agent build as a side effect, so it fires only at sessions whose agent is known live (KTD11), and its absence on older gateways is probed by name (`-32601`), never assumed. |
| E (`sessions` command) | **Keep — unchanged.** | Talaria's `/sessions` and the new `/needs` drill-down are the command-surface counterparts of D4's verdict. |
| "does not do" item 1 (single-profile process) | **Re-affirmed — and it holds at every revision examined, so the checkout-versus-serving-process distinction does not bite here.** | `session.active_list` enumerates only in-process sessions; other profiles' gateway processes are structurally invisible. One connection per configured profile endpoint (KTD1/OP1) remains the only fleet topology that works. |

No verdict above reverses an original proposal; D2's and D4's grow teeth. The inventory's other
rows (A, B, C, F, D3, D5–D9) are untouched by the fleet turn and were not re-derived.
