# Learnings - talaria

> Empirical findings, mechanisms, fixes, validations, and generalizable rules. Keep newest entries first.

## 2026-08-03

### A security rule coupled to a path string was disarmed by ordinary nesting, and its docstring claimed the opposite

**Author.** v0.1 milestone-1 integration, from an external review of the redaction boundary

**Evidence.** `talaria/recorder/redact.py`. Three defects, each putting a credential into an append-only, hash-chained frame log:

| shape | result before the fix |
| --- | --- |
| `{"method": "clarify.respond", "params": {"inner": {"answer": "..."}}}` | credential written verbatim |
| the same frame inside a batch, or under any wrapping envelope | credential written verbatim |
| `wss://operator:hunter2@gateway.local/attach?x=1` | round-tripped whole, including in the frame-log header |
| `http://user:pass@cdp.example/` in a frame body | untouched — the URL check required a `?` before it would look |

**Mechanism.** Two independent causes. The deny-set — the rule covering `answer`, `value`, `text` and `password` on Hermes's four blocking bridges — was resolved once from the outermost object and then applied only where the walker's dotted path was *exactly* `params` or `params[...]`. That coupled a security decision to a string comparison on position, so every shape that moved the frame off the top level silently disarmed it. Those keys are deny-set-only by design: the key-name net is deliberately built not to catch `answer`, so nothing stood behind it. Separately, `redact_url` rewrote only the query and handed `parts.netloc` back to `urlunsplit` verbatim — and `netloc` is exactly where `user:password@host` lives.

The most instructive part is that the walker's docstring already claimed the property it lacked: *"a credential nested inside a batch or an unexpected envelope shape is still caught."* The walk did recurse; the deny-set did not travel with it. The test named `test_catches_a_credential_nested_at_arbitrary_depth` reinforced the false impression — its fixture's credential is under the key `token`, so it exercised the key-name net at depth and never the deny-set. A reviewer checking whether nesting was covered found a green test that said yes.

**Fix.** The method is re-read from each object that carries one and governs that object's own `params` subtree to any depth; only a method actually in the deny-set takes over, so an unrelated inner `{"method": "GET"}` cannot clear a context established above it. `redact_url` withholds the whole userinfo component — the username position too, since `https://<token>@host/` is an ordinary bearer form — and rebuilds `netloc` only when userinfo is present, so clean URLs stay byte-identical for the KTD6 comparison. The frame-body URL check no longer requires a query string.

**Outcome.** All four shapes withheld, verified end-to-end through the real writer; over-redaction controls unchanged (usage counters, harmless URLs, mixed-case hosts, IPv6 literals, percent-escapes). The live U2 corpus on this machine was scanned and is clean: 46 records, zero `devtools/browser`, zero userinfo, zero query-bearing URLs.

**Generalizable rule.** When a rule's scope is expressed as a position — a path prefix, a depth, an index — moving the data is enough to defeat it, and data moves for reasons that have nothing to do with security. Bind the rule to the object that owns it and let it travel with the walk. And when a docstring asserts a property, write the test that would fail if the property were absent: a test whose fixture is caught by a *different* mechanism proves nothing about the one being claimed, while looking exactly like proof.

**A second rule, from how the reachability was argued.** The reviewer rated this P1 partly because the pinned gateway has no JSON-RPC batch support — reasoning about the one path they had in mind. But `params.inner.answer` needs no batching; it is a plain frame with one extra level. *Reasoning about a single route the data might take is the same error the code made.* When a defect is that a rule is coupled to position, an argument about reachability that walks one position inherits the bug. Ask what class of shapes defeats the rule, not which known shape does.

**And a third, learned the hard way twice in one session.** A test written to pin a fix must be run against the *pre-fix* code before it is trusted. Two tests written here — one for the empty-entry hole in `content_is_complete`, one for the deny-set — initially passed against the unfixed implementation for unrelated reasons, and would have shipped as decoration. Keeping a verbatim copy of the old function and asserting `old=True, new=False` takes a minute and is the only thing that distinguishes a regression test from a comment.

### A skipped test is invisible inside a green run, so the standing evidence for parity had never run

**Author.** v0.1 milestone-1 integration, from an external review of CI configuration

**Evidence.** All five `@requires_ts_bridge` tests in `tests/recorder/test_equivalence.py` skipped in CI. The `python-check` jobs installed `uv` and never Node, so `node_modules/.bin/tsx` did not exist; the one job that did install Node ran `npm run check`, not pytest. `test_equivalence_over_the_synthetic_credential_corpus` — whose own skip message calls it *"the CI-standing evidence"* for the KTD6/R28 parity relation — had therefore never executed in CI. It passes when actually run, so the port does not diverge; the defect was never a wrong result, only an unrun proof reported as `353 passed`.

**Mechanism.** `pytest.mark.skipif` is the correct behaviour on a developer machine without Node and the wrong behaviour on the job that exists to prove parity, and one marker cannot tell the two apart. Nothing in the run distinguishes "6 skipped" from "6 passed" at a glance, and no summary line says which claim just went unverified.

**Fix.** Node and `npm ci` added to `python-check` — the leg that fails the run, not the informational Linux leg — and `TALARIA_REQUIRE_TS_BRIDGE=1` set for its pytest step, with a test that fails when the variable is set and the bridge is missing. Suite went from 371 passed with 6 skips to 382 passed with zero skips.

**Generalizable rule.** A conditional skip is an unverified claim wearing a green check. Where a test *is* the evidence for a stated property, make its absence fail somewhere: pin the environment that runs it, and assert that environment is present rather than trusting it. Count skips in CI as deliberately as failures.

### Four of seven gate measurements could not fail, and the one that mattered compared the projection with itself

**Author.** v0.1 milestone-1 integration, from an adversarial audit of the validation gate

**Evidence.** The Textual validation gate reported `pass` on all ten checks and was about to settle the framework decision for v0.1. An audit injected, into each check, the exact defect that check exists to detect:

| injected defect | what actually happened | what the gate reported |
| --- | --- | --- |
| removed the two `widget.remove()` calls | 4,455 widgets genuinely mounted, 7.4x the 600 ceiling | `mounted_widgets: 501`, pass |
| removed the condense-before-mount guard | 540 widgets mounted in one tick against a cap of 40 | `peak_mounted: 41`, test passes |
| made `TranscriptPane.apply` a no-op | interface rendered nothing at all, blank screen | `content_loss: 0`, pass |
| discarded 9 of every 10 inbound frames | 90% of the conversation destroyed | `content_loss: 0`, `frames_applied` still matched |
| scheduled a render on every frame | coalescing entirely defeated, 6,419 real renders | rate went *down*, pass |

