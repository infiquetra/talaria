# ADR-0010: The rewrite is Rust, with ratatui, crossterm and tokio

Status: `accepted`
Date: 2026-10-22
Accepted: 2026-10-23
Deciders: operator
Supersedes: ADR-0004 and ADR-0005
Affected components: every crate of the rewrite, continuous integration, distribution, the staffing
of terminal work

## Context

ADR-0004 chose Python and ADR-0005 chose Textual. By 2026-10-22 that client was the slowest path
measured. Keystroke latency while a reply streamed was 107 ms at the 95th percentile. Streaming used
28% of a core on average, and 48% of streaming time went to Textual's whole-screen layout pass.
Textual's own vendor has wound down.

A stack evaluation ([u11](../../../docs/handoff/2026-10-22-next-steps/research/u11-language-framework.md)) compared five stacks. Rust with
ratatui scored 81 out of 100, TypeScript with OpenTUI on Bun 78, Go with Bubble Tea 73, TypeScript
with Ink 69, and Python with Textual 56. The evaluation called the top two "within noise", and named
a head-to-head agent build as the evidence that would settle them.

That build ran on 2026-10-22 ([u16](../../../docs/handoff/2026-10-22-next-steps/research/u16-stack-bakeoff.md)). Four agents built the same
client against the live gateway: Sonnet 5.5 high and Haiku 5.5 xhigh, each in Rust and in
TypeScript. All four passed every acceptance item without help.

- **Fixes needed.** The TypeScript builds needed 3 and 6 fixes. The Rust builds needed 11 and 5.
  Six of the Rust Sonnet build's fixes were in the terminal layer, and the TypeScript builds had none
  there.
- **Runtime.** The Rust builds reached the gateway in about 15 ms, held about 5 MB and shipped a
  2.2 MB binary. The TypeScript builds took about 300 ms, held 104 to 122 MB and shipped a 75 MB
  binary.

## Decision

1. The rewrite is written in Rust (stable toolchain, edition 2024) with ratatui 0.30, crossterm 0.29
   and tokio. Exact versions are pinned in the workspace.
2. Only the terminal module of `talaria-tui` writes escape sequences or touches the terminal's
   modes. Everything else draws through its interface. Its first version is built by the strongest
   tier, starting from the Rust reference slice and the Codex technique it borrows. Any copied code
   keeps its Apache-2.0 notice.
3. Two ratatui interfaces are not used:
   - **Inline-viewport resizing after start-up.** It queries the cursor position, which crossterm
     cannot answer while the key reader runs.
   - **`insert_before` for committing history.** Its scroll-up sequence may not reach native
     scrollback. History is committed by newline scrolling inside a scroll region.
4. TypeScript with OpenTUI on Bun is the named fallback, starting from the TypeScript reference
   slice. Switch when either of these happens:
   - the inline renderer (unit P1-06) fails its acceptance twice at the top tier;
   - more than two other phase-1 units need a terminal-library workaround.

## Consequences

- Supersedes ADR-0004 (Python) and ADR-0005 (Textual). ADR-0002's rule survives: the domain core
  never imports the terminal library.
- The handoff's performance budgets are reachable with margin: launch to composer 200 ms, and
  40 MB at start.
- Hermes's TypeScript client cannot be reused. Reconnect, replay, heartbeat and server-request
  deadlines are re-implemented, and a scheduled contract-drift job keeps them in step.
- Workflow scripts run in an embedded JavaScript engine (`rquickjs`).
- Settings and fleet screens are built from fewer ready-made widgets than OpenTUI has.
- Builders need Rust fluency. The bake-off showed that Sonnet and Haiku have it, with friction
  concentrated in terminal code.

## Revisit when

- The switching conditions in decision 4 occur.
- Hermes publishes its TypeScript client as a supported, versioned package and OpenTUI reaches 1.0.
  Reconsider at the start of phase 2.
- A real-terminal benchmark shows that the start-up and memory differences cannot be perceived in
  daily use.
