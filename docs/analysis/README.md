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
- [Python fallback presentation layer](2026-08-02-python-fallback-presentation-layer.md) — `prompt_toolkit` assessed against the Textual gate criteria to plausibility depth (PC8/KTD12), so a gate failure has an evaluated next step
- [Hermes reconciliation-rule catalogue](2026-08-02-hermes-reconciliation-rules.md) — every reconciliation rule in Hermes's terminal UI at `7f4d15515`, with a keep/change/drop verdict and a named test; the artifact [ADR-0003](../../platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md) depends on and R37 makes a precondition of the normalization layer
- [Textual validation gate results](2026-08-03-textual-validation-gate-results.md) — the U5 framework gate's measurements, corpus identities, exercised platform matrix and pass verdict, with the machine-readable record in [`evidence/`](evidence/2026-08-03-textual-validation-gate.json); the evidence [ADR-0005](../../platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md) rests on

- [v0.1 daily-driver verdict](2026-08-02-v0-1-daily-driver-verdict.md) — U10's closing gate: the named gateway-method table, the platform matrix as exercised, and a **not ready** verdict that follows from its own evidence rows, restated 2026-08-05 on the corrected table DRIFT-04 asked for (see the register below). Start here before relying on the client for real work
- [Conformance audit drift findings](2026-08-05-conformance-audit-drift-findings.md) — the register of what the R1–R40 conformance audit found, where "drift" means a departure from a requirement that no record explains. Five findings, all five resolved. Also records the method result: every requirement was graded twice, once by running the program and once by reading the source, and neither lens found everything in any batch

Analysis is not a substitute for implementation documentation or the engineering journal. Promote a settled repo-scoped choice to `docs/engineering-journal/DECISIONS.md` or an ADR, and keep implementation details close to the code once they exist.
