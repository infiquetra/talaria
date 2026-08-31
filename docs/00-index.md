# Talaria documentation index

Status: `active`
Authority: `reference`

## Start here

Architecture decisions are canon. Analysis is evidence for them, not a substitute.

- [ADR-0001 — Talaria is a standalone client](../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md)
- [ADR-0002 — The domain core is framework-independent](../platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md)
- [ADR-0003 — Talaria re-encodes the Hermes terminal UI's behavior](../platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md)
- [ADR-0004 — Talaria is a Python client](../platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md)
- [ADR-0005 — Textual is Talaria's presentation layer](../platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md) — `accepted`
- [ADR-0006 — Block rendering is bounded by work and height](../platform-specs/04-architecture/adrs/0006-block-rendering-is-bounded-by-work-and-height.md) — `accepted`

## User guides

- [Themes](themes.md)
- [Configuration](configuration.md)
- [Terminal UI](terminal-ui.md)
- [Visual Studio Code theme import format](formats/vscode-theme-import.md)

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
- [Public-safe project context](public-safe-summary.md)
- [Engineering journal](engineering-journal/README.md)

## Documentation boundaries

Talaria documentation describes this repository and its public integration with Hermes Agent. It must not reproduce private Infiquetra operational policy, credentials, host details, or private repository context.
