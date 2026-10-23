# Talaria documentation index

Status: `active`
Authority: `reference`

## Start here

Architecture decisions are canon. Analysis is evidence for them, not a substitute.

- [Next steps handoff, 2026-10-22](handoff/2026-10-22-next-steps/README.md) — `proposed`: rewrite
  Talaria as a fast, inline client of the current Hermes gateway; ADRs 0007 to 0012 were accepted on `2026-10-23`

- [ADR-0001 — Talaria is a standalone client](../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md) — `superseded in part by ADR-0007`
- [ADR-0002 — The domain core is framework-independent](../platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md)
- [ADR-0003 — Talaria re-encodes the Hermes terminal UI's behavior](../platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md) — `superseded in part by ADR-0008`
- [ADR-0004 — Talaria is a Python client](../platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md) — `superseded by ADR-0010`
- [ADR-0005 — Textual is Talaria's presentation layer](../platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md) — `superseded by ADR-0010`
- [ADR-0006 — Block rendering is bounded by work and height](../platform-specs/04-architecture/adrs/0006-block-rendering-is-bounded-by-work-and-height.md) — `superseded by ADR-0009`
- [ADR-0007 — Talaria attaches to the Hermes session authority as a viewer](../platform-specs/04-architecture/adrs/0007-attach-to-the-hermes-session-authority.md) — `accepted`
- [ADR-0008 — Protocol types are generated from the contract Hermes publishes](../platform-specs/04-architecture/adrs/0008-protocol-types-are-generated-from-the-hermes-contract.md) — `accepted`
- [ADR-0009 — Finished output lives in the terminal's native scrollback](../platform-specs/04-architecture/adrs/0009-finished-output-lives-in-native-scrollback.md) — `accepted`
- [ADR-0010 — The rewrite is Rust, with ratatui, crossterm and tokio](../platform-specs/04-architecture/adrs/0010-rust-and-ratatui.md) — `accepted`
- [ADR-0011 — Workflows run in a detachable headless runner](../platform-specs/04-architecture/adrs/0011-workflows-run-in-a-detachable-runner.md) — `accepted`
- [ADR-0012 — Verification is proportionate](../platform-specs/04-architecture/adrs/0012-verification-is-proportionate.md) — `accepted`

## User guides

- [Install Talaria](install.md)
- [Themes](themes.md)
- [Configuration](configuration.md)
- [Terminal UI](terminal-ui.md)
- [Visual Studio Code theme import format](formats/vscode-theme-import.md)

## Releases

v0.6.5 types schema and env load failures and ships
`summarize_schema_shape` for metadata-only diagnosis. The live T3R
probe found no-failure-class; remount was not rewritten. Installed
remote login without live access stays blocked.

- [Talaria v0.6.5](releases/v0.6.5.md)
- [Talaria v0.6.4](releases/v0.6.4.md)
- [Talaria v0.6.3](releases/v0.6.3.md)
- [Talaria v0.6.2](releases/v0.6.2.md)
- [Talaria v0.6.1](releases/v0.6.1.md)
- [Talaria v0.6.0](releases/v0.6.0.md)
- [Talaria v0.5.0](releases/v0.5.0.md)
- [Talaria v0.4.0](releases/v0.4.0.md)
- [Talaria v0.3.0](releases/v0.3.0.md)
- [Talaria v0.2.0](releases/v0.2.0.md)
- [Talaria v0.1.0](releases/v0.1.0.md)

## Release evidence

- [v0.5.0 live acceptance results](acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md)
- [v0.5.0 T1 evidence index](acceptance/v0.5.0/evidence/t1/README.md)
- [v0.5.0 T2 evidence index](acceptance/v0.5.0/evidence/t2/README.md)
- [v0.5.0 visual specification](design/2026-08-30-talaria-v0-5-0-visual-spec.md)
- [v0.5.0 run plan](plans/2026-08-30-talaria-v0-5-0-run-plan.md)

## Analysis and evidence

- [Project direction and conversation analysis](analysis/2026-08-01-hermes-tui-project-direction.md) — launcher section superseded by ADR-0001; the port framing superseded by ADR-0003
- [What Talaria should become — first full-product ideation](ideation/2026-08-02-talaria-product-shape-ideation.md)
- [Hermes gateway protocol surface](analysis/hermes-gateway-protocol-surface.md) — the event and method vocabularies, read from Hermes's own client
- [Hermes terminal UI feature inventory](analysis/2026-08-02-hermes-tui-feature-inventory.md) — what the shipping Hermes TUI does, surface by surface, with a proposed keep/change/drop verdict for each; the input ADR-0003 requires
- [Original language and TUI framework analysis](analysis/2026-08-02-language-and-tui-framework-analysis.md) — four-candidate evidence input; no decision
- [Independent language and TUI framework analysis](analysis/2026-08-02-language-and-tui-framework-analysis-independent.md) — primary-source pass written before reading the original
- [Final language and TUI framework analysis](analysis/2026-08-02-language-and-tui-framework-analysis-final.md) — previous OpenTUI-first recommendation; superseded by the reconsideration below
- [Reconsidered language and TUI framework analysis](analysis/2026-08-02-language-and-tui-framework-analysis-reconsideration.md) — selected Python/Textual; its language half is now ADR-0004, which also retires the Go/Bubble Tea fallback
- [Frame log format](formats/frame-log.md)
- [Status line format](formats/status-line.md)
- [Public-safe project context](public-safe-summary.md)
- [Engineering journal](engineering-journal/README.md)

## Documentation boundaries

Talaria documentation describes this repository and its public integration with Hermes Agent. It must not reproduce private Infiquetra operational policy, credentials, host details, or private repository context.