**Mechanism.** Six of the seven measurements were counters the object under test maintained about itself; only resident-set memory was observed from outside. The decisive one was `content_is_complete(app.state, transcript_view(app.state))` — the projection compared against a pure function of the same state. Its own docstring warns that comparing the projection with itself "would pass no matter what", and both call sites did exactly that. `mounted_count` returned `len(self._widgets)`, a private deque the pane maintained and nothing reconciled against the real tree. `render_ticks` was incremented in a `set_interval(0.05, ...)` callback, so it was bounded by 20/s by construction and could never breach its own 25/s threshold.

Notably the *thresholds* were all honest — every constant matched the plan exactly, nothing was quietly loosened. The dishonesty was entirely in what was measured, which is much harder to see in review than a moved goalpost.

**Fix.** `mounted_count` reads `len(self.children)`; renders are counted in `render_snapshot` where a render happens; content completeness is compared against the pane's actually-rendered lines at a settled checkpoint; plus new checks for frame accounting, minimum sample counts, and a missing corpus path raising instead of silently dropping three of ten checks.

**Outcome.** The repaired gate failed immediately, on a real defect: `TranscriptPane.reconcile` desynchronizes when a transient notice line appears mid-transcript and later disappears, leaving 274 lines rendered against 275 projected with one line of conversation rendered nowhere. The run halted per the plan's unattended contract. The framework question is open again — not because Textual failed, but because the evidence that said it passed was measuring itself.

**Generalizable rule.** For every check in a gate, ask what value it is *capable* of reporting, and then go and produce a failing one. A check that has never been observed to fail, and cannot be made to fail on demand, is decoration — and a gate made of such checks is worse than no gate, because it converts an open question into a settled one. Prefer measurements taken from outside the thing measured; when the subject supplies its own numbers, something independent has to corroborate them.

### A key-name matcher normalized the names it was meant to canonicalize, and never looked at values at all

**Author.** v0.1 milestone-1 integration, from a direct probe of the redaction boundary

**Evidence.** `talaria/recorder/redact.py` is the boundary that guarantees credentials never reach the frame log on disk. Fourteen adversarial frame shapes were passed through `redact_frame` and then through `FrameRecorder` end to end, checking the file's raw bytes for a canary. Eleven were caught. **Three wrote the canary to disk**: a key named `ApIkEy`, a key named `api key`, and a credential URL under the innocuous key `url`.

**Mechanism.** Three unrelated causes behind one boundary.

- `_normalize_key` inserts `_` at every lower-to-upper boundary so that `accessToken` becomes `access_token`, which the anchored patterns need. Applied to an unusually cased name it does the opposite of canonicalizing: `ApIkEy` becomes `ap_ik_ey`, separators inserted mid-word, matching nothing.
- The patterns anchor on a `[-_]` separator class, so `api key` and `api.key` do not match. Only two of the plausible separators were covered.
- The walker tests key *names* and never inspects values. A frame carrying `{"url": "ws://host/api/ws?token=..."}` has no suspicious key in it, so the token was written verbatim — and KTD11 puts the attach credential in precisely that position, which makes it the likeliest shape to occur rather than an exotic one.

**Fix.** Match key names in a squashed form (lowercased, every separator removed) *in addition to* the camel-normalized form, since neither is a superset of the other. Check string values for absolute URLs whose query actually carries a denied parameter, and redact only those. Both are new divergences from the TypeScript reference and are enumerated in the module docstring, because KTD6's requirement is that the divergence be exactly listed, not that it be small.

**Validation.** All fourteen shapes now redact; the canary is absent from the file bytes. Over-redaction controls confirm `max_tokens`, `input_tokens`, `session_total_tokens`, `tokens_per_delta` and `maxTokens` are still preserved, and a harmless `?page=2` URL is recorded untouched. `uv run pytest` — 371 passed, equivalence harness included.

**Generalizable rule.** A normalizer applied to input it was not designed for can be worse than no normalizer, because it silently produces a well-formed value that is wrong. When a matcher canonicalizes before testing, probe it with inputs that are *badly formed rather than adversarially crafted* — odd casing and unusual separators — since those are what real systems actually emit. And a filter that inspects only names will miss everything carried in values; ask which of the two the credential's own protocol actually uses.

### A byte cap applied after the read bounds the display, not the memory

**Author.** v0.1 milestone-1 integration, from an adversarial review of the status runner

**Evidence.** `talaria/status/runner.py` documented a 16 KiB stdout cap and a 4 KiB stderr cap, and enforced both by slicing the result of `Process.communicate()`. `communicate()` reads until EOF. Measured directly: a status command of `sh -c 'exec yes AAAA...'` under a 2-second timeout drove the parent's resident set from **27.6 MB to 3030 MB — +3002 MB in 2.04 seconds** — while the declared stdout cap was 16,384 bytes. A command emitting a finite 512 MB document and exiting 0 returned `outcome=ok` after buffering all 512 MB. The status runner ticks on a timer, so this recurs every tick for as long as the command misbehaves.

**Mechanism.** The limits were applied at the point of *rendering* rather than the point of *reading*, so they answered "how much do we show" when the operator-facing promise was "how much do we hold". Nothing else bounded the read: not the timeout, which only caps how long the flooding continues, and not the row limit, which applies later still.

**Fix.** Read each stream in chunks up to `limit + 1` bytes instead of calling `communicate()`, and kill the process group the moment a cap is crossed. The `+ 1` preserves the existing `len(raw) > limit` truncation test exactly, so `oversize output is a bounded success` (R22) still holds — an endless writer now returns `ok` with `truncated=True` in about 10 ms and **+0 MB**, where it previously returned `timeout` after the full budget and +3 GB.

Two things went wrong in the fix itself and are worth recording. Reading both streams concurrently and waiting for both to finish deadlocks: once stdout stops being read at its cap the child blocks on a full pipe, so stderr never reaches EOF and the tick times out instead of reporting the bounded success it already has. The cap has to kill the group at the moment it is crossed, not after both reads return. Separately, sweeping the process group unconditionally in a `finally` introduced a worse bug than it fixed — `killpg` on an already-reaped child can land on a recycled pid, which surfaced immediately as `PermissionError: Operation not permitted` and would otherwise have been SIGKILL delivered to an unrelated process group. The group must be swept while the child is still unreaped, because until it is reaped the kernel cannot reuse its pid.

**Validation.** `uv run pytest` — 366 passed. The flood, orphaned-worker and descriptor-leak cases are pinned by new tests; measured after the fix: +0 MB on the flood, 0 surviving workers, 0.00 descriptors leaked per tick against a previous steady 2.00.

**Generalizable rule.** When a limit protects a resource, enforce it at the point where the resource is consumed, not where it is displayed. And when a fix involves signals or process groups, ask what the identifier means *after* the thing it names has gone away: a pid is not a stable handle, it is a number the kernel is free to reissue the moment the process is reaped.

### A file walk that skips symlinks disagrees with the import system, and the guard blesses the gap

