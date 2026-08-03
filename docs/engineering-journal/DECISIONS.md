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

**Status.** Superseded on 2026-08-02 by the Textual-first validation decision below. The analysis remains provenance for the earlier weighting.

**Decision.** Treat TypeScript with OpenTUI as Talaria's presumptive stack, subject to a bounded frame-replay and clean-install gate. Use Go with Bubble Tea v2 if OpenTUI fails renderer-correctness, domain-isolation, package-reproducibility, or no-private-fork criteria. Do not add product behavior or renderer infrastructure to the current stock-Ink shell while the gate is open.

**Rejected alternatives.** Adopting OpenTUI immediately would convert a pre-1.0 native dependency into architecture without proving its packaging contract. Continuing on stock Ink would inherit renderer work that Hermes's own private fork demonstrates directly. Ratatui has the strongest low-level buffer test surface but leaves more whole-client infrastructure to Talaria than Bubble Tea. Textual remains the product-velocity alternative if compound widgets become the actual bottleneck, not the default.

**Rationale.** OpenTUI is the only inspected TypeScript candidate that combines a native cell renderer, synchronized-output handling, deterministic frame capture, and transcript-adjacent primitives. It preserves Talaria's real TypeScript investment—the recorder, redaction boundary, transport code, fixtures, and tooling—without preserving Ink. Bubble Tea v2 is the operational fallback because its current source verifies negotiated mode 2026, buffered cell rendering, injectable I/O and terminal size, golden output tests, headless operation, and a simpler native distribution story.

**Evidence.** [Final language and TUI framework analysis](../analysis/2026-08-02-language-and-tui-framework-analysis-final.md), reconciled from the [independent pass](../analysis/2026-08-02-language-and-tui-framework-analysis-independent.md) and the [original four-candidate analysis](../analysis/2026-08-02-language-and-tui-framework-analysis.md).

**Revisit when.** The validation gate completes; OpenTUI materially changes its runtime, native-package, or API-stability contract; the supported platform matrix becomes explicit; or compound-widget implementation cost exceeds renderer and transport work. The passing result, validated version, and package contract belong in an ADR before the implementation expands.

### Gate Textual first and keep Bubble Tea v2 as the native-distribution fallback

**Author.** Jeff Cox / framework-analysis reconsideration

**Status.** Partly promoted and partly amended on 2026-08-02. Its language half is now [ADR-0004](../../platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md): Python is settled, because the operator answered the open seventh consideration — a zero-runtime native executable is **not** a product requirement — which removed Go's only advantage Python cannot match. **The Bubble Tea fallback is retired.** A framework failure is not evidence against the language that selected it, so the fallback is another Python presentation layer; identifying one is now queued as a prerequisite of the gate, because none has ever been assessed. The Textual half stands unchanged and still runs the gate before any ADR names it.

**Decision.** Treat Python with Textual 8.2.8 as Talaria's presumptive stack, subject to a bounded protocol-replay, long-transcript, pseudo-terminal, and clean-install gate. Use Go with Bubble Tea v2 if Textual fails a material correctness or transcript-cost requirement, or if a small zero-runtime native executable becomes mandatory. Do not adopt either dependency in an ADR until the first gate completes.

**Rejected alternatives.** Keeping OpenTUI first would preserve implementation language for a greenfield codebase whose current TypeScript is not a meaningful migration estate. Adopting Textual immediately would leave long-history behavior, PTY correctness, and packaging unproved. Building complete Textual and Bubble Tea clients in parallel would create two products instead of a validation gate. Ratatui remains the alternative if exact cell-buffer equality becomes a release requirement.

**Rationale.** Talaria will be predominantly agent-built, most Infiquetra repositories and Hermes core are Python, and Textual has the broadest compound-widget surface plus a first-party `run_test()` and `Pilot` verification loop. Python can provide an ordinary `talaria --yolo` command through standard entry points and `uv tool install`; Go's distribution advantage matters only if the product requires a small native artifact without a managed runtime. Textual must still prove bounded transcript mounting, coalesced streaming, framework-independent domain state, deterministic headless behavior, PTY correctness, and clean installation.

**Evidence.** [Reconsidered language and TUI framework analysis](../analysis/2026-08-02-language-and-tui-framework-analysis-reconsideration.md), including late independent research that corrected current Ink and Bubble Tea capabilities and advanced Textual to full scoring.

**Revisit when.** The Textual validation gate completes; the missing seventh operator consideration adds a hard constraint; the supported platform matrix or native-artifact requirement becomes explicit; Textual cannot bound transcript cost without a private fork; or exact cell-buffer replay becomes a release gate. A passing result, validated version, Python support window, and package contract belong in an ADR before implementation expands.

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

### The v0.1 implementation plan pins its load-bearing technical decisions

**Author.** `/plan` run from the reviewed v0.1 requirements

**Status.** Amended on 2026-08-02 by this plan's doc review, then **settled the same day**. The original credential decision was **wrong on its facts** and is withdrawn; everything else in the entry stands. At Hermes `7f4d15515` — the revision installed on the operator's machine, `~/.hermes/hermes-agent` at `HEAD` — the WebSocket upgrade reads its credential only from query parameters (`_ws_auth_reason`, `hermes_cli/web_server.py:14443-14524`, enforced for `/api/ws` at `:15609-15617`) and never inspects a header. The two lines originally cited as witnesses govern HTTP: `:384` is the legacy Bearer branch of `_has_valid_session_token(request: Request)`, behind the preferred `X-Hermes-Session-Token`; `:398` is a query-token check restricted to `/api/files/download`.

