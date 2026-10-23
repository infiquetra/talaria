# AGENTS.md

Talaria is a public Rust terminal client for the Hermes gateway. The workspace currently
contains placeholders; it owns terminal presentation, transport adapters and client state.

## Architecture decision records

Read the architecture decision records before writing code. Their recorded statuses are:

| Record | Status |
| --- | --- |
| [0001](platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md) | `superseded in part by ADR-0007: the transport half is replaced; the decision that Talaria is a standalone client stands` |
| [0002](platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md) | `accepted` |
| [0003](platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md) | `superseded in part by ADR-0008: the protocol half is replaced; the decision to re-encode Hermes terminal behaviour stands` |
| [0004](platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md) | `superseded by ADR-0010` |
| [0005](platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md) | `superseded by ADR-0010` |
| [0006](platform-specs/04-architecture/adrs/0006-block-rendering-is-bounded-by-work-and-height.md) | `superseded by ADR-0009` |
| [0007](platform-specs/04-architecture/adrs/0007-attach-to-the-hermes-session-authority.md) | `accepted` |
| [0008](platform-specs/04-architecture/adrs/0008-protocol-types-are-generated-from-the-hermes-contract.md) | `accepted` |
| [0009](platform-specs/04-architecture/adrs/0009-finished-output-lives-in-native-scrollback.md) | `accepted` |
| [0010](platform-specs/04-architecture/adrs/0010-rust-and-ratatui.md) | `accepted` |
| [0011](platform-specs/04-architecture/adrs/0011-workflows-run-in-a-detachable-runner.md) | `accepted` |
| [0012](platform-specs/04-architecture/adrs/0012-verification-is-proportionate.md) | `accepted` |

## Rules for every unit

1. One unit, one branch, one PR. Use conventional commits. No attribution lines anywhere.
2. Read the unit's inputs. Verify Hermes facts at the revision pinned in
   `crates/hermes-contract/PINNED`, created by P0-03
   ([roadmap](docs/handoff/2026-10-22-next-steps/07-roadmap-and-work-units.md#p0-03--vendor-and-generate-the-hermes-contract)).
3. Touch only owned files; explain any necessary minimal exception in the PR.
4. Tests first when named by the unit; record their failing and passing runs.
5. Pass the project check and the unit's acceptance commands.
6. No unsafe code or `Rc<RefCell<…>>` webs. Own state and pass messages through channels.
   Every source of change sends a message into one application event channel.
   Domain crates never import terminal libraries.
7. Record non-obvious learnings with evidence and a one-line rule in
   [LEARNINGS.md](docs/engineering-journal/LEARNINGS.md). No planning codes in source comments.
8. Use the unit's suggested model tier or the nearest provider equivalent; state it in the PR.
   After a failed review, retry one tier higher; at the top tier, a second failure goes to the operator.

The [roadmap rules](docs/handoff/2026-10-22-next-steps/07-roadmap-and-work-units.md#rules-for-every-unit)
define the tier ladder. Every phase ends with an operator check before the next phase starts.

## Checks

Use rustup; [rust-toolchain.toml](rust-toolchain.toml) pins the compiler and components.

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

Line coverage must reach 80% across the workspace. Generated source under
`/generated/` is excluded; inline test code counts.

```bash
cargo llvm-cov --workspace --ignore-filename-regex /generated/ --fail-under-lines 80
```

## Boundaries and public safety

- Dependency boundaries are enforced by
  [layering tests](crates/talaria-core/tests/layering.rs), including normal and build edges
  transitively with every feature enabled. Later units may add forbidden edges, never remove them.
- Only the terminal module of `talaria-tui` writes escape sequences.
- Keep domain state out of presentation and input/output out of the pure domain core.
- Re-encode Hermes terminal behaviour rather than translating its component structure.
- Keep this public repository free of secrets and private operational context.
- Follow [public organization standards](https://github.com/infiquetra/infiquetra-context-library).
- No attribution lines in commits, PRs, code comments or generated documentation.
