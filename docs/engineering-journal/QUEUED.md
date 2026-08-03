# Queued Work - talaria

> Future work with priority, rough effort, and worth-it-when triggers.

## P0

### ~~TranscriptPane.reconcile desynchronizes from the projection~~ — CLOSED 2026-08-03

**Priority.** P0 — this is what the repaired validation gate failed on, 2026-08-03. It blocked ADR-0005 and the milestone-1 merge.

`TranscriptPane.apply` finds the common prefix between the last snapshot and the new one, drops the changed tail, mounts what is new, and stores the divergence point as a floor the next scan starts from. Storing that floor was the defect: the floor could land *inside* the provisional streaming block, above lines that were about to move, and nothing ever looked at them again.

Measured on the recorded 5,773-frame corpus at the settled checkpoint, after forced flushes: **274 lines rendered against 275 projected**, first misalignment at index 251, and one line of real conversation text rendered nowhere at all — neither mounted nor condensed. The operator silently loses a line.

**The mechanism recorded here on 2026-08-03 was wrong, and the correction is the useful part of this entry.** It said a *transient notice line appears mid-transcript and later disappears*. No line ever disappears: the domain transcript is strictly append-only and entry text is immutable, which a replay of the stress corpus confirms — 15 below-floor incidents, all of one class, zero committed lines ever changed. What actually moves is the **provisional streaming block**, which the projection places *after* the committed lines. Committing an entry while a turn is still streaming pushes every provisional line down by the length of that entry. Two consecutive snapshots then agree on a provisional line whenever the streaming text did not change between them — which multi-line streaming makes constant, since each delta only rewrites the last line — and the floor advanced on that coincidence. First occurrence in the stress corpus is frame 31, where the floor reached line 1 while **zero** entries had been committed: the floor was entirely inside the streaming block.

Both remedies proposed here followed from the wrong mechanism and neither would have worked. Reconciling the full window each tick costs O(transcript) per 50ms tick, which is the cost KTD14 exists to bound. Making notice lines non-transient fixes nothing, because the notice lines were never transient — they are ordinary committed entries, and it is the streaming block that moves past them.

**Closed by.** `TranscriptView` now publishes `committed_lines`, the boundary a consumer cannot compute for itself, and the pane clamps its stored floor to it: `self._stable = min(stable, view.committed_lines)`. Truncation still uses the true divergence point, so a streaming delta churns one widget rather than the whole block. Pinned at unit size by `tests/ui/test_transcript_bounds.py::test_an_entry_committed_mid_stream_shifts_the_provisional_block_correctly` (the symptom) and `::test_the_stable_floor_never_advances_into_the_provisional_block` (the invariant); both verified to fail against the pre-fix implementation.

**That fix alone did not turn the gate green, and the reason is worth keeping.** With the floor corrected the pane began re-deriving the provisional block — real work it had been skipping — and two further defects surfaced on checks that had been *passing* while the bug suppressed them. `condensed_count` was a cumulative eviction tally doubling as the window's start index, and reached **7,493 on a transcript of 4,454 lines**, so the pane rendered a wrong slice of a correct projection; it is now derived from an explicitly tracked position. And the mount cap was enforced *after* the mount, so a tick that re-derived the whole block transiently held **667 widgets against KTD14's ceiling of 600**; the pane now condenses from the top before mounting, and the bound is `mount_cap + 1` at every instant. The gate passed on the third run with all thirteen checks green.


### ~~Run the Textual validation gate~~ — CLOSED 2026-08-03, verdict **pass**

**Author.** Reconsidered language and TUI framework analysis
**Priority.** P0
**Effort.** Medium
**Worth it when.** Before adding product behavior beyond the current TypeScript bootstrap shell.
**Context.** Drive one bounded Textual projection from a framework-neutral Python reducer and the existing frame-log contract. Prove coalesced streaming, bounded transcript mounting, scroll anchoring, deterministic `run_test()` and `Pilot` behavior, selected pseudo-terminal behavior, framework-independent domain state, strict typing and linting, and clean `uv tool install` launch.
**Amended 2026-08-02 by ADR-0004.** The fallback is no longer Go with Bubble Tea. The language is settled as Python, so a framework failure selects a different Python presentation layer — which is why identifying one is now a prerequisite rather than a contingency. Choose the vertical slice from the Hermes terminal UI feature inventory rather than from a generic renderer stress list, so the gate produces a prototype instead of a harness.

**Closed by.** Unit U5 of the [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md), on the third run of the day. All thirteen threshold checks passed; measurements, corpus identities, the full three-verdict sequence and explicit non-claims are in [Textual validation gate results](../analysis/2026-08-03-textual-validation-gate-results.md), with the machine-readable record at [`evidence/2026-08-03-textual-validation-gate.json`](../analysis/evidence/2026-08-03-textual-validation-gate.json). Re-runnable as `uv run talaria gate --corpus <recording> --deltas 50000`, which exits non-zero on a fail verdict. Headline numbers: 501 mounted line widgets against a ceiling of 600 (peak, not steady state — the pane condenses before it mounts), 44.3 MB resident growth against 300 MB, 15.3 coalescing flushes per second under sustained streaming against 25, and zero content loss across 24 checkpoints. The framework choice is [ADR-0005](../../platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md), `accepted`.

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

