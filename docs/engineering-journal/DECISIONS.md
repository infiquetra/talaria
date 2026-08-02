# Decisions - talaria

> Repo-scoped tactical decisions with rationale and revisit conditions.

## 2026-08-01

### Name the project Talaria

**Author.** Jeff Cox / project bootstrap

**Decision.** Use `Talaria` as the project name and `infiquetra/talaria` as the public repository slug.

**Rejected alternatives.** `hermes-tui` was the clearest descriptive name, but Talaria gives the project a distinctive identity while retaining a direct Hermes reference. `mimir-tui` and `bifrost-tui` were less immediately discoverable as a Hermes TUI.

**Rationale.** The project is intended to be a serious upstream contribution candidate rather than a permanently branded private fork. Talaria is memorable and leaves room for that relationship.

**Revisit when.** Upstream Hermes adopts a conflicting name, a trademark concern appears, or the project becomes an official Hermes distribution with different naming requirements.

## 2026-08-01

### Use a fresh client with layered Hermes adapters

**Author.** Project bootstrap

**Decision.** Build a thin client around the Hermes API, optional TUI gateway, and typed Kanban adapter instead of importing Hermes core or copying the entire existing TUI.

**Rejected alternatives.** API-only loses important control-plane UX; a wholesale TUI fork carries too much existing rendering complexity and makes upstream boundaries unclear.

**Rationale.** Layered adapters preserve independent installation, make capabilities explicit, and let individual changes become focused upstream proposals.

**Revisit when.** Hermes publishes a stable external TUI SDK, the existing TUI is refactored into a smaller reusable package, or the adapter boundary proves unable to support required workflows without unacceptable duplication.

## 2026-08-02

### Gate OpenTUI first, keep Bubble Tea v2 as the fallback, and stop investing in stock Ink

**Author.** Independent framework analysis and reconciliation

**Decision.** Treat TypeScript with OpenTUI as Talaria's presumptive stack, subject to a bounded frame-replay and clean-install gate. Use Go with Bubble Tea v2 if OpenTUI fails renderer-correctness, domain-isolation, package-reproducibility, or no-private-fork criteria. Do not add product behavior or renderer infrastructure to the current stock-Ink shell while the gate is open.

**Rejected alternatives.** Adopting OpenTUI immediately would convert a pre-1.0 native dependency into architecture without proving its packaging contract. Continuing on stock Ink would inherit renderer work that Hermes's own private fork demonstrates directly. Ratatui has the strongest low-level buffer test surface but leaves more whole-client infrastructure to Talaria than Bubble Tea. Textual remains the product-velocity alternative if compound widgets become the actual bottleneck, not the default.

**Rationale.** OpenTUI is the only inspected TypeScript candidate that combines a native cell renderer, synchronized-output handling, deterministic frame capture, and transcript-adjacent primitives. It preserves Talaria's real TypeScript investment—the recorder, redaction boundary, transport code, fixtures, and tooling—without preserving Ink. Bubble Tea v2 is the operational fallback because its current source verifies negotiated mode 2026, buffered cell rendering, injectable I/O and terminal size, golden output tests, headless operation, and a simpler native distribution story.

**Evidence.** [Final language and TUI framework analysis](../analysis/2026-08-02-language-and-tui-framework-analysis-final.md), reconciled from the [independent pass](../analysis/2026-08-02-language-and-tui-framework-analysis-independent.md) and the [original four-candidate analysis](../analysis/2026-08-02-language-and-tui-framework-analysis.md).

**Revisit when.** The validation gate completes; OpenTUI materially changes its runtime, native-package, or API-stability contract; the supported platform matrix becomes explicit; or compound-widget implementation cost exceeds renderer and transport work. The passing result, validated version, and package contract belong in an ADR before the implementation expands.

### Ideation working records stay out of the repository; only the scrubbed artifact ships

**Author.** First full-product ideation run

**Decision.** Ideation runs write their working record under `.claude/saga/`, which is gitignored. Only the reviewed, scrubbed artifact under `docs/ideation/` is committed. Local probe output, profile names, local ports, local file paths, and machine-specific measurements are generalized to the claim they support before anything enters `docs/`. Citations to public Hermes Agent source are kept verbatim, because that repository is public and it carries the strongest evidence.

**Rejected alternatives.** Committing the full working record would have published a live inventory of a private Hermes install. Dropping the evidence entirely would have destroyed the basis contract that makes an ideation artifact reviewable — a surviving idea with no stated evidence is an opinion.

**Rationale.** The evidence is the quality mechanism, so it cannot be deleted; the instances are the private part, so they cannot be published. Replacing each instance with the claim it supports keeps both properties. This matches the convention `docs/analysis/2026-08-01-hermes-tui-project-direction.md` already set for itself.

**Revisit when.** The repository stops being public, an ideation run touches nothing local, or a reviewer cannot follow a survivor's reasoning because the generalization removed something load-bearing.

### Talaria reads agent state; it does not author agent identity

**Author.** Jeff Cox, mid-run scope correction

**Decision.** Talaria is a client of the Hermes agent, not an administration surface for it. Profile creation, generation, editing, pruning, rollback, and configuration writes stay outside this project. Talaria may select a profile, show which profile a value came from, and aggregate work and sessions across profiles.

**Rejected alternatives.** Five separate ideation candidates proposed profile viewing or management surfaces, including strictly read-only ones. All were cut. Two of them were rewritten mid-run into pure read-only form and were still cut, so the boundary is drawn at the _administration surface_, not merely at the write.

**Rationale.** A strong decoupling between the terminal UI and the agent it connects to keeps the client replaceable and keeps agent identity owned by the tooling that generates it.

**Revisit when.** The intended line turns out to be "no writes to agent identity" rather than "no agent-admin surface at all." Under that reading, cuts R1 and R5 in [the product-shape ideation](../ideation/2026-08-02-talaria-product-shape-ideation.md) survive as written, and the profile axis regains four candidates.