**Author.** v0.1 milestone-1 integration, from an adversarial review of the ADR-0002 guard

**Evidence.** `tests/domain/test_boundary.py` enumerated the domain package with `_DOMAIN_ROOT.rglob("*.py")`. A symlinked subpackage placed at `talaria/domain/linked` — with its real contents outside the tree, importing `textual` — was fully importable (`talaria.domain.linked.evil` resolved and loaded), and the sweep never saw it: `2 passed`. The companion test that exists specifically to close enumeration holes made it worse by *approving* the directory, since the target does contain `__init__.py`. Two sibling holes had the same cause: a sourceless `.pyc` and a compiled `.so` dropped into the package are both importable and neither ends in `.py`.

**Mechanism.** `Path.rglob` does not descend into symlinked directories; Python's import system does. The guard was therefore answering "what source files are in this subtree" when the question it needed to answer was "what can the interpreter import from this package". Detection was never the weak point — every attack that put a forbidden import in front of the sweep was caught, including both spellings of the sibling-package regression the allow-list was built for. The weak point was the list of things handed to the sweep.

**Fix.** Walk with `os.walk(..., followlinks=True)` and match `importlib.machinery.all_suffixes()` instead of the literal `".py"`, which closes the symlink, `.pyc` and `.so` cases together. `Path.rglob(recurse_symlinks=...)` would be the natural spelling but does not exist before Python 3.13, and this project supports 3.12. The `__init__.py` companion test follows symlinks now too, for the same reason. Both attacks were replanted afterwards and both go red; the clean tree still passes.

**Generalizable rule.** When a check enumerates inputs by walking the filesystem, the walk is part of the check and needs attacking separately from the logic. Ask what the *consumer* of the list can reach — here, the interpreter — and enumerate against that definition rather than against a filename convention. A guard whose detection is sound but whose enumeration is incomplete fails silently and looks green, which is strictly worse than one that errors.

### A high-water counter sampled after the trim it is meant to police reports an identity, not a measurement

**Author.** v0.1 milestone-1 integration, from a CI failure

**Evidence.** `talaria/ui/transcript.py` maintained `peak_mounted` as the KTD14 gate's mounted-widget metric, updated at the end of `reconcile()`. The gate reported 501 against a ceiling of 600 and passed. On PR #11 the required macOS CPython 3.12 leg failed on an unrelated-looking assertion in `tests/ui/test_transcript_bounds.py::test_a_resize_storm_preserves_reflow_anchors_and_content`: `assert 49 <= (40 + 1)`, mid-stream, with a test cap of 40. That is a state `peak_mounted` said was unreachable — it had never once reported above `cap + 1`. Instrumenting `mount_all` directly measured post-mount counts of up to **51** against that same cap of 40, with 28 of 33 samples above `cap + 1`, while `peak_mounted` read **41**.

**Mechanism.** `reconcile()` mounts new line widgets and *then* trims back to the cap, awaiting in between. The counter was updated after the trim, at which point the invariant it was measuring had already been restored by construction — so it could not report a value above `mount_cap + 1` whatever the pane did. "501 against 600" read like a measured safety margin; it was `500 + 1`, an identity. The module's own comment argued that a transient the operator sees as a slow frame is the thing that matters and that "a snapshot after the fact cannot see" it — which is precisely what the counter was. The test suite could not catch this either, because three separate assertions checked the tight bound *against the counter*, so they were confirming the tautology rather than the pane.

**Fix.** Sample `peak_mounted` immediately after the mount and before the trim as well. The gate was re-run on the honest metric and still passes — 501 on the stress corpus, **507** sustained, against the unchanged 600 — so the verdict did not change, but the sustained figure is now a real measurement with 93 of headroom. The worst case does not materialize because a backlog larger than the cap is condensed before it is mounted. Tests now assert the two bounds separately: `mounted_count <= cap + 1` once settled, `<= 2 * cap + 1` mid-update.

**Validation.** `uv run pytest` — 359 passed; the two formerly-flaky tests pass 5/5 locally, and the gate re-run exits 0 with `verdict: pass`.

**Generalizable rule.** A metric that samples only where its invariant is guaranteed to hold measures nothing. Before trusting a threshold check, ask what value the instrument is *capable* of reporting — if a failing reading is unreachable by construction, a passing one is not evidence. Corollary: when a metric and a test assert the same bound, the test cannot validate the metric; something outside the pair has to observe it, which here was a loaded CI runner sampling at a moment the developer machine never hit.

### Assigning `self._closing` in a Textual `App` subclass hangs every Pilot test at teardown, and the traceback names nothing in your code

**Author.** v0.1 unit U5 — the replay-driven Textual shell

**Evidence.** `talaria/ui/app.py` briefly used `self._closing = True` in its own `shutdown_sources()` to stop the coalescing render tick. Every test using `async with app.run_test()` then hung — not at the assertion, at the *end* of the block — and `faulthandler` dumped only `selectors.select` → `asyncio.base_events._run_once`, with no Talaria frame anywhere in the stack. A minimal Textual app in the same session exited in 0.30 seconds, and a `TalariaApp` over an empty corpus hung, which located the fault in the subclass rather than in the framework or the corpus.

**Mechanism.** `textual.message_pump.MessagePump.__init__` sets `self._closing = False` as an *instance* attribute; `App` inherits it. Setting it to `True` tells the framework its own shutdown is already in progress, so `App._shutdown()` skips the work it would otherwise do and `_process_messages` never returns — `run_test`'s `await app_task` then waits forever. The name is never declared at class level, so a class-dictionary comparison cannot see it, and neither mypy nor ruff has any reason to object: assigning an attribute on `self` is ordinary Python. The same class of collision had already bitten once in this unit, when a coalescing-flush callback named `_flush` silently replaced `App._flush` (which flushes captured stdout).

**Fix.** Renamed to `_teardown_started`, and added `tests/ui/test_app_shadowing.py`, which parses `talaria/ui/app.py` with `ast` and fails the build if any name defined in the class body — or any `self.<name> =` assignment inside it — collides with something in `App.__mro__` or in `vars(App())`, unless it is listed in an explicit `DELIBERATE_OVERRIDES` set. Source parsing rather than `vars(TalariaApp)`, for two reasons: `_closing` is not in any class dictionary, and Textual's `DOMNode.__init_subclass__` injects `_reactives`, `_computes` and friends into every subclass, so the class dictionary is full of names the author never wrote.

**Validation.** `uv run pytest` — 359 passed in 41.8s, from a state where a single Pilot test could not finish inside 900 seconds.

**Generalizable rule.** When subclassing a framework class with a large private surface, treat the instance namespace as shared and check it mechanically. A name collision with a framework's *instance* attribute produces a hang or a silent behaviour change, never a clean error, and the check that catches it has to read the source — the collision is invisible to `vars()` on the class.

