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

## 2026-08-03 — Prettier is scoped to the TypeScript bootstrap; docs are excluded

**Author.** v0.1 segment 1, unblocking the `check` CI job

**Decision.** `.prettierignore` now excludes `docs/`, `.venv`, and `__pycache__`. Prettier governs the superseded TypeScript bootstrap under `src/` and nothing else; ruff owns formatting for the Python tree, and the `docs/` tree is formatted by hand. The exemption is scoped to `docs/`: the root markdown files — `README.md`, `AGENTS.md`, `CLAUDE.md` — stay Prettier-governed, so the repository's front door keeps a mechanical formatting check.

**Rationale.** Prettier is here on a lease. It arrived with the TypeScript bootstrap and leaves with it (ADR-0004), so scoping it to the tree it was chosen for is the load-bearing argument: a formatter for a superseded language should not be the authority on Python-era documentation it was never configured to understand. The `check` job's `prettier --check .` step had been failing on `main` since `064967b` — not on TypeScript, which typechecks clean and passes all 45 vitest tests, but on ten markdown and JSON files under `docs/`.

A second, narrower reason applies to exactly one of those ten files. `docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md` is byte-pinned by the doc-review artifact's `target_sha256_after: 010ff5f6…`; running `prettier --write` would rewrite it and invalidate the hash that proves the review gate was satisfied. That is a real hazard, but it is one file, not a general property of `docs/` — the other pinned target on record, the requirements brainstorm, formats clean and was never in the failing set. The scoping decision would stand on the tool-lifecycle argument alone.

**Rejected alternatives.** `prettier --write .` (invalidates the pinned plan hash described above, and would keep doing so every time that plan is regenerated). Deleting the `check` job (it still carries real signal — typecheck plus 45 tests — until `src/` is removed). Per-file ignore entries (every new document under `docs/` would fail CI until someone remembered to add it, which is how this failure accumulated in the first place).

**Revisit when.** The superseded `src/` tree is removed, at which point Prettier and the entire `check` job leave the repository with it. Also revisit if documentation formatting ever needs to be mechanically enforced — that would call for a markdown-aware linter chosen for the Python era, configured to leave pinned artifacts alone.

## 2026-08-02 — The domain view model is an immutable snapshot plus a set of change markers

**Author.** v0.1 unit U3

**Decision.** Every projection emission is a frozen dataclass (`talaria.domain.projection.Snapshot`) carrying the transcript view, the sub-agent view, the prompt view, and the KTD5 status payload, plus `changed: frozenset[str]` naming which of those four regions differs from the previous emission. The UI skips untouched regions without the domain knowing what a widget is.

**Why U3 decided this and not U5.** ADR-0002 left the view-model shape open and assigned it to "the first vertical slice's re-render-cost evidence", which the plan pointed at U5. But U3 has to ship `projection.py`, the status payload, the terminal-read views, and the UI view models *before* U5 exists, so as ordered the question could not be answered where it was asked. U3 chooses; U5 measures and records the number.

**Rationale.** AE2 requires that replaying one corpus twice produces an identical projection. Comparing two values is trivial; comparing two mutation histories is not. In-place mutation would have made the determinism requirement awkward to test at exactly the point where it matters most.

