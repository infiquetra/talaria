# CLAUDE.md

Talaria is a public, experimental Hermes-native terminal UI. Read [AGENTS.md](AGENTS.md) and [README.md](README.md) before changing code.

## Navigation

- **Next steps (2026-10-22, accepted 2026-10-23):** [docs/handoff/2026-10-22-next-steps/README.md](docs/handoff/2026-10-22-next-steps/README.md) — a recommended rewrite against the current Hermes gateway, recorded in ADRs 0007 to 0012. Read it before starting new work
- Architecture decisions: [platform-specs/04-architecture/adrs/](platform-specs/04-architecture/adrs/) — read these first; ADR-0002 and ADRs 0007 to 0012 are in force; ADR-0001 and ADR-0003 are superseded in part, and ADR-0004, ADR-0005 and ADR-0006 are superseded
- User documentation: [themes](docs/themes.md), [configuration](docs/configuration.md), and the [terminal UI](docs/terminal-ui.md)
- Project direction: [docs/analysis/2026-08-01-hermes-tui-project-direction.md](docs/analysis/2026-08-01-hermes-tui-project-direction.md)
- Durable decisions: [docs/engineering-journal/DECISIONS.md](docs/engineering-journal/DECISIONS.md)
- Deferred work: [docs/engineering-journal/QUEUED.md](docs/engineering-journal/QUEUED.md)
- Organization standards: [infiquetra-context-library](https://github.com/infiquetra/infiquetra-context-library)

## Checks

The code in the tree today is the Python client, which ADR-0010 supersedes; #195 (`P0-02`) (the reset issue) removes it. Run the project check with `uv`:

```bash
uv sync --all-groups
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
uv run bandit -r scripts -q -ll
git diff --check
! git grep -nE '^(<{7}|={7}|>{7})( |$)' -- . || (echo "conflict marker" && false)
```

`npm run check` still applies to `src/`, which is no longer a bootstrap awaiting removal: it is three
files holding the TypeScript reference recorder that `tests/recorder/test_equivalence.py` asserts the
Python recorder is equivalent to. Run it when you touch them.

## Notes

- Talaria's code today is Python with Textual (ADR-0004 and ADR-0005, both superseded by ADR-0010). The TypeScript tree under `src/` is not superseded bootstrap any more — the bootstrap was removed on 2026-08-07 and what remains is the reference recorder the Python one is tested against. Do not extend it, do not port it file by file, and do not delete it without saying what replaces the redaction equivalence guarantee.
- The domain core never imports the terminal framework (ADR-0002).
- Hermes's terminal UI is documentation of behavior, not a source tree to translate (ADR-0003).
- Prefer transport interfaces and capability discovery over direct Hermes internals.
- Keep UI changes on the existing domain-to-projection-to-Textual seams; do not add transport work to a presentation surface.
- Update the engineering journal when a durable project decision or learning appears.
- Keep this public repository free of private operational context and secrets.