### A `Paste` event posted to a Textual widget inserts the text twice

**Author.** v0.1 unit U5

**Evidence.** `tests/ui/test_composer.py` asserts that a 400-line bracketed paste inserts without submitting. Posting `events.Paste(text)` to the `TextArea` produced 798 newlines instead of 399. Reduced to a five-line Textual app: `text_area.post_message(Paste("AB"))` yields `"ABAB"`, while `app.post_message(Paste("CD"))` yields `"CD"`.

**Mechanism.** `TextArea._on_paste` inserts the text and does not stop the event, so it bubbles to the `App`, which forwards it back down to the focused widget — which inserts it again. A real bracketed paste is delivered by the terminal driver to the `App`, so the doubling never happens in production; it is an artefact of addressing the widget directly.

**Fix.** Tests post `Paste` to the app, matching the real delivery path.

**Generalizable rule.** In a bubbling event system, post synthetic input where the real input enters — the top — not where you want it handled. Injecting below the real entry point can exercise a delivery path that production never takes, and here it would have hidden a genuine paste defect behind a passing test.

## 2026-08-02

### A test helper imported as `tests.x.y` collides with mypy's own `files = ["tests"]` scan the moment a `tests/` subpackage has no parent `__init__.py`

**Author.** v0.1 unit U6 — status-line runner

**Evidence.** `tests/status/test_runner.py` and `tests/status/test_process_contract.py` share one helper (`python_argv`, in `tests/status/conftest.py`) and import it with `from tests.status.conftest import python_argv`. Before this fix, `uv run mypy` failed with `Source file found twice under different module names: "status.conftest" and "tests.status.conftest"`, and only for that one file.

**Mechanism.** `pyproject.toml`'s `[tool.mypy] files = ["talaria", "tests"]` makes mypy compute each scanned file's module name by walking up through directories that hold an `__init__.py`, stopping at the first ancestor that doesn't. `tests/` itself had no `__init__.py` (only `tests/domain/__init__.py`, `tests/recorder/__init__.py`, and now `tests/status/__init__.py` did), so the scan named `tests/status/conftest.py` as top-level module `status.conftest`. The `from tests.status.conftest import ...` statement in the two test files asks mypy to additionally resolve an import named `tests.status.conftest` — a different qualified name for the identical physical file — which mypy reports as a collision rather than silently picking one. Every other test subpackage in this repo never triggered it because nothing else cross-imports between test files; each one only relies on pytest's implicit `conftest.py` fixture injection.

**Fix.** Added an empty `tests/__init__.py` so `tests/` is a real package and the whole tree resolves under one root (`tests.status.conftest`), matching the qualified name the import statement already asked for.

**Validation.** `uv run mypy` — `Success: no issues found in 46 source files`; `uv run pytest` unaffected (296 passed) since pytest's `rootdir`/`testpaths` behavior does not depend on `tests/__init__.py` existing.

**Generalizable rule.** If a test file does `from tests.<pkg>.<mod> import ...`, `tests/__init__.py` must exist — otherwise mypy's own file-list scan and that import statement disagree about the file's fully-qualified name, and the error surfaces as a confusing "found twice" rather than a missing-package message.

## 2026-08-02

### Two of Hermes's blocking-prompt bridges expire on the wire with no client handler, and approvals carry no request id at all

**Author.** v0.1 unit U3 — the ADR-0003 reconciliation-catalogue read at `7f4d15515`

**Evidence.** `tui_gateway/server.py:2989-2998` emits a `<bridge>.expire` event on timeout for all four blocking bridges it names — `secret`, `sudo`, `clarify`, `terminal.read`. The shipping terminal UI's event switch (`ui-tui/src/app/createGatewayEventHandler.ts:1174-1182`) handles exactly two of them, `sudo.expire` and `secret.expire`. Separately, `approval.request`'s payload at `:1130-1147` is `{description, command, choices, allow_permanent, smart_denied}` — no `request_id` — and `approval.respond` resolves by session key instead (`tui_gateway/methods_prompt.py:886-920`).

**Mechanism.** The gap is invisible from either side alone. Reading the client, the two handled expiries look like the complete set. Reading the gateway, the emit site looks like it has four listeners. `clarify.expire` is masked because Hermes recovers the same situation by a different route — a `tool.complete` for the clarify tool triggers an abandoned-prompt flush — and `terminal.read.expire` is masked because that bridge is desktop-only, so the terminal UI never sees it in practice. The approval finding is masked differently: a UI that shows one approval at a time never notices that it has no key to show it under.

**Fix.** Talaria routes all four expiries through one prompt registry keyed by `request_id`, and synthesizes a stable session-scoped key for approvals (`approval:<session_id>`) because only one approval can be outstanding per session on this protocol. Both are catalogued as rules RR-27 and RR-28 with named tests, so they are decisions rather than accidents.

**Validation.** `tests/domain/test_prompt_registry.py::test_every_bridge_expires_through_the_same_registry` and `::test_approval_gets_a_synthesized_session_scoped_key`, green in the U3 suite.

**Generalizable rule.** When re-encoding behaviour from a client, read the server's emit sites too — a client's handler list is evidence of what that client needed, not of what the protocol sends.

### A rule catalogue that is only prose rots silently, so the tests parse it

**Author.** v0.1 unit U3

**Evidence.** `docs/analysis/2026-08-02-hermes-reconciliation-rules.md` carries 38 rules in a markdown table whose last column names a test function. `tests/domain/test_reconciliation.py::test_every_catalogued_rule_names_a_test_that_exists` parses that table and fails if any named test is absent from `tests/domain/`; a companion test asserts the rule ids run `RR-01..RR-nn` with no gaps, so a deleted rule is visible rather than merely missing.

**Mechanism.** ADR-0003 names this failure mode precisely — "a missed rule produces a defect months later that Hermes fixed years earlier, and nothing in the codebase points at the omission" — and the same is true one step later: a rule that is catalogued and then never implemented looks identical in a diff to one that is catalogued and implemented. Prose cannot tell those apart. A parser can.

**Fix.** Every rule carries an explicit verdict (re-encode / re-encode with a change / drop) and a named test, including the drops — a dropped rule names the test that proves the drop is still deliberate, which is the only way "we decided not to" stays distinguishable from "we forgot".

**Validation.** The full domain suite is green with the catalogue in place; deleting a row's test name fails `test_every_catalogued_rule_names_a_test_that_exists`.

**Generalizable rule.** If a document is a precondition for code, make the test suite read the document.

## 2026-08-02

### `pkgutil.walk_packages` cannot see a module Python can import, so the ADR-0002 guard had a silent hole

**Author.** Code review of the v0.1 Python scaffold (segment 1)

