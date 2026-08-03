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

### ~~Prove the Hermes transport seam~~ — CLOSED 2026-08-03

**Author.** Project bootstrap
**Priority.** P0
**Effort.** Medium
**Worth it when.** The prototype shell is ready for the first real integration slice.
**Context.** Talaria needs capability discovery, session lifecycle, prompt streaming, cancellation, and approval handling before UI work can be judged against real Hermes behavior.

**Closed by.** Unit U7 of the [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md): `talaria/transport/attach.py`, `talaria/transport/rpc.py`, `talaria/transport/credentials.py`, and `LiveSource` in `talaria/transport/source.py`, with the live-submit and interrupt wiring in `talaria/ui/app.py` and `talaria/ui/composer.py`. Evidence is `tests/transport/` (78 tests) plus `tests/ui/test_live_wiring.py`.

**What is proved, and by what.** Each of these runs against a stub gateway that is a *real* WebSocket server on loopback, dialled with the real `websockets` client — not a hand-written double, because a double would prove that Talaria calls a function with a string rather than that a server which only reads the URL query can authenticate the connection.

- **Attach over the authenticated path.** The credential arrives as the `?token=` URL query parameter and nowhere else, matching the pin's `ws.query_params`-only check (`hermes_cli/web_server.py:14443-14524`).
- **Credential acquisition, per dial.** Environment, then a `?token=` already on `TALARIA_GATEWAY_URL`, then `<config_dir>/credentials` (refused unless `0600` or stricter), then a non-echoing prompt — proved against a real pseudo-terminal, and confirmed to fail if the prompt is swapped for `input()`.
- **Prompt streaming.** Identical ordered frames from a file and from a socket produce identical domain state after *every* frame, not merely at the end (AE16 / R31).
- **Cancellation.** `session.interrupt` marks the turn cancelled only on a confirmed reply; an interrupt whose outcome was lost with the transport changes nothing and says so (R4, AE8).
- **Reconnect.** Distinct visible states, a credential re-read on every dial (a rotated token is picked up), no duplicate transcript entries, outstanding prompts re-keyed once, and sub-agent terminal states preserved against a re-announced start (R35, F6).
- **Lost outcomes.** An RPC interrupted by a disconnect resolves to `unknown`, and a reply from a stale connection epoch is counted and discarded rather than resolving a reused request id into a false success (KTD13).
- **Recording.** Both directions now pass the U2 redaction boundary before any frame-log write.

**What is deliberately NOT closed by this, so it is not mistaken for done.**

- **No live attach against a real Hermes gateway was performed.** Every assertion above is against the stub. The plan's verification clause asks for a smoke attach against a harness-launched local gateway; that belongs to U10's acceptance run and is queued below.
- **Capability discovery and session lifecycle** — the two other clauses of this item's original context — are R34 and R2, owned by U10 and U3/U10 respectively. This item closes the *transport seam*; it does not close the startup path that chooses a session, which is why a bare `talaria` run still exits rather than dialling.
- **Approval and the four blocking bridges** are U8.

**Refs.** [Project direction](../analysis/2026-08-01-hermes-tui-project-direction.md), [v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md) unit U7

### Smoke-attach the transport against a real local Hermes gateway

**Author.** v0.1 milestone-2, unit U7
**Priority.** P0
**Effort.** Small
**Worth it when.** U10's acceptance run, which already launches a dedicated loopback gateway instance with an injected token and a scratch session.
**Context.** U7's whole evidence base is a stub server that reads the URL query the way the pin says Hermes does. That settles the *client* side, and it cannot settle one thing: whether a real Hermes accepts what Talaria sends. Two specific unknowns are worth naming rather than assuming away.

1. **How an authentication failure reaches the client is unverified.** `gateway_ws` calls `await ws.close(code=4401)` *before* accepting the upgrade (`hermes_cli/web_server.py:15615-15617`), and whether the client observes that as a WebSocket close with code 4401 or as an HTTP handshake rejection is decided by the ASGI server, not by Hermes. `classify_dial_error` handles both shapes deliberately; a live run would say which one actually occurs, and whether the HTTP status is 401 or 403.
2. **The frame log recorded live has never been checked against a real corpus.** The redaction assertions are against frames this repository wrote.

**Do.** Attach, submit one prompt, interrupt it, drop the socket, confirm the reconnect, and diff the resulting frame log's redaction markers against the AE3 sweep.

**Extended by U9, 2026-08-03.** U9's own verification clause — "a live isolated run dispatches one real catalogue command of each available shape and one collapse round-trip" — is unmet for the same reason and belongs on this run rather than on a second one. **No Hermes gateway was attached at any point in the U9 session.** Everything U9 asserts is against `tests/transport/conftest.py`'s loopback stub, whose reply bodies are transcribed from the pinned handlers; that settles Talaria's side of each contract and settles nothing about what a real gateway sends. Five specific unknowns:

1. **Which result shape each real command actually returns is unverified.** The stub returns the shape the test asked for. A live run would say what `/model`, `/status`, a real skill command and a real bundle answer with, and whether any of them carries a field combination the three-destination routing reads differently from intended.
2. **Whether a real `commands.catalog` decodes cleanly is unverified.** The stub serves eight entries in the documented shape. A real one carries the full registry plus quick commands plus a skills scan, and its `canon` map is the thing alias resolution depends on.
3. **Three client-local entries have never been seen in a real catalogue.** The name-plus-category rule is derived from the pinned source, not observed. One live `commands.catalog` settles whether `/density`, `/logs` and `/mouse` arrive under category `TUI` as the source says. `/sessions` is **not** among them and is not an open question: the registry defines `CommandDef("sessions", …, "Session")` (`hermes_cli/commands.py:180` at `7f4d15515`), so the catalogue builder's dedup guard drops the `TUI` extra of that name and serves it as an ordinary dispatchable command. Talaria treats it as one.
4. **`paste.collapse` has never written a real file.** The placeholder text Talaria inserts is the gateway's, so its exact shape — and whether the path in it is one an operator can open — is unobserved.
5. **The `slash.exec` / `command.dispatch` split has never been exercised against a real worker.** The pinned source is unambiguous about which handler serves what, and Talaria now calls `slash.exec` first and falls back exactly as Hermes's own client does. What a stub cannot show is what a real slash worker *prints* — whether a command's output arrives as one block, whether a warning accompanies it, and how long the first call takes while the worker subprocess is spawned on demand (`methods_tools.py:1177-1194`).

