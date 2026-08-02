# Talaria

Talaria is an experimental, Hermes-native terminal UI focused on a calmer, more capable workflow: less flicker, clearer run state, better sub-agent visibility, and an interface that can grow into an upstream contribution to Hermes.

The name comes from the _talaria_, Hermes's winged sandals. The project is intentionally named for the Hermes ecosystem rather than as a private product fork.

> **Status: prototype.** The repository currently contains the project direction, repository standards, and a minimal executable TUI shell. The Hermes integration is not implemented yet.

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

## Quick start

### Prerequisites

- Node.js 20 or newer
- npm

### Install and run the prototype

```bash
npm install
npm run dev
```

Press `q` or `Esc` to exit the prototype.

### Build and run

```bash
npm run check
npm run build
npm start
```

The validation command is deliberately narrow at this stage:

- `npm run typecheck` — TypeScript compilation without emitting files
- `npm test` — unit tests
- `npm run format:check` — Prettier formatting check

## Documentation

- [Documentation index](docs/00-index.md)
- [Project direction and conversation analysis](docs/analysis/2026-08-01-hermes-tui-project-direction.md)
- [Engineering journal](docs/engineering-journal/README.md)
- [Public-safe project context](docs/public-safe-summary.md)

## Contributing

Small, focused pull requests are preferred. Before opening a pull request, run:

```bash
npm run check
git diff --check
```

Changes that create durable project knowledge should update the relevant engineering-journal file in the same change. See [AGENTS.md](AGENTS.md) for repository-local guidance.

## Relationship to Hermes

Talaria is an independent public project built against [Hermes Agent](https://github.com/NousResearch/hermes-agent)'s public interfaces. It is not an official Hermes distribution and does not imply upstream endorsement.

## License

Talaria is released under the [MIT License](LICENSE).