**Context.** ADR-0002 — the domain core never imports the terminal framework — has exactly one enforcement mechanism: `tests/domain/test_boundary.py`. The first version enumerated domain modules with `pkgutil.walk_packages`, then asserted that no `textual.*` module appeared in `sys.modules` afterward. Both halves were wrong in ways that a green test run could not reveal.

**Evidence.** Against a synthetic package, a module at `pkg/session/handlers.py` with no `pkg/session/__init__.py` produced `walk_packages(...) == []` — the sweep listed *nothing at all* — while `importlib.import_module("pkg.session.handlers")` located and executed that same module. Reproduced in this repository: adding `talaria/domain/models/decode.py` containing `import textual`, without an `__init__.py` beside it, left the boundary test green. Separately, importing `textual` into the pytest process *before* running the committed test made it fail and accuse the domain package of a violation it had not committed.

**Mechanism.** `walk_packages` walks *packages*, and a directory without `__init__.py` is a namespace package it will not descend into by default — but Python's import system resolves the module anyway. So the guard's blind spot is precisely an ordinary omission: nobody notices a missing `__init__.py`, because imports keep working. The second defect is the mirror image — reading process-global `sys.modules` measures the whole pytest process, not the domain package, so the check breaks as soon as `talaria/ui/` lands and legitimately imports Textual.

**Fix.** The module list now comes from a filesystem walk of `talaria/domain/**/*.py`, a companion test asserts every directory under `talaria/domain` carries an `__init__.py`, and the import sweep runs in a subprocess so what it observes is attributable to the domain package alone. Falsified in both directions: the missing-`__init__.py` case now fails on the directory assertion, the packaged violation fails on the sweep, and a pre-imported `textual` in the pytest process no longer produces a false accusation.

**Generalizable rule.** When a single test is the *only* enforcement of an architecture decision, verify it fails on the violation it exists to catch — and check that its enumeration step sees everything the runtime does. A guard that enumerates differently from the import system is not a weaker guard; it is a guard with a silent hole, which is worse, because it reports success. Corollary: a check that reads process-global state measures the process, not the subject.

### An execution-spec `returns` field is a machine key list, and a passing validator did not prove the type

**Author.** Root-causing why every emitted unit carried a garbled return gate, on operator follow-up to the final doc review

**Context.** The v0.1 execution spec authored each unit's `returns` as a prose sentence ("Scaffold commit summary, check-command output, …"). The workflow emitter declares the field `list[str]` and treats each element as a JSON key name: the emitted unit prompt says "your FINAL message MUST be ONLY a single JSON object with the keys" plus the joined list, and the gate fails the unit if those keys are missing.

**Evidence.** The emitted workflow's U1 gate read `required: ["S", "c", "a", "f", "f", "o", …]` — the sentence's characters — because the loader runs `[str(r) for r in data.get("returns", [])]`, which iterates a string character by character when handed one. Yet `validate --require-receipts` reported the spec valid both before and after the fix. The doc review initially filed this as an emitter bug and recommended avoiding the workflow backend entirely (D28); reading the loader and the prompt-composition code showed the type contract was the emitter's all along.

**Fix.** All ten units' `returns` retyped from prose to snake_case key lists; the re-emitted workflow now carries sane schemas, prompts, and gates, and the launch-time re-emit reproduces them from the spec. The validator's failure to reject a string where a list is required is reported for the plugin repository.

**Generalizable rule.** A passing validator proves only what the validator checks — before filing a bug against a generator, read the type it declares for the field you fed it. And when a defect is attributed across a tool boundary, locate the exact line that misbehaves before deciding which side owns the fix; the wrong attribution here nearly cost the operator the execution backend they wanted.

### Review artifacts are repo files, and they arrive carrying their producer's context

**Author.** Reconciling an external doc review of the v0.1 requirements, at the operator's request

**Context.** An external `/doc-review` of the v0.1 requirements document left its durable artifact and four evidence receipts under `docs/reviews/`. The review itself was sound — fourteen findings fixed in place, and on reconciliation every protocol claim it introduced was verified against Hermes `7f4d15515`. But the artifacts were written from the reviewing workspace's perspective, where naming its own gate tooling, reviewer-model alias, and session identifiers is normal.

**Evidence.** The gate-status section named a private plugin repository's script path and a private reviewer-model alias, and the receipts carried local session identifiers; both referenced repositories were confirmed private via the GitHub API, while this repository is public and `docs/reviews/` is tracked. Nothing caught it at generation time because the private-context scan was a habit applied to documents this workspace writes, not to files another workspace delivers into it.

**Mechanism.** A review artifact is generated in the reviewer's context but committed in the target's. Anything the reviewer treats as ambient environment — tool paths, model aliases, session keys — becomes disclosure the moment the target repository is public, and the failure is silent because the files are correct, useful, and requested; nothing about them looks like a leak.

**Fix.** Identifiers were generalized with stable scrubbed tokens before the first commit, each edited file carries a scrub note, and the reconciliation artifact records before/after hashes, because scrubbed receipts no longer match the hashes they recorded — an accepted cost while the review gate is advisory. See the RC2 entry in the reconciliation artifact under `docs/reviews/`.

**Generalizable rule.** Run the private-context scan on every file entering a commit, whoever wrote it — externally-generated artifacts especially, because they embed their producer's environment rather than this repository's conventions. And when evidence files must be edited, record the before/after hashes somewhere durable, so the edit is itself evidenced rather than silent.

### A recommendation's fallback is a test of its reasoning, and this one failed it

**Author.** Promoting the framework analysis chain into ADRs

**Context.** Four analysis documents chose a stack, and the last of them switched the recommendation from TypeScript with OpenTUI to Python with Textual. The switch was argued on four newly-weighted constraints, three of which are about the **language** and not about any framework: the surrounding repositories are Python, Hermes core is Python, and the code will be predominantly agent-authored so a broadly-documented language matters. The same document named Go with Bubble Tea v2 as the fallback if Textual failed its validation gate.

**Evidence.** Those two positions cannot both be right. If the alignment constraints are strong enough to select Python, then a **framework** failing a **rendering** gate is not evidence against the language, and falling back to Go would discard three of the four reasons the choice was made. If the constraints are not that strong, they should not have selected the language. The document also contained its own tiebreaker, unanswered as an explicit empty item on the constraint list: must Talaria be a small native executable that runs without a managed runtime? The operator answered no, which removed the only advantage Go had that Python cannot match — and with it the fallback's entire justification.

**Mechanism.** The error is easy to make and hard to see because the primary choice and the fallback get written at different moments and are read as separate paragraphs. The primary is argued carefully; the fallback is often the runner-up from the previous ranking, carried forward without re-checking that it still answers the same question. Here the ranking had changed underneath it — Go was the fallback under the earlier weighting that valued native distribution, and it survived a rewrite whose whole point was that distribution no longer decided the outcome.