**Do, additionally.** With the session from the smoke-attach: fetch the catalogue and record its entry count and category names; dispatch one command of each shape the catalogue actually offers; paste 400 lines and confirm the placeholder and the file it names.

### Audit the egress surfaces as a set, rather than the redactor's call sites

**Author.** v0.1 milestone-2, unit U7 — proposed by the milestone-1 review agent
**Priority.** P0
**Effort.** Medium
**Worth it when.** Before U8 lands, because U8's four blocking-prompt bridges and U10's live acceptance run each add several more surfaces of exactly these kinds.
**Context.** The same defect has now appeared three times in three different places, and each time it was found by someone tripping over it rather than by looking. Milestone-1's P0 was two call sites and disk. U7's `describe_dial_error` was *zero* call sites and the screen — the operator's session token rendered into a composer notice by a wrong-scheme endpoint typo, and re-leaked on every reconnect. The common root is that `redact_url` is a function invoked where somebody remembered, not a boundary that everything crosses.

Enumerating the redactor's call sites is the wrong direction: it can only find places already thought about. Enumerate the places data *leaves* the process and ask which of them crosses the boundary. From the incidents so far, the kinds worth sweeping are:

- exception stringification (`str(exc)` anywhere a dial target, URL, or credential can be in scope — `talaria/transport/source.py:385` and `:541` are the same shape as the fixed leak, safe today only because those exceptions happen not to carry the endpoint)
- `repr()` and `__str__` on transport objects and anything reachable from them
- retained failure state (`LiveSource.last_failure` and anything like it)
- operator-visible text that formats a dial target — notices, status lines, diagnostics
- log lines and the status payload

**Do.** Build the list of surfaces first and write it down, then check each one, then decide which need the boundary applied. Record the negatives too — `credentials.py`'s `tomllib` parse error was checked across four malformed files and is genuinely safe, and that should not need re-checking. The rule to test against is both halves: the credential is absent **and** the diagnostic survives. A redaction that eats the message it was protecting has already happened twice in this module.

**What U8 added, and what it swept.** The four blocking-prompt bridges are the first path on which Talaria sends an operator-typed *credential*, so this item's "before U8 lands" trigger has now fired. U8 closed the surfaces it created rather than the standing list: the respond value is proven absent from the recorded frame log's raw bytes (over a real socket, not by unit test), from the status document, from every transcript entry, from the composer notice, from `LiveSource.last_failure`, and from the rendered screen via `App.export_screenshot`; the terminal-read failure text goes through `scrub_urls` and is asserted on both halves. It also found a case of the "redaction ate the message" failure this item warns about twice — the failure line was clipped at `SYSTEM_LINE_CLIP` because the constant repeated the exception's own first clause. **Still unaudited:** the list as a *set*, including `repr()` on transport objects generally and log lines.

**Already covered by U7, so start after these.** `describe_dial_error` now scrubs through `scrub_urls` plus a literal pass over the credential in use, and `LiveSource` scrubs `last_failure` on the disconnect and close-error paths (`source.py:385`, `:541`), so the three exception-stringification sites known at the time are closed and pinned by tests that assert both halves. `AttachTarget.safe_url` is the choke point for the endpoint and is credential-free twice over — stripped at construction, then routed through `redact_url` so userinfo is withheld too. The outcome object is swept by `test_the_attach_outcome_carries_no_credential_anywhere`, with one documented exclusion: the live `GatewayConnection`, because `websockets` retains `connection.request.path` including the query and no client library can avoid that. **What remains unaudited** is the rest of the list as a *set* — `repr()` on transport objects generally, log lines, the status payload, and every new surface U8's bridges and U10's acceptance run introduce.

## P1

### ~~Make the macOS checks required status checks on `main`~~ — CLOSED 2026-08-03

**Author.** v0.1 scaffold code review, 2026-08-02
**Priority.** P1
**Effort.** Small
**Worth it when.** Before the next unattended run merges anything — this is the only remaining gap between a red check and `main`.
**Context.** `.github/workflows/validate.yml` runs the `python-check` job on macOS arm64 with no `continue-on-error`, so a failure fails the workflow run. That is not the same as blocking a merge, and nothing in a workflow file can be. Verified 2026-08-02: `gh api repos/infiquetra/talaria/branches/main/protection` returns HTTP 404 "Branch not protected" and `gh api repos/infiquetra/talaria/rulesets` returns `[]` — `main` has neither branch protection nor a ruleset, so a pull request with a red check can still be merged.

An earlier version of the workflow carried a `required: true` matrix key, which reads as though it configured this; it only negated `continue-on-error`. That key has been removed, and job names are now free of incidental matrix values so the check names (`python-check (3.12)`, `python-check (3.13)`) stay stable if protection is configured against them.

**Left to the operator deliberately.** Repository governance is not something an unattended run should change on the operator's behalf, and requiring a check name that does not match exactly would deadlock every merge — including the ones the run was authorized to make. Until it is configured, the gate is enforced behaviorally: this run does not merge without observing the required legs green.

**Closed 2026-08-03 on explicit operator instruction**, immediately after PR #11 merged. Classic branch protection on `main` requires exactly `python-check (3.12)` and `python-check (3.13)`, with `enforce_admins: true`, force pushes and deletions denied.

Three choices inside "set the protection" that the instruction did not spell out, recorded so they can be reversed knowingly:

- **`enforce_admins: true`.** The gap this item exists to close is an unattended run merging a red check, and such a run holds the operator's own admin rights. With admins exempt the protection would not constrain the only actor it was queued for. The cost is that the operator has no bypass either; lifting it is one API call.
- **Only the two macOS legs are required.** `python-check-linux` is informational by ADR-0005, and `check` covers the TypeScript bootstrap under `src/`, which is superseded and slated for removal — requiring a check that later stops being reported blocks every merge permanently, which is the deadlock this entry already warned about.
- **`strict: false`.** A branch does not have to be up to date with `main` before merging. This closes the stated gap (red check cannot merge) without forcing a rebase on every merge. It leaves the semantic-merge-conflict hole open, which is a separate concern from this item.

**Verified, not assumed.** `git push origin main` with an empty commit was rejected: `remote: - 2 of 2 required status checks are expected` / `! [remote rejected] main -> main (protected branch hook declined)`. Reading the configuration back would not have proved the hook fires.

### ~~`test_mounted_widgets_stay_under_the_cap` fails intermittently under load~~ — CLOSED 2026-08-03

**Author.** v0.1 milestone-2, unit U7 — found while running the project check, and fixed rather than queued
**Priority.** P1
**Context.** `tests/ui/test_transcript_bounds.py::test_mounted_widgets_stay_under_the_cap_while_content_stays_reachable` failed intermittently on `pane.rendered_lines == view.lines[pane.condensed_count:]` — **one line of skew**, `'line 38.3' != 'line 38.4'` at index 30. The file already documented the symptom: the `_drain` helper's docstring says the accounting assertions "fail roughly one run in three, and only under whole-suite load", and `_drain` forces a flush specifically to suppress it. Measured with U7 present: 2 failures in 12 paired runs, and 2 failures in 9 whole-suite runs.

**It was not a test bug.** `TalariaApp.render_snapshot` has two kinds of caller — the coalescing timer, which Textual runs on the message pump and never re-enters, and forced flushes (`drain`, and the gate's checkpoints) which run on the *calling* task. Those two were never serialized against each other, and `TranscriptPane.apply` is a read-modify-write over its own window bookkeeping spanning several awaits. Two passes over different projections interleave and leave the pane holding a window the projection does not have. The U5 gate calls `drain` at every checkpoint, so this was live in the gate too.

**Closed by.** An `asyncio.Lock` around `render_snapshot`, uncontended in the ordinary path because Textual's timer never re-enters its own callback. Pinned by `tests/ui/test_transcript_bounds.py::test_two_renders_can_never_reconcile_the_pane_at_the_same_time`, which measures overlap depth: 2 against the unfixed code, 1 with the lock. Rate after the fix: 12 of 12 paired runs clean.

**Author.** v0.1 scaffold code review, 2026-08-02 (rated P2 advisory by the review; carried at P1 here because the trigger is the next milestone)
**Priority.** P1
**Effort.** Small
**Worth it when.** Before U6's status runner executes `status.command` — that is what turns this from a precedence question into an execution path.
**Context.** KTD15 ranks a repo-local `./.talaria/config.toml` above the operator's global `~/.talaria/config.toml`. Nothing in the scaffold executes anything, so there is no vulnerability today. But KTD5 specifies `status.command` is executed on an interval, and the precedence chain is being locked in now: an operator who clones an untrusted repository, `cd`s into it, and runs `talaria` would execute a command supplied by that repository's contents.

The review named two viable resolutions: require a repo-local config file to be explicitly trusted before it is honored, or exclude command-valued keys from the repo-local level while leaving the rest of KTD15's order intact. Either is a change to a key technical decision the plan settled, which is why it is recorded for decision rather than made unilaterally mid-run.
**Refs.** [v0.1 plan KTD15 and KTD5](../plans/2026-08-02-talaria-v0-1-prototype-plan.md)

### Let the operator decline a blocking prompt without waiting for it to expire

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)
**Priority.** P1
**Effort.** Small
**Worth it when.** Someone runs Talaria against a real session and hits a sudo prompt they do not want to answer.
**Context.** U8's controls can answer a prompt but cannot decline one. The shipping terminal UI can: `answerSudo('')` and `answerSecret('')` clear the overlay locally when the operator submits nothing, and `useInputHandlers.ts:119` / `:128` send `{password: ''}` / `{value: ''}` on escape, which releases the gateway's blocking wait immediately instead of making the tool sit out its full 120-second (sudo) or configured (secret) timeout. Talaria's operator has to let it expire.

The reason it is not in U8 is that "an empty value" and "no answer" are the same thing on this wire, and the deliberate choice — decline — deserves a distinct affordance rather than an accidental one, which is a keybinding decision rather than a transport one. Note the interaction with U8's own decision that a cleared prompt is not re-offered: a declined prompt must clear like an answered one, not restore.
**Refs.** [DECISIONS.md](DECISIONS.md) "A prompt is cleared before its answer is sent", `ui-tui/src/app/useMainApp.ts:936-975` at Hermes `7f4d15515`

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

**The transport seam for this now exists, in code — updated 2026-08-03 by unit U7.** `talaria/transport/credentials.py` defines `CredentialProvider.acquire()`, and `LiveSource._dial` calls it on **every** dial including every reconnect; `tests/transport/test_reconnect.py::test_the_provider_is_invoked_once_per_dial_and_again_on_reconnect` fails if a credential is cached across a reconnect, and `::test_a_credential_rotated_between_attach_and_reconnect_is_picked_up` fails if the re-read does not reach the wire. `redact_url` already denies `ticket` and `internal` by name (KTD6's Python-only superset) and `AttachTarget` strips all three credential forms from an endpoint, so the recording and hygiene boundaries are ready too.

So this work is a new `GatedTicketProvider` returning `("ticket", <single-use value>)`, plus keychain storage and the PKCE loopback listener. Nothing in `LiveSource`, `RpcCorrelator`, or the UI should need to change — and if it does, that is the signal the seam was drawn in the wrong place, which is worth reporting rather than working around.
**Refs.** [v0.1 plan KTD11](../plans/2026-08-02-talaria-v0-1-prototype-plan.md), [ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md)

### A Talaria-authored delivery note is longer than the transcript's own line clip

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), third adversarial round
**Priority.** P1 — raised from P2 in the fourth round.
**Effort.** Small
**Worth it when.** Before a live run leans on the transcript as the record of what happened to an unconfirmed answer — or sooner, if an operator reports being told to "send it again" with the sentence cut off.

