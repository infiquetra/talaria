# AGENTS.md

## Repository purpose

Talaria is an experimental, public terminal UI for Hermes Agent. It owns the client-side terminal experience, transport abstractions, and UI-specific state. It does not own Hermes core, provider implementations, or private Infiquetra operational policy.

Talaria is a standalone client that connects to a Hermes gateway it did not launch; it is not built to be loaded by `hermes --tui`. Focused improvements may still be proposed upstream where they fit, but upstreamability does not constrain the architecture, and divergence is accepted. See [ADR-0001](platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md).

## Read the ADRs before writing code

Six decisions are settled and are not open for re-litigation in an implementation change:

| ADR                                                                                               | What it settles                                                                              |
| ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| [0001](platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md)                | Talaria owns its own process lifetime and dials a gateway it did not launch                  |
| [0002](platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md)      | The domain core never imports the terminal framework; the UI is a projection of domain state |
| [0003](platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md)        | Hermes's terminal UI is documentation of behavior, not a source tree to translate            |
| [0004](platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md)                    | Talaria is written in Python; the terminal framework is decided by a validation gate         |
| [0005](platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md)        | Textual 8.2.8 is the accepted presentation layer                                             |
| [0006](platform-specs/04-architecture/adrs/0006-block-rendering-is-bounded-by-work-and-height.md) | Block rendering is bounded by work and rendered height                                       |

**The transition is done, and `src/` is what survived it.** The Ink prototype, the command-line entry point, the recording command and the transport shim were removed on 2026-08-07. The three remaining TypeScript files are the reference recorder and its redaction rules, kept because `tests/recorder/test_equivalence.py` runs the real thing in a subprocess and asserts the Python recorder matches it across the credential redaction boundary. Do not add behavior to them. Do not port them file by file. Do not delete them without first saying what replaces that guarantee.

## Source of truth

- Repository purpose and commands: [README.md](README.md)
- Project analysis: [docs/analysis/](docs/analysis/)
- Local durable knowledge: [docs/engineering-journal/](docs/engineering-journal/)
- Public Hermes integration reference: [Hermes Agent](https://github.com/NousResearch/hermes-agent)
- Organization-level standards: [infiquetra-context-library](https://github.com/infiquetra/infiquetra-context-library)
- SDLC process: [infiquetra-sdlc](https://github.com/infiquetra/infiquetra-sdlc)

## Commands

The active implementation is Python (ADR-0004). The project check is:

```bash
uv sync --all-groups
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

A change touching `src/` runs the Node check as well:

```bash
npm install
npm run check
git diff --check
```

The equivalence assertion between the two recorders runs from the Python suite, not this one. The
continuous-integration leg that sets `TALARIA_REQUIRE_TS_BRIDGE=1` makes a missing Node toolchain a
failure rather than a skip, because the harness once skipped invisibly inside a green run.

## Working rules

- Keep changes scoped and prefer small, composable interfaces.
- **The domain core does not import the terminal framework.** Transport, protocol parsing, normalized events, domain state, commands, clocks, and record/replay are plain Python. The UI consumes view models and may request commands; it never holds protocol or session state. This is enforced by a check, not by intention — if you add a domain module, the check covers it. See [ADR-0002](platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md).
- **Re-encode Hermes's terminal UI behavior; do not translate its code.** Take the protocol contract, the reconciliation rules, and the hard-won terminal knowledge. Leave the component structure, the state shape, and the framework workarounds. Record a keep/change/drop verdict for the features of any surface before implementing it. See [ADR-0003](platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md).
- **Cite Hermes source by revision, and read the Hermes that is running.** Where a local checkout and an installed copy both exist, read the installed one — resolve it with `python3 -c "import hermes_cli, os; print(os.path.dirname(hermes_cli.__file__))"` and record the revision (`git -C <dir> rev-parse --short=9 HEAD`) alongside any `file:line` you commit. A stale checkout never fails loudly: it answers every question fluently, because it is the same project, only older. Every citation in this repository was once wrong this way.
- Do not import Hermes implementation modules when an API, gateway, or typed adapter boundary is sufficient.
- Do not assume optional Hermes features exist. The Hermes API server publishes `GET /v1/capabilities`; the terminal gateway publishes nothing equivalent, so gateway capabilities must be inferred by probing each seam at startup and naming what is absent.
- Preserve graceful fallback behavior when a richer capability is unavailable.
- Update tests for behavior changes.
- Update `docs/engineering-journal/` when work creates a durable learning, decision, deferred item, shipped/rejected item, superseded entry, narrative, or audit.
- Do not commit secrets, local environment files, private operational details, or copied private policy text.
- Keep public documentation safe for readers outside Infiquetra.
- Use conventional commits and pull requests for non-trivial changes.
- **No attribution lines anywhere** — not in commit messages, pull request bodies, code comments, or generated documentation. No generated-with notices, no co-authored-by trailer naming a tool, no machine-added sign-off. This is stated here rather than left to each contributor's own configuration because agents that do not read that configuration still read this file, and one such agent put an attribution trailer on `main`.

## Architectural guardrails

1. Core run/session behavior should prefer a stable Hermes API contract.
2. Hermes-specific control-plane behavior may use the TUI gateway through an explicit adapter.
3. Kanban UI operations must be deterministic and typed; do not rely on a model deciding to call a board tool as the UI's data API.
4. A feature that requires a new gateway or core contract must be capability-gated and must degrade visibly when the contract is absent.