**Consequence.** [ADR-0004](../../platform-specs/04-architecture/adrs/0004-talaria-is-a-python-client.md) settles Python and retires the Bubble Tea fallback. This exposed something the analysis had hidden: **every candidate in the chain except Textual was in another language, so settling on Python leaves the fallback set entirely unevaluated.** Identifying at least one alternative Python presentation layer is now queued as a prerequisite of the gate rather than a contingency after it.

**Generalizable rule.** When a recommendation names a fallback, check that the fallback preserves the reasons for the primary. If choosing the fallback would discard the argument that selected the primary, one of the two is wrong, and the disagreement is load-bearing — it usually means a criterion is doing work in one paragraph that it is not doing in the other. The check costs one question and it is the cheapest audit available on any comparative analysis: _what would we lose by taking the fallback, and is that a thing we just said we needed?_

### A line count is not a reuse estimate — reading the file cut the estimate rather than confirming it

**Author.** Reading Hermes's gateway event handler to settle a scoring criterion, at the operator's request

**Context.** The language and TUI framework analysis scored "protocol-knowledge reuse from the existing Hermes terminal tree" at 5 points out of a possible 15, and attached a falsifiable condition: the score rises only if someone reads `ui-tui/src/app/createGatewayEventHandler.ts` and finds that reconciliation and error-recovery logic exceed 40% of it. The whole basis for the criterion existing at all was a line count — the file is 1,419 lines at Hermes `7f4d15515`, and nobody had opened it.

**Evidence.** The read was done in full. Reconciliation, ordering, and race logic is roughly 200 lines — about 22% of the file's 894 code lines and 14% of its total. The bar was not cleared. Three structural findings explain why:

1. **Only about a third of the file is protocol at all.** Roughly 310 lines are terminal theme and background detection with zero protocol content — and four of the file's five exports come from that block, imported by slash commands and config sync rather than by anything event-shaped. Another ~300 lines are Hermes product features (voice, wake word, billing, mixture-of-agents) that Talaria may never want.
2. **The reconciliation engine is in a different file.** `turnController.ts` is a separate 1,092 lines holding the streaming buffer, segment flushing, and interrupt handling. The handler delegates to it at more than twenty call sites; most of its 45 event cases are four to twenty lines of null-check-and-forward.
3. **The densest engineering in the file is a cost of the framework, not an asset.** A forced full redraw after every theme swap because the renderer's diff cache tears; a config fetch deferred out of handler construction to avoid React's "too many re-renders" guard in embedded PTYs; a submit deferred by a tick because React strict mode double-invokes updaters. A non-React stack inherits none of those problems and needs none of those lines.

**Mechanism.** The hard-won logic that _is_ protocol-shaped consists of **rules, not machinery** — "a late live event must never overwrite a terminal sub-agent status" is a two-line predicate; "never resurrect a sub-agent whose start was missed" is one boolean flag. Each cost a bug to discover and costs about a line to re-encode. The file's 22% comment density is the reason the transfer works: the lessons are written down, and a written-down lesson is portable in a way that a codebase is not. Reading the file **is** the reuse.

**Consequence for the analysis.** The read lowers the criterion rather than raising it, because the same knowledge is now available to every candidate stack equally. This weakens the strongest remaining argument for staying on TypeScript.

**Side finding.** Hermes's own client states the profile constraint in a comment at `createGatewayEventHandler.ts:965-967` — "the TUI is a single-profile process" — and acts on it by refusing to route a wake-word event belonging to another profile. That is a second, client-side witness for [ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md)'s narrowed conclusion that Talaria should expect one gateway connection per profile.

**Limit.** `turnController.ts` was not read, only its surface. If its reconciliation engine is less annotated than the handler, part of the reuse argument recovers there.

**Generalizable rule.** A line count measures a file's size, never its value to you — the two diverge most when the file is large, because large files accumulate concerns you did not come for. Before pricing a codebase as a reuse asset, read it and classify it by concern; the estimate that survives contact is usually a fraction of the estimate that motivated the reading. And when a piece of analysis states in advance what evidence would change its score, run that test before re-litigating anything else.

### The stack was never decided, and six weeks of Hermes drift is now a measured number rather than a worry

**Author.** Persisting the language and TUI framework analysis, at the operator's request

**Context.** The operator asked whether TypeScript/Ink had actually been decided, having noticed the project was building in it. It had not. A four-frame comparative analysis of Rust/ratatui, Python/Textual, TypeScript/Ink, and Go/Bubble Tea existed only in a gitignored working directory, and no ADR had ever been written.

**Evidence.** Two findings, both structural rather than incidental.

1. **The stack was inherited from a bootstrap commit under a framing the project has since abandoned.** Compatibility mode — being loadable by an unmodified `hermes --tui` — is the **only** row that changes between the two scoring framings in either scorecard. Every other row is identical. TypeScript/Ink's decisive advantage was that it alone could inhabit `HERMES_TUI_DIR`. [ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md) removed that mode from the product, and the analysis's own swing rule says that scores the criterion at zero. Nobody re-opened the stack question afterwards.
2. **Re-verifying the analysis against the running Hermes turned a hypothesis into a measurement.** The analysis was written against the six-week-stale fork. Re-read at `7f4d15515`: `createGatewayEventHandler.ts` is 1,419 lines, not 945 (+50%); its test suite 1,984, not 1,601 (+24%); the gateway defines 130 methods, not ~90 (+44%). The analysis had hypothesized "an ongoing drift-tracking tax" on protocol reuse without a rate. The rate is now roughly +80 lines per week on the handler alone.

**Mechanism.** A stale reference does not merely make numbers wrong — it can make an argument _look_ weaker than it is while hiding the reason to distrust it. Here the correction made the reuse asset 34% larger, which strengthens the case for staying on TypeScript, **and** revealed that the asset is a fast-moving target, which weakens the case for treating reuse as a reason. Both effects come from the same correction, and only one of them was visible from the line count.

**What did not change.** Roughly four-fifths of the scoring never touched Hermes at all — every claim about ratatui, Bubble Tea, Textual, upstream Ink, and the JSON-RPC library ecosystems was read from those projects' own repositories. Of the fifth that did touch Hermes, every substantive finding held; only magnitudes moved. So a full re-run was not warranted, and saying so is a decision that had to be defended rather than assumed.

**One claim got stronger on re-verification.** The analysis argued Ink cannot carry Talaria's rendering because two better-resourced teams both replaced it. The hardest evidence for that was not what it cited: `ui-tui/package.json:33` aliases the dependency as `"ink": "npm:@hermes/ink@0.0.1"`. Hermes does not use Ink. It uses its own fork wearing Ink's name.

**Fix.** The analysis is persisted at [docs/analysis/2026-08-02-language-and-tui-framework-analysis.md](../analysis/2026-08-02-language-and-tui-framework-analysis.md), scrubbed, with every Hermes-derived number and citation re-verified and an errata table at the top. **No ADR was written** — the operator asked to review and deliberate first.

