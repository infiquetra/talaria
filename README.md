# Talaria

Talaria is a terminal client for the Hermes agent gateway, being rewritten in Rust.
It owns the terminal experience; Hermes owns agent execution and durable sessions.

The last Python release is v0.6.5, preserved at [python-final](https://github.com/infiquetra/talaria/tree/python-final).

## Install

Install: see [docs/install.md](docs/install.md) (Homebrew, release archives, or build from source).

## Build and check

Install Rust through rustup; the exact toolchain and components are pinned in
[rust-toolchain.toml](rust-toolchain.toml). A Rust installation without rustup ignores that file.

```bash
cargo build --workspace
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo run -q -p talaria-cli -- --version
```

Install `cargo-llvm-cov` once with `cargo install cargo-llvm-cov --locked`.
Line coverage must be at least 80% across the workspace. Generated source under
`/generated/` is excluded; inline test code counts.

```bash
cargo llvm-cov --workspace --ignore-filename-regex /generated/ --fail-under-lines 80
```

## Workspace

```text
crates/hermes-contract/   Generated protocol types and canonical events
crates/hermes-client/     Gateway discovery, transport and request correlation
crates/talaria-core/      Pure domain state and reducers
crates/talaria-app/       Application services and effect execution
crates/talaria-tui/       Terminal rendering, input and overlays
crates/talaria-workflow/  Detached workflow runner
crates/talaria-cli/       The talaria binary and command wiring
tools/contract-gen/      Future contract generation tools
tools/bench/             Future performance harness
fixtures/frames/         Redacted frame fixtures
```

The domain crates never import terminal libraries. Dependency boundaries are checked
by the layering tests. See [AGENTS.md](AGENTS.md) for contribution rules.
Keep this public repository free of secrets and private operational context.
No attribution lines belong in commits, PRs, code or generated documentation.

## Read next

- [Handoff](docs/handoff/2026-10-22-next-steps/README.md)
- [Roadmap](docs/handoff/2026-10-22-next-steps/07-roadmap-and-work-units.md)
- [Architecture](docs/handoff/2026-10-22-next-steps/05-architecture.md)
- [Architecture decisions](platform-specs/04-architecture/adrs/)
- [Changelog](CHANGELOG.md)
- [License](LICENSE)

- [Guide](docs/guide.md)
- [Keybindings](docs/keybindings.md)
- [Configuration](docs/configuration.md)
- [Migrating from 0.6.5](docs/migrating-from-0.6.5.md)