**Context.** `record_local_note` bounds an entry at `SYSTEM_LINE_CLIP` (120 characters) and marks the cut with an ellipsis, which is correct for text the gateway wrote. `DELIVERY_NOTES["not_sent"]` is **121 characters on its own**, so every line that prefixes it loses its tail — `"…send it again when the connection is bac…"`. Measured: `"2 approvals not denied — " + DELIVERY_NOTES["not_sent"]` is 146 characters, and the single-answer path's `"sudo not answered — …"` is 143.

**Correction to this entry's own wording, fourth round.** It described the defect as "pre-existing and not introduced by the deny-all fix". That is misleading in a public journal: U8 is *entirely uncommitted*, so nothing in it is pre-existing relative to `main`, and "pre-existing" reads as "inherited from a shipped release". What is true is narrower — it predates this **round**, not this unit. The deny-all path made it visible by putting the same note on a second path; both paths arrived in U8.

The clip is not the wrong behaviour — a bound on transcript entries is deliberate — but *which half survives* it should be a decision, not an accident. `record_submission` already solves this for a submitted message by writing the note as its own transcript entry rather than as a suffix, and that is the obvious shape here too.

**Not fixed now** because the change touches the wording shared with the submit path and the combined-line assertions in four existing tests, which is a wider blast radius than the defects each round was scoped to. The deny-all line was arranged so the operative clause ("delivery unconfirmed", "not sent") lands well inside the bound and only the explanatory tail is cut.

**Partly mitigated, fourth round, on a different surface.** The composer notice bar is one row with no wrap, and a long note used to stop mid-clause with nothing marking the cut. It now carries `text-wrap: nowrap` plus `text-overflow: ellipsis`, so the *screen* marks its own truncation the way the transcript already did. That is the marker, not the length — the notes are still longer than either surface.

**Worth checking at the same time.** Whether `record_local_note` should clip at all for a Talaria-authored constant, or whether the clip belongs only on the paths that embed gateway text (`outcome.notice` does).

### A card whose control is on screen and working is titled "answer below — scroll"

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P1
**Effort.** Small — the mechanism is identified precisely.
**Worth it when.** Before the reachability marker is presented to anyone as a reliable cue. It is currently right in the dangerous direction and wrong in the safe one.

**Context.** The `answer below — scroll` border title is applied, after an ordinary terminal resize with no operator scroll, to a card whose buttons are drawn on screen and answer a click. Reproduced against the real app: an approval plus a `secret.request` plus a `sudo.request` queued, mounted at 120x40, then `pilot.resize_terminal(60, 20)`. From one `export_screenshot()` the card's title reads `answer below — scroll` while `"once"` is present on the same screen, and `pilot.click("#choice-0")` sends `("approval.respond", {"session_id": "s1", "choice": "once"})`. No card on that screen says `waiting for you`, so the operator's only cue is the false one — and the action it names, scrolling down, moves the working buttons off screen.

It is stale rather than transient: twelve consecutive settle cycles leave the title unchanged, and one direct call to `mark_unreachable_controls()` against the final geometry flips it back.

**Mechanism.** `reveal_actions` (`talaria/ui/prompts.py`) scrolls with `scroll_to_widget(...)` and then schedules the marking with a **single** `call_after_refresh(self.mark_unreachable_controls)`. One deferral is not enough — the marking reads the region's geometry before the scroll has committed, records the pre-scroll answer, and nothing recomputes afterwards. The comment above that line says the deferral exists precisely so the check runs against the post-scroll arrangement; measured, it does not.

**Severity, stated fairly.** This is the *inverse* of the defect the fix was written for, and the inverse is the safer direction: the control is present and clickable, the operator is merely told something untrue about it. A sweep for the original direction — 26 terminal heights at width 120, four approvals plus a clarify mounting into a full region, every scroll position at 80x30, and five other arrangements — found **zero** cards left looking live with an unreachable control. The multi-card fix achieved its safety goal; this is the bill for it.

**Not the same as the hand-scrolled item below.** That one's premise is that resize and re-wrap recompute correctly and only hand-scrolling escapes them. There is no operator scroll in this reproduction: the resize path itself yields the stale label.

### The resize trigger for the reachability marker is unpinned, and it is the only trigger a command-less prompt has

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P1
**Effort.** Small — one test.
**Worth it when.** Immediately. This is the round-5 hole entered from the other side, and the code is correct today only by luck of nobody having touched it.

**Context.** Round 5 asked whether the `CommandPanel.Rewrapped` channel was dead weight and pinned it on both halves. It did not ask the mirror question. Deleting `self.call_after_refresh(self.reveal_actions)` from `PromptRegion.on_resize` leaves **all 651 tests green**, and the call is load-bearing: six clarify prompts (no command body, so no `CommandPanel` exists and no `Rewrapped` message is ever posted) mounted at 120x40 and resized to 60x12 give, unmutated, five cards correctly marked `answer below — scroll`; with the trigger deleted, **zero** cards marked and the same five controls still unreachable — the original defect restored in full, suite green.

Every round-5 test that resizes uses approval cards, whose command body posts `Rewrapped` on the same resize and masks the missing trigger. The uncovered case is any prompt *without* a command — clarify, secret, sudo — in a region that resizes.

