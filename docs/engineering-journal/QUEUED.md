# Queued Work - talaria

> Future work with priority, rough effort, and worth-it-when triggers.

## P0

### ~~Run the Textual validation gate~~ — CLOSED 2026-08-03, verdict **pass**

**Author.** Reconsidered language and TUI framework analysis
**Priority.** P0
**Effort.** Medium
**Worth it when.** Before adding product behavior beyond the current TypeScript bootstrap shell.
**Context.** Drive one bounded Textual projection from a framework-neutral Python reducer and the existing frame-log contract. Prove coalesced streaming, bounded transcript mounting, scroll anchoring, deterministic `run_test()` and `Pilot` behavior, selected pseudo-terminal behavior, framework-independent domain state, strict typing and linting, and clean `uv tool install` launch.
**Amended 2026-08-02 by ADR-0004.** The fallback is no longer Go with Bubble Tea. The language is settled as Python, so a framework failure selects a different Python presentation layer — which is why identifying one is now a prerequisite rather than a contingency. Choose the vertical slice from the Hermes terminal UI feature inventory rather than from a generic renderer stress list, so the gate produces a prototype instead of a harness.

**Closed by.** Unit U5 of the [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md). All ten threshold checks passed; measurements, corpus identities and explicit non-claims are in [Textual validation gate results](../analysis/2026-08-03-textual-validation-gate-results.md), with the machine-readable record at [`evidence/2026-08-03-textual-validation-gate.json`](../analysis/evidence/2026-08-03-textual-validation-gate.json). Re-runnable as `uv run talaria gate --corpus <recording> --deltas 50000`, which exits non-zero on a fail verdict. Headline numbers: 501 mounted line widgets against a ceiling of 600, 35.6 MB resident growth against 300 MB, 15.2 coalescing flushes per second under sustained streaming against 25, and zero content loss across 24 checkpoints. The framework choice is drafted as [ADR-0005](../../platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md), `proposed` pending operator acceptance.

**What the gate did not measure, so it is not mistaken for closed.** Real-terminal behaviour (tmux, a live emulator, IME composition) was not exercised — the run is headless, and those belong to U10's acceptance. The fallback assessment of `prompt_toolkit` stands unretired.
**Refs.** [Reconsidered language and TUI framework analysis](../analysis/2026-08-02-language-and-tui-framework-analysis-reconsideration.md), [ADR-0004](../../platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md), [ADR-0005](../../platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md)

### ~~Identify and assess a Python fallback presentation layer~~ — CLOSED 2026-08-02

**Author.** ADR-0004
**Priority.** P0
**Effort.** Small
**Worth it when.** Before the Textual gate runs, so a failure has somewhere to go.
**Context.** Every analysis in the chain evaluated Textual as the only Python candidate; the others were all in other languages. Settling the language on Python therefore leaves the fallback set unevaluated. If Textual fails on transcript cost or pseudo-terminal correctness, nobody has assessed what replaces it. Name at least one alternative and check it against the same gate criteria — enough to know it exists and is plausible, not a full comparative analysis.
**Closed by.** Unit U4 of the [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md), discharging PC8/KTD12. `prompt_toolkit` is assessed against the gate criteria to plausibility depth (with `urwid` recorded as secondary candidate) in [Python fallback presentation layer](../analysis/2026-08-02-python-fallback-presentation-layer.md), dated before the U5 gate verdict. Verdict: plausible on all five assessed criteria — bounded transcript strategy, streaming coalescing, multi-line editing/bracketed paste, headless test story, install cleanliness.
**Refs.** [ADR-0004](../../platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md), [Python fallback presentation layer](../analysis/2026-08-02-python-fallback-presentation-layer.md)

### Prove the Hermes transport seam

**Author.** Project bootstrap
**Priority.** P0
**Effort.** Medium
**Worth it when.** The prototype shell is ready for the first real integration slice.
**Context.** Talaria needs capability discovery, session lifecycle, prompt streaming, cancellation, and approval handling before UI work can be judged against real Hermes behavior.
**Refs.** [Project direction](../analysis/2026-08-01-hermes-tui-project-direction.md)