**Rejected alternatives.** In-place mutation with dirty flags (cheaper per frame, but AE2's comparison becomes a bespoke differ nobody trusts). Emitting a diff instead of a snapshot (smaller payloads, but the UI then has to reconstruct state, which puts a second copy of the domain in the presentation layer — the thing ADR-0002 exists to prevent).

**Cost.** One allocation per emission. That is precisely what U5 is asked to measure against KTD14's thresholds; if the measurement is bad, the ADR records it with the evidence rather than U3 having guessed silently.

**Revisit when.** U5's gate publishes its re-render cost and memory growth curve.

## 2026-08-02 — Cancelled is a sticky turn state, and sub-agent rows outlive their turn

**Author.** v0.1 unit U3, from the reconciliation-catalogue read

**Decision.** Two deliberate divergences from Hermes, both recorded in the catalogue with named tests.

`turn == "cancelled"` survives until the next `message.start`. Hermes's `interrupted` latch does the same thing internally (`ui-tui/src/app/turnController.ts:989` is the only site that clears it), but it settles the *displayed* status to `ready` immediately. R4 requires the transcript to show that a turn was cancelled rather than that it ended, and KTD5's status enum already has a `cancelled` member — so a status payload sampled after a cancelled turn reports `cancelled`, not `idle`.

Sub-agent rows are cleared by the next `message.start` rather than at turn end. Hermes drops them at `idle()` and archives the fan-out to disk via `spawn_tree.save`. Talaria has no archive to move them into, because R17 forbids authoring sub-agent state and `spawn_tree.save` is the concrete method that rule excludes.

**Rationale.** The second decision is what makes AE14 testable rather than vacuous. AE14 asks that a terminal sub-agent row survive a late progress event; if rows vanish at turn end, the late event has no row to fail to overwrite and the guard is never exercised.

**Rejected alternatives.** Clearing rows at turn end and testing the guard only mid-turn (matches Hermes, but leaves the AE14 sequence untested where it actually occurs — after `message.complete`). Building a Talaria-side spawn archive (violates R17, and R17 exists because a read-only client is the whole standalone-client boundary in ADR-0001).

**Cost.** One turn's fan-out is retained after the turn ends. Queued at P2 alongside the reasoning-buffer decision, with the U5 growth curve as the input.

**Revisit when.** U5's memory growth curve makes domain-side eviction a requirement, or a live session shows a fan-out large enough for the one-turn retention to matter.

## 2026-08-03 — Textual passed its gate; the presentation layer is settled and ADR-0005 is accepted

**Author.** v0.1 unit U5 — the replay-driven Textual shell and framework validation gate

**Decision.** Textual 8.2.8 is Talaria's presentation layer for v0.1, recorded as [ADR-0005](../../platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md), `accepted` by the operator on 2026-08-03. The gate ran three times that day — `pass` on a gate that was measuring itself, `fail` on the repaired gate, and `pass` on the repaired gate once the three defects it found were fixed. Acceptance rests on the third run and its thirteen checks; measurements and the full sequence are in [Textual validation gate results](../analysis/2026-08-03-textual-validation-gate-results.md).

**The gate is a shipped command, not a one-off script.** `uv run talaria gate --corpus <recording> --deltas 50000` replays both corpora through the real `TalariaApp`, compares every measurement against KTD14's thresholds, prints the whole record as JSON, and exits non-zero on a fail verdict. Re-establishing the verdict after a Textual upgrade is therefore one command rather than an archaeology exercise.

**Rationale.** The plan's central bet was that the prototype and the gate are the same build, because a gate that measures a purpose-built harness proves the harness. That held: every number came from the app the operator runs.

**Two passes, not one, and the reason is a correction.** The first version of the gate measured only an unbounded replay, and reported 1 render tick — because the domain reducer drains 53,516 frames in 2.3 seconds, long before a 50ms coalescing tick can fire more than a handful of times. That is a real result about the reducer and a meaningless one about the renderer, and reporting it alone would have claimed a renderer verdict on reducer evidence. The gate now also replays on the corpus's recorded cadence, scaled to a 60-second window, where the render loop runs 2,907 times against a live stream.

**Answers to two questions earlier ADRs deferred.** ADR-0002 asked for the re-render cost of U3's immutable-snapshot view model: it is not the bottleneck, because one snapshot is allocated per flush rather than per frame. ADR-0004 warned that transcript virtualization would have to be owned explicitly: true, and it cost about 120 lines, holding 501 mounted widgets across a corpus that produced 4,454 lines. It also turned out to be where all three gate failures lived, which is the honest version of "cheap".

**Rejected alternatives.** Switching to `prompt_toolkit` anyway (trades a measured framework for an assessed one). Deferring the framework decision further (the deferral existed to buy evidence; the evidence exists).

**Cost.** A `textual>=8.2.8,<9` pin, and a namespace shared with a large private framework surface — see the LEARNINGS entry on `_closing`.

**Revisit when.** The Textual pin is widened, a real terminal host exercises what the headless gate could not, or the recorded steady-state memory slope of 0.23 MB per 1,000 frames starts to matter in a real session. That slope rose from 0.11 when the reconciliation defect was fixed, because the pane now does work it had been skipping — see the LEARNINGS entry on defects that suppress the cost of the work they suppress.

## 2026-08-03 — The projection publishes its committed boundary; a bounded window tracks a position, not a tally

**Author.** v0.1 milestone-1, closing the U5 gate failure

**Decision.** Two rules for any renderer that diffs against `TranscriptView`.

1. **The domain names the settled region; the renderer never infers it.** `TranscriptView.committed_lines` is the index where the provisional streaming block begins. A renderer may skip re-examining lines below it and must re-examine everything above it, every tick. Inferring "settled" from two snapshots agreeing on a line is invalid, because the provisional block sits *after* the committed lines and moves down whenever an entry commits mid-stream.
2. **A bounded window stores where it starts, not how much it has evicted.** `TranscriptPane._top` is an absolute index. `condensed_count` is derived from it, so mounted-plus-condensed equals the transcript length by construction, and the number can fall when the window is re-derived further up.

**Rejected alternatives.** *Reconciling the full window each tick* — correct, and O(transcript) per 50ms tick, which is the cost KTD14 exists to bound. *Making notice lines non-transient* — proposed in the defect report and does nothing, because the notice lines were never transient; the streaming block is what moves. *Placing the provisional block before the committed lines* so the projection is append-only — contradicts KTD10, which requires a mid-stream `read_terminal` to describe the screen the operator is actually looking at.

**Rationale.** The renderer cannot compute the boundary from the data it receives, and every attempt to guess it is a guess about immutability — exactly the class of assumption that should be stated by the party that owns it. The cost is one integer per snapshot. Adding it to a frozen dataclass in the domain does not import a framework, so ADR-0002 is untouched.

**Cost.** `TranscriptView` gains a field that every hand-built view in a test must now either pass or default. The default is `0` — "assume nothing is settled" — so omitting it makes a consumer do more work rather than skip work it should have done.

**Revisit when.** The transcript grows a second provisional region (an editable draft in the scroll-back, say). Then one integer stops being enough and the projection should publish spans rather than a boundary.

## 2026-08-03 — The gate's corpora are cited by digest, and the stress corpus is generated rather than committed

**Author.** v0.1 unit U5

**Decision.** Neither gate corpus enters version control (R29). The recorded session is cited by opaque label, sha256 and frame count. The 53,516-frame stress corpus is *generated* from a seed by `talaria.replay.stress.build_stress_corpus`, and the results doc records the seed and the digest rather than shipping the file.

**Rationale.** A checked-in corpus is a provenance claim nothing verifies — it is whatever was committed, and a later edit is invisible. A seed plus a digest is checkable in one command: regenerate, compare. It is also the only form of provenance compatible with a public repository whose corpora may carry session content the redaction deny-set missed.

**Why the stress corpus is synthetic at all.** The recorded session proves the interface handles *actual* traffic (R30). It cannot carry the thresholds, because its size is whatever the session happened to be, and a threshold measured against an accidental number is not a threshold. The two corpora answer different questions and the results doc keeps them separate.

**Rejected alternatives.** Committing a small real corpus (R29 forbids it, and small defeats the purpose). Generating without a seed (reproducibility is the whole claim). Citing a local path (the public-context rule forbids it, and a path is not evidence anyway).

**Cost.** Anyone reproducing the gate must either supply their own recording or accept that the recorded-session half is skipped. The command degrades honestly: without `--corpus` it runs the stress passes and omits the recorded-session checks rather than pretending.

**Revisit when.** A corpus is needed by a consumer that cannot run the generator — for example a cross-language comparison that has no Python.

## 2026-08-03

### Redact URL credentials by position, and do not redact URL paths at all

**Author.** v0.1 milestone-1 integration, after external review of the redaction boundary

**Decision.** The recorder withholds credentials from the two URL positions that are *defined* to hold them — userinfo and named query parameters — and withholds nothing from a URL's path, even when the path is known to carry a bearer capability.

**Rejected alternatives.** A Hermes-shaped rule for `/devtools/browser/<segment>` was rejected as worse than doing nothing: it protects exactly one known shape while creating the appearance that paths are handled, so the next capability-bearing path leaks silently against a reader's belief that it cannot. That is the identical staleness failure the method deny-set was already bitten by, and the reason the key-name net exists behind it. A "high-entropy path segment" heuristic was rejected because it redacts the commit SHAs, content hashes and UUID resource ids the corpus exists to study — over-redaction is a different failure, not the safe direction, which is the same principle that keeps the sixteen non-credential `token` key names out of the net.

**Rationale.** Userinfo and query parameters have credential semantics independent of any application: `user:password@host` is a credential by RFC, and a query key named `token` or `ticket` is one by the gateway's own protocol. A path segment has no such semantics — it is a capability only because some specific service decided it was, which means any rule covering it is a bet on one service's URL shape. Redaction rules that encode a bet age badly and hide their own staleness.

**Cost, stated plainly.** A capability-bearing path is recorded verbatim. This is *not* mitigated by loopback: loopback is the default CDP host, not a constraint, and Hermes documents `BROWSER_CDP_URL` to operators as accepting any Chromium-family browser, so remote CDP is an ordinary configuration today. The residual exposure needs an operator who has configured a remote endpoint *and* a corpus that leaves the machine.

**The candidate fix, and what blocks it.** Withholding the path of non-loopback `ws`/`wss` URLs carries no service-specific shape and costs nothing on study data, since SHAs and resource ids live in `http`/`https` document URLs. It is blocked on *sequencing*, not on harness cost: the comparator would have to encode an expectation about a redactor rule the remote-attach work has not yet defined. The comparator change itself is around ten lines mirroring the existing query-key allowance — an earlier version of this entry priced it as a change to the parity relation, which overstated it and would have caused whoever picked it up to defer it again.

**Revisit when.** Remote gateway attach is implemented (the natural place to extend the comparator), or a non-loopback host is observed in a recorded URL. Both triggers and the mechanical check are in `QUEUED.md`. The trigger this entry originally carried — "remote CDP becomes supported" — was wrong: it had already happened, so it could never fire.

## 2026-08-03

### Tests over real subprocesses assert invariants, not schedules

**Author.** v0.1 milestone-1 integration, from external review; written before segment 3 rather than after its first flake

**Decision.** A test that spawns a real process, or shares a resource with a background loop, asserts a load-independent invariant. Where it needs a specific outcome, it *drives* the precondition rather than hoping the scheduler supplies it.

**The evidence this is a family, not an incident.** Three in this suite already, all with the same mechanism — an unstated timing assumption that holds on an idle machine and dissolves under CI load, where the window is usually exactly the cost of a process spawn:

- `test_the_status_command_runs_and_renders_under_replay` asserted `outcome == "ok"` on a tick fired while the app's own status loop (`talaria/ui/app.py:154`, which ticks before its first sleep) still had one in flight. The KTD5 guard correctly returned `overlapped_skip`. The guard was working; the test assumed it was the only caller.
- An overlap test configured a 0.3s timeout against a child that slept 0.2s, leaving 0.1s for a Python interpreter to start.
- `test_overlap_at_most_one_child_ever` reported zero successful invocations of three on a CI leg. Still unexplained, deliberately not folded into the first item's fix — a fix that explains one member of a family and absorbs the other closes an open defect on a resemblance.

**The two rules.**

1. *Assert the invariant, not the schedule.* "At most one child ever ran" is load-independent. "This particular attempt returned `ok`" is a bet on scheduling. Both were available in the status case and only one of them is a test.
2. *A test sharing a resource with a background loop must stop the loop or account for it.* Accounting can be a bounded retry, but the comment must say which background actor it is racing, or the next reader will read the retry as superstition.

**Rejected alternative.** Reruns, retry plugins, or marking the tests flaky. That treats an unstated precondition as noise, and it hides exactly the class of defect where the production code has a real race — the reason the still-unexplained third instance is kept open rather than retried away.

**Why now.** Segment 3 (U7 transport, U8 remote attach, U10 acceptance against a launched gateway) adds a live socket, reconnect timers, and PTY-driven credential prompts: the highest concentration of background actors and real process spawns in the plan. The cost of this convention is a comment and a driven precondition; the cost of discovering it there is a CI flake that reads as a transport bug.

**Revisit when.** A fourth instance appears despite the rule, which would mean the rule is being read as advice rather than as a precondition to state, or a test genuinely needs to assert a timing property — in which case it should measure a distribution, not a single attempt.

## 2026-08-03

### The credential is a per-dial provider, and the correlation key carries a connection epoch

**Author.** v0.1 milestone-2, unit U7 (live transport)

**Decision.** Two shapes in the live transport are fixed here, and neither is negotiable by a later unit without a new entry.

1. **Credentials are acquired through `CredentialProvider.acquire()`, called on every dial including every reconnect.** v0.1 ships `LoopbackTokenProvider` only. The environment and the credential file are re-read per call, so a rotated token is picked up by the next reconnect; a credential that came from the interactive prompt is held in memory and never re-prompted.
2. **In-flight RPCs are keyed by `(connection epoch, request id)`, and request ids restart at 1 on every epoch.** A reply read from a connection that is no longer current is counted and discarded. Every call interrupted by a disconnect resolves to `unknown` — never to an error and never to a success.

**Rejected alternatives.** Fetching a token once at startup is correct for the loopback `?token=` form and only for it, and it makes the reconnect path silently depend on the credential being a fixed string — adding gated `?ticket=` support later would then mean rewriting reconnect, which is the most concurrency-sensitive code in the client. Keying replies by request id alone is correct until a reconnect races a late reply, at which point it converts an honest `unknown` into a reported success; that is the one failure R35 names explicitly. A globally monotonic id counter would make the epoch key look correct while making its guard permanently unreachable (see LEARNINGS).

**Rationale.** Both decisions cost one small object each and buy the property that the *shape* of the code does not change when the deferred work lands. The provider is one interface and one class; the epoch is one integer and a tuple key.

**Cost, stated plainly.** Two counters and a per-dial round trip that, for the loopback provider, reads one environment variable and possibly stats one file. Nothing measurable.

**Revisit when.** Remote or gated attach lands (`GatedTicketProvider` is specified in QUEUED.md), or the client ever needs more than one connection open at a time — at which point "the current epoch" stops being a single value and the discard rule needs restating.

### The transport publishes connection state by callback, and `connect_failed` is a cause, not a state

**Author.** v0.1 milestone-2, unit U7 (live transport)

**Decision.** `LiveSource` reports connection lifecycle through an `on_connection(state, detail)` callback rather than by injecting synthetic frames into the frame stream. R35's four conditions map onto KTD5's five frozen states as: authentication failure → `auth_failed`; initial connection failure → `disconnected` with `failure_kind == "connect_failed"` and no epoch ever opened; disconnect → `disconnected` after an epoch was opened; reconnect → `reconnecting`. The cause travels beside the state as a `detail` string. `auth_failed` survives `close()`.

**Rejected alternatives.** Emitting a synthetic `gateway.disconnected` event would carry the state into domain state through the existing reducer with no new plumbing — and would put a frame in the recorded corpus that no gateway ever sent, poisoning the one artifact whose entire value is that it is a faithful record. Adding a sixth `connect_failed` member to `ConnectionStatus` would be a `version: 2` change to the status contract KTD5 froze at the first commit, for a distinction that an accompanying string carries adequately.

**Rationale.** The seam between transport and domain is narrow on purpose (KTD3); a second channel for a second kind of information is cheaper than widening the first one, and it keeps "what arrived on the wire" and "what the transport is doing" separable in the corpus.

**Cost, stated plainly.** The transport re-declares `LiveConnectionState` rather than importing `ConnectionStatus` (ADR-0002 keeps the domain out of the transport's imports), so two spellings of one enum exist. `tests/transport/test_reconnect.py::test_the_transport_and_domain_connection_enums_are_identical` is the price, paid once.

**Revisit when.** A third consumer needs transport state and the callback starts growing parameters, or the status contract moves to `version: 2` for an unrelated reason — the natural moment to fold `connect_failed` in properly.

### The credential file is TOML, and looser-than-0600 is refused before it is read

**Author.** v0.1 milestone-2, unit U7 (live transport)

**Decision.** `<config_dir>/credentials` is a TOML document with a `token` key and an optional `url` key. Any file whose mode has a group or other bit set is refused with an error naming the mode and the `chmod` that fixes it; the check runs before the file is opened. Owner-execute (`0700`) is not refused — it exposes the file to nobody the way `0640` does.

**Rejected alternatives.** A bare single-line token file is what an operator would produce with `echo`, and it is genuinely more convenient — but it cannot carry the endpoint, and supporting both formats means guessing which one a file is, which is exactly the ambiguity a credential path should not have. Reading the file first and checking permissions afterwards produces a friendlier error and defeats the check: a file the whole machine can read has already leaked, and opening it anyway is the one action that makes the leak useful.

**Rationale.** KTD15 already establishes TOML as this repository's configuration language and `talaria/config.py` as the only reader of `config.toml`; using a second format for the file next to it would be a decision with no argument behind it. Whitespace is stripped from the value because `echo "$TOKEN"` appends a newline, and the gateway's constant-time comparison rejects it with no useful message.

**Cost, stated plainly.** An operator who writes the file by hand must type `token = "..."` rather than the bare value, and the error for a malformed file says so explicitly.

**Revisit when.** A second credential form needs storing (a refresh token for the deferred RFC 8252 native-app flow), which the TOML shape already accommodates — or an OS keychain becomes the primary store, at which point this file becomes a fallback and its format matters less.

### An agent that deliberately breaks code works in a disposable clone, never in the shared checkout

**Author.** v0.1 milestone-2, unit U7 (live transport) — raised by the milestone-1 review agent after a misattribution

**Decision.** Any agent whose method is deliberate breakage — mutation testing, "prove this test can fail", injected-failure verification — extracts its own copy with `git archive <ref> | tar -x -C <scratch>` and works there. It does not mutate files in the operator's working tree, even transiently, and it does not create scratch test files under `tests/`. **The rule is carried in the agent's prompt, not only here.** A subagent never reads this file; its entire world is the text it was spawned with, so a standard recorded only in the journal reaches the humans and the parent and no one who has to follow it. Treat the prompt as a configuration surface: anything an agent must do belongs in the prompt text verbatim, and the omission recurs once per agent that is not told.

**Rejected alternatives.** Coordinating between adversaries with a snapshot protocol — announce, mutate, restore, announce — does not work, because **snapshot-and-restore is only sound for a single writer.** Agent A snapshots, Agent B mutates the same file, A restores from its snapshot and writes back a baseline that already contains B's edit — or erases it. Both agents `diff` against their own snapshot, both report a clean restore, and the file is still wrong. That is not a hypothetical; it is the state this incident produced. Serializing adversarial agents so only one mutates at a time is sound, but it buys correctness by deleting the parallelism the verification panel exists for. Doing nothing and relying on each agent to restore what it broke is what we did; it held for each agent's own mutations and did not survive a second writer.

**Rationale.** Two U7 verification agents mutated `talaria/transport/rpc.py` concurrently. One saw the other's breakage, assumed a closed world of itself plus the one agent it knew about, attributed the damage to the milestone-1 reviewer, and deliberately left the file broken so as not to disturb what it believed was that agent's in-flight experiment. The reviewer had never written to the tree at all and proved it: `rpc.py` does not exist on the branch it reviewed. So a real defect — the epoch guard deleted from the one function whose job is to never confirm an unconfirmed call — sat in the checkout preserved by an act of care aimed at the wrong thing. A guard protecting a false premise reads as diligence and costs more than no guard, because the next reader trusts it.

Two things generalize past the incident. First, **an agent's model of "who else is writing here" is whatever its prompt happened to mention, which is almost never the real set** — the reporting agent knew about neither the peer it blamed nor its own sibling. That is why the fix cannot be a coordination protocol between agents: they cannot enumerate each other. Second, **the danger is worst for exactly the files an adversary is most likely to be pointed at.** `rpc.py` was untracked, so there was no committed baseline and no `git restore`; the only recovery path was one agent's private scratch backup. This inverts the usual intuition — the safest file to break in a shared tree is a committed one, and the most dangerous is the in-progress uncommitted file the implementer is still writing, which is precisely the file under review. A disposable copy is not tidiness there; it is the only thing standing between an experiment and work git cannot recover.

**Cost, stated plainly.** About 200-500ms and a working tree's worth of disk per agent, and the agent's findings cite paths inside its clone, so line numbers must be translated back before they are actionable. Both are cheap against one contaminated verification round.

**Revisit when.** Verification agents stop mutating code to prove their point, or the harness gains first-class per-agent worktrees that make the extraction implicit rather than something each prompt has to ask for.
