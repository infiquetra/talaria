# CLAUDE.md

Talaria is a public Rust terminal client for the Hermes gateway, currently a placeholder.
Read [AGENTS.md](AGENTS.md) and [README.md](README.md) before changing code.

## Navigation

- [Handoff](docs/handoff/2026-10-22-next-steps/README.md)
- [Architecture decisions](platform-specs/04-architecture/adrs/)
- [Engineering journal](docs/engineering-journal/)
- [Deferred work](docs/engineering-journal/QUEUED.md)
- Organization standards: [infiquetra-context-library](https://github.com/infiquetra/infiquetra-context-library)

## Checks

Use rustup with the pinned toolchain.

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

Coverage has an 80% line floor; generated source under `/generated/` is excluded
and inline test code counts.

```bash
cargo llvm-cov --workspace --ignore-filename-regex /generated/ --fail-under-lines 80
```

## Notes

- The domain core never imports terminal libraries.
- Verify Hermes facts against the revision in `crates/hermes-contract/PINNED` once it exists.
- Only the terminal module of `talaria-tui` writes escape sequences.
- Keep this public repository free of secrets and private operational context.
- Update the engineering journal for durable decisions and learnings.
- No attribution lines anywhere.