## P1

### Make the macOS checks required status checks on `main`

**Author.** v0.1 scaffold code review, 2026-08-02
**Priority.** P1
**Effort.** Small
**Worth it when.** Before the next unattended run merges anything — this is the only remaining gap between a red check and `main`.
**Context.** `.github/workflows/validate.yml` runs the `python-check` job on macOS arm64 with no `continue-on-error`, so a failure fails the workflow run. That is not the same as blocking a merge, and nothing in a workflow file can be. Verified 2026-08-02: `gh api repos/infiquetra/talaria/branches/main/protection` returns HTTP 404 "Branch not protected" and `gh api repos/infiquetra/talaria/rulesets` returns `[]` — `main` has neither branch protection nor a ruleset, so a pull request with a red check can still be merged.

An earlier version of the workflow carried a `required: true` matrix key, which reads as though it configured this; it only negated `continue-on-error`. That key has been removed, and job names are now free of incidental matrix values so the check names (`python-check (3.12)`, `python-check (3.13)`) stay stable if protection is configured against them.

**Left to the operator deliberately.** Repository governance is not something an unattended run should change on the operator's behalf, and requiring a check name that does not match exactly would deadlock every merge — including the ones the run was authorized to make. Until it is configured, the gate is enforced behaviorally: this run does not merge without observing the required legs green.

### Decide the trust boundary for repo-local `.talaria/config.toml` before U6 executes a command

**Author.** v0.1 scaffold code review, 2026-08-02 (rated P2 advisory by the review; carried at P1 here because the trigger is the next milestone)
**Priority.** P1
**Effort.** Small
**Worth it when.** Before U6's status runner executes `status.command` — that is what turns this from a precedence question into an execution path.
**Context.** KTD15 ranks a repo-local `./.talaria/config.toml` above the operator's global `~/.talaria/config.toml`. Nothing in the scaffold executes anything, so there is no vulnerability today. But KTD5 specifies `status.command` is executed on an interval, and the precedence chain is being locked in now: an operator who clones an untrusted repository, `cd`s into it, and runs `talaria` would execute a command supplied by that repository's contents.

The review named two viable resolutions: require a repo-local config file to be explicitly trusted before it is honored, or exclude command-valued keys from the repo-local level while leaving the rest of KTD15's order intact. Either is a change to a key technical decision the plan settled, which is why it is recorded for decision rather than made unilaterally mid-run.
**Refs.** [v0.1 plan KTD15 and KTD5](../plans/2026-08-02-talaria-v0-1-prototype-plan.md)

### Build the stable screen model

**Author.** Project bootstrap
**Priority.** P1
**Effort.** Medium
**Worth it when.** Transport fixtures exist and event/state transitions can be tested independently of a terminal.
**Context.** Stable rendering and reduced flicker depend on state-driven rendering rather than direct callback-driven writes.

### ~~Read Hermes's turn controller and complete the reconciliation-rule catalogue~~ — CLOSED 2026-08-02

**Author.** ADR-0003
**Priority.** P1
**Effort.** Small
**Worth it when.** Before the normalization layer is written, since a rule discovered afterwards is a defect found in production.
**Context.** The 2026-08-02 read covered `ui-tui/src/app/createGatewayEventHandler.ts` and found its reusable content is a set of short rules rather than portable machinery. That handler delegates to `ui-tui/src/app/turnController.ts` (1,092 lines) at more than twenty call sites, and only its call surface has been read. The rule catalogue ADR-0003 depends on is incomplete until the controller is read at a pinned revision.
**Closed by.** Unit U3 of the [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md), discharging R37. `turnController.ts` is now read in full at `7f4d15515` and the catalogue is [Hermes reconciliation-rule catalogue](../analysis/2026-08-02-hermes-reconciliation-rules.md): 38 rules, each with an explicit verdict (re-encode / re-encode with a change / drop) and a named test under `tests/domain/`. `tests/domain/test_reconciliation.py::test_every_catalogued_rule_names_a_test_that_exists` parses the document and fails if any rule names a test that does not exist, so the catalogue cannot rot quietly — which is the failure mode ADR-0003 names.