**Suggested framing.** One test: several clarify cards, resize the terminal narrower, assert every card whose control leaves `scrollable_content_region` is retitled. Closing this also closes the mount-path item below, which is the same hole from the other direction. This is the repository's own "a guard nothing can exercise is a guard nobody can trust" rule applied to round 5's own new code.

### A control-only card mounting into a full prompt region is never recomputed

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round — found while fixing the multi-card reveal, queued rather than fixed. Re-priced from P2 to P1 by the round's verification, which disproved the mitigation this item originally claimed.
**Priority.** P1
**Effort.** Small
**Worth it when.** A session queues a clarify, a secret or a sudo behind enough other prompts to fill the region — or before the "answer below — scroll" marker is relied on as a guarantee rather than as a best effort.

**Context.** `PromptRegion` recomputes which cards can reach their control on two triggers: its own `Resize`, and `CommandPanel.Rewrapped`. Instrumented over a run, a card mounting into a region that has already reached `max-height: 70%` fires **only** `Rewrapped` — which is what pins that channel — but `Rewrapped` comes from the command body, and only approvals have one. A clarify, secret or sudo mounting into a full region therefore fires **neither** trigger: measured, `on_resize` and `on_command_panel_rewrapped` both stayed silent, and the new card kept the default `waiting for you` title with its input below the fold.

**This item first said the case was "partly masked" because `PromptRegion.apply` calls `card.focus_answer()` and Textual scrolls a focused widget into view. That mitigation was measured and does not happen — raise the priority accordingly.** Four approvals at 120x40, then a `sudo.request`: `app.focused` is the sudo card's `Input` at region `(2, 49, 115, 1)`, the region's `scrollable_content_region` is `(0, 10, 119, 25)`, and `contains_region` is **False**. Pressing six keys leaves the input's `value` six characters long while the same screenshot contains neither the words `sudo password` nor a single password-mask glyph anywhere.

So the operator types a password into a focused control that draws nothing, with no visual confirmation of any kind. Nothing leaks — R9 holds, the value never reaches the transcript, the screen or a log — but "type your password blind" is a materially worse failure than the stale-title one this item was originally filed under, and it is the reason this is P1 rather than P2.

**Suggested framing.** Schedule the recomputation from `apply` whenever it mounted or removed a card. Note that doing so makes the `Rewrapped` channel redundant for the mount case — check, by deleting it and running `test_a_card_mounting_into_a_full_region_is_still_recomputed`, whether it still has a case of its own before keeping both.

## P2

### Command entry is a listing, not completion — three catalogue fields go unread

**Author.** v0.1 milestone-2, unit U9 (slash commands and paste collapse)
**Priority.** P2
**Effort.** Medium
**Worth it when.** Talaria is being used as a daily driver against a gateway whose catalogue runs to ninety-odd commands, at which point reading the list stops being a substitute for finding one.
**Context.** U9's plan entry asks for a "minimal entry affordance", and `talaria/ui/palette.py` is exactly that: a foldable region on F3 listing every command with its availability marker. It is a listing. Typing still means typing the whole name, and three fields the gateway already sends are decoded and then unused:

- **`canon`** *is* used, for alias resolution — this one is wired.
- **`sub`** maps each command to its tab-completable subcommands (`tui_gateway/methods_tools.py:352`). Nothing reads it, so `/goal ` offers nothing after the space.
- **`skills`** carries a per-skill `{usage, origin}` pair, which the gateway assembles specifically so a client can rank the listing — its own comment says "every consumer that renders the catalog also wants to rank it" (`:337-341`). The listing is in catalogue order instead.

The gateway also publishes `complete.command`, `complete.path` and `complete.at` (`tui_gateway/methods_complete.py`), none of which Talaria calls.

**Deliberately deferred, not overlooked.** A completion overlay is a second focus owner in front of the composer, and the composer is the widget the interface is built around — U9's own decision to make the sub-agent interrupt a row action rather than a key binding turned on the same concern. Getting that wrong costs more than the typing it saves.

**Do.** Filter the listing as the composer's text changes, rank skills by `usage`, and read `sub` for a second-word completion, before reaching for an overlay or for the `complete.*` methods.

### A deny-all that succeeds can re-offer a control the gateway already resolved

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P2
**Effort.** Small
**Worth it when.** Before an operator is expected to trust that a card which reappears is a live question. Also whenever the deny-all-while-in-flight decision is revisited — this is one of its costs.

**Read this beside the decision it comes from:** *"Deny-all reports what it decided and what it cannot know as two clauses, not one total"* in `DECISIONS.md`, whose rejected alternatives include refusing deny-all while an answer is in flight. This item is the price of that rejection and should be weighed with it rather than on its own.

**Context.** `approval.respond` with `all: true` resolves the whole session queue, including an entry whose own `approval.respond` is still travelling. When that in-flight call comes back `not_sent`, `_record_prompt_outcome` takes the restore branch and `restore_prompt` puts the card back — offering `once` for an approval the gateway denied a moment earlier. Pressing it sends a second `approval.respond` into a queue that should be empty; the reply's `resolved: 0` is caught and reported (`GATEWAY_HAD_NO_APPROVAL`), so the outcome is a confusing screen rather than a wrong grant.

**PLAUSIBLE, not confirmed.** Read from two code paths and **not reproduced**. Reproducing it needs a `not_sent` outcome on the single-answer call while a deny-all succeeds on the same session, which the `HoldingDispatcher` can arrange.

**Likely shape of the fix.** `restore_prompt` already declines to restore a prompt whose id is in `flushed_prompt_ids`; a deny-all that resolves the queue could latch every id it swept, which is the same latch an expiry uses and would need no new mechanism.

### The `feed()` fixture's clock is nine days behind the age-out's

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P2
**Effort.** Small
**Worth it when.** Before any prompt test is given a real `coalesce_interval`, or the next time a prompt test flakes on an empty screen.