The replacement is now decided. Gate selection is not an operator flag: `should_require_auth` (`:437-460`) returns true for any bind host that is not `localhost`, `127.0.0.1`, or `::1`, and the legacy `--insecure` escape hatch is accepted but **ignored** since the June 2026 `hermes-0day` campaign. The default bind is loopback (`start_server`, `:17059-17061`), so a default Hermes is ungated and takes `?token=`. Gated mode also turned out to be fully reachable for a dial-don't-launch client, contrary to the review's initial doubt: a complete RFC 8252 native-app flow exists (`dashboard_auth/routes.py:289`, `:841`, `:799`, `:894`) minting single-use 30-second tickets. **v0.1 targets loopback `?token=` only**, with remote/gated attach queued. See the [plan doc review](../reviews/2026-08-02-talaria-v0-1-prototype-plan-doc-review.md).

**Decision.** The [v0.1 implementation plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md) fixes sixteen KTDs; the load-bearing subset: wire models are frozen dataclasses with explicit decoders, no Pydantic in the domain core. Attach credentials are acquired env-first via `HERMES_DASHBOARD_SESSION_TOKEN`, then `~/.talaria/credentials` at `0600`, then a hidden prompt, and never appear in argv — the acquisition chain survives the amendment above. The credential rides the URL as `?token=` and is minted by a `CredentialProvider` invoked **on every dial including every reconnect**, never fetched once at startup: a gated ticket is single-use with a 30-second life, so building the seam per-connection now is what keeps the deferred remote path from becoming a reconnect rewrite. Because the credential must ride the URL, `redactUrl` (`src/record/redact.ts:106-122`) becomes the load-bearing control keeping it out of the frame log's `endpoint` field; it covers the bare `token` key but neither `ticket` nor `internal`, so the Python port is a **strict superset** of the TypeScript redactor with those two added and the divergence enumerated in a test. Configuration lives in a two-level `~/.talaria` directory (repo-local `./.talaria` overrides it), read only by `talaria/config.py`. The composer is Textual's `TextArea` in plain-text configuration with Enter-submits and Ctrl+J-newline. The status contract v1 delivers one JSON document on the child's stdin, renders rows as literal text, and gives the child a default-deny environment with an operator allowlist. prompt_toolkit is the named Python fallback presentation layer, assessed before the Textual gate verdict. Milestone 2 transport is asyncio plus `websockets`, with every RPC lost to a disconnect resolved as unknown-outcome, never success. One `FrameSource` seam feeds replay and live identically, and the compatibility baseline is pinned checked-in data — mutating gateway methods are never invoked as probes.

**Rejected alternatives.** Pydantic at the wire (its coercion masks exactly the malformations R5/R37 exist to surface); the query-parameter token (reaches URLs, the frame-log `endpoint` field, and process listings); Textual's `Input` (single-line, fails multi-line R12); Shift+Enter as the newline binding (not deliverable without kitty-protocol support the matrix does not assume); `aiohttp` (a larger dependency for the same client capability); argv or shell delivery of the status payload (R18 forbids interpolating session data into a command).

**Rationale.** Each decision carries its tradeoff, falsifier, and requirement trace as KTD1–KTD14 in the plan, which is the full record; this entry is the journal mirror.

**Learning.** The withdrawn credential claim is the generalizable one: both of its "independent witnesses" were real lines in the right file that answered a *different question* — HTTP request auth, not WebSocket upgrade auth. Two citations agreeing is not corroboration when both are drawn from the same misread. The rule this repository now applies: cite the function that the caller you care about actually invokes, and name that caller in the citation — here, `/api/ws` calls `_ws_auth_ok`, so nothing outside `_ws_auth_reason` could have settled it. The plan had correctly labelled this its least-proven external claim and scheduled a live test, which is why the error cost a review cycle instead of a milestone.

**Revisit when.** The Textual gate fails (composer and streaming decisions route to the assessed fallback); the live attach in U7 either confirms or refutes the loopback `?token=` form; the operator wants a remote gateway, which activates the queued `GatedTicketProvider` work; Hermes adds header acceptance to the WS upgrade, which would restore the original decision; the U5 memory growth curve shows a slope that makes domain-transcript eviction a requirement; or decoder boilerplate materially outgrows its value in milestone 2, which reopens the model-library question for the wire boundary only.

---

## 2026-08-02 — v0.1 proceeds without further independent-review ceremony

**Author.** Operator, recorded by the v0.1 plan doc review

**Decision.** The inherited finding DR15 — that the independent review panel over the v0.1 requirements dispatched three units but recorded only one completed final response — is **overridden**, and implementation may begin. It does not block `/work`.

**Rationale.** DR15 is a receipt-keeping gap, not a review gap, and it is unsatisfiable as written: the requirements reconciliation itself records that "the panel-independence property has no mechanical verifier in the available tooling," so no re-run can close it either. The substantive obligation has been discharged well past its bar — the requirements carry a doc review plus an external reconciliation, and the plan carries a doc review plus a two-engine external panel (`codex/gpt-5.6-sol` and `ollama-cloud/kimi-k3`, both at maximum reasoning effort) whose findings were verified against primary sources and applied across two rounds, the second of which was a check on the first round's own corrections.

**Rejected alternatives.** Re-running the panel to produce better receipts (buys a receipt, not information, against a checker that does not exist); leaving the block in place (indefinite, since nothing can clear it).

**Learning.** A process gate whose verifier was never implemented becomes a permanent block that looks like diligence. When a finding requires a mechanical check, confirm the checker exists before the finding is allowed to gate anything; otherwise record it as advisory from the start.

**Revisit when.** Talaria gains contributors beyond the operator, or a mechanical panel-independence verifier lands in the saga tooling — at which point review ceremony has a real reader and a real check.
