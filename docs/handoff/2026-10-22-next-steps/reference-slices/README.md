# Reference slices

Status: `reference` — working prototypes from the 2026-10-22 stack bake-off. Read them; do not
extend them.

Two agents each built the same minimal Hermes client in about 20 minutes against the live gateway:
one in Rust, one in TypeScript. Both attach through the supported ticket path, create a session,
stream replies inline, show tool cells with diffs, answer approvals, interrupt, detach, and record
and replay frames. The bake-off and its results are in
[../research/u16-stack-bakeoff.md](../research/u16-stack-bakeoff.md). The language decision they
informed is in [../05-architecture.md](../05-architecture.md) §Stack.

| Folder | Stack | Built by | Offline tests |
|---|---|---|---|
| [rust/](rust/) | Rust, ratatui 0.30, crossterm 0.29, tokio, tokio-tungstenite 0.26 | Sonnet 5.5, high effort | 10 (`cargo test --release`) |
| [typescript/](typescript/) | TypeScript on Bun 1.3, `@opentui/core` 0.6.6, split-footer mode | Sonnet 5.5, high effort | 17 (`bun test`), and `bunx tsc -p .` is clean |

## How to use them

- **The Rust slice is the starting point for the recommended stack.** Read it before units P1-01
  (attach), P1-05 (the reducer) and P1-06 (the inline renderer) in
  [../07-roadmap-and-work-units.md](../07-roadmap-and-work-units.md):
  - `src/net.rs`: identify, ticket and WebSocket;
  - `src/proto.rs`: frame decoding;
  - `src/state.rs`: the turn state machine;
  - `src/tui.rs`: native-scrollback insertion on a fixed viewport.
- **The TypeScript slice is the fallback's starting point,** if a switching condition in §Stack
  occurs.
- **Copy ideas, not files.** Both slices are single-crate or single-package prototypes. They have no
  reconnect, no replay after a dropped connection, no multi-line editor, no Markdown, and no resize
  testing. The rewrite's layering in [../05-architecture.md](../05-architecture.md) §Layers
  replaces their structure.
- **Never build inside this folder.** Build output (`target/`, `node_modules/`) must not be
  committed. Copy the folder somewhere temporary first:

  ```bash
  cp -R rust /tmp/slice-rust && cd /tmp/slice-rust && cargo test --release
  cp -R typescript /tmp/slice-ts && cd /tmp/slice-ts && bun install && bun test
  ```

- **Live runs need a running Hermes gateway** and create real sessions in the profile they attach
  to. Use a throwaway profile. Both slices use the model `minimax-m3.1-flash` from the provider
  `ollama-cloud`.

## Changes since the bake-off

- **The TypeScript slice's `--approve` flag now fails closed.** As built, it answered an unrecognised
  choice (a typo such as `dney`) with the first choice the gateway offered, which is "once", so it
  approved the command. It now refuses an unknown choice at start-up (exit code 2) and answers an
  approval only on an exact match; the replay path follows the same rule. The Rust slice's
  `--auto-approve` already answered only on an exact match. Copy the fail-closed form, never the
  original.

## The fixtures

`rust/tests/fixtures/` and `typescript/tests/fixtures/` hold frames recorded from the live gateway.
Machine-specific data was scrubbed before they were added:

- paths became `/work/project`;
- the profile became `default`;
- identifiers were renumbered consistently;
- the inventory frames (skins, the system prompt, tool and skill lists) were cut to the fields the
  code reads.

Frame order and the message, tool, approval and usage frames are unchanged, and both test suites pass
on the scrubbed files. Until P1-04 records fresh fixtures, P0-03's round-trip test may use these.

## Licence

Both slices are covered by the repository's MIT licence. `src/tui.rs` in the Rust slice follows the
history-insertion technique of OpenAI Codex's `insert_history.rs` under the Apache License 2.0. It
was written independently, and `rust/NOTICE` records the attribution. Keep that notice with any code
derived from it.