**Context.** `tests/ui/test_prompts.py`'s `feed()` stamps frames at `1_785_000_000 + seq`, roughly nine days behind wall clock, while the live age-out reads `time.time()`. Every approval fed that way is already stale by nine days. Nothing fails today only because `live_app` parks the coalesce timer at 3600 s, so the tick that would withdraw them never runs — roughly thirty prompt tests are one `coalesce_interval=` argument away from silently asserting against an empty screen. The two round-5 age-out tests avoid it by ingesting a `FrameRecord` stamped from the real clock.

**Product behaviour is unaffected, and this was checked rather than assumed.** `LiveSource` stamps frames from its own clock, defaulting to `time.time` (`talaria/transport/source.py:37,181,517`), so a live approval's `opened_at` and the age-out's `now` come from the same clock.

**Suggested framing.** Either stamp `feed()` from the real clock, or make `live_app` refuse a real coalesce interval unless the test opts in explicitly — the second is the stronger version, because it makes the trap loud at the moment a test walks into it.

### A hand-scrolled prompt region carries a stale reachability marker

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P2
**Effort.** Small
**Worth it when.** Alongside the item above — the two share a fix.

**Context.** `mark_unreachable_controls` runs after a resize or a re-wrap and not after a scroll, so an operator who scrolls the region by hand can leave a card marked `answer below — scroll` whose control is now on screen, or leave the ordinary `waiting for you` on a card whose control has just scrolled off. The direction of the error is a wrong label rather than the original silent inertness, and the scrolled state is one the operator caused and can undo — which is why this is separated from the defect it comes from. `ScrollableContainer` exposes `scroll_y` as a reactive, so a `watch_scroll_y` that chains to `super()` is the obvious hook; it needs a throttle, because it would otherwise run on every row of a drag.

### Two queued approvals with realistic commands: the second one is unreadable

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round
**Priority.** P2
**Effort.** Medium — this is a layout redesign, not a scroll fix.
**Worth it when.** A real session queues two approvals with long commands, or before an operator is expected to *read* rather than merely dismiss a queue.

**Context.** At 80x24 with two long commands, the second card is cut by `PromptRegion`'s `max-height: 70%`. Measured: `deny all` appears once rather than twice, and `max_scroll_y == 9`.

**Severity is below the resize defect fixed this round, and the reason is what makes it a P2 rather than a P1.** The *first* card's `deny all` is visible, and one press denies the whole queue — so the operator is never trapped without a safe action. The harm is being unable to read the second command before denying it, and for a denial that is the survivable direction.

**Worth fixing at the same time.** The overflow marker reads `… +N more lines (whole command in transcript)`. The second clause is true of the transcript *data* and false of the transcript *pane* at the moment the decision is live — the pane is scrolled to the bottom and the arrival entry has usually left it.

### Honour the clarify bridge's multi-select hint

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)
**Priority.** P2
**Effort.** Small
**Worth it when.** A real session raises a clarify that sets the flag, or checkbox-style answers appear in a corpus.
**Context.** `clarify.request` carries `multi_select: true` when the tool wants several choices, and the gateway emits the field *only* when it is true so that single-select payloads keep their pre-multi-select shape (`tui_gateway/server.py:5506-5519`). Its own comment says renderers without checkbox support "ignore the extra field and stay single-select (a single answer still parses as a one-element list on the tool side)", so Talaria's single-choice control is a supported degradation rather than a defect. It is recorded here because the degradation is currently *silent*: the operator is not told that the question wanted more than one answer.
**Refs.** `tui_gateway/server.py:5506-5519` at Hermes `7f4d15515`

### Send `approval.respond`'s `all` flag, or establish that the choice string carries it

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)
**Priority.** P2
**Effort.** Small
**Worth it when.** An operator reports that picking "always" did not stop the next identical approval.
**Context.** `approval.respond` accepts an `all` parameter, passed to `resolve_gateway_approval(..., resolve_all=params.get("all", False))` (`tui_gateway/methods_prompt.py:887-905`). The shipping terminal UI never sends it — it sends `{choice, session_id}` and nothing else (`useMainApp.ts:928`) — so the `session`/`always` semantics must ride the choice string, and Talaria matches that exactly. What is *not* established is whether the flag does something the string does not; the read stopped at the call site rather than following `resolve_gateway_approval` into `tools/approval.py`. Cheap to settle, and the failure it would produce is an approval the operator believes they answered permanently.
**Refs.** `tui_gateway/methods_prompt.py:887-905`, `ui-tui/src/app/useMainApp.ts:928` at Hermes `7f4d15515`

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

**Half-resolved by U9, 2026-08-03 — the paste half only.** The consuming unit arrived and answered its own question: `PasteThreshold` treats a non-positive bound as "this half is off", re-encoding the shipping client's `pasteCollapseLines > 0 && …` guard (`ui-tui/src/app/useComposerState.ts:277-280` at `7f4d15515`). So `TALARIA_COMPOSER_PASTE_COLLAPSE_LINES=0` no longer collapses every one-word paste; it disables the line bound and leaves the byte bound working. `talaria/cli.py:_build_paste_threshold` also falls back to the KTD16 defaults for a non-integer value rather than raising, because a malformed paste setting should not stop the client from starting. Pinned by `tests/domain/test_commands.py::test_a_non_positive_line_bound_switches_that_half_off` and `::test_both_bounds_off_collapses_nothing`, both watched to fail with the `> 0` clause removed.

**Not yet reachable end to end.** `talaria/cli.py` builds a `TalariaApp` in exactly one place — `run_replay`, at line 200 — and that is where `_build_paste_threshold(cfg)` is passed. There is no live launcher yet; a bare `talaria` run prints that session startup is not wired, which is U10's work. So the configuration path above is proved by its unit tests and by the replay construction, and is unexercised in the only mode where a paste is ever collapsed. U10 wiring the live launcher closes that, and until it does this item is half-resolved in semantics rather than in reach.