### ~~`test_the_status_command_runs_and_renders_under_replay` flakes under load~~ — EXPLAINED and fixed 2026-08-03

**Priority.** P3 — reported as an unexplained CI flake; root cause found by reproducing it locally under CPU load.

Not a race in the KTD5 overlap guard. `TalariaApp` starts a background status loop as soon as it runs (`talaria/ui/app.py:154`), and that loop ticks *before* its first sleep — so the app has a tick in flight almost immediately. The test then fired a second tick explicitly and asserted `outcome == "ok"`. When the app's own tick was still awaiting its Python subprocess, the guard correctly returned `overlapped_skip`, which is the guard doing exactly what R21 specifies.

It is load-sensitive because the window is the duration of an interpreter spawn: slow under CI load, near-instant on an idle developer machine. Reproduced deliberately by giving the status child a 0.6s sleep, which makes the first attempt `overlapped_skip` **every** time; the fixed test retries and reaches `ok` after 11 attempts.

**The family, not just the instance.** This is the third load-sensitive assertion in this suite — an earlier one asserted an overlap count against a 0.3s timeout that left 0.1s for an interpreter spawn. Tests that spawn real subprocesses and assert exact outcomes are betting on machine speed. Worth a sweep if another appears.

**Still unexplained, and deliberately not claimed as fixed:** the CI failure at `3231803` was `test_overlap_at_most_one_child_ever` reporting zero successful invocations of three — a *different* test with its own runner and no app involved. Same family (load-sensitive, spawns real interpreters), but the mechanism above does not explain it, and it has not recurred.

### Status child can escape the process group with setsid, and stdout EOF is not child exit

**Priority.** P2 — found by adversarial review of the status runner, 2026-08-03; not fixed in milestone 1.

Two residual defects in `talaria/status/runner.py`, both bounded in blast radius but real:

- A descendant that calls `setsid()` leaves the group, so `_kill_process_group` does not reach it. It survives the tick and, if it inherited the pipes, holds them open — which costs a second timeout budget before the runner gives up. Measured at 4.01s against a configured `timeout_seconds` of 2.0.
- Output is read until stdout reaches EOF, not until the child exits. A command that backgrounds anything inheriting stdout (`worker & echo ok`) therefore reports `timeout` and discards output the direct child already produced correctly.

Both need the same thing to fix properly: wait on child exit and treat the streams as separately terminable, rather than treating stdout EOF as the completion signal. Deferred because it changes the completion semantics of every tick and milestone 1 is closing; the memory, orphan and descriptor-leak defects found alongside these were fixed because they were unbounded.

### A bearer capability carried in a URL path is recorded verbatim

**Priority.** P2 — found by external review of the redaction boundary, 2026-08-03; deliberately not fixed.

`redact_url` withholds credentials from a URL's userinfo and its query string. It does not touch the path, so the concrete Chrome DevTools Protocol form — `ws://127.0.0.1:9222/devtools/browser/<GUID>`, where the GUID alone drives the browser — reaches the frame log intact. It is reachable: at Hermes `7f4d15515`, `browser.manage` returns the operator's configured CDP override on an ordinary status call (`tui_gateway/server.py:13405` → `methods_tools.py:1349`), and that override may be set to the concrete WS form.

Not fixed because both obvious rules are bad. A Hermes-shaped path rule (`/devtools/browser/<segment>`) protects exactly one known shape and will not generalize to the next capability-bearing path — and it is worse than doing nothing, because it creates the appearance that paths are handled, which is the same staleness failure the deny-set was already bitten by. A generic "high-entropy path segment" heuristic would redact ordinary URLs — commit SHAs, content hashes, UUID resource ids — and the corpus exists to be studied; over-redaction is a different failure, not the safe direction (the same reasoning that keeps `max_tokens` out of the key-name net).

**Correction, 2026-08-03.** This entry originally justified the deferral on the grounds that the form is "loopback-only". That is false, and the error mattered: it made the risk look structural when it is merely default. Loopback is the *default* (`hermes_cli/browser_connect.py:21`, `DEFAULT_BROWSER_CDP_URL = "http://127.0.0.1:9222"`) and nothing constrains the override to it. Hermes documents the opposite to operators directly — `hermes_cli/tips.py:306` at the pin reads *"BROWSER_CDP_URL connects browser tools to any running Chromium-family browser — accepts WebSocket, HTTP, or host:port"* — and `_resolve_browser_cdp_url` is deliberately structured to avoid blocking on an unreachable host, which is code written for the remote case. Remote CDP is configurable today.

