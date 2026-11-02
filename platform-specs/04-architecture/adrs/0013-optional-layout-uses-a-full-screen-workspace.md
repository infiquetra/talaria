# ADR-0013: Optional layout uses a full-screen workspace

Status: `accepted`
Date: 2026-11-01
Accepted: 2026-11-01
Deciders: operator
Supersedes: none (amends ADR-0009 for the opt-in workspace)
Affected components: terminal modes, transcript rendering, the pager, layout, view preferences, input, status

## Context

ADR-0009 makes inline rendering with native scrollback the default. A fixed top menu and a right
work panel beside the conversation require a different rendering mode. The operator's standing
rule is: "The interface is good, clean and light; preserve that in future updates."

The operator accepted the layout study's Option A on 2026-11-01. The approved record is issue #473,
comment 6569001361, Decisions 1–8. It accepts an optional full-screen workspace while preserving
the existing inline experience when both additions are off. This record documents that decision;
it does not implement the layout or the voice features.

## Decision

1. Adopt Option A: a one-row top menu, `Talaria   Session   Model   View   Voice   Settings   Help`,
   and a compact right work panel with 32 content columns and a one-column divider. Option B is
   not adopted.
2. Both additions are off by default and independent. Startup configuration is `[ui]`
   `work_panel = false` and `menu_bar = false`. These are client-view preferences, not gateway-session
   or voice-backend state. With both off, the existing appearance and behavior remain unchanged.
   Alt+D, Alt+I and the `/layout` commands change the current view without rewriting configuration.
3. The panel hosts these contents in order:
   - The attention count.
   - The goal, the running turn and the verdict. Only an explicit verdict is shown; otherwise use
     `pending` or `unknown`. A completed checklist never counts as success.
   - Tasks from the existing checklist, with one visible copy. Ctrl+T expands or collapses it
     wherever it is hosted.
   - Agents: waiting requests first, then running, queued and a short summary of recent completions.
     Model and effort come only from an authoritative source; unavailable values are unknown.
     Alt+S keeps the full subagent controls and Alt+A keeps next-waiting-request navigation.
   - Overflow is counted as `+N more`. There is no silent scrolling and there are no progress bars.
4. Alt+D shows or hides the panel. Alt+I shows or hides the menu and focuses it when shown.
   `/layout panel`, `/layout bar` and `/layout menu` are the keyboard fallbacks. Both keys are
   configurable; report a clash with the effective keymap instead of silently shadowing an action.
   Esc in the menu never interrupts a running turn. Plain letters reach the menu only while it has
   focus.
5. The panel docks only at a terminal width of at least 112 columns and height of at least 28 rows.
   Below either threshold it opens on demand as an overlay. Esc dismisses it; only an explicit
   panel action reopens it. Below 16 rows neither addition reserves space. Below 60 columns the
   menu collapses to a single `Menu` entry.
6. Voice phase, read-aloud state and target appear only in the existing bottom status row, never
   in the menu or the panel. The voice work in issues #426, #427 and #428 follows this rule.
7. Inline rendering with native scrollback remains the default. When either the panel or the menu
   is on, Talaria enters an opt-in full-screen workspace and draws the conversation itself.
   Turning both off restores the inline view, subject to these conditions:
   - The workspace and transcript pager (Ctrl+O) share one transcript renderer and the same tests
     for both views. Implementation starts from the pager.
   - Nothing that finishes while the workspace is on is lost. On return to the inline view, text
     that finished during the workspace is committed to native scrollback once.
   - Terminal mode changes and escape sequences remain in the terminal module. Mouse capture
     stays off by default.
   - Verification covers Herdr, tmux and Zellij, as required by ADR-0009.
8. This record is ADR-0013, the next free number at the approved baseline. ADR-0009's status becomes
   `accepted; amended by ADR-0013`. The closing pull request for issue #473 delivers this record;
   implementation belongs to separate build cards.

## Consequences

- The default stays clean and light: enabling either addition is an explicit choice, and turning
  both off restores the existing inline conversation.
- While the workspace is on, the terminal's own scrollback and search do not cover the live
  conversation. Multiplexer pane reads see only the screen. The operator accepts this cost.
- Sharing the pager's transcript renderer avoids a separate rendering implementation for the
  workspace. Both views must pass the same rendering tests.
- Completed text needs once-only custody across workspace entry and return to native scrollback;
  switching modes must neither lose output nor commit it twice.
- Menu, panel and voice indicators have fixed homes. Tasks and subagent activity reuse their
  existing state and controls instead of introducing duplicate state or a separate voice surface.

## Revisit when

- The accepted loss of native scrollback/search or screen-only multiplexer reads makes the
  optional workspace unsuitable for daily use.
- The shared pager renderer or once-only scrollback commit cannot satisfy correctness or
  performance requirements in Herdr, tmux and Zellij.
- The operator changes the clean-and-light guardrail or the approved layout decisions.
