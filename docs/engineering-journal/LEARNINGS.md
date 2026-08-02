# Learnings - talaria

> Empirical findings, mechanisms, fixes, validations, and generalizable rules. Keep newest entries first.

## 2026-08-02

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
