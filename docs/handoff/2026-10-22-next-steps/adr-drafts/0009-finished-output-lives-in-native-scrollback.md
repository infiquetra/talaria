# ADR-0009 (draft): Finished output lives in the terminal's native scrollback

Status: `proposed`
Date: 2026-10-22
Deciders: operator
Affected components: rendering, the transcript, overlays, performance budgets

## Context

Talaria renders its transcript as retained widgets on the alternate screen. ADR-0006 bounded the
widget count, height and apply latency of that design. On 2026-10-22 its gate failed (a growing table's
99th-percentile apply was 56.5 to 66.4 ms against a 50 ms ceiling), 48% of streaming time was the
framework's whole-screen layout pass, bytes per frame grew with the message, and the terminal's own
select-to-copy never worked in the Talaria pane.

The fastest clients write finished output into the terminal's scrollback once and redraw only a small
live region: OpenAI Codex (`insert_history.rs`), Claude Code's default renderer, pi-tui's main-screen
mode, OpenTUI's split-footer mode, and Hermes's own `--native` mode.

## Decision

1. The transcript is inline: a cell is committed to native scrollback when it is final and never
   redrawn.
2. Only a live region at the bottom is redrawn: the streaming tail, the status line, the composer,
   and transient notices. Frames are wrapped in synchronized output when the terminal supports it,
   negotiated at startup.
3. Full-screen views (fleet, subagents, pickers, settings, diff review, a transcript pager) take the
   alternate screen temporarily and restore the inline view when closed.
4. Mouse capture is off by default so the terminal's own selection, copy and search work.

## Consequences

- ADR-0006's ceilings, the mounted-widget cap and the condensation of older lines disappear.
- Re-reading a long session uses the terminal's scrollback or the pager overlay, not an in-app
  scrolled transcript.
- Switching the foreground session prints the new session's recent history into scrollback; side-by-side
  sessions are the multiplexer's job.
- Inline mode inside multiplexers needs care (Codex keeps a separate strategy for Zellij); test in
  Herdr, tmux and Zellij.

## Revisit when

- A daily-driver workflow needs a persistent multi-pane layout that a multiplexer cannot provide.