**Generalizable rule.** An analysis that never becomes a decision record decays into a de-facto decision anyway, made by whatever the code already is. Persist the reasoning even when the call is not ready, or the next reader infers the call from `package.json`.

### Hermes's own TUI answers protocol questions the recorder structurally cannot, and reading it caught an over-redaction bug

**Author.** First live recording run, at the operator's prompting

**Context.** The plan was to settle protocol questions by recording traffic. Recording answers "what arrives"; it cannot answer "what does a client send", because a listener never sends. Hermes ships a working client — `ui-tui/`, 58,581 lines across 277 TypeScript files — which sends all of it.

**Evidence.** Two things fell out of reading it that recording could not have produced.

1. **The send-side vocabulary.** `ui-tui/src/gatewayClient.ts` exposes a generic `request(method, params)`, and its call sites name 32 distinct methods — `session.create`, `prompt.submit`, `model.save_key`, `shell.exec`, and so on. The gateway defines 130 methods in total, so the shipping client exercises roughly a quarter of the surface. That ratio is itself the finding: the protocol is much larger than its only real client uses.
2. **`model.save_key` sends an API key**, as `params.api_key`. It is not one of the four blocking bridges and is not in the deny-set. It was caught only by the key-name net — which is the net working as designed, but it proves credentials on this protocol are not confined to the methods named for them.

**Mechanism.** A protocol has two directions and a recorder attached to one end sees one of them. Prior art is the other end. The asymmetry is structural, not a gap in the tooling.

**What it cost to not have read it sooner.** Checking the net against real key names rather than invented ones exposed the reverse failure. The net matched `token` as a substring. Hermes has **seventeen** distinct key names containing "token" and exactly one — the bare `token` — is a credential. The other sixteen are usage counts: `input_tokens`, `output_tokens`, `max_tokens`, `tokens_per_delta`, `session_total_tokens`. The recorder was withholding all seventeen, destroying the token-accounting fields a fleet console exists to display, and marking every affected frame as redacted so a reader would distrust it.

**Fix.** The single substring regex became a set of anchored patterns, with keys normalized to snake_case first so `authToken` and `access_token` are one case. Tests now pin both directions using key names harvested from the gateway, not invented ones: five that must be withheld, ten that must survive.

**Generalizable rule.** Over-redaction is a failure, not the safe direction — it silently corrupts the data the corpus was built to study. And when integrating against a protocol, read its existing client before instrumenting it: the client is documentation of the half you cannot observe.

### Every source claim in this repository was read against a Hermes checkout six weeks behind the one actually running

**Author.** First live recording run

**Context.** The recorder was pointed at a real Hermes gateway for the first time. It attached and recorded correctly, but the run began by trying to start a gateway from the reference checkout, and that command did not exist there.

**Evidence.** Two different trees, both named `hermes-agent`:

| tree                                        | revision    | dated      |
| ------------------------------------------- | ----------- | ---------- |
| the reference checkout read for every claim | `f5382752f` | 2026-06-21 |
| the Hermes actually installed and running   | `7f4d15515` | 2026-08-01 |

Between them, `tui_gateway/server.py` was split into `methods_*.py` modules. Every `file:line` citation in this repository — ADR-0001's launcher evidence, the three-processes entry, the credential deny-set — pointed into a file layout that the running Hermes no longer has.

**Mechanism.** A stale checkout does not fail loudly. It answers every question fluently and plausibly, because it _is_ the same project — just an older one. Nothing about reading it feels like reading the wrong thing. The error surfaced only because a subcommand that exists in the running Hermes (`serve`) was absent from the old tree, and it surfaced by accident: the interpreter resolved `hermes_cli` from the working directory rather than from the installed package.

**What it cost.** The credential deny-set was derived from the old tree and had three of the four methods. Hermes calls them its four "blocking bridges" and groups all four as sensitive prompts in its own protocol test; `clarify.respond`, carrying the operator's free-text `answer`, was missing. Its key name defeats the key-name net, so it would have been written in full.

**Fix.** The deny-set now covers all four and is pinned to a named Hermes revision in a comment, with a matching test that enumerates the bridges as data. Same pin added to [the frame-log format](../formats/frame-log.md).

**Generalizable rule.** When citing another project's source as evidence, cite the revision, and confirm that revision is the one running. A checkout is a claim about a moment in time, and it goes stale silently. Where both a checkout and an installed copy exist, read the one that is executing.

### The gateway carries plaintext credentials on ordinary frames, so the recorder needed its deny-set on day one

**Author.** ADR-backlog ideation run

**Context.** Three separate analyses independently recommended building a raw protocol-frame recorder first, because a recorded corpus is language-neutral and settles the renderer question by measurement instead of argument. That recommendation was right, and following it naively would have written credentials to disk.

**Evidence.** Read directly from the gateway's `_respond` dispatch. The citation below is preserved as written because the entry above it is about how it was wrong: this was read from `tui_gateway/server.py` in a six-week-stale checkout, it lists two of the four blocking bridges, and in the running Hermes these live in `tui_gateway/methods_prompt.py`.

```
@method("sudo.respond")   -> _respond(rid, params, "password")
@method("secret.respond") -> _respond(rid, params, "value")
```

The plaintext sudo password arrives as `params["password"]` on an ordinary client-to-server JSON-RPC frame — the same connection a recorder captures. The request side is safe: `sudo.request` is emitted with an empty payload, so the exposure is entirely in the direction Talaria _writes_, which is exactly the direction "fire and observe" (survivor 7) requires Talaria to record. A grep of `tui_gateway/*.py` found no existing frame logging anywhere: Talaria's recorder would be the first thing that writes these frames to disk.

**Mechanism.** The danger compounds with the planned append-only hash-chained ledger (survivor 4). **A hash chain cannot be redacted — that is the entire point of a hash chain.** Removing a record afterwards either breaks verification for every record after it or requires re-chaining the tail, destroying the tamper-evidence that was the reason to chain. So the decision about what may be written is made once, at the first write, permanently. The window closes at the first session in which the operator answers a sudo prompt.

**Fix.** The redaction boundary was written before the recorder and before the socket client — `src/record/redact.ts`, with an explicit deny-set keyed by method plus a key-name net (`password|secret|token|credential|api_key|...`) that catches credentials on methods the deny-set has never heard of. Withheld values are recorded as first-class `redactions` entries with a path and a reason, so a reader sees a marked hole rather than clean-looking data. The connection URL's query token is stripped too. Format contract in [docs/formats/frame-log.md](../formats/frame-log.md).

