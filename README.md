# Talaria

Talaria is an experimental, Hermes-native terminal UI focused on a calmer, more capable workflow: less flicker, clearer run state, better sub-agent visibility, and an interface that can grow into an upstream contribution to Hermes.

The name comes from the _talaria_, Hermes's winged sandals. The project is intentionally named for the Hermes ecosystem rather than as a private product fork.

> **Status: pre-implementation.** The repository contains the project direction, repository standards, the architecture decisions below, and a small TypeScript shell left over from repository bootstrap. The Hermes integration is not implemented yet.
>
> **Talaria is written in Python** ([ADR-0004](platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md)) with **Textual** as its terminal framework, which passed its validation gate on 2026-08-03 — see the [gate results](docs/analysis/2026-08-03-textual-validation-gate-results.md) and [ADR-0005](platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md) (`proposed`). The TypeScript shell described under "Quick start" is superseded and receives no new behavior.

## Goals

- Provide a polished terminal workflow closer to the strengths of Claude Code-style interfaces.
- Preserve Hermes-native capabilities instead of reducing Hermes to generic chat.
- Use stable integration boundaries where possible: the Hermes API for core runs and sessions, the TUI gateway for richer Hermes control-plane behavior, and typed adapters for deterministic surfaces such as Kanban.
- Make changes small and reviewable. Contributing useful work upstream to Hermes is welcome when it fits, but it is not a constraint on Talaria's architecture — see [ADR-0001](platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md).
- Keep the local development installation separate from an official Hermes installation.

## Non-goals

- Talaria is not a replacement for Hermes core.
- Talaria does not import Hermes internals as its primary architecture.
- Talaria does not copy private Infiquetra operational context into this public repository.

## Current architecture direction

```text
                      Talaria
                         │
              transport and capability layer
                         │
       ┌─────────────────┼─────────────────┐
       │                 │                 │
  Hermes API        TUI gateway       Kanban adapter
  runs/sessions     rich events       typed operations
```

The first implementation will validate the boundary before building a large UI. The architecture is a direction, not a claim that all adapters exist today.

Across all of it, one rule is already settled: the domain core has no dependency on the terminal
framework. Frames become normalized events, normalized events become domain state, and only then does
anything render. See [ADR-0002](platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md).

## Quick start

The Python implementation has started. There is no live gateway transport yet — milestone 1 drives
the entire interface from a recorded frame log, with no socket open anywhere in the process.

```bash
uv sync --all-groups

# Record a session from a running Hermes gateway.
uv run talaria record ws://127.0.0.1:9119/api/ws?token=<token>

# Replay that recording through the full interface. F8 pauses, F9/F10 change
# speed, F2 folds the sub-agent rows, F5 re-follows the newest line. Controls
# that would change something on the gateway are visibly inert in replay.
uv run talaria replay <recording.jsonl>

# Re-run the framework validation gate. Exits non-zero on a fail verdict.
uv run talaria gate --corpus <recording.jsonl> --deltas 50000
```

When the live transport lands, Talaria installs as an ordinary command:

```bash
uv tool install talaria
talaria
```

### The superseded TypeScript bootstrap

The repository still carries the shell it was initialized with — a small Ink prototype, a frame
recorder, and a redaction boundary. It runs, and its checks pass, but it is not the product and it is
not being extended. It stays until the Python tree replaces it, because its recorder and redaction
rules are still the only working evidence of the protocol.

```bash
npm install
npm run check
npm run dev
```

Press `q` or `Esc` to exit. `npm run check` runs TypeScript compilation, unit tests, and a Prettier
formatting check.

## Documentation

- [Documentation index](docs/00-index.md)
- [Architecture decisions](platform-specs/04-architecture/adrs/) — what is settled and why
- [Project direction and conversation analysis](docs/analysis/2026-08-01-hermes-tui-project-direction.md)
- [Engineering journal](docs/engineering-journal/README.md)
- [Public-safe project context](docs/public-safe-summary.md)

### Settled decisions

| ADR                                                                                          | Decision                                                                    |
| -------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| [0001](platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md)           | Talaria is a standalone process, not a TUI bundle Hermes launches           |
| [0002](platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md) | The domain core is framework-independent; the terminal UI is a projection   |
| [0003](platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md)   | Talaria re-encodes the Hermes terminal UI's behavior rather than porting it |
| [0004](platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md)               | Talaria is a Python client; the terminal framework is decided by a gate     |
| [0005](platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md)   | Textual is the presentation layer (`proposed`, on the gate's pass verdict)  |

## Contributing

Small, focused pull requests are preferred. Before opening a pull request, run the repository's check
command. Until the Python implementation lands, that is still the TypeScript bootstrap's:

```bash
npm run check
git diff --check
```

Changes that create durable project knowledge should update the relevant engineering-journal file in the same change. See [AGENTS.md](AGENTS.md) for repository-local guidance.

## Relationship to Hermes

Talaria is an independent public project built against [Hermes Agent](https://github.com/NousResearch/hermes-agent)'s public interfaces. It is not an official Hermes distribution and does not imply upstream endorsement.

## License

Talaria is released under the [MIT License](LICENSE).
