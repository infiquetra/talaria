# CLAUDE.md

Talaria is a public, experimental Hermes-native terminal UI. Read [AGENTS.md](AGENTS.md) and [README.md](README.md) before changing code.

## Navigation

- Architecture decisions: [platform-specs/04-architecture/adrs/](platform-specs/04-architecture/adrs/) — read these first; four are settled and ADR-0005 (Textual) is proposed
- Project direction: [docs/analysis/2026-08-01-hermes-tui-project-direction.md](docs/analysis/2026-08-01-hermes-tui-project-direction.md)
- Durable decisions: [docs/engineering-journal/DECISIONS.md](docs/engineering-journal/DECISIONS.md)
- Deferred work: [docs/engineering-journal/QUEUED.md](docs/engineering-journal/QUEUED.md)
- Organization standards: [infiquetra-context-library](https://github.com/infiquetra/infiquetra-context-library)

## Checks

The Python implementation has started (ADR-0004). Run the project check with `uv`:

```bash
uv sync --all-groups
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

`npm run check` still applies to `src/`, which is no longer a bootstrap awaiting removal: it is three
files holding the TypeScript reference recorder that `tests/recorder/test_equivalence.py` asserts the
Python recorder is equivalent to. Run it when you touch them.

## Notes

- Talaria is written in Python (ADR-0004) with Textual as the terminal framework (ADR-0005, `proposed` on the U5 gate's pass verdict). The TypeScript tree under `src/` is not superseded bootstrap any more — the bootstrap was removed on 2026-08-07 and what remains is the reference recorder the Python one is tested against. Do not extend it, do not port it file by file, and do not delete it without saying what replaces the redaction equivalence guarantee.
- The domain core never imports the terminal framework (ADR-0002).
- Hermes's terminal UI is documentation of behavior, not a source tree to translate (ADR-0003).
- Prefer transport interfaces and capability discovery over direct Hermes internals.
- Keep the initial UI small until the integration seam is proven.
- Update the engineering journal when a durable project decision or learning appears.
- Keep this public repository free of private operational context and secrets.