**A third rule exists and is the leading candidate:** withhold the path only for `ws`/`wss` URLs whose host is not loopback. It carries no Hermes-specific shape, and it costs nothing on the study data, because commit SHAs, content hashes and UUID resource ids live in `http`/`https` document URLs rather than in remote WebSocket paths.

**What actually blocks it is the redactor rule, not the harness.** An earlier version of this entry priced the blocker as "teaching the KTD6 comparator a new kind of authorized divergence", which overstates it — and a queued item whose stated blocker is larger than the real one gets mispriced and deferred again, the same failure as the dead revisit trigger above. The harness now has frame-body divergence authorization (`_frames_equivalent` plus `PYTHON_ONLY_FRAME_REDACTION_REASONS`), so a path allowance is roughly ten lines mirroring the existing query-key allowance in `_compare_endpoint`. One genuine difference in kind survives: the query allowance is set membership over an enumerable list of key names, while a path allowance must be a *predicate* on the URL (`scheme in {ws, wss} and host not loopback`). That is three lines and deterministic.

The real dependency is sequencing: the comparator has to encode an expectation about a redactor rule that does not exist yet. Defining the rule is the remote-attach work's job, and building the check for it first is designing against nothing.

**Revisit when** — the original trigger here was "remote CDP becoming supported", which describes a state that had already arrived and so could never fire. Two triggers that can:

1. **Attach to a remote (gated) gateway** (P1, above) is implemented. That work makes non-loopback hosts routine and is the natural place to extend the comparator.
2. A non-loopback host appears in a recorded URL. Mechanically checkable against any corpus rather than a standing intention to notice something:
   `grep -oE '"wss?://[^"/]+' <corpus> | grep -vE '127\.0\.0\.1|localhost|\[::1\]'`

This is a check someone runs, not an alarm that fires by itself. Instrumenting the redaction boundary to count non-loopback hosts would make it self-firing, and was not done because adding a counter to the security boundary for a P2 is a poor trade — but that is the honest limitation of trigger 2, and trigger 1 is the one to rely on.


## P3

### Desktop-like configuration views

**Author.** Project bootstrap
**Priority.** P3
**Effort.** Large
**Worth it when.** The core session workflow is reliable and the gateway configuration methods are capability-described.

### Status runner: URL path is forwarded to the child, and a few outcomes are misreported

**Priority.** P3 — same review, 2026-08-03.

- `_strip_query` now drops the query, fragment and userinfo, but keeps the path. An operator-supplied `TALARIA_GATEWAY_URL` carrying a ticket in its path (`/attach/TICKET-ABC`) still reaches the status child. Talaria's own URLs put the credential in the query and the path is meaningful (`/api/ws`), so stripping the path outright is not obviously right — decide deliberately rather than by default.
- A nonexistent `launch_cwd` raises `FileNotFoundError` and is reported as `missing_executable`, telling the operator "command not found" about a command that is present.
- The timeout path sends SIGKILL with no preceding SIGTERM, so a child never gets a chance to flush or clean up temporary files.
- `is_suspicious_key` anchors its API-key pattern to the whole variable name, so `ANTHROPIC_API_KEY`, `AWS_ACCESS_KEY_ID`, `GITHUB_PAT` and similar are not "suspicious" to it. They are denied by default anyway, but `contract.py`'s claim that the credential-name deny outranks the operator allowlist is false for them — an operator who allowlists one gets it forwarded.


### Replay determinism exercises two speeds, not the speed range AE11 asks for

**Priority.** P3 — found by external review of the gate, 2026-08-03; description corrected, coverage not widened.

`run_gate` proves AE11 ("identical domain state at any speed") with three replays: 64x, 64x with a pause and resume halfway, and unbounded. That is **two** distinct speeds, since the paused run is also 64x. `MIN_SPEED` is never replayed, so the slow end of the range is unmeasured.

The check's published `description` claimed "1x-with-pause, 64x and unbounded", which overstated it — a treatment named in a check description but never run is the same class of defect as a threshold quietly loosened, and it was corrected in place. The results doc described the replays accurately; only the code's own string was wrong.

Not widened because a genuinely slow replay is wall-clock expensive: `speed` multiplies the corpus's recorded cadence, so replaying 53,516 frames at 1x takes as long as the original session did. A cheap partial improvement is a third speed between 1x and 64x over a truncated corpus, which would give the fit three points instead of two without a real-time replay.

**Revisit when.** The determinism check is next touched, or a timing-dependent reducer bug escapes to a user — that would make the unmeasured slow end the first place to look.

## Maybe

### Package Talaria as an independently installable distribution

**Author.** Project bootstrap
**Priority.** Maybe
**Effort.** Medium
**Worth it when.** The client can launch against both local and remote Hermes instances with a stable compatibility story.