**Validation.** Unit tests in `src/record/redact.test.ts`, plus an end-to-end run against a synthetic gateway emitting four distinct credential canaries — a `sudo.respond` password, a `secret.respond` value, an `api_key` on an unknown method, and a token in the connection URL. All four were absent from the resulting corpus; 7 frames recorded, 3 values withheld, 1 unparseable frame recorded as a hole.

**Limit of that validation, recorded later the same day.** The synthetic gateway emitted those canaries _inbound_, because the recorder only listens. Every one of the four blocking bridges travels _outbound_. The redaction logic is direction-agnostic and the tests are real, but no credential-bearing frame has ever been observed in the direction credentials actually travel. That gap closes when Talaria gains a send path, not before.

**Generalizable rule.** Before recording a protocol, read what the protocol carries in the direction you will be writing. Build the redaction boundary before the thing that writes, not after — for append-only or tamper-evident storage there is no "clean it up later," only "delete it all."

### "The gateway" is three processes, and the one you want depends on what you are asking for

**Author.** First full-product ideation run; corrected the same day by the ADR-backlog run.

**Correction.** This entry was first published saying **two** processes. That was wrong, and the error mattered: it under-applied its own generalizable rule by stopping at the first two it found. There are three, and the third is the one that carries the standalone integration path.

**Context.** The project direction assumed a phase that adds a typed Kanban adapter behind the TUI gateway. That phase would not have worked, and the failure would only have surfaced at implementation time.

**Evidence.** Re-verified 2026-08-02 against the Hermes that is installed and running, `7f4d15515`. The line numbers first published here came from a checkout six weeks older and were all wrong; see the correction below, which also overturns one of the claims outright.

1. `tui_gateway/server.py` — the terminal gateway. It registers **zero** Kanban RPC methods: no `@method("kanban.*")` anywhere in `tui_gateway/`. It speaks the session and conversation protocol, and there is no board surface to query through it.
2. `gateway/run.py:5432` — `class GatewayRunner(GatewayAuthorizationMixin, GatewayKanbanWatchersMixin, GatewaySlashCommandsMixin)`, which starts the Kanban dispatcher watcher (gated by `kanban.dispatch_in_gateway`) and hosts the HTTP API platform adapter.
3. `hermes_cli/web_server.py` — the dashboard, started by `hermes dashboard`. It mounts `@app.websocket("/api/ws")` at line 15609 and hands the socket to `handle_ws` from `tui_gateway.ws` **inside its own process**, gated by `_ws_auth_ok` (defined at line 14527, enforced at 15615) and `_ws_request_is_allowed`.

**Mechanism.** These are separate programs that share a name, not layers of one server. Connecting to the terminal gateway yields exactly zero board visibility no matter what the client asks for. And the "attach to a gateway you did not launch" path that [ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md) depends on lives in the **third** process — so it carries a dependency the first two do not: the dashboard must be running, and the connection is authenticated.

**And there is no capability endpoint to ask.** The product-shape ideation's survivor 3 cites `/v1/capabilities` as evidence that per-feature booleans are already published. That endpoint is real but it is on the **API server** — `gateway/platforms/api_server.py:1815` registers `GET /v1/capabilities` and `:2836` handles it, inside `GatewayRunner`. A grep of `tui_gateway/*.py` for `capabilit` returns **zero** hits in the running Hermes. **The terminal gateway publishes no capability surface at all**, so its capabilities cannot be asked for and must be inferred by probing. This is the same error one process further along: evidence gathered from one program, attributed to another.

**Fix.** Talaria names the seams separately in its own code and probes each independently at launch, rather than modelling "gateway connected" as one boolean. Sized against the original two-process version, survivor 3's probe set was one seam short; the real set is at least four, counting the pane manager's Unix socket. And the probe cannot rely on a capability endpoint for the terminal gateway, because there isn't one.

**Validation.** Every claim above re-verified by direct read on 2026-08-02.

**Correction, same day.** This entry originally validated itself with a live process probe that "found the terminal gateway and `GatewayRunner` both absent while four dashboards were running." That probe was wrong. A later probe found four `hermes -p <profile> gateway run --replace` processes — which _are_ `GatewayRunner` — running continuously since six days before the first probe. The first probe misread them as dashboards and then concluded the thing it was looking at was absent. The distinction the entry draws still holds; the evidence offered for it did not, and it was evidence pointing the opposite way.

**Correction, on re-reading the running Hermes rather than the stale checkout.** This entry claimed the terminal gateway "`grep -c -i kanban` returns `0`, and it has zero `from gateway` or `import gateway` statements." **Both halves are false in the Hermes that is running.** `tui_gateway/server.py` has 43 case-insensitive `kanban` matches, `project_tree.py` has 26, `methods_session.py` has 4; and `server.py` imports the `gateway` package four times (`gateway.config` at `:644`, `gateway.run` at `:1671`, `gateway.session_context` at `:2900` and `:2941`), with a fifth in `methods_session.py:1064`.

What is actually there is a **notification poller, not a board API**: `_KANBAN_POLL_SECONDS = 5.0` (`server.py:8677`) drives a poll of `kanban_notify_subs` (`:8834-8845`), and `_format_kanban_event_text` (`:8680`) renders one-line task events — completed, blocked, gave_up, crashed, timed_out — deliberately mirroring `gateway/kanban_watchers.py` so the wording matches. So the entry's conclusion, "connecting to the terminal gateway yields exactly zero board visibility," is **overstated**. The accurate statement is narrower and sharper: the terminal gateway pushes board _notifications_ and exposes no board _query surface_. A typed Kanban adapter still cannot be built behind it, which is what the decision rested on — but not for the reason originally given, and a reader who wanted only "tell me when a task finishes" would have been wrongly told to look elsewhere.

**Generalizable rule.** Before building an adapter behind a service, confirm the service exposes the _surface_ you intend to reach through it — not merely whether it imports, or fails to import, the subsystem behind that surface. An import proves coupling exists; only a registered method proves you can call it, and the two answers differ in both directions. A shared name is not a shared process — and having found two, keep counting.

## 2026-08-01

### Hermes already exposes two useful client boundaries

**Author.** Project bootstrap

**Context.** The project needed to decide whether a new TUI should call only an OpenAI-compatible endpoint, import Hermes internals, or use the existing Hermes TUI architecture.

**Evidence.** Public Hermes source areas include `ui-tui/`, `tui_gateway/`, and `gateway/platforms/api_server.py`.

**Mechanism.** The API server is the better portable run/session boundary, while the TUI gateway carries richer Hermes-native control-plane behavior. They solve different integration problems.

**Fix.** Talaria uses an API-first direction with an optional gateway adapter and typed adapters for non-chat surfaces.

**Validation.** The direction is documented in [the project analysis](../analysis/2026-08-01-hermes-tui-project-direction.md), with the initial prototype checks passing once dependencies are installed.

**Generalizable rule.** Do not mistake a generic chat protocol for a complete agent control-plane contract.
