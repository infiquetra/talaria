# Analysis

This directory holds public-safe analysis that explains the problem framing, explored alternatives, evidence, and current direction of Talaria.

## Documents

- [Hermes TUI project direction](2026-08-01-hermes-tui-project-direction.md)
- [Hermes gateway protocol surface](hermes-gateway-protocol-surface.md)
- [Original language and TUI framework analysis](2026-08-02-language-and-tui-framework-analysis.md) — four-candidate evidence input; no decision
- [Independent language and TUI framework analysis](2026-08-02-language-and-tui-framework-analysis-independent.md) — primary-source pass written before reading the original
- [Final language and TUI framework analysis](2026-08-02-language-and-tui-framework-analysis-final.md) — previous reconciled recommendation; superseded by the reconsideration below
- [Reconsidered language and TUI framework analysis](2026-08-02-language-and-tui-framework-analysis-reconsideration.md) — selected Python/Textual; its language half is now ADR-0004, which also retires the Go/Bubble Tea fallback
- [Hermes terminal UI feature inventory](2026-08-02-hermes-tui-feature-inventory.md) — every surface of the shipping Hermes TUI with a proposed keep/change/drop verdict; the input [ADR-0003](../../platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md) requires before implementation

Analysis is not a substitute for implementation documentation or the engineering journal. Promote a settled repo-scoped choice to `docs/engineering-journal/DECISIONS.md` or an ADR, and keep implementation details close to the code once they exist.