**What the controller read settled.** ADR-0003 left open whether the controller's engine would recover part of the reuse argument. It does not: its density is streaming *presentation* — segment assembly, tool-shelf coalescing, reasoning pulse timers, notice TTL machinery, markdown and diff de-duplication — none of which transfers to a plain-text client. Nine of the thirty-eight rules come only from the controller, and each is one to four lines. The catalogue recovers; the reuse argument does not.

**Two gaps found in Hermes rather than rules taken from it**, both recorded as catalogue entries RR-27 and RR-28: the gateway emits `.expire` for all four blocking bridges but the shipping terminal UI handles only two of them, and `approval.request` carries no `request_id` at all, so R8's keyed registry needs a synthesized key for approvals.
**Refs.** [ADR-0003](../../platform-specs/04-architecture/adrs/0003-talaria-re-encodes-hermes-tui-behavior.md), [Hermes reconciliation-rule catalogue](../analysis/2026-08-02-hermes-reconciliation-rules.md), [LEARNINGS](LEARNINGS.md)

### Add the sub-agent monitor

**Author.** Project bootstrap
**Priority.** P1
**Effort.** Medium
**Worth it when.** The gateway advertises enough delegation or spawn-tree state to make the pane meaningful.
**Context.** Sub-agent visibility is a primary UX goal.

### Attach to a remote (gated) gateway

**Author.** v0.1 prototype plan doc review, 2026-08-02
**Priority.** P1
**Effort.** Medium
**Worth it when.** The operator wants Talaria pointed at a Hermes running on another host. v0.1 is loopback-only by decision, not by limitation.
**Context.** Whether the gateway's auth gate is active is decided entirely by its bind host: `should_require_auth` (`hermes_cli/web_server.py:437-460` at `7f4d15515`) returns true for anything that is not `localhost`, `127.0.0.1`, or `::1`, and RFC1918 addresses count as public. The legacy `--insecure` escape hatch is accepted but **ignored** since the June 2026 `hermes-0day` campaign, so a remote bind cannot be un-gated — the gated path must be implemented.

It is fully reachable for a client that dials a gateway it did not launch, which the v0.1 plan initially doubted. The complete RFC 8252 native-app flow, verified at the pin: `GET /auth/native/authorize` (`hermes_cli/dashboard_auth/routes.py:289`) runs PKCE against a loopback redirect; `POST /auth/native/token` (`:841`) exchanges the code for `{access_token, refresh_token, token_type: "Bearer"}` explicitly intended for OS-keychain storage; `POST /api/auth/ws-ticket` (`:799`) turns that session into `{ticket, ttl_seconds: 30}`; the ticket goes on the `/api/ws` upgrade URL as `?ticket=`; `POST /auth/native/refresh` (`:894`) rotates. Tickets are single-use with a 30-second TTL (`hermes_cli/dashboard_auth/ws_tickets.py:42`).

**The transport seam for this already exists.** v0.1 ships `CredentialProvider` invoked on every dial including reconnects (KTD11), specifically so a per-connection ticket does not require rewriting reconnect. This work is a new `GatedTicketProvider` plus keychain storage and the PKCE loopback listener — not a transport change.
**Refs.** [v0.1 plan KTD11](../plans/2026-08-02-talaria-v0-1-prototype-plan.md), [ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md)

## P2

### Bound the domain transcript, not just the mounted widget count

**Author.** v0.1 unit U3 (reconciliation catalogue rule RR-21), 2026-08-02
**Priority.** P2
**Effort.** Medium
**Worth it when.** The U5 gate publishes its memory growth curve, or a real session runs long enough that resident memory becomes visible.
**Context.** KTD14 bounds *mounted widgets* (default 500) and explicitly leaves the domain transcript accumulating without eviction. U3 widened that gap on purpose in two places, and both are catalogued rather than hidden.

