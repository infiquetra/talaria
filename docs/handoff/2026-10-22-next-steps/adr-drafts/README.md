# Draft ADRs

These are **proposed** architecture decision records. They are not canon until the operator accepts
them. On acceptance, move each file to `platform-specs/04-architecture/adrs/`, change its status to
`accepted`, mark the ADRs it supersedes as `superseded`, and update the tables in `README.md`,
`AGENTS.md` and `docs/00-index.md`.

| Draft | Decision | Supersedes |
|---|---|---|
| [0007](0007-attach-to-the-hermes-session-authority.md) | Talaria attaches to the per-machine Hermes gateway's session authority (`hermes-gateway-v1`) as a viewer | The transport half of ADR-0001; the per-profile gateway fleet |
| [0008](0008-protocol-types-are-generated-from-the-hermes-contract.md) | Protocol types are generated from the contract Hermes publishes, pinned to a Hermes release | The protocol half of ADR-0003 (behaviour re-encoding stays) |
| [0009](0009-finished-output-lives-in-native-scrollback.md) | Inline-first rendering: finished output is written once into the terminal's scrollback | ADR-0006 |
| [0010](0010-rust-and-ratatui.md) | The rewrite's language and terminal framework | ADR-0004, ADR-0005 |
| [0011](0011-workflows-run-in-a-detachable-runner.md) | Workflows run in a detachable headless runner; the terminal UI is a viewer | — |
| [0012](0012-verification-is-proportionate.md) | Verification is proportionate: replay fixtures, performance budgets in CI, one live drive per release | The per-release acceptance-receipt practice |