**Still open: `status.interval_seconds`.** `TALARIA_STATUS_INTERVAL_SECONDS=-5` still resolves to `-5` and still hands U6's status runner a negative sleep. There is no equivalent Hermes reading of a negative interval to re-encode, so the bound is a decision the status runner has to make, and it was out of U9's scope. Nothing in this update changes that path.

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


### The withdrawn-approval hedge does not retire when the screen shows the agent working

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P2
**Effort.** Small
**Worth it when.** Alongside any other work on the clearing rule. The line is stale rather than false, which is why it is not P1.

**Context.** `_clear_withdrawal_on_progress` (`talaria/domain/state.py`) clears the withdrawn count only when `_turn_progress` — `(turn, turn_index, streaming_text, segments)` — moves. `_on_tool_start`, `_on_tool_complete`, `_on_status_update` and `_on_session_info` all only call `_append`, which touches none of those four fields.

Reproduced with `APPROVAL_STALE_AFTER` patched to 0.05s: after a withdrawal, feeding `tool.complete {name: "bash", summary: "denied by timeout"}` — exactly the event the default-configuration case produces, the gateway's own fail-closed timeout coming back through the tool — leaves `withdrawn_approvals` at 1, and one screenshot contains both `bash ✓ denied by timeout` and `1 approval withdrawn — whether the agent is still blocked is unknown`. Unchanged after `tool.start`, `status.update` and `session.info` too. The hedge survives an unbounded number of tool calls and only ends on a `message.delta` or a turn-phase change.

This contradicts the clearing rule's own docstring: *"The moment the agent produces a token or the turn changes phase, it is no longer unknown — it is observed."* A tool completing is such an observation.

**Severity, stated fairly.** The sentence is a hedge about Talaria's knowledge, so a stale one is a screen contradicting itself rather than a screen lying about the session being busy — and it never reinstates `working…`, which is the failure R8 actually forbids.

**Suggested framing.** Widen `_turn_progress` to include tool activity, but do it narrowly and pin both directions: the case that must *not* clear is the bad one, where the gateway still holds the approval and the agent is blocked inside a tool call producing nothing. A tool *completing* is progress; a tool *starting* may not be.

### `focus_session` does not clear `withdrawn_approvals`

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P2
**Effort.** Trivial — one field, one assertion.
**Worth it when.** Before anything calls `focus_session`. There is still no production caller, which is the only reason this is latent.

**Context.** `focus_session`'s docstring says it exists to stop "session A's state bleeding into session B", and it clears `prompts`, `answering`, `approvals_seen`, `flushed_prompt_ids`, `turn`, `segments` and `streaming_text`. Round 5 added `withdrawn_approvals` and did not add it to the reset. Measured: `focus_session(SessionState(withdrawn_approvals=3, approvals_seen=5), "s2")` returns `approvals_seen=0` and **`withdrawn_approvals=3`**.

A screen consequence was attempted and could not be built: `focus_session` sets `turn="idle"`, and any later event that makes the turn stream also trips the clearing rule. So this is a genuinely broken invariant on a reset function, latent rather than visible, and unpinned. It belongs beside the existing `focus_session` item rather than separately from it.

### The terminal-read arrival line is the residual self-contamination, and it is measured

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P2
**Effort.** Small
**Worth it when.** Before a real session does many reads, or whenever the terminal-read decision is next opened.

**Context.** The decision to keep `terminal_read prompt awaiting an answer: …` in the transcript was originally justified with the claim that it "does not compound". Two independent lenses measured that it does — the correction is now in `DECISIONS.md` beside the decision. Six sequential reads in a session whose only real content is one `message.delta` grow the served payload from 172 to 512 bytes, and the sixth answer is six copies of the arrival line plus one line of actual content.

No individual line grows — that was round 3's defect. But the buffer accumulates one self-generated line per read, without bound in the number of reads, which is the same property the decision's rejected-alternatives paragraph uses to turn *outcome* lines down. Net accounting: the fifth round removed between zero and one line per **failed** read and left one line per **every** read, so the residual is strictly larger than what was removed.

**Suggested framing.** Move the arrival record to a side channel the read projection does not serve, keeping the audit property without the feedback. Dropping it entirely is the alternative already rejected, on the grounds that an agent reading the operator's screen should not be invisible in the operator's own record.


## P3
### Four guards inside round 5's reachability code have no test that can fail

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), verification of the fifth adversarial round
**Priority.** P3
**Effort.** Small
**Worth it when.** Alongside the P1 resize-trigger item, which is the same rule applied to the same code.

**Context.** Each of these survives deletion with the entire 651-test suite green, and each has a docstring paragraph arguing it matters. The behaviour is correct in every case — it was verified separately — so these are test gaps, not defects.

- **Both-directions retitling.** Making `mark_unreachable_controls` one-directional (mark, never un-mark) leaves the suite green. Real behaviour verified by hand: at 120x40 both cards read `waiting for you`; at 60x20 the approval reads `answer below — scroll`; back at 120x40 it reads `waiting for you` again.
- **`scrollable_content_region` rather than `region`.** The docstring argues the outer region is wrong because it includes the border and the scrollbar column. Swapping it leaves the suite green.
- **The zero-height clause.** Dropping `and target.region.height > 0` — whose docstring says "a control laid out at zero height is mounted, focusable, and draws nothing" — leaves the suite green.
- **The withdrawn line's absence from an idle turn.** Adding it to the idle branch of `activity_line` leaves the suite green.

There is also a predicate asymmetry worth resolving while here: `reveal_actions` breaks on `target.is_mounted` alone, while `mark_unreachable_controls` additionally requires `target.region.height > 0`. A first card whose control laid out at zero height would consume the reveal and leave every card marked with none revealed. Reasoned from the source, not reproduced.