Hermes truncates its reasoning buffer at 80,000 characters by discarding all but the last 60,000 (`ui-tui/src/app/turnController.ts:778-780` at `7f4d15515`) and gates reasoning capture on a display setting. Talaria does neither, because R6 puts reasoning *presentation* out of scope while requiring that its *content* is never dropped, and both of Hermes's behaviours drop content. `tests/domain/test_transcript_state.py::test_reasoning_is_committed_at_turn_end_and_never_truncated` pins that with a 100,000-character block.

Talaria also keeps sub-agent rows past the end of their turn, where Hermes drops them at `idle()` and archives the fan-out to disk. Talaria cannot archive: R17 forbids authoring sub-agent state, so `spawn_tree.save` is not in its vocabulary (catalogue rule RR-32). Rows are cleared by the next `message.start`, so the bound is one turn's fan-out rather than a session's.

Neither is a leak today at prototype scale, and eviction interacts with replay determinism (AE2) and scrollback in ways that need the projection to exist first — which is exactly why the plan deferred it. The input to the decision is the growth *slope* the U5 gate records, not the endpoint.
**Refs.** [Hermes reconciliation-rule catalogue](../analysis/2026-08-02-hermes-reconciliation-rules.md) rules RR-21 and RR-32, [v0.1 plan KTD14](../plans/2026-08-02-talaria-v0-1-prototype-plan.md)

### Range-validate the integer configuration settings

**Author.** v0.1 scaffold code review (delta re-review), 2026-08-03
**Priority.** P2
**Effort.** Small
**Worth it when.** U6 builds the status runner and U5 the composer — those units know what a valid bound is, and this module does not.
**Context.** `talaria/config.py` type-checks integer settings but does not bound them. Verified 2026-08-03: `TALARIA_STATUS_INTERVAL_SECONDS=-5` resolves to `-5`, and `TALARIA_COMPOSER_PASTE_COLLAPSE_LINES=0` resolves to `0`. KTD16 defines the paste thresholds as "6 or more lines, or 512 or more bytes", so a threshold of `0` collapses every paste including a one-line one; a negative interval hands U6's status runner a negative sleep.

Deliberately not fixed in the scaffold. The bound is a semantic property of the consuming unit, and inventing minimums in the config loader mid-run would be this session guessing at values the plan does not specify. The class predates the scaffold's coercion rewrite — the old code accepted these values too — but the rewrite is the natural place bounds will land.

### Add MoA progress and fallback rendering

**Author.** Project bootstrap
**Priority.** P2
**Effort.** Medium
**Worth it when.** The first transport path is proven and MoA event capability differences are captured in fixtures.

### Add a deterministic Kanban adapter

**Author.** Project bootstrap
**Priority.** P2
**Effort.** Medium to large
**Worth it when.** The board contract and ownership boundary are clear enough to avoid taking accidental responsibility for dispatcher internals.
**Constraint, verified against Hermes `7f4d15515`.** The adapter cannot sit behind the terminal gateway: `tui_gateway/` registers no `@method("kanban.*")` at all. It does push board _notifications_ — a 5-second poll of `kanban_notify_subs` emitting completed/blocked/gave_up/crashed/timed_out events — so "tell me when a task finishes" is already available there, while "query the board" is not. A queryable adapter has to reach `GatewayRunner` or the API server instead. See [LEARNINGS](LEARNINGS.md).

## P3

### Desktop-like configuration views

**Author.** Project bootstrap
**Priority.** P3
**Effort.** Large
**Worth it when.** The core session workflow is reliable and the gateway configuration methods are capability-described.

## Maybe

### Package Talaria as an independently installable distribution

**Author.** Project bootstrap
**Priority.** Maybe
**Effort.** Medium
**Worth it when.** The client can launch against both local and remote Hermes instances with a stable compatibility story.
