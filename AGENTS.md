# AGENTS.md

## Repository purpose

Talaria is an experimental, public terminal UI for Hermes Agent. It owns the client-side terminal experience, transport abstractions, and UI-specific state. It does not own Hermes core, provider implementations, or private Infiquetra operational policy.

Talaria is a standalone client that connects to a Hermes gateway it did not launch; it is not built to be loaded by `hermes --tui`. Focused improvements may still be proposed upstream where they fit, but upstreamability does not constrain the architecture, and divergence is accepted. See [ADR-0001](platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md).

## Source of truth

- Repository purpose and commands: [README.md](README.md)
- Project analysis: [docs/analysis/](docs/analysis/)
- Local durable knowledge: [docs/engineering-journal/](docs/engineering-journal/)
- Public Hermes integration reference: [Hermes Agent](https://github.com/NousResearch/hermes-agent)
- Organization-level standards: [infiquetra-context-library](https://github.com/infiquetra/infiquetra-context-library)
- SDLC process: [infiquetra-sdlc](https://github.com/infiquetra/infiquetra-sdlc)

## Commands

```bash
npm install
npm run dev
npm run check
npm run build
npm start
git diff --check
```

## Working rules

- Keep changes scoped and prefer small, composable interfaces.
- Do not import Hermes implementation modules when an API, gateway, or typed adapter boundary is sufficient.
- Do not assume optional Hermes features exist. The Hermes API server publishes `GET /v1/capabilities`; the terminal gateway publishes nothing equivalent, so gateway capabilities must be inferred by probing each seam at startup and naming what is absent.
- Preserve graceful fallback behavior when a richer capability is unavailable.
- Update tests for behavior changes.
- Update `docs/engineering-journal/` when work creates a durable learning, decision, deferred item, shipped/rejected item, superseded entry, narrative, or audit.
- Do not commit secrets, local environment files, private operational details, or copied private policy text.
- Keep public documentation safe for readers outside Infiquetra.
- Use conventional commits and pull requests for non-trivial changes.

## Architectural guardrails

1. Core run/session behavior should prefer a stable Hermes API contract.
2. Hermes-specific control-plane behavior may use the TUI gateway through an explicit adapter.
3. Kanban UI operations must be deterministic and typed; do not rely on a model deciding to call a board tool as the UI's data API.
4. A feature that requires a new gateway or core contract must be capability-gated and must degrade visibly when the contract is absent.