**Context for why this is P3 and not higher.** This repository's recorded rule is that a guard nothing can exercise is a guard nobody can trust — round 4 deleted its own unreachable latch on exactly that basis. These are the same shape in round 5's new code. They are P3 because the behaviour is right today; the risk is that the next edit breaks one silently.



### Replay can fabricate an age-out the recording never contained

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P3
**Effort.** Small
**Worth it when.** A recorded corpus is used as evidence of what a session showed, rather than as a rendering exercise.

**Context.** `_age_out_approvals` uses the recorded clock in replay, which is what keeps AE2's "replay it twice, get the same state" true. But a corpus where an approval sits outstanding across 400 recorded seconds will withdraw the card and inject a `prompt-expired` transcript entry the gateway never emitted. Determinism holds — the fabricated entry is identical on every replay — so the failure is not a flake; it is that the replayed transcript shows an event that did not happen. Reachability in a real corpus is unproven; the 400-second gap is a synthetic construction.

**Suggested framing.** Either mark locally synthesized entries distinctly in replay, or suppress the age-out entirely in replay on the grounds that a recording's approvals have already had whatever fate they had. The second is simpler and loses nothing, because the reason the age-out exists — a phantom approval disabling the correlation rule for the *next* one — is a live-session concern.

### The age-out sentence speaks about "the gateway" from a pinned constant

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P3
**Effort.** Small
**Worth it when.** A deployment is known to run a non-default `HERMES_APPROVAL_TIMEOUT`, or the gateway starts publishing its configuration.

**Context.** `APPROVAL_AGED_OUT` reads "the gateway's default wait is 5 minutes and it announces no approval timeout, so it has probably stopped waiting". Both facts are read from Hermes at `7f4d15515` (`tools/approval.py:2648-2657`, `tui_gateway/server.py:2981-2998`), not from the connected gateway. The hedge — "probably" — keeps the sentence from being false, but it reads as a statement about *this* deployment when it is a statement about the pinned source. The same gap is what makes `withdrawn_approvals` necessary at all; see the post-withdrawal decision in `DECISIONS.md`.

**Suggested framing.** Say whose default it is ("Hermes's default wait is 5 minutes"), or probe the seam at startup and name the number when it is knowable and its absence when it is not — the pattern AGENTS.md already asks for on gateway capabilities.

### `answer_terminal_read`'s unavailable-projection path leaves the prompt registered

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round
**Priority.** P3 — **latent**; the code is correct today and the invariant it depends on is not enforced.
**Effort.** Small
**Worth it when.** Before anything else can make `transcript_view_for_read()` return `None`.

**Context.** `talaria/ui/app.py:1201-1215` discards the request id from `_answering` and writes a note, but leaves the prompt registered. `_answer_unattended_prompts`'s docstring now depends on the opposite: "this dispatches on sight, so the bound is that every outcome settles the prompt". Proven **unreachable as a loop today** — the only trigger is `transcript_view_for_read()` returning `None`, which happens only during teardown or before the first render, and both of those stop the render tick that would re-dispatch.

**Frame it as making the code honour the invariant its docstring depends on**, not as fixing a loop. The one-line change is to settle the prompt on that path as every other outcome does; the reason it needs a moment's thought is deciding whether a read that could not be answered should leave any trace in the registry at all.

### `focus_session` disarms the in-flight bookkeeping the prompt registry depends on

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round
**Priority.** P3 — **latent**, and the priority is low only because of that. Fix it before anything calls the function.
**Effort.** Small
**Worth it when.** Before the first production caller. `grep -rn "focus_session" talaria/` finds exactly two hits — the definition and the `__all__` entry — so there is no caller today.

**Context.** `focus_session` (`talaria/domain/state.py:237-260`) clears `prompts`, `answering` and `flushed_prompt_ids`. The last two are precisely what the third round added to make the in-flight answer window safe:

- clearing `answering` re-enters the shipped uncorrelated-approval defect through another door — `outstanding_approvals` searches `answering` so an approval whose answer is travelling still counts, and a focus switch inside that round trip makes it stop counting;
- clearing `flushed_prompt_ids` removes the latch that stops `restore_prompt` resurrecting a prompt the gateway has already closed, and `prompt_view` (`talaria/domain/projection.py:321`) applies no session filter, so a resurrected foreign-session prompt renders.

Reproduced at the domain level.

**Its own docstring is currently false**, and that is worth fixing whichever way this goes: it says "the caller here is reconnect, not a UI control", describing a caller that does not exist. Reconnect does not call it today.

**Decide, don't patch.** Either the reconnect path should call it (in which case the in-flight sets need a considered policy — probably settle rather than drop), or the function should not survive to v0.1 with no caller.

### The terminal-read bridge serves un-defanged bytes, and the screen it claims to describe is defanged

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round — found while fixing the bidi/zero-width defect, queued rather than fixed.
**Priority.** P3
**Effort.** Small, once the boundary is decided.
**Worth it when.** An agent is observed acting on terminal-read output rather than treating it as prose, or the defang set changes again.

**Context.** `defang` now replaces bidirectional-override and zero-width characters as well as C0 controls, and it runs at *render* time — `literal_text` is the one door onto a widget. So the card, the command panel and the transcript pane are all clean, and the stored `TranscriptEntry.text` deliberately keeps the gateway's bytes as the audit record.

`terminal_read` (`talaria/domain/projection.py:446`) serves `TranscriptView.lines`, which are built straight from those stored entries. So the buffer the agent receives is **not** the buffer the operator sees whenever a defanged character is present: the agent gets `‮`, the screen shows `�`. The bridge's contract is "what is on screen".

**Not fixed now** because the fix has to choose a boundary and the choice is not obvious. Defanging in `transcript_view` would put the terminal layer's rule into `talaria.domain`, which ADR-0002 forbids; defanging in `answer_terminal_read` puts it on the UI side of the seam but then the projection's own tests describe a buffer nobody is served. Neither is a five-line change, and the divergence is currently cosmetic — an LLM reading JSON does not perform bidi reordering.

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
