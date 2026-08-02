# CLAUDE.md

Talaria is a public, experimental Hermes-native terminal UI. Read [AGENTS.md](AGENTS.md) and [README.md](README.md) before changing code.

## Navigation

- Project direction: [docs/analysis/2026-08-01-hermes-tui-project-direction.md](docs/analysis/2026-08-01-hermes-tui-project-direction.md)
- Durable decisions: [docs/engineering-journal/DECISIONS.md](docs/engineering-journal/DECISIONS.md)
- Deferred work: [docs/engineering-journal/QUEUED.md](docs/engineering-journal/QUEUED.md)
- Organization standards: [infiquetra-context-library](https://github.com/infiquetra/infiquetra-context-library)

## Checks

```bash
npm run check
git diff --check
```

## Notes

- Prefer transport interfaces and capability discovery over direct Hermes internals.
- Keep the initial UI small until the integration seam is proven.
- Update the engineering journal when a durable project decision or learning appears.
- Keep this public repository free of private operational context and secrets.
