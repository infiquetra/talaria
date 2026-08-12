# Learnings - talaria

> Empirical findings, mechanisms, fixes, validations, and generalizable rules. Keep newest entries first.

## 2026-08-11

### Two tests in this suite decide correctness by elapsed time, and both went red on documentation

**Evidence.** Two Linux continuous-integration failures on 2026-08-11, on two different pull requests, neither of which could have caused them. `test_the_projection_and_the_domain_transcript_agree_at_every_pause_point` (`tests/ui/test_transcript_bounds.py:313`) failed `assert 2 >= 3` on pull request 61, whose entire diff is three files under `docs/`. `test_reachability_coverage_rides_the_poll_not_the_checkpoint_schedule` (`tests/replay/test_gate.py:1381`) failed `assert 1 == 0` on pull request 64, whose change provably never executes under the replay gate — the gate builds `TalariaApp` at `gate.py:1217`, `:1433` and `:1454` with no startup selection, so `begin_live_startup` short-circuits and neither `open_session` nor `_land_session` runs. Both passed the same Python version on a different runner, and both pass locally on the branches that "broke" them. Both were confirmed by a clean re-run of only the failed job.

**Mechanism.** Each test asks a real question and then answers a timing question instead. The first wants "is the invariant true at several points during a replay", and enforces it as "did I get at least three samples before the replay drained" — which measures how the runner scheduled `pilot.pause()` against the replay source. The second wants "does a correct pane record zero content loss", and separates correct from broken by whether the pane caught up inside a 200 ms grace window — which measures runner load. Neither dependence is hidden: the gate test's own comment says the window was chosen "comfortably above the pane's 50 ms coalescing flush so a correct, legitimately-lagging pane cannot fail the first arm (the run-9 lesson)". It already failed this way once and the response was to widen the margin, which postpones the failure rather than removing it. That is why a second instance was predictable and why the third will be too.

**Generalizable rule.** A wall-clock margin is not a test boundary, it is a bet on the slowest machine that will ever run the suite — and widening it after a failure renews the bet at longer odds instead of settling it. Where a test needs time to pass, make the clock an input the test controls: drive the sampler in explicit ticks, or size the workload so the count the test needs is a property of the data rather than of the schedule. Until that is done, treat any red job on a documentation-only diff as a claim to disprove with a re-run before it is treated as a regression — and file the flake, because the real cost is not the wasted re-run, it is that a suite which cries wolf trains everyone to merge past red.

### A wait that returns instantly reads exactly like work that finished instantly

**Evidence.** The v0.3 release runs its units in child sessions hosted by the operator's terminal multiplexer, and the root session waits on them with that tool's own "wait until this agent settles" call. On two child sessions launched on the Claude command-line interface pointed at a third-party endpoint, the wait returned in under a second with a success status and an agent state of `idle` — while both sessions were demonstrably mid-turn, their rendered panes showing a working spinner, their token counts climbing past 38,000, and their internal revision counters at 68 and 41. The multiplexer's own `state_change_seq` for both was frozen at the value it held at launch. The same launch path also returns `agent_prompt_stalled` on a prompt that in fact landed and was being worked on.

**Mechanism.** Lifecycle detection is inference from what a pane renders, and it is calibrated per agent kind. A session started through a route the detector does not recognize never produces an observed transition, so its status stays pinned at whatever it was when the pane was first classified — which is `idle`, because a freshly started agent genuinely is idle. The wait call is then correct on its own terms: it waits for the first settled state and finds one immediately. Both halves fail in the direction that looks like success. A wait that returns instantly is indistinguishable, at the call site, from work that completed instantly; and because the *launch* succeeded and the *prompt* succeeded, nothing upstream suggests the detector is blind. The compounding cost is that the orchestrator's entire unattended loop hangs off that wait, so a silently no-op wake-up mechanism does not present as a broken tool — it presents as an orchestrator that keeps stopping and handing control back for no stated reason.

**Generalizable rule.** Never let a completion signal be the same shape as its own failure. Where a wait can return without having waited, prove liveness independently before trusting it: check that a monotonic counter actually moved between two reads, or poll the rendered surface for stillness, rather than reading a status field the detector may never update. The polling detector is engine-agnostic and worth preferring outright — it works for agent kinds the multiplexer does not recognize at all, which the status-based one cannot. Correspondingly, treat a stall or timeout error as a claim to verify rather than a fact: read the surface before resending, because a resend on a false stall double-prompts a session that is already working.

### A local branch can be dozens of commits ahead of its remote and hold the only copy of published evidence

**Evidence.** `feat/v0-2-block-markdown-build`, deleted 2026-08-11 during branch cleanup. Its remote tip was `5fd0dea`; its **local** tip was `179a16a`, **39 commits ahead and never pushed**. Those 39 commits included `67589a9`, `4498bec` and `2e96324` — the three confirming runs that [the block-markdown gate results](../analysis/2026-08-09-block-markdown-gate-results.md) cite by hash, in a document the v0.2.0 release notes link to as the record behind the 24-of-24 verdict. None of the three is reachable from `main`, because that work reached `main` through pull request 49 as a different set of commits. A `git branch -D` would have destroyed all three with no remote copy anywhere. Preserved instead as the annotated tag `evidence/block-markdown-gate`, pushed and verified to resolve on GitHub before the branch was deleted.

**Mechanism.** Two ordinary facts compose into a trap. First, a branch's local and remote refs are independent, and nothing in `git branch -a` output distinguishes "this local branch is pushed" from "this local branch is 39 commits ahead" — both print as one line. Second, the standard safety check for deletion asks the wrong question: `git merge-base --is-ancestor <branch> main` answers "has this content landed", and the answer was correctly "no", but the natural reading of a *stale-looking* branch is that the work landed by another route and the branch is residue. That reading was right about the code — `main` holds roughly 9,500 lines the branch never got — and wrong about the history, because a gate document cites *commits*, not content. Content can be superseded while its provenance stays load-bearing.

**Generalizable rule.** Before deleting any branch, check two things the merge-status question does not cover: whether the local ref is ahead of its remote (`git rev-list --count origin/<b>..<b>`), and whether any committed document cites a commit only that branch can reach (`git log --oneline main..<b>` against a grep of the docs tree). Where published evidence names a commit by hash, that hash is part of the deliverable — preserve it with a tag before the branch goes, and verify the tag resolves on the remote rather than assuming the push carried it.

### A release published by hand and a release published by the tag are the same release, and the hand-made one wins by losing

**Evidence.** The v0.2.0 release, cut 2026-08-11. The tag was pushed and the release was then created by hand with `gh release create` roughly a minute later. `.github/workflows/release.yml`, triggered by the same tag push, ran steps 1 through 15 green — tag-versus-package-version agreement, ruff, mypy, pytest, bandit, the gate block reading READY, the distributions built, and the built artifact installed clean into a fresh environment and reporting its version — then failed at step 16 with `a release with the same tag name already exists: v0.2.0`. Net effect: `v0.1.0` carries `talaria-0.1.0-py3-none-any.whl` and `talaria-0.1.0.tar.gz`; **`v0.2.0` carries no assets at all.**

**Mechanism.** The workflow's create step is not merely the last step, it is the *delivery* step — the only place the artifacts built and verified in steps 14 and 15 become reachable by anyone. Doing it by hand first does not duplicate that work, it discards it, and it discards it in the one failure shape that looks harmless: a red run whose every substantive check is green, failing on a name collision. Reading the run's conclusion alone says "the release failed"; reading its steps says "the release was fully validated and then not delivered". Neither reading is available from the release page, which shows a normal-looking release with the correct notes.

**Generalizable rule.** When a pipeline both validates and publishes, the publish step is load-bearing evidence, not a formality — never race it by hand, and after any release check the *assets*, not just the release. In this repository: push the tag and let `release.yml` do the rest. Where a hand-published release already exists, the provenance-preserving repair is to delete the release (never the tag) and re-run the failed workflow run, so the attached artifacts are the ones continuous integration built from the tagged tree.

### A check that runs but is not required is a check that has already stopped checking

**Evidence.** Branch protection on `main` requires exactly `python-check (3.12)` and `python-check (3.13)`. The Validate workflow's Node job, `check` (`npm run typecheck && npm test && npm run format:check`), is not in that set. It began failing on a Prettier violation in ADR-0006 introduced during the block-markdown build, and merged red twice — with the block-markdown work (`05ecaa6`) and again with the v0.2.0 release merge (`d925891`) — before anyone noticed. Fixed in `5211a8c`; `main` green again at `06dc858`.

**Mechanism.** A non-required job still runs, still reports, and still shows a red mark next to a pull request — so it produces exactly the signal a person would notice *if they were looking at the run rather than at the merge button*. The merge button stays green, and the merge button is what gets looked at. The gap is silent by construction: nothing distinguishes "this job passes" from "this job cannot stop anything", and the difference only surfaces as a commit history where the failure is already two merges old.

**Generalizable rule.** For every job in a validation workflow, decide explicitly whether it is required, and treat "runs but is not required" as a state that needs a written reason rather than a default. The audit is one call — `gh api repos/<owner>/<repo>/branches/main/protection` — and it belongs in the release checklist, because a release is exactly when the discrepancy costs the most.

### Notes written for fidelity carry the operational detail a public repository forbids

**Evidence.** `docs/analysis/2026-08-10-v0-2-hands-on-notes.md`, written to capture a live hands-on session in the operator's own words and merged on 2026-08-10, named the operator's Hermes profile in five places — including the full command line the gateway runs under. Requirement R12 is explicit: no profile name, profile path, or other operator-specific inventory from `GET /api/profiles` in a committed fixture, document, or commit message, because this repository is public. Redacted 2026-08-11; `git grep` for the name now returns nothing across the tracked tree.

**Mechanism.** The two goals pull in opposite directions and only one of them announces itself. Fidelity is the stated purpose of a notes document — quote verbatim, name the exact command, record what was actually configured — and every one of those instincts produces operator-specific inventory. The redaction rule, meanwhile, is written for *fixtures and inventory dumps*, where the leak is obvious; it reads as inapplicable to prose. Root-causing a live configuration problem in writing is precisely where the two collide, because the root cause **is** the operator's configuration.

**Generalizable rule.** Any document that narrates a live session against real infrastructure gets a redaction pass before it is committed, separate from the review of whether it is correct — correctness review does not catch this, because the leaked detail is exactly the detail that makes the account true. The mechanical check is one command per known-sensitive term (`git grep -n <term>`), run against the tracked tree rather than the file being edited.

## 2026-08-10

### Two hours in a real terminal found seven defects that 24-of-24, six review rounds and 1,700 tests could not

**Evidence.** The first hands-on drive of v0.2 on a real desktop, against a live gateway, immediately after the release merged: [docs/analysis/2026-08-10-v0-2-hands-on-notes.md](../analysis/2026-08-10-v0-2-hands-on-notes.md). Nineteen operator notes produced seven defects and four design questions. The two worst were invisible to every existing check. The approval card has no keyboard path on macOS at all — cards never auto-focus unless input-backed (`talaria/ui/prompts.py:1171`) and are reachable only through the `F1` jump, and `F1` never arrives because the desktop claims it; both keys the card advertises (`enter select · esc decline`) were tested and neither works. Mouse selection lands several rows above the click. Neither involves a wrong value, a dropped row or a missed deadline, so neither is expressible as a gate check.

**Mechanism.** The gate's checks are all statements about the process's own state — rendered rows against projected lines, mounted widget counts, apply latency, RSS. Every defect found today is a statement about the *boundary* between that process and a human at a desk: which keystrokes the window manager delivers, where a click lands, whether a printed sentence means to a reader what it means to its author. Both v0.1 and v0.2 record "no run on either platform has used a real terminal emulator" as a known limitation, and the release was shipped with that limitation accepted — correctly, since it was named. What was not appreciated is that the limitation is not a *coverage gap of the same kind* as the others, to be closed later by more of the same instrument. It is a different class of claim: no pseudo-terminal has a window manager to intercept a key, and no assertion can tell you a status row reads as gibberish to the person it was written for. Confirmation came from within the run too — the same session falsified two of its own conclusions within minutes (a "macOS eats function keys" generalisation killed by `F8` and `F9` working, and a `platforms.changed` flood confirmed as faithful rendering only after the frame log showed 26 events on the wire).

**Generalizable rule.** A verification apparatus certifies the seam it is defined over and says nothing about the seam outside it — and the more thorough it is, the more it reads as a statement about the whole product. When a known limitation names a whole class of claim the instrument cannot make (real input devices, real displays, real readers), treat it as a required second method with its own schedule, not as coverage to be extended later. Ship a release only after someone has driven it; budget the hours before the tag, not after.

**Corollary, from the same run.** A design that surfaces unknown inputs rather than dropping them (`talaria/domain/decode.py:114`, unknown gateway event types drawn as named rows) is right, and its failure mode is only visible against a live peer: one ordinary turn carried 26 `platforms.changed` events, each drawn as a red row, while 204 `sessions.changed` events passed in silence purely because that type happened to be in the known set. Every gap in such a set is a flood waiting for a real peer to find it, so the set needs a live capture on a schedule — which is exactly what `_OBSERVED_ON_A_LIVE_GATEWAY` (`decode.py:110`) says in its own comment.

## 2026-08-09

### The same text, two row conventions: a seam between them needs a count check, not a text check

**Evidence.** The CR2 re-review fix commit on `feat/v0-2-block-markdown-build`: the commit handoff adopted a line-kind tail widget-for-widget on `record.raw_body == tail.applied_text` alone. A body ending in a newline has one more committed row (`split("\n")` — the span convention) than tail rows (`splitlines()` — the projection's tail convention), so adoption mounted a one-row-short unit under the entry's two-row span and `rendered_lines == view.lines` broke — probed live (`('',)` against `('', '')`) while every existing test passed. Pinned by `tests/ui/test_transcript_blocks.py::test_a_committed_trailing_newline_body_never_adopts_a_short_line_tail`. CR1 finding 4 was the same convention split at the weld/fold seam.

**Mechanism.** Text equality across a seam between two splitting conventions is not row equality. The conventions diverge twice: a trailing newline is one extra `split("\n")` row (count differs), and a `\r\n` or bare `\r` is one `splitlines()` boundary but stays *inside* a `split("\n")` row (counts equal, row content differs — Codex's `" \r\n "` probe adopted `(" ", " ")` under a view of `(" \r", " ")`). The guard therefore compares the **complete projected row sequences** — `_welded_tail_lines` (splitlines) against `_welded_entry_lines` (split, the convention the span and the rendered identity are stated over) — never just counts. Two wrong cuts preceded it: `len(tail.lines)` (the *retained* widget count, which a capped monster tail can never satisfy, so every capped adoption was rejected and the full body remounted fresh at commit — the exact transient the cap exists to prevent), then a complete-count comparison (which the `\r` shape slips through). Pinned by `test_a_capped_monster_tail_is_still_adopted_at_commit` and `test_a_carriage_return_body_never_adopts_the_tails_row_content`.

**Generalizable rule.** When an artifact is handed across a seam between two conventions, guard the handoff by comparing the downstream convention's own complete output against what will actually be shown — not a proxy for it. Counts are a proxy; retained windows are a proxy; only the full sequences say the two sides agree.

**The remedy is a rewrite, not a rejection.** The third confirm round showed why "reject and rebuild" is the wrong arm of the guard: a divergent capped monster fell through to an uncapped fresh build while the old tail was still mounted — 1,203 new widgets beside 501 existing, a 1,704-widget transient no settled metric sees (`peak_mounted` samples after condensation). Both sides of the seam hold *the newest rows of the same text*, so the correct move is a positional in-place retarget (`_retarget_line_unit`): reuse every mounted widget, rewrite sources to the committed rows, mount only the rare extra row. Zero fresh mounts on the monster shapes; pinned by `test_a_divergent_capped_monster_commits_without_a_mount_transient` with a mount spy, because only an instrument at the mount seam can see a transient the settled checks cannot.

### A reset check keyed on mounted state has a hole exactly where eviction is most aggressive

**Evidence.** CR1 confirm round (Codex re-review, 2026-08-09, evidence cited by sha256 `3749408165adbe75…` in the git-ignored saga dir): a live probe drove a monster fallback tail that folded every committed entry (`_top=1`, `_tail_top=596`, `_entries` empty), then switched sessions — the new session came up with `condensed_count=1` and `rendered_lines=()`, its first row treated as already folded. Fixed in `talaria/ui/transcript.py` (`_reset_if_history_changed` + the `_last_entry_id` watermark), pinned by `tests/ui/test_transcript_blocks.py::test_a_session_switch_after_a_monster_tail_folded_everything_still_resets`, which fails on the pre-fix code at the mount assertion.

**Mechanism.** The session-switch reset detected a swapped history by looking for a *mounted* entry id absent from the new records — but folding is precisely the process that empties the mounted set, so the check went blind in the one state where the pane carried the most cross-session residue (a stale `_top` describing the outgoing session's line arithmetic, under which the new session's line spans, restarting at zero, all read as folded). The fix keeps a one-integer lineage watermark — the newest entry id of the last accepted records set — which survives folding and is lineage-sound by two invariants `land_session` documents: entries are never deleted within a lineage, and `entry_seq` climbs across a session clear rather than restarting, so a swapped-in history can never contain the outgoing lineage's newest id.

**Generalizable rule.** State that summarizes evicted content (a fold counter, a condensed prefix) must be guarded by an identity that survives the eviction; a guard that reads only what is still present goes blind at maximum eviction, which is exactly when the summarized state is largest and stalest.

### The "escape valve" was the hot path: a fallback branch treated as rare carried quadratic growth and no bound at all

**Evidence.** The first full-scale replay gate run (stress corpus `talaria-stress-v1-50000d-seed20260802`, 2026-08-09) failed `workload_latency_growing-open-fence` at p99 **17,697 ms** against KTD1(d)'s 50 ms ceiling, with `peak_descendants` **10,002** for a single live tail. After the fix pair — incremental append in the fallback growth path plus the tail mount-cap — the same workload's steady-state applies measure ~35 ms with mounted tail widgets bounded by `mount_cap`. Commits: the incremental-growth fix, the RA4 measurement amendment, and the tail-cap change on `feat/v0-2-block-markdown-build`.

**Mechanism.** Three findings stacked. First, `_reconcile_tail`'s fallback growth branch dropped and rebuilt every line widget on every delta, with a comment declaring the path "the degenerate-content escape valve, not the common case" — but a model streaming a long fenced code block is the single most common heavy event this product renders, and it lives exactly on that path, so the escape valve *was* the hot path: O(total lines) per delta, quadratic over a stream. Second, the plan's ceiling sentence — "the tails, each bounded by the two-condition trigger" — read as though demotion capped the tail; demotion only *switches the rendering*, after which nothing bounded the widget count at all. Third, the quantile's tolerance was an illusion: nearest-rank p99 over ~90 post-warmup samples is arithmetically the **maximum**, so the ceiling as stated demanded the one-time block-to-lines demotion apply finish under 50 ms, which mounting a capped widget run cannot do (RA4 now excludes that one flagged boundary and reports it verbatim).

**Generalizable rule.** When a "rare-case" branch is reachable from streaming input, benchmark it as the hot path — the comment saying it is rare is a hypothesis, not a measurement. And before making a quantile a ceiling, compute what it actually selects at your sample size: p99 of 90 samples is the max, and a ceiling that is secretly a max forbids every one-time cost you meant to tolerate.

### A mid-stream invariant checker must know when the mutator is mid-flight

**Evidence.** The same gate run failed `content_loss` with 2 of 11 stress checkpoints reporting "projected line 0 … owned by no block" — on a plain paragraph line that is always owned. The count fluctuated across identical replays (1–3), and the settled checkpoint always passed. Textual's `Markdown.update` sets a document's `source` synchronously and then *awaits* the child remount; `document_ownership` checks `source == applied_text` first, and that passed at the failure instant — the widget's text was current while its children were stale.

**Mechanism.** The ownership proof was documented as "a true invariant at every instant", but the invariant spans an await boundary inside the mutator: between `source` assignment and child remount, "blocks cover the text" is legitimately, transiently false, and a concurrent sampler task landing in that window reports scheduling as corruption. The fix publishes the mutator's own state — `TranscriptPane.apply` counts itself in flight — and the sampler yields until quiescence (bounded) before proving ownership, while the settled checkpoint independently asserts the marker cleared, so a stuck marker fails loudly instead of silently excusing every mid-stream sample.

**Generalizable rule.** "Invariant at every instant" is only true of code with no awaits inside the mutation; if the mutator suspends, the checker needs an in-flight signal from the mutator itself, plus a settled assertion that the signal cleared — the second half is what keeps the signal from becoming a hole.

### A session has two ids and only the durable one survives the process — and "most recent" belongs to whoever spawned last

**Evidence.** The U8 live acceptance run,
[docs/plans/2026-08-09-u8-live-acceptance-results.md](../plans/2026-08-09-u8-live-acceptance-results.md)
observations 1 and 2, recordings `a1583ff3…` and `3934ee30…` (sha256-cited there). Resuming by the
runtime id the `session.create` reply returned (`07e299c5`) was refused with gateway code 4007;
the durable `stored_session_id` (`20260809_084412_b08629`) resumed correctly. Separately,
`--resume` attached to a session a background webhook automation had spawned seconds earlier,
because `session.most_recent` answers for the whole gateway, not for the operator's own activity.

**Mechanism.** The gateway issues a fresh runtime id per attachment and keys durable storage by
`stored_session_id`; the runtime id dies with the process that held it. `session.most_recent` is
global: any spawner — webhook, cron, another client — moves it. The v0.2 `/sessions` picker
already respects this split (it lists, highlights, and resumes by durable id — verified in leg 7).

**Generalizable rule.** Address sessions by durable id everywhere a session outlives a process,
and treat "most recent" as nondeterministic on any machine where automations spawn sessions —
scripted drives must capture the durable id at create time and resume by it explicitly.

### A race you can reason about but cannot lose on demand still needs a test that fails — drive the sequence instead of hoping to lose the schedule

**Author.** unit U6 of the v0.2 plan, adding KTD2's landing barrier to
`talaria/ui/app.py:_land_session`

**Evidence.** The window is real and structural: `LiveSource._ingest` resolves the RPC future
(`talaria/transport/source.py:589`) and then enqueues the frame record (`:601`), while the app's
frame pump (`talaria/ui/app.py:_pump`) drains that queue in a separate task. An event the gateway
sent immediately after a `session.resume` reply can therefore reach `apply_frame` before the
coroutine awaiting the reply has seeded the history that event follows. The plan asked for a
"reply-then-event back-to-back transport test" to pin it. That test was written — the stub gateway
gained a `follow_ups` hook that writes the reply and the next event with no await between them
(`tests/transport/conftest.py`) — and **it passes with the barrier removed**, measured across five
runs. Instrumenting `ingest` showed why: with both processes on one event loop, only the JSON-RPC
reply frame arrives inside the landing window; the event is read off the socket a loop turn later,
after `_land_session` has already run.

**Mechanism.** How many await layers sit between the resolved future and the seeding coroutine
decides who wins, and here there are two (`asyncio.wait_for(asyncio.shield(future), timeout)`) —
enough for the awaiting side to be scheduled before the frame that would beat it has even been
read. The schedule is an accident of the harness, not a property of the code, so a test that hopes
to lose the race asserts nothing on every run where it wins. The fix was to keep the end-to-end test
for what it *can* prove (the outcome is right, and the barrier is engaged — it asserts inbound
frames were actually held) and add a second test that holds the landing open with `app._landing()`,
delivers the frame inside it, and seeds — the same sequence with the timing taken out. That one
fails without the barrier, and the failure is the exact defect: the reducer adopts the event's
session id, and the resumed history is appended *below* a line from the turn that came after it.

**Generalizable rule.** Before trusting a concurrency test, delete the thing it tests and watch it
fail. If it still passes, it is pinning an outcome, not a mechanism — keep it, label it as such, and
write a second test that drives the interleaving directly.

### A fixture that cannot exist on the wire hides the feature it was supposed to cover

**Author.** unit U6 of the v0.2 plan, repairing `RESUMED` in
`tests/transport/test_session_startup.py`

**Evidence.** The `session.resume` stub fixture read `"message_count": 3` with `"messages": []`.
No gateway sends that: an empty array is the *omission* shape and it arrives with
`"messages_omitted": true` (`tui_gateway/methods_session.py:494-500`). Because every startup test in
the suite ran against that fixture, no test in the repository had ever seen a resumed message, and
`--resume` shipped rendering an empty transcript with nothing red anywhere.

**Mechanism.** The fixture was internally inconsistent in a direction that made the untested path
*look* covered: the count said there was history, the array said there was nothing to render, and
assertions written against "the session was focused" passed either way. An impossible fixture does
not fail — it silently narrows what the suite can observe.

**Generalizable rule.** Check a fixture against the code that *produces* it, not only against the
code that consumes it. A reply shape no server can emit is a coverage hole wearing a test's clothes.

### mypy narrows a variadic tuple to `tuple[()]` after an emptiness assert, and the next comprehension over it stops type-checking

**Author.** unit U6 of the v0.2 plan

**Evidence.** In one test, `assert app.state.transcript == ()` (and equally `assert
len(...) == 0`) narrowed `tuple[TranscriptEntry, ...]` to `tuple[()]`; a later comprehension over
the same expression then failed with `Need type annotation for "entry" [var-annotated]`, pointing at
a line that had nothing wrong with it. `reveal_type` was what found it.

**Mechanism.** Tuple-length narrowing is sound for the assert and wrong for everything after it,
because the attribute is re-read and mypy keeps the narrowed type for the member expression.

**Generalizable rule.** Assert emptiness through a value you have already extracted
(`rows = [e.text for e in x]; assert rows == []`), not against the tuple attribute itself.


### A per-session counter and a retained per-session tombstone set are safe apart and unsafe together, and the switcher is what puts them together

**Author.** unit U5 of the v0.2 plan, hardening `talaria/domain/state.py:focus_session` before the
`/sessions` switcher makes it reachable

**Evidence.** Approval prompts carry no request id on the wire, so Talaria synthesizes one:
`approval:<session>#<n>`, numbered by `SessionState.approvals_seen` (`state.py`,
`_on_prompt_request`). `focus_session` reset that counter to zero on every switch and cleared
`flushed_prompt_ids`, the set of prompt ids Talaria has already told the operator are gone. U5 had
to keep the tombstone set across a switch — dropping it is what lets a late `restore_prompt` put a
closed control back on screen, and the gateway never emits a second expiry to close it again. With
the set retained and the counter still restarting, switching away from a session and back to it
mints `approval:<that session>#1` a second time, the retained tombstone from the first visit matches
it, and the returning session's first approval is swallowed as an already-closed prompt: no card,
for a command the gateway is still holding.

**Mechanism.** Session-qualifying the key looks like the whole fix and is not. Qualification
separates *concurrent* sessions; it does nothing about the same session being focused twice, which
is exactly what a switcher introduces and what a reconnect-only caller never did. The uniqueness
property the retained set actually needs is uniqueness over the lifetime of the retention, not over
the set of session names. A counter that only ever climbs supplies that; qualification then just
makes the key legible in a log. The fix is one line — stop resetting `approvals_seen` — and it is
invisible without asking what the *retained* set is keyed on.

**Consequence.** `focus_session` now keeps `approvals_seen` and `flushed_prompt_ids`, resets
`withdrawn_approvals` (which describes the session being left, and made the switched-to session's
activity line hedge about a withdrawal that never happened there), and refuses the switch entirely
while an answer is in flight. `prompt_view` (`talaria/domain/projection.py`) gained the focused
session filter as second-line defence: a prompt belonging to a session Talaria no longer shows would
render an answer control that `respond_to_prompt` refuses every keystroke into.

**Generalizable rule.** When a dedupe or tombstone set outlives the scope its keys are minted in,
check what makes the keys unique over the *set's* lifetime rather than over the scope's. A key that
is unique per scope and a set that survives across scopes is a collision waiting for the first
caller that revisits a scope.

### A whole-queue resolution has to latch the ids it swept but did not take, or a failed single answer resurrects a control the gateway already resolved

**Author.** unit U2 of the v0.2 plan, while binding `escape` to decline
(`talaria/ui/app.py:deny_all_approvals_live`, `talaria/domain/state.py:latch_resolved_prompts`)

**Evidence.** `approval.respond {all: true}` resolves every entry in the gateway's queue, and
Talaria's own accounting already knew that: `DenyAllScope` splits what the call *took* out of the
registry from the approvals `already_in_flight`, whose own single `approval.respond` has not come
back yet, precisely so the transcript can name the second group without claiming they were denied.
What no code did was act on it. When one of those in-flight answers returned a definite `not_sent`,
its own owner did the correct thing for a call that reached no socket — `restore_prompt` put the
control back — for an approval the deny-all had already resolved at the gateway. The gateway emits no
second expiry for an approval, so the resurrected card stays on screen, live-looking and unanswerable
in fact, for the rest of the session.

**Mechanism.** Two owners for one prompt, each locally correct. The single answer owns the prompt's
outcome; the deny-all owns the gateway's queue. `not_sent` is a true statement about the *call* — it
reached no socket — and a false statement about the *question*, which a different call resolved. The
existing tombstone set, `flushed_prompt_ids`, is exactly the mechanism for "this may not come back",
and `restore_prompt` already consults it; nothing was writing into it from the resolution path. The
fix is one pure transition (`latch_resolved_prompts`) applied to `taken` **and**
`already_in_flight`, and only when the deny-all was not itself a definite `not_sent` — the one
outcome where the gateway resolved nothing and the cards must go back.

**Consequence.** A resolved deny-all latches every id its `all` flag reached. The same latch covers
F4's interrupt sweep for free, because that sweep resolves approvals through the same deny-all call.
A single decline keeps the unchanged discipline: restore on definite `not_sent`, latch on every other
known outcome.

**Generalizable rule.** When one call resolves work that other in-flight calls also own, the
resolution must tombstone every id it covered — not only the ones it took. A per-call outcome is a
statement about that call, and a rollback driven by it is only safe while no other call can have
settled the same question.

## 2026-08-08

### "The project is abandoned" and "the owner is unreachable" are separate questions, and only the second one decides what to do about it

**Author.** the v0.1.0 release preparation, researching a PEP 541 name claim for `talaria` on the Python Package Index

**Evidence.** The plan filed a step to request the `talaria` name through PEP 541, PyPI's process for claiming names from abandoned projects. Every abandonment signal was present and then some: one release ever, version 0.2.0 uploaded **2010-06-19**, a 12,758-byte tarball, no surviving source repository anywhere public, and — strongest of all — the package's own metadata carrying the classifier `Development Status :: 7 - Inactive`. The author had declared it dead himself.

Then the owner was checked rather than assumed. The homepage recorded in the 2010 package metadata still resolves and redirects to a live personal site. The matching account on GitHub has 128 public repositories and profile activity dated **2026-06-05**, ten weeks before this was written. The project is abandoned; the person is not.

**Mechanism.** The two facts feel like one because abandonment is usually inferred *from* silence, so a dead project and a vanished author normally arrive together. When they come apart, the evidence for the first is exactly as strong as before and the correct action inverts completely. PEP 541 exists for disputes that cannot be settled directly; its issue template makes "Contact and additional research" a required field for that reason. Filing against a reachable, active maintainer skips the obvious step, spends volunteer moderator time on a conversation two people could have had, and asks a stranger's property be reassigned without asking the stranger.

The generalisable failure is doing the research that supports the plan and stopping there. Every check performed — release dates, classifiers, repository searches — was aimed at the question "is this project dead?", which was the question whose expected answer justified the step already written down. Nobody had asked the question that could have changed it.

**Consequence.** Step S5 became "write the letter, and file the PEP 541 request only after a reasonable wait with no reply", with both artifacts drafted in `docs/plans/2026-08-08-pypi-name-request.md`. Nothing about the v0.1.0 release depends on the outcome; it installs from a git tag either way.

**The letter was never sent.** Later the same day the step was deferred entirely — not because the finding was wrong, but because of it: asking an active person for a name is a favour, and Talaria could not yet promise it would keep the name. See [DECISIONS.md](DECISIONS.md#the-talaria-name-on-the-python-package-index-is-not-asked-for-yet-because-the-project-cannot-yet-promise-it-will-keep-it) for the reasoning and the reopen condition.

**Generalizable rule.** Before invoking any process that exists for unreachable parties — name claims, abandoned-account recovery, escalation over a non-responder — spend two minutes checking whether the party is reachable. Evidence that the *artifact* is dead says nothing about whether the *person* is, and only the second fact chooses the procedure.

## 2026-08-07

### A source distribution built from a deny-list ships untracked working-tree files, so the contents of a release artifact depend on whose machine built it

**Author.** the v0.1.0 release preparation, which built a wheel only to confirm the dynamic version resolved and then read the tarball out of habit

**Evidence.** `uv build` on a clean checkout produced `talaria-0.1.0.tar.gz` containing `.claude/settings.local.json`. That file is not committed — `git ls-files .claude` returns nothing — and it is not ignored; `.gitignore` names `.claude/saga/` only. It was 35 bytes holding one output-style preference, so the exposure was nil, but it reached a distributable artifact by a route nobody had chosen. The tarball also carried `platform-specs/`, `docs/`, `.github/` and the whole superseded TypeScript tree, at 1.1 MB against a 300 KB wheel. Fixed in `pyproject.toml` by giving `[tool.hatch.build.targets.sdist]` an explicit `include` allow-list; the rebuilt tarball holds `talaria/`, `README.md`, `LICENSE`, and the two files hatchling generates.

**Mechanism.** Hatchling's default sdist selection is *everything in the project directory that version control does not ignore*. The word doing the damage is "ignore": untracked-but-unignored is a third state, and it is the state every machine-local scratch file passes through. So the default is not "ship what is in the repository" — which is what it looks like, and what it behaves as on a fresh CI checkout — it is "ship the repository plus whatever the builder happened to leave lying around". The failure is invisible on the machine most likely to catch it, because continuous integration always builds from a clean clone.

This is the general shape of a deny-list guarding an open set: it can only exclude what somebody anticipated. An allow-list inverts the default so that a file nobody thought of is excluded by construction rather than by vigilance. For a public repository shipping artifacts, that inversion is worth the small cost of naming three paths.

**What it did not catch, and why the fix is still worth it.** The v0.1.0 release is built in GitHub Actions from a fresh checkout, where no untracked file exists, so this defect would not have shipped anything. The reason to fix it anyway is that "the artifact is clean" was true by accident of build location rather than by any property of the build, and the first local `uv build` before an upload would have ended that.

**Generalizable rule.** Before a project's first release, extract the artifact and read its file list. Packaging defaults are deny-lists over the working tree, and a deny-list cannot exclude what nobody anticipated — so name what ships.

### A gate row that no action by the graded subject can move is mis-scoped, and that is a diagnosable condition rather than a judgement call

**Author.** the row-13 re-scoping pass, after the operator answered the scoping question with "we are grading Talaria"

**Evidence.** Row 13 of the v0.1 daily-driver verdict read "**R1** — the environment carries no credential" and sat at `partially unmet` for five days. Its final residual was an inherited `HERMES_DASHBOARD_SESSION_TOKEN`: exported by the operator's shell before Talaria starts, snapshotted by the kernel at `exec`, served from `/proc/<pid>/environ` for the life of the process. Two separate attempts to close it — deleting the variable from the credential chain on 2026-08-06, removing the endpoint-URL route on 2026-08-07 — each removed a real thing and moved the row not at all. The row cleared only when it was re-titled to "**R1** — Talaria places no credential in its environment" and the inherited value was scoped out on a named condition.

**Mechanism.** The row's subject and its requirement's subject had drifted apart without anyone noticing, because the row was named after the requirement. R1 grades a *machine state* — a running process's environment. The table grades *Talaria*. As long as the row's title asserted the requirement, no amount of work on Talaria could satisfy it, and each removal produced the demoralising shape of a real security improvement that moved no grade. The tell is mechanical and worth naming: **if you can enumerate the actions available to the graded subject and none of them changes the grade, the row is grading something else.** That is checkable without a judgement about whether the requirement matters.

The correction is not to relax the requirement. R1's environment clause is still not met when an operator exports a credential, that sentence is unchanged in the verdict document, and the test asserting the failure is untouched. The correction is to make the row's title say what the row actually grades, so the grade and the prose stop contradicting each other — and to state the exclusion as a falsifiable condition rather than a convenience.

**What made this one safe to do, where the same move would usually be suspicious.** Re-scoping a row is how a gate gets closed dishonestly, and this repository has been burned that way twice. Three things distinguished it. The exclusion's falsifier is four tests that already exist and run every suite — not an experiment somebody has to remember to perform, which is the weaker form the `terminal.read.respond` exclusion had to settle for. The excluded fact stayed measured and named, in the row, in the prose and in a test that asserts the failure. And the consequence — that this was the last blocking row, so the verdict would flip to READY — was worked out and put in front of the decision-maker *before* the decision, not discovered while writing the diff.

**Generalizable rule.** When a gate row will not move under any action available to the thing it claims to grade, stop trying to satisfy it and check whether the row is named after a requirement whose subject is broader than the table's. Fix the title, scope the excess out with a continuously checked falsifier, and leave the requirement's own wording alone.

### A "revisit when" condition had already been satisfied for two days by code in this repository, and nobody checked

**Author.** the row-13 residual pass, which set out to report what removing the endpoint-URL credential route would cost

**Evidence.** `DECISIONS.md`'s 2026-08-06 entry deferred removing route 1 — a `token` on `TALARIA_GATEWAY_URL` — and filed it under **Revisit when.** *"Hermes gains an HTTP or file-based way to hand a client its session token directly."* `talaria/transport/refresh.py` had shipped on 2026-08-04 and does precisely that: `talaria refresh-credential` fetches the dashboard index over plain HTTP, unauthenticated, and reads the injected session token out of the page — "precisely what the dashboard's own web UI does on every load", in the module's own words. The same entry's rejection also asserted that "`talaria record`'s design leans on `TALARIA_GATEWAY_URL` resolving both halves". Measured on 2026-08-07 with `env -i HOME=$HOME PATH=/usr/bin:/bin TERM=dumb .venv/bin/talaria record --out …` against a live Hermes: it authenticated and recorded a `gateway.ready` frame from the `0600` credential file alone, whose `url` key had supplied endpoints since before the entry was written.

**Mechanism.** Both claims were about the state of the world, and both were written from what the author remembered of the world rather than from a check. Neither is a subtle call: one is a two-command experiment, the other is one file read in this repository. What made them survive is that a deferral produces no failing test, no red CI, and no symptom — it produces a document that reads as considered. The rejection was even *correct in form*: it named a condition, it named a cost, it recorded the residual rather than dropping it. Every quality signal was present except the one that mattered, which is whether the premises were true.

**Cost of the delay.** One day, because the residual came up again quickly. That is luck. The entry's own "Revisit when" was the mechanism meant to catch this, and it would not have fired on its own — it names a condition nobody was watching for, about a capability that already existed.

**Generalizable rule.** A deferral rests on claims about the world, so verify them the way you would verify a bug report: before writing "out of scope" or "blocked on X", run the check that would refute it, and before writing a **Revisit when** condition, confirm it is not already true. A condition that is already satisfied when written does not read as an error — it reads as patience.

### A baseline read from Hermes source, and a stub built from the same reading, agreed with each other and were both wrong

**Author.** the first reply-side pass ever run over the recording corpus, which found two wrong pins in the thirteen shapes it checked

**Evidence.** `talaria/domain/compat.py` pinned `approval.respond` as returning `{"resolved": "bool"}`. Three live replies in the corpus carried `{"resolved": 0}`, `{"resolved": 1}` and `{"resolved": 0}` — JSON integers, never `true` or `false`. The gateway handler returns `resolve_gateway_approval(...)` verbatim, and that function is typed `-> int` and documented "Returns the number of approvals resolved (0 means nothing was pending)" (`tools/approval.py:2490-2505`). The same pass found `session.resume` returning a `messages_omitted` key the baseline did not record at all, set on all three of that method's success paths (`methods_session.py:466`, `:551`, `:712`). Two wrong pins out of the thirteen evidence-only shapes, neither of which any test had ever contradicted.

**Mechanism.** The baseline was transcribed by reading Hermes's source. The stub gateway that the compatibility tests run against was transcribed from *the same reading*. So the tests asked "does the check behave correctly when a reply matches the pin" and answered yes, with a reply the pin was written to match — a closed loop with no external input. `tests/transport/test_compat_baseline.py` says as much in its own docstring ("a stub answers every name it is asked… that is precisely what it cannot testify about"), and the gap it names is exactly the one that hid these two. The only thing outside the loop is a reply from a real gateway, and nothing had ever compared one against the pin.

**Why nothing misbehaved.** `talaria/ui/app.py:527-529` already read `resolved` with `isinstance(resolved, int) and not isinstance(resolved, bool)` and rendered "*n* resolved". The consuming code had the right model; only the record of what Hermes returns was wrong. That is why five days of live use produced no symptom — and why the error was invisible to every route except comparing a reply to the pin.

**Validation.** Both pins corrected on the source line that confirms them, not on the reply alone. `tests/domain/test_recorded_reply_shapes.py` stores the recorded top-level shape of every evidence-only reply in the corpus — kinds only, never values, because a real `session.resume` reply carries the operator's message text and a real `paste.collapse` reply carries a local path — and runs the production `compare_shape` against each, so reverting either pin fails. Full suite 1332 passed before the new module, 1347 after.

**Generalizable rule.** When a pinned contract and the fixtures that test it come from the same reading of a source you do not control, no test in that set can catch a misreading. The only evidence that counts is a message the other system actually sent — and if you have a recording corpus, comparing it against the pin costs one pass and is worth running before the pin is trusted.

### "Provoking it means installing a credential-capturing skill" overstated the cost by describing the realistic use case instead of the minimum trigger

**Author.** the operator asking what a credential-capturing skill actually is, which turned a deferred decision into a ten-minute run

**Evidence.** `secret.respond` had been the single method blocking row 6, recorded as "not attempted" because provoking it was said to require installing or configuring a credential-capturing skill on the operator's own machine. Reading the trigger instead of paraphrasing it: `tools/skills_tool.py` fires the secret-capture callback for any skill whose frontmatter declares a `required_environment_variables` entry not already persisted in `~/.hermes/.env` — the variable's purpose is never inspected. The gateway's callback branches on the answer first (`val = _block("secret.request", …)` then `if not val:` returns `skipped`) so an empty answer never reaches `save_env_value_secure`, and `save_env_value_secure` itself returns `validated: False` — nothing is checked against any service. The run that closed the row used a throwaway skill declaring one variable nothing reads, answered with an empty field, and deleted afterwards. No credential existed at any point.

**Mechanism.** The description was written from what such a skill is *for* rather than from what the code branches on. Both halves of the overstatement came from that: "credential" is what the variable usually holds, not what the callback requires, and "installing and configuring" is what adopting a real skill costs, not what one file on disk costs. Neither is wrong as a description of the feature; both are wrong as a description of the trigger, and the trigger is what a provocation has to reproduce.

**The cost of the error.** It converted a ten-minute task into a decision requiring the operator's authorisation, and it did so in a document whose whole purpose is to say precisely what remains. The row sat one method short overnight on a sentence.

**Generalizable rule.** State a provocation's cost as the minimum that reaches the branch, not as the realistic use case that would reach it — and derive that minimum from the condition in the code, not from what the feature is for. This is the same failure as the `command.dispatch` entry below, in the opposite direction: that one described a branch in the remote system's vocabulary, this one described it in the feature's.

### A checklist step marked "provocation not established" was a two-line derivation from source, and the search it recommended would not have found it

**Author.** driving F2 through F6 live to move row 6 of the v0.1 gate

**Evidence.** `docs/plans/2026-08-07-f2-f6-operator-checklist.md` marked `command.dispatch` as having no established provocation, reasoning that the commands it serves are assembled at runtime from the operator's own config and installed skills and so are "not derivable from source". Its advice was to work down the catalogue looking for a command that errors on `slash.exec`. Two reads settled it instead: `talaria/ui/app.py` calls `command.dispatch` whenever the `slash.exec` RPC returns not-`ok`, full stop; and Hermes's own test `test_slash_exec_rejects_skill_commands` says in its docstring "slash.exec must reject skill commands so the TUI falls through to command.dispatch", asserting error code 4018. Any skill command therefore takes the fallback, and the first one tried did. The same two-read approach then produced exact triggers for `sudo.respond` (the terminal tool's sudo-password callback, which is skipped entirely on a host with passwordless sudo) and `terminal.read.respond` (a desktop-renderer tool that reports itself unavailable anywhere else).

**Mechanism.** The checklist asked the wrong question. "Which commands does this serve?" genuinely is not derivable — it depends on installed skills. "What makes Talaria take this branch?" is one `if` statement, and it is the question that matters, because the branch is Talaria's and only the *condition* had to be reproduced, not the whole category of commands satisfying it. Writing the step in the remote system's vocabulary instead of the local branch condition turned a two-minute derivation into an open-ended search — and that search would have been slow *and* inconclusive, because a catalogue command that happens not to error proves nothing about whether the fallback works.

**What was correctly marked.** The other three "not established" steps stayed hard for real reasons: one needs a credential-capturing skill installed on the operator's machine, one needs a host without passwordless sudo, one appears to need a desktop renderer. Three of the four cautions were right, so the lesson is not that the caution was wrong.

**Generalizable rule.** Before recording that a code path cannot be provoked, find the local branch that reaches it and read its condition. "Unreachable" is a claim about a condition, not about a feature — and a step written in the remote system's vocabulary hides the condition that would have answered it.

### The picker marked the wrong model after a switch, and no amount of refetching could have fixed it — the gateway does not publish the fact at all

**Author.** the operator's first report against the modal picker: "after making the selection and then re-opening the modal with `/models` the current model isn't what is currently selected in the model list"

**Evidence.** `/models <n>` dispatches `/model <name> --provider <slug>` over `slash.exec`, and Hermes's own reply names its scope: "(session only — add `--global` to persist)". The marker that failed to follow it came from `ProviderCatalog.current_model`, decoded from the `model` field of `GET /api/model/options`. That field is built by `build_models_payload(load_picker_context(), …)`, and `load_picker_context()` is sixteen lines of `load_config()` — it reads `config.yaml` off disk (`hermes_cli/inventory.py:79-105` at `f1470ec76`). The endpoint's own docstring says so from the other direction: "`profile` scopes the picker context (current model/provider …) so the Models page reads the SAME profile `/api/model/set` writes."

**Mechanism.** Two different facts were arriving under one name. `GET /api/model/options` reports the model a **new** session on that profile would start with; `slash.exec` changes the model the **running** session uses. Neither writes to the other's store, so after a switch the catalogue returns byte-identical bytes forever. Talaria had named its field `is_current` — the ambiguous word — and the ambiguity is the entire defect: the marker was not stale, it was answering a different question correctly.

**The fix that would not have worked, recorded because it is the obvious one.** Refetching the catalogue on picker open. It costs an HTTP round trip on every open, it adds a failure mode where there was none, and it returns exactly the same answer — the source it reads was never written to. A caching bug and an "asking the wrong store" bug present identically from the outside (a value that will not update), and the first fix anyone reaches for only helps with one of them.

**What was built instead.** `SessionModel` in `talaria/ui/picker.py` records the switch Talaria itself made, keyed by the session id it was made on, and is written only on `RpcOutcome.confirmed` — an `unknown` outcome means the call went out with no reply, so the model may or may not have changed and Talaria will not claim it did. `SelectableRow.is_current` is renamed `is_profile_default`. Both facts render, separately annotated, so a diverged pair reads as two true statements rather than one contradiction.

**Generalizable rule.** When a value that should have changed did not, establish which store the read actually reaches before treating it as staleness. And when a remote publishes a field called `current`, make the local name say *current for what* — `is_current` cost a correct-looking marker on the wrong row, and `is_profile_default` could not have.

### The dialog numbered its rows from the top of each stage while a docstring claimed those numbers were the ones `/models <n>` takes

**Author.** driving the fixed picker against a live gateway before committing it, entirely to confirm an unrelated fix

**Evidence.** Opening `/models`, descending into the third provider, and reading the pane showed its ten models numbered `1.` through `10.`. The listing numbers those same models 14 through 23 — `flatten_selectable` numbers across every provider in one sequence, and that number is what `select_model` resolves. So the screen said `1. gpt-5.6-sol` while `/models 1` selected a model belonging to the first provider. The dialog's own `_row_text` docstring asserted the opposite in as many words: "`/models <n>` still accepts it from the composer, so the two surfaces agreeing is worth the width."

**Mechanism.** The number came from `enumerate` over the window — `offset + position + 1` — which is the row's place on the stage. The listing index it was standing in for lived in `Choice.payload`, one field away, and the two coincide exactly on a flat single-stage listing. Profiles are a flat listing, so profiles were right; the model picker's second level is the only place they diverge, and it diverged silently. The full suite passed: every assertion about numbering had been written against the flat case or against the first provider, where position and index are the same number.

**How it surfaced, which is the part worth keeping.** Not a test — 1,331 of them passed on the defect. It was visible in the first screenful of a live run, in a session opened to check something else entirely. The same run also caught a provider row wrapping to a second line because two annotations had been stacked on it. Both are one-look defects and neither is a plausible unit test to have written in advance.

**Generalizable rule.** A rendered number that stands for an identifier elsewhere must be carried as that identifier, not recomputed from position — they agree in the simple case, which is exactly why the recomputation survives review. And a docstring claiming two surfaces agree is a testable claim: assert it, or the docstring becomes the thing that convinces the next reader not to check.

## 2026-08-06

### Six units of work landed and the release gate's live-evidence row did not move, because the only unit whose product was evidence produced a checklist instead

**Author.** unit U7 of the model-picker plan, re-grading rows 6, 13 and 19 of `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`

**Evidence.** The plan ran six units — the admin HTTP surface, the model picker, the credential decision, the profile picker, the default-model picker, and the scripted live acceptance run. Re-grading row 6 by enumerating outbound `method` fields across every frame log in `~/.talaria/recordings` found **eight distinct methods of the required eighteen**, identical to the set the row already named on 2026-08-05. The corpus had gained nothing: its newest header reads `2026-08-04T19:43:17.075Z`, and re-deriving the aggregate sha256 over the directory still yields the `bd69e537f1d9…` prefix the document was written against two days earlier. Unit U6's deliverable, `docs/plans/2026-08-06-u6-row19-operator-checklist.md`, is a well-specified six-step checklist still carrying `status: ready-for-operator`.

**Mechanism.** Units whose product is code close themselves — the tests either pass or they do not, and merging is the completion signal. A unit whose product is *evidence from a live system a person must drive* has no such signal, so the agent-executable part of it (write down precisely what to run) completes and looks like completion, while the part that actually moves the gate waits on a human sitting down at a terminal. Both halves ship in the same commit, under the same unit number, and a status roll-up that counts finished units cannot tell them apart. The gate is only protected from this because row 6 is graded by counting frames rather than by reasoning about which units ran: had it been re-graded on "six units of picker work happened, so live coverage widened", it would have improved on nothing.

**The re-grade's actual outcome, so the record is unambiguous.** Row 6 stays `inferred` (ten of thirteen evidence-only methods have zero runtime evidence, named individually now rather than counted). Row 19 stays `unmet` (no checklist step executed). Row 13 stays `partially unmet` — its open precedence question *was* decided by unit U3, which narrows the reason without clearing the row, and U3's own decision record forbids grading it `met`. The verdict stays **NOT READY** on all three.

**Generalizable rule.** Grade an evidence row by re-deriving the measurement, never by reasoning about which work has since happened — and when a plan contains a unit whose deliverable requires a human at a live system, track "checklist written" and "checklist executed" as two separate states, because the first is what an agent can finish and the second is the only one a gate may read.

### A structural absence test written for one decision quietly encoded a broader one, and the next legitimate feature that needed the broader thing tripped it

**Author.** unit U5 of the model-picker plan, adding `POST /api/model/set` to `talaria/transport/admin.py`

**Evidence.** U4's `test_the_admin_client_has_no_way_to_set_the_active_profile` (`tests/transport/test_admin.py`) asserted `assert code.count("urllib.request.Request(") == code.count('method="GET"') == 1` against the module with docstrings and comments stripped. Its docstring named the guarantee as KTD5 — "Talaria never calls `POST /api/profiles/active`" — but the assertion it wrote checks something strictly larger: that no request in the entire module is ever anything but a GET. U5 needed a real POST for an unrelated, sanctioned write (`POST /api/model/set`, whose own effect and non-effect Hermes's docstring names explicitly), and the test failed on the first run — not because KTD5 was violated, but because the test's literal assertion outran the decision it was written to protect.

**Mechanism.** A source-level absence check is easy to write more strongly than the decision requires, because "no POST in this module" is a simpler sentence to encode than "no POST reaches this one path" — counting `method="GET"` occurrences is one line; proving a specific call site's path argument is a different call site's constant takes an AST walk. The simpler assertion was correct at the moment it was written, since KTD5 and "no POST at all" happened to coincide when the module had exactly one write it refused to make. They stopped coinciding the moment a second, legitimate write joined the module, and nothing about the test's own text said which half of "no POST anywhere" — the part that mattered (no `/api/profiles/active`) or the part that was incidental (no POST, full stop) — was the actual guarantee.

**How it was repaired rather than deleted.** The fix re-derived the check from the decision's actual words: parse `AdminClient` with `ast`, confirm `list_profiles` never calls the module's POST helper, confirm exactly one method (`set_default_model`) does, and confirm that method's POST always targets the `MODEL_SET_PATH` constant by name. The literal-path absence check (`"/api/profiles/active" not in code`) survived unchanged — it was never over-broad, only the verb-count check next to it was.

**Generalizable rule.** When a test's assertion is stronger than the sentence its docstring uses to justify it — "no X anywhere" standing in for "no X *reaching this one place*" — treat the gap as latent breakage waiting on the next legitimate X, not as extra safety margin. Write the assertion at the same granularity as the decision it protects, even when that costs an AST walk instead of a substring count; the substring count is the version that fails for the wrong reason later.

### A 65-character return key passes `execution_spec.py validate` and then kills the run four units in, because the limit belongs to the API and the validator does not know it

**Author.** driving `/work` for the model-picker plan; the workflow halted at U4 after U1, U3 and U2 had completed

**Evidence.** `Workflow` returned `API Error: 400 tools.20.custom.input_schema.properties: Property keys should match pattern '^[a-zA-Z0-9_.-]{1,64}$'`. The offending key was U4's return `credential_refusal_surfaces_as_credential_unavailable_with_reason` — 65 characters against a 64-character ceiling, one over. The same spec had passed `execution_spec.py validate --require-receipts` three times, including immediately before launch, and passed again after the rename. The next-longest key in the spec is 57 characters, so nothing else was near the edge.

**Mechanism.** A unit's `returns` list is compiled into a JSON Schema whose *property keys* are those strings, and that schema is submitted as a `StructuredOutput` tool definition. The constraint is therefore the API's tool-schema property-key pattern, which the local validator has no reason to model — it checks the spec's own grammar, not the wire format of an artifact derived from it two steps later. Nothing catches the key between authoring and dispatch, so the first observer is the model API, at the moment that unit is spawned.

**Why the failure shape is the expensive part.** It is not a fast failure. Cost admission had narrowed the run to one agent at a time, so U1, U3 and U2 ran to completion first — roughly 52 minutes and 503k subagent tokens — before U4 was spawned and rejected. A schema-shaped defect surfaced as a mid-run halt with three units of uncommitted work in the tree. The `resumeFromRunId` cache made the recovery cheap (the three completed units replay from cache; only the edited call onward re-runs), but that is a property of the runtime, not of the spec being safe.

**Generalizable rule.** Descriptive `returns` keys drift long — every one of this spec's reads like a sentence, which is the point. Before dispatch, assert every return key against `^[a-zA-Z0-9_.-]{1,64}$` directly; `validate` passing is not evidence the keys will survive contact with the API. The check is one regex over a list and it belongs next to the emit step, not in a postmortem.

### Removing the top level of a precedence chain silently promotes the level beneath it, and the tests that could tell two credentials apart were the first casualty

**Author.** unit U3 of the model-picker and v0.1-closure plan, removing `HERMES_DASHBOARD_SESSION_TOKEN` from the credential chain (KTD8)

**Evidence.** `talaria/transport/credentials.py`'s `_resolve` had four levels; deleting the first left `TALARIA_GATEWAY_URL`'s `token` query parameter at the top. `tests/recorder/test_command.py::test_the_dialled_url_carries_the_credential_exactly_once` then failed with `['NOT-A-REAL-STALE-0000'] != ['NOT-A-REAL-CANARY-8ae13c']` — not because the "exactly once" property broke, but because the test's whole setup depended on the endpoint's token and the real credential being *different values from different levels*, which the removal made impossible to arrange that way.

**Mechanism.** A test that distinguishes two sources needs two sources. That test put a deliberately stale token on the endpoint URL and the real one in the environment variable, so a dial URL carrying the stale value proved the strip-then-append in `AttachTarget.dial_url` had failed. With the environment variable gone, the endpoint's token *is* the credential, and the two strings collapse into one — at which point the assertion compares a value against itself and the defect it guarded becomes invisible. The failure was loud only because the two literals happened to differ; a test written with one literal would have gone green and stopped guarding anything.

**The operator-visible half, worth saying out loud.** A stale `?token=` left on an exported `TALARIA_GATEWAY_URL` now outranks a credential file that `talaria refresh-credential` has just rewritten. That ordering is unchanged — the endpoint URL always outranked the file — but it used to sit two levels down and now sits at the top, so the number of ways an operator can be quietly served a stale credential went up by removing a route rather than adding one. `README.md` names the file as the route to prefer.

**How it was re-expressed rather than weakened.** The two values were separated through the *other* seam that still holds them apart: the stale token rides the command-line endpoint override, which `AttachTarget` strips, and the real one rides the exported endpoint. Same two literals, same assertion, same defect guarded.

**Generalizable rule.** When you delete a level from a precedence chain, grep the tests for every pair of literals that existed only to tell that level apart from another one. Each pair is either re-expressed against two surviving sources or it silently degrades into a tautology — and a tautological assertion is worse than a deleted one, because it still reports green.

### `git log -S` on a shallow checkout dates every line to the single commit it has, and answers with total confidence

**Author.** pinning the HTTP credential form against Hermes source (unit U1 of the model-picker plan)

**Evidence.** Deciding whether Talaria could send only Hermes's newer `X-Hermes-Session-Token` header turned on one question: how old is that header? `git log --oneline -S'X-Hermes-Session-Token' -- hermes_cli/web_server.py` in the installed checkout at `~/.hermes/hermes-agent` returned exactly one commit — `863e31318`, dated today, which is also `HEAD`. Read at face value that says the header shipped hours ago, which would have made "send only the new header" an obvious mistake and "send only Bearer" the obvious answer. `git rev-list --count HEAD` returns **1** and `.git/shallow` exists: the clone has no history. The pickaxe had reported the only commit it could see, for every line in the repository.

**Mechanism.** `-S` counts occurrences of a string across a diff and reports commits where the count changed. Against a shallow clone, `HEAD` has no parent, so every file reads as wholly added in that one commit and every string in the repository "first appears" there. The result is indistinguishable in form from a real answer: same output shape, same plausible date, no warning, exit status zero. A tool that cannot know is not silent — it is confidently wrong, and it is wrong in the direction of "this is brand new", which is the direction that most changes a compatibility decision.

**What made it survivable.** The claim was load-bearing enough to be worth a second source, and the second source contradicted the first by being unable to confirm it. Two cheap probes — `test -f .git/shallow` and `git rev-list --count HEAD` — cost seconds and turned a hard finding into a known unknown. The decision was then made on evidence that did exist: Hermes's own docstring calls the Bearer path "legacy… for backward compatibility with older dashboard bundles", which establishes the ordering of the two headers without needing either one's date. Talaria sends both.

**Generalizable rule.** Before believing any `git log`, `git blame`, `-S` or `-G` result about *when* something appeared, establish that the repository has the history to answer — `git rev-list --count HEAD` and `.git/shallow`, or `git log --oneline | wc -l`. Shallow clones, squashed imports, and vendored snapshots all produce archaeology that is fluent and false. More generally: when a history query returns a date equal to `HEAD`'s, treat that as a symptom of a truncated repository until proven otherwise, not as a finding. And when the history genuinely cannot answer, look for a claim in the *source text* — a docstring saying "legacy" ordered the two headers here without any dates at all.

### The check that was supposed to catch an inverted verdict passed when the verdict was inverted

**Author.** building the gating-document check that closes DRIFT-04's general case

**Evidence.** `tests/docs/test_gating_documents.py` asserts that a gating document's machine-readable block states the same verdict the document itself does. Written the obvious way, it asked whether the declared verdict appeared in any heading — `assert any(gate.verdict in heading for heading in headings)`. Six mutations were run against the finished module to check each assertion could fail. Five fired the intended test and only that one. The sixth — flipping the block's `verdict: NOT READY` to `verdict: READY` while the document still read "Talaria v0.1 is **NOT READY** as a daily driver" — stayed green: nine passed. `"READY" in "Talaria v0.1 is NOT READY as a daily driver"` is `True`, because the wrong verdict is a substring of the right one. The check now compares the declared verdict against the heading's emphasized span exactly, and the same mutation fails.

**Mechanism.** Negation in English is usually a *prefix*, so the false claim is a substring of the true one — `READY`/`NOT READY`, `met`/`unmet`, `supported`/`unsupported`. Any containment test over prose therefore passes for the inverted claim as readily as for the correct one, and it passes *more* readily the more emphatic the document is, because a longer, more explicit heading contains more substrings. The failure is invisible from the passing side: a green containment assertion looks identical whether it matched the claim or matched the negation of the claim.

**Why it survived writing and would have survived review.** The assertion reads correctly in English — "the verdict the block declares appears in a heading" is exactly the intent, and the code says that. Nothing about the line looks wrong; the defect lives in the gap between `in` and *is*. It was caught only because the module was mutation-tested before being trusted, which took about five minutes and was the whole reason the sixth case existed.

**The part worth keeping is where it was found.** The module's entire purpose is to stop a gating document from asserting something the evidence contradicts. Shipped as first written, its verdict check would have permitted a block claiming READY on a document that says NOT READY — the strongest possible version of the failure it exists to prevent, in the test written to prevent it. That is the second time in two changes on this thread that the defect being fixed reappeared one level out, inside the artifact certifying the fix.

**Generalizable rule.** When a test compares a claim against prose, never use containment for a value whose negation contains it — compare against a delimited span exactly. More generally: a new check is not evidence until each of its assertions has been observed failing for the reason it exists. Write the mutation for the case where the check is *inverted*, not only where it is absent; absence is the easy half, and the inverted case is the one containment silently permits.

## 2026-08-05

### Widening a redactor is a three-file change, and a unit test of the redactor reports success after only one of them

**Author.** closing the P2 URL-fragment gap left open by the code-review gate on the credential-and-bridge-drift remediation

**Evidence.** `talaria/recorder/redact.py` — `redact_url` now withholds URL fragments (divergence 5). Making only that change, and testing it only through `redact_url`, would have left credentials in fragments reaching disk from frame bodies while the suite stayed green. Verified by deleting each branch in turn and counting the red across `tests/recorder/test_redact.py` and `tests/transport/test_attach.py`: the gate branch in `_redact_credential_url` fails **exactly one** test out of 149, `test_a_credential_in_a_frames_url_fragment_is_withheld_too`, and that test exists only because the hazard was anticipated.

**Mechanism.** `redact_url` is not applied to every string in a frame. `_redact_credential_url` gates it, and the gate is deliberately narrow so the corpus keeps its harmless URLs: it returns early unless the value has userinfo, or a query parameter with a denied name. `ws://h/api/ws#token=…` has neither. So the redactor and its gate disagree about what counts as a credential-bearing URL, and the gate wins — silently, because a value that never reaches `redact_url` is never compared before and after. The frame-log *header* goes through `redact_url` directly and would have been fixed; every URL in a frame *body* would not have been; and a unit test of `redact_url` cannot tell those two apart.

The third file is the test harness, for the opposite reason. `tests/recorder/test_equivalence.py`'s `_is_authorized_url_divergence` pinned `ts.fragment == py.fragment` as a component that must survive untouched, so a Python redactor withholding more than the TypeScript reference reported as `parsed frame value differs` — which reads as a port bug, not as the intended divergence. A security widening in this module therefore lands in three places at once: the redactor, the gate that decides whether to call it, and the comparator that decides whether the difference is authorized.

Adding the comparator branch is not enough on its own either. The fixture corpus had no URL with a fragment in it, so the new branch was unreachable and untested until `tests/recorder/fixtures/equivalence_corpus.json` gained one — the same gap the module docstring already records for divergences 2 and 4, repeated because the fixture is where it hides. Confirmed by reverting the comparator with the fixture frame in place and watching the harness fail on `seq 22`.

**Generalizable rule.** When a function is reached through a gate that re-decides the same question, changing the function changes nothing the gate excludes — so pin the change through the *outermost* caller (`redact_frame`, not `redact_url`), and count which tests go red when each branch is deleted. If deleting a branch turns nothing red, the branch is either dead or the test is passing for another reason; both are worth knowing before the commit, not after.

### A subcommand can require the exact thing the rest of the module forbids, and every test still passes

**Author.** post-v0.1 conformance audit, DRIFT-03 remediation (plan `docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md`, unit U2)

**Evidence.** `talaria/cli.py` — `record`'s positional argument was required and its own help text read `ws://127.0.0.1:9119/api/ws?token=<token>`; `README.md:78` printed that command for operators to copy. `talaria/recorder/command.py` took the URL string and handed it straight to the connector. R9 forbids exactly this: attach credentials must stay out of command-line arguments, shell history, and process listings. The audit found it by running the command with a canary value and reading the value back out of `ps -ww -Ao pid,command` from a separate process — not by reading the code.

**Mechanism, and it is a coverage shape rather than a coding mistake.** Every control R9 needs was already built and working. `LoopbackTokenProvider` walks an environment-then-file-then-prompt chain that never touches argv. `AttachTarget` strips credential query parameters at construction, so the object the rest of the system holds is credential-free by invariant. `redact_url` withholds both the query credential and URL userinfo. `talaria refresh-credential` exists so the file route is one command that prints nothing secret. What was missing was a route from `talaria record` into any of it: the subcommand predates the transport module and was never rewired when the chain arrived, so it kept its own URL-shaped door while the building around it got locks.

The reason nothing caught it is narrower and more useful than "no test covered it". The process-surface sweep that measures R9 does exist, and it does launch a real process holding a real credential and search its argv — but it builds its probe with `parse_args([])` (`tests/transport/test_process_surface.py:79`), the bare launcher with no subcommand. It measured one entry point and the journal reported the result as a property of the program: `QUEUED.md:106` said "the argv half holds and is measured" and `DECISIONS.md:406` said attach credentials "never appear in argv". Both sentences were written about the acquisition chain, both were true of it, and both were read afterwards as covering every way the program can be started. The audit's own independent static reviewer read one of them and graded R9 as an accepted divergence.

**The fix, and the one design choice inside it worth keeping.** `record`'s positional is now optional and means the endpoint, never the credential; when present it is passed as `override=` to `AttachTarget.from_environment`, which is the same call the live launcher makes, so the two entry points are one code path instead of two that have to be kept in agreement. Resolution moved out of the dial loop into `resolve_record_target`, which returns an endpoint and a credential as separate halves.

The choice worth keeping is that a credential-bearing URL is **refused**, not silently stripped. `AttachTarget.from_url` would have stripped it happily and the recording would have worked. But by the time `argparse` sees the argument the leak has already happened — the value is in the process table and in the shell history — so stripping it preserves the exact habit that leaked it and teaches the operator nothing. The refusal exits 2, names the two routes that do not involve a command line, and tells the operator to treat the value they just passed as exposed. Userinfo (`ws://user:pass@host/api/ws`) is refused on the same terms even though Hermes would never have authenticated it, because what it did do is put a secret in two places it does not belong.

The refusal reproduces nothing it was given — not the value, not the URL, not a fragment of either. Adding a third copy in stderr, which in continuous integration is a log file in a public repository, would be a strange way to warn someone about the first two.

**Generalizable rule.** A guarantee measured at one entry point is a guarantee about that entry point. Before writing "X never happens" in a journal, name the entry points the measurement actually covers and say so in the sentence — and when a shared control (a credential chain, a redaction boundary, an allowlist) lands, audit every existing caller for one that predates it and still has its own door.

### A sweep parametrized over one entry point measures one entry point, no matter how broadly the surrounding prose describes it

**Author.** post-v0.1 conformance audit, DRIFT-03 remediation (plan `docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md`, unit U3)

**Evidence.** `tests/transport/test_process_surface.py`'s R1 (the requirement that a running Talaria's command line never carries a credential) built exactly one probe, from `parse_args([])` — the bare `talaria` launcher, no subcommand. `docs/engineering-journal/QUEUED.md:106` then reported the result as "The argv half holds and is measured," unqualified. `talaria record` is a second, separate entry point into the same process image, and at the time it took its gateway URL — credential included, as `?token=` — as a required positional command-line argument (`talaria/cli.py:68-75`, before the 2026-08-05 credential-and-bridge-drift remediation). Nothing in the sweep ever launched it, so nothing could have caught it. Confirmed by running the command against a dead port with a canary value and reading it back out of `ps -ww -Ao pid,command` from a separate process — the same falsification the sweep itself performs, just done once by hand instead of continuously in the suite.

Three documents repeated the overclaim in different registers: `QUEUED.md` stated the measurement as complete, `docs/engineering-journal/DECISIONS.md:406` generalized "never appear in argv" from the `CredentialProvider` acquisition chain (true) to the whole program (not measured), and `README.md:78` printed the leaking command as the documented way to run `talaria record`. An independent static reviewer read one of the three, took it as settled, and graded the requirement an accepted divergence rather than an open defect.

**Fixed** by parametrizing the sweep over every shipped entry point that can hold a credential — the launcher, `record`, `refresh-credential` — and adding a guard, `test_the_subcommand_set_is_exactly_the_classified_set`, that reads the live subparser choices off `talaria.cli.build_parser` and fails when a subcommand exists that nobody has classified as credential-holding or not. Demonstrated red: a throwaway subcommand added to the parser failed the guard with `unclassified=['throwaway-red-demo']`; removing the subcommand restored green. Both journal entries above are corrected in place to say what is now measured and to record that the earlier wording was an overclaim, rather than being left to quietly become true once `record` was fixed — a true sentence with a false history reads exactly like a true sentence, and the only way to know it once covered a real gap is to have written that down.

**Mechanism.** The sweep's own docstring named its narrow scope accurately — "a running process built by the real launcher" — but the journal entry summarizing it dropped the qualifier "built by the real launcher" and kept only "a running process," which is a claim about the whole program. The gap opened at the point of *summarization*, not at the point of measurement: the test was honest about what it covered, and the sentence describing the test to a future reader was not. This is the same shape as the six remediated earlier in this file — an assertion, or here a claim, that is a consequence of something already established (one probe ran clean) rather than an independent check of the thing actually asserted (every entry point runs clean) — except one level up, in prose rather than in a `assert` statement.

**Generalizable rule.** When a probe, sweep, or test is scoped to one code path, the sentence that reports its result must name that path, every time it is repeated — "the launcher's argv" is a different claim from "argv," and only the narrower one was paid for. And a claim that a sweep covers "every X" is itself a testable claim: pin it with a guard that enumerates the live X (a parser's subcommands, a bridge's methods, a module's public functions) and fails when the enumeration grows past what the sweep or the classification list was told about, rather than leaving completeness to be re-asserted by hand each time something is added.

### A refusal's exit code proves nothing when the fallback path exits the same way

**Author.** code-review gate on the credential-and-bridge-drift remediation (plan `docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md`, gate on U2)

**Evidence.** U2 added the control this whole plan exists to ship: `talaria record` refuses an endpoint carrying a credential and exits 2. It arrived with eight tests. Deleting the refusal outright — replacing its condition with `if False:` — left **seven of the eight still passing**. Only `test_the_record_refusal_names_both_supported_routes`, which asserts on the message text, went red.

**Mechanism, and it is a two-exits-one-code problem.** `run_record_command` returns 2 for two unrelated reasons, which KTD7 chose deliberately: the refusal, and a credential the chain could not supply. Under pytest the chain supplies nothing and has no terminal to prompt on, so it raises `CredentialError` and returns 2 by the *other* route. Every assertion the tests made was satisfied by that route too: the exit code was 2, `run_record` was never reached because the failure happened earlier, and the canary was absent from the message because an unrelated error message does not contain it either. The R6 tests were the worst affected — "the refusal does not echo the credential" is vacuously true of any output that is not the refusal.

The tell was available and unread: the refusal path and the failure path are distinguishable only by what they *say*, and exactly one of the eight tests read that. The `pre_existing` shape here is worth naming too — the fragment gap the same gate found was a missing branch, a thing not written; this was eight things written that did not test what they claimed. The second is harder to see, because the suite is green either way and the count went up.

**Fixed** by giving the refusal a signature the other exit cannot produce — `REFUSAL_SIGNATURE = "refusing to record"` — and asserting it in every refusal test before any other assertion. Re-run of the same deletion now fails 7 of 8, exactly inverting the original result; removing only the fragment branch fails exactly the 2 fragment rows. The one test that still passes under a fully deleted refusal, `test_an_unparseable_record_endpoint_is_refused_rather_than_parsed`, is correct to: it exercises `resolve_endpoint`'s own refusal, a different control.

**Generalizable rule.** When a function has more than one route to the same exit code, an exit-code assertion is not a test of any one of them. Before trusting a test of a new guard, ask what *else* produces the value being asserted, and under a test harness ask it twice — a harness with no credential, no terminal, no network and no config reaches error paths that a real run rarely does, and those paths are exactly the ones that mimic a refusal. The cheap check is mechanical and should be routine for any security control: delete the control, run its tests, and count. If the number that go red is not the number that claim to cover it, the difference is the measure of how much of that suite was decoration.

## 2026-08-04

### Prior art: Qwen Code, a mature terminal agent that answers four of Talaria's open questions in the opposite direction

**Author.** post-v0.1, conformance audit, prior-art survey

**Source.** [QwenLM/qwen-code](https://github.com/QwenLM/qwen-code) — an open-source terminal coding agent, Apache-2.0, TypeScript. Its README states it was originally based on [Google Gemini CLI](https://github.com/google-gemini/gemini-cli) v0.8.2 and stopped syncing with upstream at Qwen Code v0.1, developing independently since. Read at v0.21.5, whose distribution bundles its own user documentation; the paths cited below are that documentation's paths within the upstream repository. It is prior art in ADR-0003's sense — documentation of behaviour, worth reading and not worth translating.

**Why it is worth the shelf space.** Talaria and Qwen Code solve the same problems for the same operator, and Qwen Code has been solving them longer and in public. The overlap is not incidental: an operator-configurable status line driven by a JSON payload, a permission surface that blocks a turn until a human answers, sub-agent display, markdown rendering of agent prose, session management, and a structured event stream that lets a second surface observe and steer a live session. Four of those are places where Talaria has an open question *right now*, and Qwen Code has already picked a side. Its answers are not automatically better — in at least one case Talaria's is the safer one — but each disagreement names the assumption Talaria's side rests on, which is what makes a revisit condition checkable.

**The status line, where the disagreements are sharpest** (`docs/features/status-line.md`). Talaria's status line is command-only: an operator executable, a JSON payload on stdin carrying a version identifier, newline-separated rows, a documented row bound, and ANSI never interpreted (R18, R19, R22). Qwen Code's is the same shape plus four things Talaria does not have.

- **A preset mode with no command at all** — a fixed menu of about sixteen data items (model, git branch, context used and remaining, working directory, session id, run state, version) selected through a dialog and joined with a separator. This is a direct answer to the failure QUEUED carries as a P2: a malformed `status.command` disables Talaria's status line silently with nothing for the operator to read. A preset has no command to malform, so the whole failure mode is absent rather than handled.
- **A `refreshInterval` setting**, documented as being for "data that changes without an Agent state event (clock, quota, uptime)" — the same need behind Talaria's interval default, and a reminder that the interval exists to serve payload-independent data, not to poll the agent.
- **A `respectUserColors` opt-in** that preserves ANSI colour in the command's output instead of applying the footer's own styling. This is the real disagreement. Qwen Code treats the status command's output as trusted because the operator supplied the command; Talaria's R22 treats every byte that reaches the screen as untrusted regardless of who configured its source, and defangs unconditionally. Talaria's side is the defensible one — an operator-supplied command routinely prints bytes the operator did not author, such as a branch name from a fetched remote — but the assumption had never been written down as a choice, only as a rule.
- **A nested payload.** Qwen Code's stdin object nests under `context_window`, `model`, `workspace`, `metrics`, and includes three objects that are present or absent rather than null — `git` only inside a repository, `worktree` only inside one, `vim` only when vim mode is on. That is precisely the payload class Talaria's shape comparison cannot see: `compare_shape` compares the top level only, so a wholly restructured nested payload reads as present (QUEUED P2). Qwen Code's payload is a ready-made adversarial case for that limitation.

Its row budget is also much tighter than Talaria's: one line by default, an optional second, and a third for the mode indicator, with the text truncated to the available width and temporarily overridden by high-priority messages such as exit prompts and vim mode. Talaria's bound is eight rows and it has no notion of a transient override.

**Dual Output, which is Talaria's architecture pointed the other way** (`docs/features/dual-output.md`). Talaria attaches to a Hermes gateway socket and re-renders what it sees. Qwen Code's Dual Output emits a structured JSON event stream *out of* its own terminal UI while continuing to render normally, and pairs it with a reverse channel a second program can write to in order to submit prompts and answer tool-permission requests as if a human were at the keyboard. Two of its stated conclusions match Talaria's independently:

- The structured stream, not the screen, is the canonical transcript — the documentation's phrasing is that history is captured verbatim so an external surface has a machine-readable record "without parsing ANSI". This is the same conclusion behind Talaria's recorder and R3's byte-identical replay comparison, reached from the emitting side rather than the receiving side.
- On approval prompts appearing in two places at once, its rule is that whoever answers first wins. Talaria has not faced this yet because only one client has ever attached, but it will the moment a second dashboard joins a session — and Talaria's prompt keying is exactly where that lands, since `approval.request` carries no request id and Talaria synthesises a counting key per session. A counting key is stable for one observer and is not obviously stable for two.

**Graduated approval modes, a question Talaria has not asked** (`docs/features/approval-mode.md`). Qwen Code has five standing permission postures — plan (read-only), ask, auto-edit, auto (a classifier decides), and yolo — cycled with a keystroke and displayed in the status bar. Talaria has five *prompt kinds* and no standing posture at all: every blocking request is answered individually, every time. The two fives are unrelated, and the distinction is worth keeping straight when reading its documentation.

**Generalizable rule.** When a mature implementation of the same problem disagrees with a decision that is already settled here, record the disagreement even when the decision stands. A settled rule states what is done; the disagreement states what it rests on. "ANSI is never interpreted" survives contact with `respectUserColors` unchanged, but only after answering *why* — because trust in the configurer is not trust in the bytes — and that answer, unlike the rule, has a condition under which it could stop being true.

### The compatibility check verified the fallback route and not the one commands actually take

**Author.** post-v0.1, conformance audit, first pass

**Evidence.** `talaria/domain/compat.py` pinned seventeen gateway methods and `slash.exec` was not among them — while `talaria/domain/commands.py:54-66` sends every ordinary catalogue command over `slash.exec` and reaches `command.dispatch` only for what that handler refuses. The seventeen included the fallback and not the primary. Talaria's own frame logs settled that this was live behaviour rather than a reading of the source: across seventeen recordings from 2026-08-04, Talaria called `slash.exec` ten times against a real Hermes dashboard and was answered ten times with no error. Fixed by pinning the entry (evidence-only, `tui_gateway/methods_tools.py:1073-1211` at `7f4d15515`, request `{session_id, command}`, reply `{output}` plus a conditional `warning`) and by `tests/domain/test_compat_coverage.py`, which fails on the pre-fix tree naming `talaria/domain/commands.py::SLASH_EXEC_METHOD`.

**Mechanism.** R34's promise is that a missing required method is *named* and blocks the daily-driver verdict. The check keeps that promise by iterating `COMPAT_BASELINE`, so its reach is exactly the baseline's contents: a method absent from the list is not reported missing, it is not reported at all. Startup would have announced daily-driver ready against a gateway that had renamed or dropped `slash.exec`, and the failure would have surfaced on the first slash command an operator typed. Every existing test shared the blind spot for the same reason — `EXPECTED_PROBE_SET`, `FORBIDDEN_AT_STARTUP` and the report's own "unverified at runtime" count are all derived from the baseline, so a method missing from it is missing from its tests too and the suite stays green.

**Why a recording found what a thousand tests could not.** The tests reason over what the client *declares*; the frame log records what it *did*. Those two disagree exactly when a declaration is missing, which is the one case the declaration-derived tests cannot represent. Nothing about the gap was subtle once the two were put side by side — the method appears ten times in traffic and zero times in the baseline — but no artifact in the repository held both facts until the corpus was read as evidence rather than kept as replay input.

**Generalizable rule.** A completeness check is only as complete as its list, so the list needs a check of its own, and that check has to draw on a source the list does not control. Here the independent source is the code's own `*_METHOD` constants, read by parsing rather than importing. A scan like that is worth having only with a falsifiability control beside it: a scan that silently matches nothing satisfies its assertions forever, and that failure looks exactly like success.

### R3's evidence method could not have passed, and the reason was a line nobody thought of as content

**Author.** post-v0.1, second operator session against a live gateway

**Evidence.** R3 — "the operator can submit a prompt and watch the response stream into the transcript" — was carried as unmet because no Hermes gateway had ever been attached. It has now been attached repeatedly: a dashboard on loopback, `talaria --record`, a prompt submitted, the reply streamed to completion. The comparison the repo itself specifies for R3 (`talaria/cli.py`: "one live turn streamed to completion and its transcript compared against a replay of the same frames") now passes on a 32-frame recording — three live rows, three replayed lines, byte-identical, `interface_shows_everything` true, corpus cited by digest and count under R29.

**Mechanism, and why the comparison could not have passed before.** The first attempt compared a live screen against a replay and reported zero differences, which was true and meaningless: the pane had scrolled, so the rows compared happened to exclude the operator's own line — and that line was exactly the one a replay could not produce. `record_submission` writes it locally at submit time because the gateway never echoes a prompt back as an event, and a replay never submits anything. `TalariaApp.ingest` then discarded every outbound frame on the reasoning that a request is not a description of what the session became. Correct for the *response* to a request; wrong for this one, whose params carry text that exists nowhere else. So a replay of a real session rebuilt the agent's half of the conversation and left out the question it was answering — and under R30 ("drives the entire interface from a frame log") that is an interface driven to a visibly incomplete state.

**The fix is mode-scoped, and the constraint is the interesting half.** In replay mode a recorded `prompt.submit` now becomes the operator's line. In live mode it must not, because the local write has already happened and folding the frame as well would print the message twice — which reads as a message that was genuinely sent twice, a worse defect than the one being fixed. Both directions are pinned: removing the app branch fails the replay test, and making it unconditional fails the live test.

**Generalizable rule.** A verification that passes is not evidence until you can say what it would have caught. This one compared whatever happened to be on screen, so its scope was set by a scroll position rather than by the claim — and the single row it omitted was the row the system could not produce. When a comparison's inputs are captured rather than constructed, check that the capture covers the thing most likely to be missing, and prefer a case small enough to fit entirely in view over a realistic one that does not.

### A gating verdict is a snapshot with no inbound link, so the work that clears its own blockers cannot re-open it

**Author.** restating the v0.1 daily-driver verdict, closing DRIFT-04 in [the conformance audit's finding register](../analysis/2026-08-05-conformance-audit-drift-findings.md)

**Evidence.** `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md` named, on 2026-08-02, exactly the two conditions that would change its verdict: one real Hermes gateway attach (R2) and one real conversational turn streamed to completion (R3). Both happened on 2026-08-04, and the second was even recorded correctly at the time — the entry directly above this one already logged R3 as done, verified by a byte-identical replay comparison. The verdict document itself still graded both rows `unmet` a full day later, giving reasons ("no gateway has answered", "nothing was submitted") that a `grep` of the very recordings it was gating against would have disproved in minutes.

**Mechanism.** Nothing in the repository treats "a condition named in document X has been met" as an event that triggers a look back at document X. The live-attach work landed in the correct place for *forward-looking* record-keeping — a `LEARNINGS.md` entry describing what happened — but a `LEARNINGS.md` entry is written once and read by scrolling, not searched for its consequences on other documents. The verdict is dated and reads like a snapshot; nothing about writing the correct entry in one file updates the stale sentence in the other, and no CI check or convention closes that loop. This is not particular to this verdict — it is a property of any document that (a) states a conclusion, (b) names the specific facts that would change it, and (c) is not itself re-read every time related work lands.

**The direction of the resulting error is the reason it went unnoticed for a day.** DRIFT-04 was an *under*-claim — the document was more pessimistic than the evidence supported — which cannot mislead anyone into relying on something unproven. An over-claim (the shape DRIFT-03 was) gets noticed because someone trusts it and is burned; an under-claim just sits there being needlessly conservative, and nothing forces a second look.

**Generalizable rule.** A document that names the specific conditions under which its own verdict would change is making a promise it cannot keep unaddressed: something has to point *back* at it from the work that satisfies those conditions, or the verdict silently outlives the facts it rests on.

**Closed 2026-08-06, and neither of the two candidate fixes was the answer on its own.** This was filed as a deferred general problem because the project had no convention for choosing between a backlink convention and a periodic re-read sweep. The choice, recorded in [`DECISIONS.md`](DECISIONS.md), is that the backlink is the *notation* and a test is the *mechanism*: a gating document declares its blocking conditions in a fenced `gate` block, and `tests/docs/test_gating_documents.py` holds the block to the evidence table it summarizes. A convention alone would have depended on the same act of remembering that failed here; see the 2026-08-06 entry "The check that was supposed to catch an inverted verdict passed when the verdict was inverted" for what building it turned up.

### Fixing a stale document broke the citations pointing at it, in the entry certifying the fix

**Author.** the pre-PR code-review gate on the DRIFT-04 restatement, 2026-08-05

**Evidence.** The restatement rewrote `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md` and moved its content by roughly two hundred lines. The finding register's DRIFT-04 entry cited that document by line number in five places, and the same commit that rewrote the verdict also rewrote the register entry — carrying the old numbers forward unchanged. Verified by reading each: `:85` had pointed at the R2 row and landed on row 6a; `:86` had pointed at R3 and landed on row 7; `:428` had pointed at the NOT READY heading and landed on the restatement preamble. One was worse than a dangling pointer — "`:443-472` names **five** items, not two" is present tense, so it did not merely fail to resolve, it asserted something false about the current file. A citation of `talaria/ui/app.py:1839` had the same fault, the corrected docstring having moved to `:1845`.

**Mechanism.** A line number is a coordinate in a file's current layout, not a name for anything in it. It stays correct only while nothing above it changes, which makes it precisely the wrong way for document A to refer to document B when A cannot see B's edits. The failure is silent in both directions: the rewrite has no idea anyone was pointing at those lines, and the pointer has no way to notice it now lands somewhere else. Nothing in the tests or the linters can see it either, because both files are prose.

**The part worth keeping.** The restated verdict itself was clean — it cited its own rows by row number throughout and introduced no self-referential line numbers, which is why the defect appeared only in the register. The same writer got it right inside the document and wrong across documents, so this is not carelessness; it is that the stable-anchor habit had not been generalized past the file being edited.

**Generalizable rule.** Cite by a name the target controls — a section heading, a table row, a function or test name — never by line number, whenever the citation lives outside the file it points at. Line numbers are acceptable only within a file that is edited as a unit, or as an explicitly dated snapshot that is not expected to resolve later. This one is sharper than it looks given where it was found: the change it nearly shipped in was a change whose entire purpose was correcting a document whose claims had outlived the facts, and it would have reproduced that exact defect one level out, inside the entry certifying the fix. See [`DECISIONS.md`](DECISIONS.md) for the corpus-label decision from the same work, which turns on the same idea — a reference has to carry enough to be re-derived without the prose around it.

### A security control stated as a property of characters was really a property of pictures, and only the second version can be tested

**Author.** post-v0.1, second operator session against a live gateway

**Evidence.** Agent prose arrived on screen as `⚠� check the lock file` — the warning sign, then a replacement character. `defang`'s table covered U+FE00–U+FE0F, so VARIATION SELECTOR-16 was marked and every emoji that asks for its coloured form was split. Narrowed to exempt U+FE0E and U+FE0F; verified live, where a reply came back reading `Status ✅ done, ⚠️ warning, ℹ️ note, family 👨�👩�👧` — the three presentation-selector emoji whole, the ZWJ family still marked, which is the intended split.

**Mechanism.** The table had been described as "characters that change what a terminal draws without drawing anything themselves", and the second clause did the selecting. That reading admits the presentation selectors, because they do draw nothing themselves. But the control exists so that an approved *picture* identifies the bytes that run, and under that purpose the criterion is "draws nothing **and leaves the picture the same**" — which admits U+200D (`rm` and `r<ZWJ>m` are one picture) and excludes U+FE0F (`⚠` is one cell, `⚠️` is two). The restatement is not a loosening dressed up; it is the criterion the rest of the table already satisfies, made explicit enough to exclude something.

**And the restated version is measurable, where the original was not.** "Changes what a terminal draws without drawing anything itself" cannot be checked in a test — there is no way to ask a string whether it is sneaky. "Renders at a different width than the character alone" can: `cell_len("⚠") != cell_len("⚠️")` is now an assertion, and it fails if a Rich or terminal change ever collapses the two, which is exactly the condition under which the exemption should be withdrawn. The same measurement is what killed the wider version of the change — `cell_len("r<ZWJ>m")` returns 1 where a terminal draws 2, so exempting the joiner would have desynchronised `wrap_command`'s column arithmetic from the screen.

**Generalizable rule.** When a security control's stated criterion is a property of the *input* ("these characters are invisible"), check it against the property of the *outcome* it exists to protect ("no two inputs may look alike"). The two agree on most cases and disagree exactly at the edge you are being asked about — and the outcome version usually turns out to be measurable, which converts a judgment call into a test that will fail when the judgment stops holding.

### Two event names that mean the same English word carried opposite things, and reading only the client is what hid it

**Author.** post-v0.1, second operator session against a live gateway

**Evidence.** The reasoning line on screen read `· (◐) indexing...The user wants me to reply with exactly a specific string`. The recording of that turn (`2026-08-04T18-38-10-881Z.jsonl`, frames 21-25) shows why: `thinking.delta` carried `(◐) indexing...`, then an empty `thinking.delta` to retire it, then fifteen `reasoning.delta` frames carrying the actual reasoning. `talaria/domain/state.py` routed both event types to `_on_reasoning_delta`, which appends, and which ignored the empty frame because it ignored every falsy text. Fixed with a separate `_on_thinking_delta` writing a replaceable `SessionState.thinking_notice`, surfaced on the activity line; pinned by `tests/domain/test_thinking_status.py`, whose defect tests were watched to fail against the pre-fix tree with the operator's own line in the assertion output.

**Mechanism.** Hermes names the two channels almost identically and uses them for opposite things. `thinking.delta` is the *spinner*: `run_agent._emit_wait_notice` writes it so a long provider stall says what it is waiting on, and the docstring at `run_agent.py:1047` states the contract outright — the callback "is bridged to the `thinking.delta` event, which both render as the live spinner/status line". The model's actual thinking never touches it; it reaches the client through `agent._fire_reasoning_delta` (`chat_completion_helpers.py:3629-3633`) as `reasoning.delta`. Hermes's own TUI does feed `thinking.delta` into its reasoning buffer as well, which is what a reading of the client alone shows — but Hermes gates reasoning *display* on a setting that is off by default, so the spinner text almost never surfaces there. Talaria removed that gate deliberately for R6, and removing it is what turned a latent quirk into a visible one.

**The visible line was the mild half.** `reasoning.available` delivers the complete reasoning block and declines to overwrite deltas that already built one — correct, or the block would duplicate what is on screen. A spinner sitting in the delta buffer is indistinguishable from that, so the guard fired and the whole block was discarded. Confirmed on a live turn after the fix was written (`2026-08-04T19-00-12-373Z.jsonl`): `thinking.delta` sent `(▶) optimizing...` at frame 20 and `reasoning.available` sent the reasoning at frame 26. Replayed against the pre-fix tree, the transcript's only reasoning entry is `(▶) optimizing...` and the model's reasoning is gone — content loss, which is exactly what R6 forbids. A prefix on screen is a cosmetic complaint; the same bug one branch over was silently deleting the thing the requirement protects.

**Generalizable rule.** When a protocol has two events whose names are synonyms in English, do not infer their contract from how one client happens to handle them — find the server-side emitter and read what writes each one. A client's handling can be wrong, can be historical, or can be invisible behind a display setting that hides the disagreement. And when a channel that was being *dropped* starts being *kept*, re-examine every guard that treats "we already have some of this" as a reason to refuse more: un-gating an input does not just add content, it changes what every downstream emptiness check is looking at.

### The framework's answer for "where does focus go now" was a widget that accepts the caret and discards every key

**Author.** post-v0.1, second operator session against a live gateway

**Evidence.** Mid-session Talaria stopped accepting typed text. The app was demonstrably alive — `ctrl+c` rendered its quit toast and `escape` dismissed it — and the composer still showed its placeholder, because a composer with no text always does. A fresh launch accepted text immediately, so the transport was not at fault. Reproduced headlessly in three ways, each printing `app.screen.focused` after the event: answering a `clarify` prompt (keyboard only), answering an approval by clicking a button, and letting a sub-agent finish while its row held the caret. The first two left focus on `PromptRegion#prompts` and the third left it on an `AgentRow` reporting `focusable=False, in_chain=False`; in all three a subsequent keypress left `app.composer.text == ''`. Fixed by `talaria/ui/focus.py` (`CaretReleased`, `holds_caret`) with `TalariaApp.on_caret_released` returning the caret to the composer; pinned by `tests/ui/test_focus_returns.py`, whose four defect tests were watched to fail against the pre-fix tree.

**Mechanism.** Two distinct paths, one symptom. When a widget is *removed*, Textual's `Screen._reset_focus` hands the caret to the neighbouring entry in the focus chain — and the neighbour above every control Talaria mounts is the scrollable region containing it. Both `PromptRegion` and `TranscriptPane` are `VerticalScroll`, which sets `can_focus = True` so arrow keys scroll it, so the region accepts the caret and then silently drops every printable key. When a widget instead stops being *focusable* without being removed — `AgentRow.bind_row` flips `can_focus` to `False` the moment a child reaches a terminal status — Textual does nothing at all, because `_reset_focus` is only reached from `App._prune`. The row keeps the caret, drops out of the focus chain, and eats keys. Nothing in the interface could report either state: the app answers `ctrl+c` throughout because those bindings are `priority` and never needed the caret, and the composer deliberately has no focus-dependent border (a focus-styled border made the layout jump two rows, `talaria/ui/composer.py:181-189`).

**A terminal screenshot is a cache, and it produced a confident wrong diagnosis.** Verifying live, `herdr pane send-text` appeared to do nothing: the composer kept showing its placeholder across repeated reads. Counting how many `tab` presses it took before typed text showed up gave a focus position, and that position implied `set_focus(None)` — a specific, plausible, entirely fabricated failure. The text had in fact landed on the first attempt; the pane had not repainted, so every read returned the same stale rows. The next keypress flushed it and the composer read `typing worksa` — both inputs, in order. Settled by adding a temporary trace of `screen.focused` on every render pass: across 471 renders the caret was only ever the composer or a sub-agent row, and **never `None`**. The same trace caught the real fix working (`caret-released-handled focused=AgentRow(focusable=False)`, composer on the next render).

**Generalizable rule.** A framework's default for "the focused widget went away" is a layout answer, not an intent answer — it picks a *neighbour*, and only your app knows which widget the operator was reaching for. Wherever you remove a control or revoke its focusability, decide where the caret goes; and prefer an event at those transitions over a clamp in the render pass, which would drag the caret back from anywhere the operator deliberately put it. Assert the operator's question, not the framework's: `app.screen.focused` is the wrong thing to test, because it passes for a widget that holds the caret and discards every key. Press keys and read the editor. And when the only instrument is a screen scrape of a full-screen terminal app, treat a *negative* reading as unverified until a second, independent instrument agrees — a captured screen is a cache, it goes stale silently, and it fails in the direction that looks like evidence. Concretely, for a Textual app on the alternate screen, `herdr pane read --source visible` returned stale rows for minutes at a time while `--source recent-unwrapped` was current; the same text was absent from one and present in the other in the same second.

### The reported bug was the specification, and the real hazard was in the fix nobody had asked to be careful about

**Author.** post-v0.1, second operator session against a live gateway

**Evidence.** "Markdown is not rendering" was carried on the defect list from live testing. It is not a defect: R6 says so in one sentence (`docs/brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md:121`), `talaria/domain/projection.py` repeats it in a comment beside the speaker prefixes, and `tests/domain/test_projection.py::test_every_transcript_entry_survives_into_the_line_buffer` enforces the half of R6 that *is* an obligation. A live sample confirmed the specified behaviour was working: heading, bold, code span, bullets, and a fenced Python block all arrived intact with their syntax literal. The item was reported back as a requirement change rather than fixed as a bug, and the operator chose the scope — inline now, block level queued.

**Mechanism, and where the real risk turned out to be.** Implementing inline emphasis is trivial; implementing it without corrupting text is not. `__init__.py` and `**kwargs` are *valid CommonMark emphasis*, so a faithful renderer turns a Python dunder into a bold `init` and a keyword-arguments signature into bold prose. On a client whose stated posture is that the rendered path is the executed path, that is a worse defect than the asterisks it removes — and it arrives disguised as correctness, because the parser is doing exactly what the spec says. Two things contained it: dropping underscore emphasis entirely, and taking only the whitespace clauses of CommonMark's flanking rules, which is what leaves `def handler(*args, **kwargs)` and `2 * 3 * 4` alone. Both are pinned by `tests/ui/test_markdown.py::UNTOUCHED`, and both were confirmed on a live gateway in the same reply that rendered `**Judgment**` bold and `` `concise` `` in cyan.

**A second-order effect worth the note.** Rendering shortens a line, so the pane's text and the projection's text stopped being the same string — and `interface_shows_everything` in `talaria/replay/gate.py`, the check that proves the conversation is actually on screen, compares them position by position. Left alone it would have reported content loss on every styled reply. Resolved by having each line widget carry the projection line it was built from (`TranscriptLine.source`), so `rendered_lines` still answers "which content is the pane holding" and a new `drawn_lines` answers "what does the terminal paint". Two questions that were one string before the feature, and are two after it.

**Generalizable rule.** Check a reported bug against the requirement before fixing it — a defect list carried across sessions accumulates items that are specification, and "fixing" one silently changes a settled decision without the decision being made. Then, when a rendering feature is approved, ask what the transform does to text that only *looks* like markup: the failure that matters is not the construct that fails to render, it is the identifier that renders when it should not have. And when a presentation layer starts changing text, find every check that compares the rendered thing to the source thing before shipping — those checks do not fail loudly on a presentation change, they fail as a false report about content.

### A re-encoded constant carried Hermes's number and dropped the reason for it

**Author.** post-v0.1, second operator session against a live gateway

**Evidence.** A live `status.update` arrived reading `⚠️  Context file AGENTS.md TRUNCATED: 74668 chars exceeds limit of 65280 — trim the file, pin a larger context_file_max_chars, or use a larger-context model!` — 157 characters. `SYSTEM_LINE_CLIP = 120` cut it to `…pin a larger context_file_max_` plus an ellipsis, discarding `chars, or use a larger-context model!`. The clip landed **mid-identifier**, so the setting the warning named could not even be copied. Read from the recording rather than the screen, so the cut is attributable to the clip and not to terminal wrapping. The same bound had already truncated a handshake failure to `handshake rejected with HTTP 500 (server rejected Web…` while that failure was being diagnosed. Fixed by splitting one constant into `DETAIL_LINE_CLIP` (120) and `TRANSCRIPT_LINE_CLIP` (2000) in `talaria/domain/normalize.py`; pinned by `tests/domain/test_normalize.py::test_a_gateway_warning_reaches_the_transcript_with_its_remedy_intact`, watched to fail against the old bound with exactly the observed cut.

**Mechanism.** The constant's own comment recorded the error in plain sight: *"Hermes clips a `gateway.stderr` line to 120 characters … so a runaway line cannot own the activity area. Re-encoded for every system line, since the same argument applies to any gateway-authored text."* The first sentence is accurate and the second does not follow from it. Hermes's two 120-char clips are both scoped to `gateway.stderr` and both feed a **one-row activity region** — the second site says so outright, *"to match the 120-char clip used for `gateway.stderr` activity entries"* — and Hermes's `status.update` handler passes `p.text` to `pushActivity` with no `slice` at all. So the bound was never about the text being gateway-authored; it was about the height of the region receiving it. Talaria's transcript scrolls, which is precisely the property that makes the original argument inapplicable. The number was re-encoded faithfully and the justification was widened in the same breath, which is how a correct citation ends up supporting a claim its source does not make.

**Generalizable rule.** When re-encoding a bound from another system, carry its *scope* with its value — which inputs it applies to and which region it protects — and re-derive it for the local surface rather than generalizing from "the same argument applies". A truncation is also a content decision, not only a layout one: whatever it cuts is a claim about what the reader does not need, and the remedy is the half of a diagnostic that a length-based cut takes first.

### A nine-day-old process failed against source that was correct on disk

**Author.** post-v0.1, second operator session — diagnosis, not a Talaria code change

**Evidence.** Four of five Hermes dashboards refused Talaria's WebSocket handshake with HTTP 500 while a fifth accepted it. Dashboard stderr gave `ImportError: cannot import name 'DEFAULT_INDICATOR_STYLE' from 'hermes_constants'` at `tui_gateway/server.py:26`. The constant **exists** in the checked-out source. The four failing dashboards were launchd jobs started Jul 26 19:52; the constant landed Aug 3 14:42 in commit `0845232d7`. Each held a `hermes_constants` module imported nine days earlier and satisfied a *lazy* import from today's `server.py` against it. `launchctl kickstart -k gui/$(id -u)/ai.hermes.<profile>.labyrinth` on each returned all five to HTTP 101.

**Mechanism.** Long-lived processes make "the code" ambiguous — there is the code on disk and the code in memory, and a lazy import is the seam where a process can hold both at once and disagree with itself. Hermes's own terminal UI cannot reach this state because it does not dial a gateway, it spawns one per launch (`ui-tui/src/gatewayClient.ts:356`); Talaria dials a dashboard it did not start, so by [ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md) it inherits that process's age. Reading the repository would have proved the constant present and concluded, wrongly, that the error was impossible.

**Generalizable rule.** When a running service contradicts its own source, compare *process start time* against the commit that introduced the symbol before doubting either. And a client that inherits a service's lifetime inherits its staleness: what it owes the operator is not a proxied status code but the age and identity of what it dialled.

### The first real launch failed on the one interaction no test could have had — asking a human a question

**Author.** post-v0.1, first operator attach against a live Hermes gateway

**Evidence.** The first `talaria --record` against a real gateway appeared to hang on "connecting to gateway" and went half-deaf to typing. It was not a network problem: the process had **no TCP socket open at all**, the frame log's header recorded `ws://127.0.0.1:9119/api/ws` with no token, and a `sample` of the process showed a thread parked in a `read()` syscall. The credential chain (`talaria/transport/credentials.py`) had fallen through all four non-interactive levels — no `HERMES_DASHBOARD_SESSION_TOKEN`, no token in the URL, no `~/.talaria/credentials`, nothing remembered — and reached level 5, `getpass.getpass()`. That call happens inside the dial; the dial happens in `on_mount`; `on_mount` runs inside a Textual app that already owns the screen and is reading stdin. The prompt was painted where nothing could show it and blocked a worker thread on a read racing the UI's input driver.

**Mechanism.** KTD11 puts credential acquisition inside the dial for a good reason — a gated `?ticket=` must be minted fresh on every reconnect — and that reasoning is about *frequency*, which is orthogonal to *when the first one happens*. Every test in the repository supplies the credential from an environment mapping, a file, or an injected prompt double, so the suite exercised all five levels and could never once observe that level 5 needs a terminal a running TUI has already taken. The defect lived in the seam between two correct components, and the only witness able to see it was an operator at a real terminal.

**Generalizable rule.** A test double for a human is not a human. Any code path whose contract is "ask the operator" is untested by construction, however green the branch coverage: what is being asserted is the answer, never the asking. Locate every such path and pin its *ordering* against the thing that owns the terminal — priming before the interface starts, and sealing the prompt afterwards, are both assertions about sequence, which is the one property the doubles were silent about.

### The secure route lost to the insecure one on ergonomics, so the fix was ergonomic

**Author.** post-v0.1, after the first operator attach

**Evidence.** R1 asks that a running Talaria's environment carry no credential, and that half cannot hold when the token arrives through `HERMES_DASHBOARD_SESSION_TOKEN`: the kernel snapshots the environment at `exec` and serves it for the process's life. The mitigation — a `0600` file at `<config_dir>/credentials` — existed, was measured, and was still the route nobody used, because Hermes mints the dashboard session token with `secrets.token_urlsafe(32)` at server start and holds it in memory only (`hermes_cli/web_server.py:300`). Every dashboard restart therefore invalidated the file, and refreshing it meant reading the token out of the served page by hand and pasting it at a shell prompt — into shell history, on the way to a file whose whole purpose was keeping it off the process surface. `talaria refresh-credential` now does it in one command that prints nothing secret.

**Mechanism.** The insecure option was not winning because operators disagreed about the risk. It was winning because it was the one that survived a restart without work. A security control whose recurring cost is higher than the alternative's is a control that degrades to advice, and advice loses to a working shortcut every time — especially under the conditions where it matters most, which are the tired, unattended, restart-at-midnight ones.

**Generalizable rule.** When the documented-safe path keeps losing to the unsafe one, measure the *recurring* cost of each, not the one-time setup. If the safe path costs something every restart and the unsafe one costs nothing, the fix is to build the missing tool, not to write a firmer warning. Then say so in the queued decision, because "we removed the practical objection" is a real change to a trade-off that was previously balanced.

### `getpass` does not fail when it cannot hide input — it warns and echoes

**Author.** post-v0.1, found while verifying the fix above

**Evidence.** Running the launcher with no credential and no controlling terminal printed `GetPassWarning: Can not control echo on the terminal`, then `Warning: Password input may be echoed`, then prompted anyway. `getpass.getpass` falls back to `fallback_getpass`, which reads through plain `input()`. On an unattended launch with a token on stdin, that writes the credential into the launching process's scrollback and logs — the exact surface R9 exists to keep it off. Now refused outright via `_has_controlling_terminal()`, which raises `EOFError` and becomes the existing `CredentialError` naming both non-interactive routes.

**Mechanism.** The standard library treats "hide this input" as best-effort and degrades to a visible read rather than failing. That is a defensible default for a password an interactive user retypes; it is the wrong default for a credential a supervisor pipes in, because the degradation is silent to everything except a warning nobody reads.

**Generalizable rule.** When a library's failure mode is "warn and continue" and the thing being continued with is a secret, the warning is not the safeguard — check the precondition yourself and refuse. Then assert the dangerous call was *not reached*, not merely that the operation failed: the first version of this test asserted only that a `CredentialError` was raised, and passed identically with the guard deleted, because pytest's captured stdin makes the fallback fail anyway. A test that cannot tell the guarded path from the unguarded one is evidence about neither.

## 2026-08-03

### Two intermittent failures, same week, opposite causes — which is why neither was guessed at

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), found by CI

**Evidence.** `test_teardown_stops_a_status_child_this_app_does_not_own` and `test_a_card_mounting_into_a_full_region_is_still_recomputed` both failed intermittently on GitHub runners and neither reproduced locally. They look identical from the outside — a slow machine, a timing-sensitive UI-adjacent test, a green developer machine. The first was a **real R36 process leak** in shipped code. The second was a **test sampling one refresh cycle too early** against correct code. The same diagnostic settled both: stop re-running, find the window in the source, and make it deterministic. For the leak, widening the window with `await asyncio.sleep(0.5)` turned a 1-in-13 failure into a 13-in-13 one. For the geometry test, counting the deferrals — two chained `call_after_refresh` calls against a helper that pumps two refresh cycles — showed a margin with no slack, and instrumenting it showed the marking always *does* land.

**Mechanism.** "Flaky test" is a description of a symptom and it silently proposes a cause. Both of these were timing-sensitive; only one was a defect. The tell is not the failure rate, it is what the code does in the window: `aclose` read a field that had not been written yet and concluded there was nothing to clean up, which is wrong at any speed and merely improbable at speed. The prompt test asserted a specific number of refresh cycles, which was never the behaviour under test.

**Generalizable rule.** Never let "it's flaky" stand as the diagnosis. Find the window in the source, widen it deliberately, and see which way it breaks — a real defect becomes deterministic, and a sampling problem stays green with the assertion given more room. Fixing before that measurement is a coin flip between hiding a bug and hardening a test.

### A child is alive before the parent has recorded it, so teardown found nothing to kill

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), found by CI

**Evidence.** `python-check (3.13)` failed on the pull request carrying U10, on a **documentation-only commit**, with `tests/ui/test_teardown.py::test_teardown_stops_a_status_child_this_app_does_not_own` reporting a status child alive after teardown. Not reproducible locally: 12 runs of that test alone and 3 full-suite runs on Python 3.13 all passed. Reproduced deterministically instead, by inserting `await asyncio.sleep(0.5)` between the spawn returning and `self._process = process` — the test then failed every time, on the same assertion, with the same message. Fixed by having `aclose()` wait on a new `_spawn_settled` event; with the fix in place and the artificial half-second window still widened, the test passes. Pinned deterministically by `tests/status/test_process_contract.py::test_aclose_sweeps_a_child_whose_spawn_has_not_been_recorded_yet`, watched to fail with only the wait removed.

**Mechanism.** `asyncio.create_subprocess_exec` forks and execs the child and *then keeps awaiting* while the subprocess transport is wired up. Throughout that tail the child is running — in this case far enough to write its own pid to a file — while the parent is still suspended inside the `await`, so `StatusRunner._process` is still `None`. Because the parent is suspended, other coroutines run: a teardown landing in that window read `self._process is None`, concluded there was no child to kill, and returned. The child leaked. Cancellation could not save it either, since the `finally` that sweeps the group is inside a `try` that begins *after* the spawn, so a spawn interrupted before recording has no sweep at all. The window is real but short, which is exactly why it passed thirteen local full runs and appeared only on a slower CI runner.

**Generalizable rule.** Between "a resource exists" and "the program has recorded that it exists" there is a window, and it is wide open precisely because the recording step sits after an `await`. Any teardown that decides from a `None` — *no handle, so nothing to clean up* — is wrong in that window; it must wait for the acquisition to settle rather than read a field that has not been written yet.

### Two of the daily-driver verdict's weakest rows were weak only because nobody had pushed

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), commit verification

**Evidence.** The verdict document shipped with row 12 marked *measured on macOS only* — "the reader has a `/proc` branch for Linux and it has never executed" — and row 14 marked *CI job declared, not observed*, both filed as real gaps, one of them a P2 in `QUEUED.md`. Pushing the branch and opening its pull request ran seven checks in about two and a half minutes and closed both. `python-check-linux` executed all five process-surface tests on `ubuntu-latest` under Python 3.12 and 3.13, all passing, so `/proc/<pid>/cmdline` and `/proc/<pid>/environ` have now been read against a real process; the `install` job passed on both versions. The same run also put all fourteen pseudo-terminal teardown tests through Linux with no skips.

**Mechanism.** The work was deliberately left uncommitted through build, two adversarial reviews and a fix pass, which is right — but CI is a measurement instrument that only reads when work is pushed, so every claim depending on it stayed unmeasured for the whole unit and got written up as a limitation of the *build* rather than of the *process*. Nobody was wrong; the evidence was simply on the other side of a `git push`. It is worth noticing that the Linux job carries `continue-on-error: true`, so a green tick alone would have proved nothing — the job's own output had to be read line by line, which is what the queued item had asked for.

**Generalizable rule.** Before writing "this has never been measured" into a verdict, ask which of the unmeasured things a push would measure — CI is usually the cheapest instrument available and it is easy to forget it is switched off while a branch sits local.

### A fire-and-forget task that dies leaves an interface that looks perfectly healthy — and there were three of them, not one

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification

**Evidence.** U10 fixed exactly this defect in `TalariaApp._pump`, wrote it up, and shipped the same defect one function away in `begin_live_startup` and `fetch_catalog`. Measured on the real app with a dispatcher whose `call` raises: the startup task finished with a `RuntimeError`, `app.return_code` was `None`, `app.compat` was `None`, `_startup_done` was `False`, the transcript contained no line mentioning any failure, and the app kept running. asyncio printed `Task exception was never retrieved` to stderr, which under a full-screen Textual application goes somewhere nobody is looking. The client was connected, had never run its compatibility check, had never opened a session, and said nothing. Fixed with one shared supervisor, `TalariaApp._supervise`, which reports the failure into the transcript and exits 70; pinned by `tests/transport/test_session_startup.py::test_a_startup_sequence_that_raises_is_named_and_brings_the_app_down` and `::test_a_catalogue_fetch_that_raises_is_named_too`, mutated separately because one fix with two call sites is two mutations (the shared-path rule).

**Mechanism.** `asyncio.create_task` returns a future, and an exception inside the coroutine is stored on that future rather than raised anywhere. If nobody calls `result()` or `exception()` the exception surfaces only in asyncio's garbage-collection warning, on stderr. Every one of these three tasks is held in an attribute and awaited by nothing outside teardown, so all three had the same shape. The reason it was fixed in one and missed in two is that `_pump` failed *visibly* during development — the pseudo-terminal test hung to its timeout — while the other two fail invisibly by construction, which is the whole point.

**Generalizable rule.** After fixing a defect, grep for the *construct* rather than the symptom: every `create_task` whose result nobody awaits is the same bug, and the copies that were never noticed are the ones that fail silently.

### A test whose docstring names four things and collects three is worse than a test that names three

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification

**Evidence.** `tests/ui/test_teardown.py::test_teardown_stops_every_task_talaria_started` opened "The pump, the status loop, the catalogue fetch and the startup sequence." Its task list was `(_pump_task, _status_task, _catalog_task)`. The app it built was given no `StartupSelection`, so `begin_live_startup` returned at its first guard and `_startup_task` was permanently `None` — meaning removing `self._startup_task` from `shutdown_sources`' cancel set left the entire suite green. Three other lines were unpinned the same way and each survived deletion with 858 tests still passing: `status_runner=` in `build_live_app`, `status_interval=` beside it, and `return app.return_code or 0` in `run_live`. All four now have tests, each watched to fail with the line removed.

**Mechanism.** Two separate mistakes reinforce each other. A `for … if task is not None` filter silently accepts a `None`, so a task that never started reads as a task that started and finished. And a docstring is the only place the *intent* is written down, so once it overstates the collection nobody re-derives what is actually covered. The filter now collects by name into a dict and asserts none is missing, so a task that fails to start is a failure rather than a shrug.

**Generalizable rule.** When a test collects a set of things, assert the set is the size you meant — a filter that drops absent members converts "this never ran" into "this passed".

### Two comments about the same teardown line contradicted each other, and the one in the file that owns the behaviour was right

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification

**Evidence.** `TalariaApp.shutdown_sources` said cancelling the status task "is not enough on its own to satisfy R36 … a status command outliving Talaria depended entirely on the tick happening to be idle at exit". `StatusRunner.aclose`'s own docstring, forty lines away in another file, said the opposite: the tick's `finally` runs on cancellation, "so Talaria's own shutdown cannot leave a status child behind even if nothing calls `aclose()`". Measured by removing the `aclose()` call and re-running the pseudo-terminal teardown tests: no status child, no backgrounded grandchild, three times out of three. The runner's docstring was right; the app's comment overstated the line above it.

**Mechanism.** `task.cancel()` schedules a `CancelledError` into the coroutine, and the coroutine's `finally` blocks run as it unwinds. `_run_once`'s `finally` calls `_kill_process_group` *synchronously* before its first `await`, so the group is gone whether or not anything else asks. What `aclose()` actually adds is narrower and worth keeping: it sweeps before the loop reschedules anything, and it covers a tick driven by a task the app does not hold. That is now what the comment says, and `test_teardown_stops_a_status_child_this_app_does_not_own` is the case only `aclose()` can pass.

**Generalizable rule.** When two files comment on the same behaviour, believe the one that owns it — and when you cannot make a line's claim fail, either narrow the claim to what you *can* demonstrate or delete the line.

### A negative assertion with no positive in the same observation is one platform quirk away from vacuous

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification

**Evidence.** `test_talaria_adds_no_credential_of_its_own_to_its_environment` ended in `assert carrying <= launched_with`, where `carrying` is the set of environment variable names that hold the credential. An empty `carrying` satisfies it, so a platform whose environment read came back blind — a truncating `ps`, a hardened kernel, a container without `/proc` — would have passed the test while measuring nothing. Its only safety net was a *different* test in the same file proving the reader works at all. Changed to `assert carrying == launched_with`, which holds on this platform (measured: exactly `HERMES_DASHBOARD_SESSION_TOKEN` and `TALARIA_GATEWAY_URL`) and carries the positive half — the reader saw both names — in the same observation.

**Generalizable rule.** Write set assertions as equality, not containment, whenever the empty set would be a lie: `<=` is the shape an unmeasured measurement passes.

### A test that asserts against an environment block publishes that block to a public CI log the first time it fails

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification

**Evidence.** Two tests in `tests/transport/test_process_surface.py` wrote `assert CANARY_TOKEN in surface.environ` and `assert surface.environ.strip()`. pytest prints the operands of a failing assertion, truncating a long string to its leading characters — so the first red run on a shared machine would put an arbitrary slice of the developer's real environment into the output. This repository is public and its CI logs are public. Fixed by making the raw block private (`Surface._environ`) and exposing `carries()` (a bool), `names_carrying()` (variable *names*, never values), and `environ_is_readable` (a bool); a leaked name is a fact about Talaria, a leaked value is a fact about the machine.

**Generalizable rule.** Whatever a test puts inside an `assert` expression, assume it will one day be printed into a public log — reduce secrets to booleans and identifiers *before* the assertion, not inside its message.

### The verdict document understated its own evidence: two of the twelve "never called" methods had real Hermes answers on disk

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure), adversarial verification

**Evidence.** The daily-driver verdict said "Twelve of seventeen required methods have never been called against Hermes" and marked `session.create` and `prompt.submit` "not verified at runtime". Unit U2's live capture — taken by the TypeScript reference recorder against a real Hermes dashboard on loopback, 46 frames, sha256 `04c556ac…` — contains two JSON-RPC responses. Run through the repository's own `compare_shape` against U3's pins: the first (`session_id`, `stored_session_id`, `message_count`, `messages`, `info`) matches `session.create` with **no drift** and mismatches `session.resume` on seven keys; the second (`status`) matches `prompt.submit` with no drift. The document now says so, with the two qualifications that make it honest — the capture recorded server-to-client frames only, so the method names are read off the shapes and the sequence rather than off a recorded request, and the client that made those calls was not Talaria.

**Mechanism.** The sentence was true of *Talaria* and false as written, and the difference is one word. "Talaria has never connected to a Hermes gateway" survived scrutiny intact; "these methods have never been called against Hermes" did not, because a different client in an earlier unit had called two of them. An under-claim is safer than an over-claim and it is still an inaccuracy, and an inaccuracy in the direction of pessimism is the kind nobody checks.

**Generalizable rule.** Before writing "this has never been done", search the repository for the artefact that would prove otherwise — evidence collected by an earlier unit for a different purpose does not announce itself.


### The status runner leaked one process per tick, for exactly the shape its own comment claimed to have fixed

**Author.** v0.1 milestone-2, unit U10 (daily-driver closure)

**Evidence.** `StatusRunner._run_once`'s `finally` swept the status child's process group only `if process.returncode is None`, under a comment reading *"A command that backgrounds a worker with its pipes redirected (`worker & echo ok`) exits 0 immediately, so every non-timeout path used to return leaving that worker running — one leaked process per tick, forever"*. Measured against that exact shape with the pipes **not** redirected — `sh -c 'sleep 941 & echo marker'`, one `runner.tick()`, no teardown — the worker was still alive after the tick returned. The guard was skipping the sweep every time. Fixed by sweeping unconditionally in that `finally`; pinned by `tests/status/test_process_contract.py::test_a_backgrounded_worker_that_keeps_the_pipes_also_does_not_outlive_the_tick`, watched to fail with the `returncode is None` guard restored (1 failed, and 3 failed with the two pseudo-terminal teardown tests included).

**Mechanism.** Two facts had to be held at once and were not. `sh -c 'worker & echo ok'` exits its *leader* immediately, and asyncio's child watcher reaps that leader within milliseconds — so `returncode` is `0` long before the `finally` runs. Meanwhile the worker inherited the pipes, so `read_capped` never sees EOF and the tick blocks until its timeout. By the time the sweep was reached, its own guard was false. The one existing test that looked at this shape (`test_a_backgrounded_worker_does_not_outlive_the_tick`) wrote `sleep 937 >/dev/null 2>&1 &` — with the pipes redirected away, stdout reaches EOF at once, the tick ends normally, and the sweep that runs *before* the leader is reaped catches the worker. The redirection in the fixture was the difference between the covered case and the leaking one.

**Generalizable rule.** When a guard exists to make a fix safe, write the test for the case the guard *excludes*, not only the case the fix was written for — and if a fixture contains a detail that makes the scenario easier (a redirection, a short timeout, a flush), ask what the same scenario does without it.

### An interpretive comment is evidence about intent, never about behaviour

**Author.** v0.1 milestone-2, unit U10

**Evidence.** The leak above sat under nineteen lines of comment that described the leak accurately, named the command shape that causes it, and then guarded the fix in a way that disabled it for that shape. Nothing in the suite disagreed, because the only test aimed at the shape had redirected the pipes.

**Mechanism.** A comment is written when the author understands the problem, which is usually *before* the last edit. This one survived a later correctness edit — the pid-recycling guard — that silently negated it. A reader auditing this file for R36 would have read the comment, agreed with it, and moved on.

**Generalizable rule.** Treat a comment that claims a behaviour as a claim to be measured, not as a claim already measured. This repository has now had three of these; the rule is cheap and the alternative is auditing by reading.

### A frame source that raises left the interface running over a stream that had ended

**Author.** v0.1 milestone-2, unit U10

**Evidence.** `TalariaApp._pump` re-raised `CancelledError` and let every other exception escape into an `asyncio.create_task` nobody awaits. The task died, Python logged *Task exception was never retrieved*, and the app kept running: transcript frozen at the last frame, every control still live, nothing on screen saying the stream was gone. Reproduced on a real pseudo-terminal with a source that yields two frames and raises; before the fix the run hung until the harness's own 40-second timeout. Fixed by reporting the failure into the transcript and calling `App.exit(70)`; pinned by `tests/ui/test_teardown.py::test_a_failed_stream_is_named_and_closes_the_source` and `::test_an_induced_mid_stream_failure_still_restores_the_terminal`, both watched to fail with the clause reverted to a bare `pass`.

**Mechanism.** The `finally` closed the source and set `replay_complete`, so everything that *inspects* teardown state looked correct. What no observer had was the difference between "the corpus ended" and "the source broke" — one is the ordinary end of a replay and the other is a client that can no longer hear anything. R36 asks that teardown be reachable from an induced failure, and it was not reachable from this one at all.

**Generalizable rule.** A background task that owns the only path for data into the interface must not be allowed to die quietly; either its failure reaches the screen or the process ends. Silent is the one option that looks identical to working.

### The test suite could have attached to the operator's real Hermes gateway, and stopped only by luck

**Author.** v0.1 milestone-2, unit U10

**Evidence.** Wiring the live launcher made `tests/test_cli.py::test_main_exits_zero_on_a_well_formed_invocation` — a test written in U1 that simply asserted `main([]) == 0` — walk KTD11's credential chain and, had it found a credential, dial `DEFAULT_GATEWAY_URL`. On the machine running it, `hermes dashboard` was listening on `127.0.0.1:9119` at that moment (confirmed with `lsof -nP -iTCP:9119 -sTCP:LISTEN`). The run stopped at the interactive prompt because that shell had `HERMES_DASHBOARD_SESSION_TOKEN` unset and no `~/.talaria/credentials` file — verified after the fact, not before. Two fixes: the CLI test now replaces `run_live` with a double, and `tests/conftest.py`'s autouse fixture clears `HERMES_DASHBOARD_SESSION_TOKEN` for every test in the suite.

**Mechanism.** The repository-wide isolation fixture cleared every `TALARIA_*` variable, because it was written to isolate *configuration*. The credential variable is not a `TALARIA_*` variable and is not configuration, so it was never in scope — and it did not matter until a unit turned a previously inert code path into one that dials. The dangerous property of this class of near-miss is that nothing failed: a green suite that attached to a live gateway and created a session would look exactly like a green suite that did not.

**Generalizable rule.** When a unit makes a previously inert path live, re-ask what the *existing* tests now do — the new tests were written with the new behaviour in mind and the old ones were not. And isolate credentials by name, not by the namespace the configuration happens to use.

### R1's environment clause cannot be met by any change to Talaria, and the honest scope is narrower than the requirement

**Author.** v0.1 milestone-2, unit U10

**Evidence.** Measured on a running process built by the real launcher and holding a live credential: argv carries no token, no `?token=` URL and no endpoint (`ps -ww` on macOS, `/proc/<pid>/cmdline` on Linux), and every environment entry carrying the credential is one the process was launched with. The inherited `HERMES_DASHBOARD_SESSION_TOKEN` is visible for the process's whole life. Both halves are pinned in `tests/transport/test_process_surface.py`, including a test that asserts the *failure* so it cannot quietly become true.

**Mechanism.** The kernel snapshots the environment block at `exec`; `/proc/<pid>/environ` serves that snapshot regardless of what the process later does to `os.environ`, and macOS exposes the same through `ps -E` to the owning user. KTD13's rule — credential in the query string, never argv — is about the half a client controls, and R1's wording extended it to a half no client controls.

**Generalizable rule.** When a requirement cannot be met in full, split it at the boundary of what the code controls, prove that half, and file the other half against the operator's procedure — never widen the passing half's wording until it covers both.

### A mutant that survives because a second code path did the work is a redundancy report, not a coverage report

**Author.** v0.1 milestone-2, unit U10

**Evidence.** Reverting the status runner's per-tick group sweep left both pseudo-terminal teardown tests green, because `StatusRunner.aclose` had been widened in the same change and was doing the same job at teardown. Splitting the mutation — tick-only, teardown-only, both — showed the teardown half was reachable from no test the suite could construct: cancelling the status task runs the tick's `finally` before `aclose`'s first `await` returns, so the group is already gone by the time control arrives. The widened branch was removed rather than kept, and the per-tick sweep was pinned by a new unit test that kills its own mutant.

**Mechanism.** Two paths to one effect make each other's tests pass. The mutation table read as "well covered" until the paths were separated, at which point one of them turned out to be code nothing could exercise — this repository's own standing rule against exactly that.

**Generalizable rule.** When a fix touches more than one path to the same effect, mutate each path alone before mutating them together; a mutant that survives alone is either an untested path or an unreachable one, and those need opposite responses.

### A working call is not a working feature: `command.dispatch` serves about a seventh of the catalogue

**Author.** v0.1 milestone-2, unit U9 (slash commands and paste collapse), adversarial closing round

**Evidence.** U9 shipped with `TalariaApp.dispatch_command_live` sending every gateway-owned command through `command.dispatch`, and every test passed, because the loopback stub answered every name the test asked about. Reading the handler at the pin settles it: its last line is `_err(rid, 4018, f"not a quick/plugin/bundle/skill command: {name}")` (`tui_gateway/methods_tools.py:1070`). Parsing the registry at the same pin gives 90 `CommandDef` rows — 34 `Session`, 19 `Configuration`, 19 `Tools & Skills`, 17 `Info`, 1 `Exit` — of which 8 are `gateway_only`. The catalogue builder drops a row when `cmd.name in _TUI_HIDDEN or cmd.gateway_only` (`methods_tools.py:272`) and the four hidden registry names are all inside the `gateway_only` eight (`tui_gateway/server.py:11504`), so a real catalogue carries **82** — and only about 11 of those names are among the twelve `command.dispatch` hardcodes. Hermes's own client calls `slash.exec` first and falls back to `command.dispatch` in the `.catch()` (`ui-tui/src/app/createSlashHandler.ts:147-166`). Fixed by re-encoding that ordering; pinned by `tests/transport/test_commands.py::test_an_ordinary_registry_command_runs_over_slash_exec`, watched to fail with `slash.exec` removed from the call path (12 failures).

**Mechanism.** The stub was built from the *shapes* `command.dispatch` returns, which is the right source for decoding and the wrong source for routing. Nothing in the six result shapes says which commands reach them, so a suite that transcribes reply bodies faithfully can be complete about decoding and silent about reachability. The failure would have been invisible in use, too, until an operator typed `/model`: the catalogue's blank availability marker means "this dispatches", so the listing asserted that most of those 82 rows worked.

**Generalizable rule.** When a client re-encodes a protocol, read the handler's **refusal** paths, not only its success paths — the last line of a dispatcher tells you what it is *not* for, and that is the half a fixture built from success bodies can never contradict.

### An alias the gateway resolves can land on a name the client owns

**Author.** v0.1 milestone-2, unit U9, adversarial closing round

**Evidence.** `resolve_command` consulted the Talaria-local four using the typed name, then resolved the gateway's `canon` map, then dispatched. The registry defines `CommandDef("quit", …, "Exit", cli_only=True, aliases=("exit",))` (`hermes_cli/commands.py:330-331` at `7f4d15515`), so a real catalogue carries `canon["/exit"] = "/quit"`. Measured against the shipped code with a catalogue built as the gateway builds it: `resolve_command("/exit", catalog)` returned `GatewayInvocation(name='/quit', …)`. The operator types the exit command, `quit` goes over the socket, the gateway does not implement it, and nothing exits. Fixed by re-checking the local set against the *canonical* name; pinned by `tests/domain/test_commands.py::test_an_alias_the_gateway_resolves_onto_a_local_name_stays_local` and `tests/transport/test_commands.py::test_exit_leaves_talaria_instead_of_going_to_the_gateway`, both watched to fail with the second check removed.

**Mechanism.** The listing-level protection was real but pointed the wrong way: `decode_catalog` shadows a gateway *row* whose name collides with a local one, so `/quit` never appears twice. An alias is not a row. It lives only in `canon`, so nothing in the listing could shadow it, and the ordering guarantee ("local before catalogue") was stated over the typed name while the dangerous name was the one the catalogue produced.

**Generalizable rule.** A precedence rule has to be applied at every point where the name can change. Resolving an alias *is* such a point, so every check that ran before resolution runs again after it.

### Parametrizing over the constant under test writes a test that deletes its own case

**Author.** v0.1 milestone-2, unit U9, adversarial closing round

**Evidence.** A verifier reported that dropping `/mouse` from `CLIENT_LOCAL_NAMES` left the whole suite green. The fix was a test asserting each of the four names is refused, written as `@pytest.mark.parametrize("name", sorted(CLIENT_LOCAL_NAMES))`. Re-running the same mutation: **still green** — removing `/mouse` from the constant removed the `/mouse` case from the parametrization, so the test that existed to catch the deletion silently shrank by one. Fixed by writing the four names out literally in the test; the mutation then fails.

**Mechanism.** This is the "assertion that cannot fail" that U8's five rounds kept producing, in its parametrized form. It is harder to see than the blank-screen version because the test body contains a real, sharp assertion — the vacuity is in where the cases come from, not in what each case claims.

**Generalizable rule.** A test's *inputs* must not be derived from the thing it is testing. If deleting a value from the code under test also deletes the case that would have caught the deletion, the test is decorative.

### Overriding a Textual handler does not replace the one it overrides

**Author.** v0.1 milestone-2, unit U9 (slash commands and paste collapse)

**Evidence.** `ChatTextArea._on_paste` was added to intercept a large paste. Every paste was then inserted **twice**: `tests/ui/test_composer.py::test_a_several_hundred_line_paste_inserts_without_submitting` measured **798 newlines from a 400-line paste**, and `::test_wide_and_combining_characters_survive_the_round_trip` measured `'端末エミュレータ端末エミュレータ' != '端末エミュレータ'`. The two tests already existed and both caught it on the first run after the override landed.

**Mechanism.** Textual does not resolve a handler through the MRO the way a normal method call does. `MessagePump._get_dispatch_methods` (`textual/message_pump.py:758-798` in Textual 8.2.8) loops `for cls in self.__class__.__mro__` and yields `cls.__dict__.get(f"_{method_name}")` from **each** class, so both `ChatTextArea._on_paste` and `TextArea._on_paste` were invoked for one event. The override called `super()._on_paste(event)` to do the literal insert, and then the framework called the base handler again. The loop's one exit is `if message._no_default_action: break`, which `Message.prevent_default()` sets — so `prevent_default()` is what makes an override *be* the handler rather than an addition to it.

The same class's existing `_on_key` override never showed this, and the reason is worth recording because it is why the hazard stayed invisible: `TextArea._on_key` calls `prevent_default()` itself for an ordinary printable key, so the MRO loop broke before reaching the base handler a second time. `TextArea._on_paste` does not. An override is safe or unsafe depending on what the *base* handler happens to do, which is not a property you can read off your own code.

**Generalizable rule.** In Textual, an `_on_*` / `on_*` override runs *in addition to* every same-named handler in its base classes. Call `event.prevent_default()` when the override is meant to replace one, and never infer from a working override in the same class that the pattern is safe.

### A background call added at mount changed three shared surfaces at once

**Author.** v0.1 milestone-2, unit U9

**Evidence.** U9 made `TalariaApp` read `commands.catalog` once when it mounts in live mode. Nothing about the catalogue was wrong; the *unprompted call* broke 29 existing tests and one production behaviour, in three distinct ways:

1. **Exact-call assertions.** Twenty-five tests in `tests/ui/test_prompts.py` and `tests/ui/test_live_wiring.py` assert `dispatcher.calls == [(method, params)]`. Every one now had `('commands.catalog', {})` at index 0.
2. **A one-shot gate in a test double.** `HoldingDispatcher` parks its *first* call so a test can observe the window while an answer is in flight. The catalogue fetch became the first call and consumed the gate, so four in-flight tests ran with the window already closed — they failed loudly here, but the failure mode of a test that keeps passing over a consumed gate is silent.
3. **A stolen notice bar.** `tests/transport/test_reconnect.py::test_a_local_credential_problem_is_not_reported_as_a_gateway_rejection` went red because the composer showed `commands.catalog was not sent — not connected to a gateway` where it had shown `set HERMES_DASHBOARD_SESSION_TOKEN`. The background read overwrote the one line that told the operator what to do, with a line naming a *symptom* of that same problem.

**Mechanism.** All three follow from the same thing: a call nobody asked for shares every surface with calls somebody did — the call log, the double's ordering assumptions, and the single-line notice bar. The first two are test-side and the third is real: precedence on a one-line surface is decided by arrival order unless someone decides it deliberately, and background work always arrives last.

A fourth, separate defect came from the same change and is worth its own note: issuing the fetch from `on_mount` is too early against a real socket. `LiveSource` dials asynchronously, so the call resolved `not connected` before the handshake completed and the listing stayed unavailable for the whole session. The fix makes the fetch idempotent and re-runs it whenever the transport reports `connected` — one path covering the mount case (a dispatcher double is usable immediately) and the socket case (the connect callback retries).

**Generalizable rule.** Adding an unprompted call to a shared component is a change to every surface that call touches, not just to the feature that wanted it. Before adding one, ask what already reads the call log, what already assumes the first call is the operator's, and what one-line surface it will now write to last.

### The inert-control rule had only ever been checked in one direction

**Author.** v0.1 milestone-2, unit U9

**Evidence.** AE11's rule is that a control which cannot act must say so rather than quietly do nothing, and the suite checks it thoroughly — for *replay*. Adding U9's `/pause`, `/resume` and `/speed` surfaced the mirror case, which nothing covered: in a **live** session, F8, F9 and F10 flipped `ReplayControls` and reported `paused · 1x`. `ReplaySource` is the only consumer of that object (`talaria/replay/source.py:132,136`), and a live session is fed by `LiveSource`, which never reads it. So three bound keys reported success and changed nothing observable.

Fixed by routing all six entry points — the three keys and the three commands — through one `_pacing_refused_live` helper, and pinned by `tests/transport/test_commands.py::test_the_pacing_function_keys_refuse_a_live_session_too` and `::test_a_pacing_control_in_a_live_session_refuses_out_loud`. Removing the mode check fails six tests; verified in a disposable clone.

**Mechanism.** The refusal was implemented as "replay refuses the controls that need a gateway", which is a statement about one mode. The constraint underneath is symmetric — *a control that cannot act in the current mode says so* — and the other half of it had no owner because replay-only controls were never thought of as controls that could be wrong.

**Generalizable rule.** A rule stated for one mode is half-tested by construction. When a rule mentions a mode, write the sentence without the mode and check the other side of it.

### The four commands the gateway advertises and does not implement need two facts to identify

**Author.** v0.1 milestone-2, unit U9

**Evidence.** `commands.catalog` lists `/density`, `/logs`, `/mouse` and `/sessions` from `_TUI_EXTRA` (`tui_gateway/server.py:11514` at `7f4d15515`). No gateway handler implements them; they are handled inside Hermes's own React terminal UI, so Talaria must list them as unsupported rather than dispatch them. Matching on the four names alone is wrong, and the gateway's own code says why: the catalogue builder skips a `_TUI_EXTRA` row whose name collides with a real registry command, with the comment "The registry entry is canonical" and `/sessions` as its worked example (`tui_gateway/methods_tools.py:290-297`). A name-only rule would mark a genuinely dispatchable `/sessions` unsupported the day the registry adds one.

The second fact is the category. The four extras are filed under `TUI`, and **no** `CommandDef` in `hermes_cli/commands.py` uses that category — verified at the pin by counting every three-positional-argument `CommandDef` call: 33 `Session`, 17 `Info`, 12 `Configuration`, 1 `Exit`, and zero `TUI`. Requiring name **and** category `TUI` is therefore precise where either half alone is not. Pinned by `tests/domain/test_commands.py::test_a_registry_command_reusing_a_tui_name_is_not_marked_unsupported`, which was watched to fail with the category clause removed.

**Mechanism.** The catalogue is a merge of three sources — a registry, a config file, and a filesystem scan — and the merge has a dedup rule. Any classifier that reads one field is reading the merge's output while ignoring the rule that produced it.

**Generalizable rule.** When a remote list is built by merging sources with a precedence rule, classify entries on enough fields to reconstruct which source won; a single field describes the entry, not its provenance.

### Four fixes that worked for the first case, and the shape they share

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth and final adversarial round

**Evidence.** Round 4 shipped eight fixes. Two adversarial reviews re-broke them and found that four were complete only for the case they were written against:

- `PromptRegion.reveal_actions` looped over the cards and `return`ed inside the loop body, so it inspected the first card and stopped. With a clarify parked above an approval, the clarify's one-row input was already visible, the scroll was a no-op, and the approval below kept its `waiting for you` title while its four buttons sat at `y=16` against a region ending at row 14. Reproduced: `"once"` **absent** from `export_screenshot()` with the command body, the card border and the title all **present** in the same screenshot, `scroll_offset == (0, 0)`, `max_scroll_y == 3`, and `pilot.click("#choice-0")` yielding `dispatcher.calls == []`.
- The guard that keeps a bridge from writing into the buffer it serves keyed off `verdict.restore`, which is `disposition == "not_sent"` and nothing else. Refused, expired and delivery-unconfirmed `terminal.read.respond` outcomes each still wrote one line into the transcript the read serves.
- `defang`'s table said it was "the Unicode bidirectional formatting set plus the invisible-but-not-formatting characters that share its effect" and held eighteen codepoints. Verified by direct call, `U+E0001` and `U+E0020`–`U+E007F` — the Unicode Tag block, category `Cf`, no ink, and the current standard carrier for text hidden inside a string aimed at a language model — passed straight through, as did `U+2061`–`U+2064`, `U+FFF9`–`U+FFFB`, `U+180E`, the variation selectors and the Hangul fillers.
- `age_out_approvals` removed the prompt from `state.prompts`, and `turn_status` reports `waiting` only while `state.prompts` is non-empty — so the withdrawal sent the turn back to `streaming` and the screen said `working…` about a session Talaria had just stopped offering any way to unblock.

**Mechanism.** All four are the same move: the fix was written against the reproduction rather than against the rule the reproduction violated. "Bring the control back" became "look at the first card"; "a bridge must not write into the buffer it serves" became "the restore branch must not write"; "replace what a terminal would obey or hide" became "replace the characters in the Trojan Source paper"; "a waiting session must not look like a working one" became "a *registered* prompt makes the turn `waiting`". Each is a correct statement about the instance and a strictly narrower statement than the constraint, and the gap is invisible from the reproduction because the reproduction is inside it.

**Generalizable rule.** After a fix passes its reproduction, restate the constraint in one sentence with no reference to the case, then ask which inputs satisfy the sentence and not the code. A fix aimed at a reproduction converges on the reproduction; only a fix aimed at the constraint converges on the constraint.

### Two of eight round-4 fixes were unpinned, and the suite could not tell

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round

**Evidence.** Two round-4 behaviours survived deliberate removal with all 634 tests green. Moving `_age_out_approvals()` from above `_render_tick`'s `if not self._dirty: return` to below it changed nothing the suite could see. Deleting the whole `CommandPanel.Rewrapped` channel — the `post_message` and the handler together — changed nothing either; both reviews found this independently.

The causes are different and both are worth naming. The age-out's ordering was untested because **every stale approval in the suite was stale on arrival**: ingest marks the app dirty, so the first tick after ingest passes the dirty check and withdraws it either way. No test had an approval that went stale while the session sat quiet, which is the only way it ever happens in a real session — and the only arrangement in which the ordering matters. The `Rewrapped` channel was untested because it and `PromptRegion.on_resize` are two triggers for one action, and every test exercised an arrangement where both fired, so the suite proved only that at least one did.

Instrumenting both triggers over a run answered the second question rather than arguing it. Feeding approvals into a 120x40 screen: the first three mounts each produce a region `Resize` *and* a `Rewrapped`; from the fourth on the region has reached its `max-height: 70%` and stops resizing, so `Rewrapped` fires **alone**. That is the third-or-later approval at an ordinary terminal size. Deleting only the `post_message` now fails exactly one test and leaves the resize tests green.

**Mechanism.** A test suite measures the behaviours it has arrangements for. Both gaps are arrangement gaps rather than assertion gaps: no assertion could have caught the age-out ordering without a quiet session, and no assertion could have separated the two reveal triggers without a mount that fires only one of them. Redundant triggers are the more dangerous of the two, because the redundancy makes the suite pass *while the design intent is unrecorded* — nobody can tell whether the second trigger is a considered belt-and-braces or a leftover.

**Generalizable rule.** When two mechanisms can satisfy one requirement, the suite must contain an arrangement in which only one of them can fire — otherwise delete one, because a guard nothing can exercise is a guard nobody can trust. And when a behaviour is driven by the passage of time rather than by an event, the test has to let time pass with nothing else happening; a fixture that supplies an event supplies the wrong clock.

### A withdrawal removes the evidence that the session was blocked

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round

**Evidence.** Measured on the real app with a turn streaming and an approval arriving mid-turn: once `age_out_approvals` fires, `turn` is `streaming`, `pending_prompts` is `0`, the activity line is `working…`, and `working…` is present in `export_screenshot()`. `turn_status`'s own docstring forbids exactly that claim, and KTD5's status contract carries it too — so an external consumer reads `turn='streaming', pending_prompts=0` for a session that may be blocked.

**Getting the severity right mattered more than the finding.** The gateway fails **closed and returns**: `tools/approval.py:4050` yields `"approved": False, "outcome": "timeout"`. So under the default 300-second configuration the agent genuinely resumes and `streaming` is not a lie. The lie is the other case — a deployment that raised its approval timeout above Talaria's hardcoded `APPROVAL_STALE_AFTER`, where Talaria withdraws early, the gateway is still waiting, and `working…` describes a session that will never move. Talaria cannot tell the two apart.

**Mechanism.** The status was derived from the *registry* rather than from the *history*: `waiting` meant "a prompt is registered", so unregistering one asserted "not waiting" as a side effect of forgetting. A derived state that reads only the current collection cannot express "this used to be true and I no longer know", which is the honest answer after any local withdrawal. `SessionState.withdrawn_approvals` carries that third state, and it is spent on the screen rather than on the status document because KTD5 freezes the turn field at four values.

**Generalizable rule.** When a status is derived from a collection, removing an entry silently asserts the negative. If the removal was the *client's own decision* rather than an observation, the negative is unproven — carry the withdrawal explicitly and say "unknown", because "unknown" and "no" are different claims and only one of them is defensible.

### A raw Cf ban is one lookup table; a bidi ban is a reading list

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fifth adversarial round

**Evidence.** `defang`'s table was assembled from the Trojan Source enumeration — bidi overrides, isolates, implicit marks and the common zero-widths — and its docstring claimed the general property. Unicode 15.0 has **170** characters in general category `Cf`; the table covered 18 of them. The gap that matters is the Tag block, `U+E0020`–`U+E007F`: a complete invisible copy of ASCII, which is how hidden instructions are carried into text destined for a language model. On an approval card that means the rendered command and the executed command can differ with nothing on screen to see, which is the exact defect the bidi work was done to close.

The table is now 26 ranges — every `Cf` character plus the variation selectors and the Hangul fillers, which are not `Cf` and draw nothing anyway — expanding to ~430 entries once at import. Round 4's reason for enumerating rather than deriving still holds and is kept: no per-character Python loop on the hot path, no 1.1M-codepoint scan at import. What changed is that the enumeration is now **pinned against `unicodedata` by a test** that walks the code space and asserts the table covers every `Cf` character, so a Unicode release that adds one fails the suite instead of passing silently.

**Mechanism.** The table was built from a threat write-up, and a threat write-up is a list of *examples of a property*, not the property. Copying the examples produces a control that stops the attack in the paper and the next variation of it, and nothing else. The property here — "this codepoint changes the drawing without occupying a cell" — is already computable, which is what makes the derivation-as-test possible.

**Generalizable rule.** When a security control is a list, find the machine-checkable property the list is a sample of and assert the list against it in a test. Enumerate for speed if you must, but never let the enumeration be the only statement of the rule — and never let a comment claim the property while the code holds the sample, because the next reader will trust the comment.

### A retry loop that writes into the buffer it reads, and the render tick that fed it

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round

**Evidence.** `terminal.read` is the one bridge Talaria answers with no human: `_answer_unattended_prompts` runs at the end of every render pass, dispatches an answer for any terminal-read row it sees, and `_answering` — the set that stops a second dispatch — is discarded in a `finally`, so it re-arms the instant the call ends. When that call ended in `not_sent`, `_record_prompt_outcome` took the generic `restore` branch: it put the prompt **back** into the registry and wrote `terminal read not answered — …` into the transcript.

Both halves are the loop. The restored row is dispatched again on the next tick; the transcript line goes into the very buffer the read serves, so each answer is larger than the one before it. Measured with the render loop actually turning: three cycles produced **6** `terminal.read.respond` calls with the answer body growing from **159 to 884 characters**, and at a 10ms coalesce interval **136 respond calls and 137 transcript lines in 400 ms**. Production's interval is 50ms, so about twenty calls a second, unbounded, for as long as the socket is down.

The same function states the rule this branch broke, two branches later: a *clean* terminal-read writes nothing, because "the line would go into the very buffer this bridge serves, which makes the next read differ from this one because of this one". The failure path did not inherit the rule the success path was written around.

**Why three rounds of tests could not see it.** `live_app` sets `coalesce_interval=3600.0` and every test calls `render_snapshot()` explicitly. That is a good decision for assertions about what is on screen after a specific change — it is documented, and it removed three real flakes — and it makes every self-re-arming defect invisible, because the loop needs a second tick nobody fires. The fix's own test takes a 10ms interval and lets forty ticks run.

**Mechanism.** An error path was written by pattern-matching the four bridges that have an operator, and "restore" means "re-offer the control to the human" — which is meaningless when there is no human. Underneath that, the dispatcher was a *level-triggered* loop: it acts on the presence of a row rather than on the event of a row arriving, so any path that leaves the row in place is a retry at the tick rate, whether or not anyone designed a retry.

**Generalizable rule.** A component that both reads a buffer and can write to it must never write to it on its own failure path — the write changes the next read's answer, so the failure is not idempotent and cannot converge. And when a dispatcher fires on *state* rather than on *events*, every outcome of the thing it dispatches has to remove that state; a bound placed on the dispatcher instead treats the symptom and leaves a row on screen that nothing will ever answer.

### The buttons left the screen when the terminal narrowed, and the card went on looking live

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round

**Evidence.** `CommandPanel` recomputes its wrap in `on_resize` and calls `update()`, which grows the widget *after* layout has already placed the card's height. Reproduced by mounting an approval with a 346-character single-line command at 120x40 and calling `pilot.resize_terminal(60, 20)`: the panel went from four rows to seven, the three buttons landed at `y=17` while the prompt region ended at row 14, `"once"`, `"session"` and `"deny"` were each **absent** from `export_screenshot()` while six rows of command body were present, the card's bottom border was gone, and `await pilot.click("#choice-0")` produced `dispatcher.calls == []`.

Stable, not transient — six further renders did not correct it. The recovery existed and was never offered: three `Tab` presses reach the buttons, and `app.prompts.scroll_end()` makes the same click work.

**Mounting fresh at 60x20 is fine.** The buttons land at `y=14`, inside the region, and the click works. The defect is the resize path alone, which is precisely the path `tests/ui/test_prompts.py` had zero occurrences of the word `resize` in: every screenshot in the file was taken at a size the card was *mounted* at.

**Mechanism.** Two correct decisions compose into a failure. The panel must wrap to its rendered width, so it can only know its height after layout; the region must be bounded (`max-height: 70%`) so a queue of approvals cannot eat the transcript. Nothing owned the interval between "the content grew" and "the viewport is unchanged", so the growth went below the fold. `PromptCard` now names its answering control (`action_widget`) and `PromptRegion` scrolls it back into view on both triggers — the terminal resize and the panel's own `Rewrapped` message. It scrolls to the **first** card rather than the last, because `deny all` applies to the whole queue from whichever card carries it, and reaching for the first keeps the oldest command — the one the gateway resolves first — on screen.

**Generalizable rule.** A widget whose height depends on its rendered width has not finished laying out when its parent has, so a bounded scrolling parent needs an explicit "keep the control visible" step. Test it by *changing* the terminal size rather than by choosing one: a card that composes correctly at every size you mount it at can still be broken by every size you resize it to, and the second set is the one an operator produces.

### Bidi overrides and zero-width characters became an attack surface the moment the command was rendered

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round

**Evidence.** `defang`'s translation table covered `range(0x00, 0x20)` plus `0x7F` and nothing else. A command containing U+202E RIGHT-TO-LEFT OVERRIDE survived unmodified into `CommandPanel.rows`, into `export_screenshot()`, and into the transcript's arrival entry; a U+200B ZERO WIDTH SPACE inside `rm -rf /home/build` was invisible on screen and counted as zero cells by `chop_cells`, so the wrap's column arithmetic disagreed with what the terminal draws.

**State precisely what is reproduced and what is not.** That the characters survive unmodified is reproducible and is now pinned. That a real terminal *reorders* the glyphs is not demonstrable through `export_screenshot()`, because an SVG screenshot performs no bidi reordering — the assertion is about the bytes reaching the renderer, not about the picture a terminal would draw from them.

**Mechanism.** The module's docstring enumerated three interpreters between a string and a terminal cell — Rich markup, ANSI escapes, other C0 controls — and all three are found by looking for a *marker byte*. The fourth interpreter is the terminal's own Unicode bidirectional algorithm, which has no marker to look for and is not opt-in. This was harmless while the command was never rendered; the third round's fix, which put the command on the card because the operator could not otherwise see what they were granting, is what turned it into a surface. **A fix that increases what is shown increases what can be shown dishonestly**, and the review of that fix has to include the new surface, not only the defect it closed.

One cost is taken deliberately: U+200D ZERO WIDTH JOINER builds emoji sequences, so agent prose containing one now renders as its component emoji with a marker between them. `defang` is deliberately one function rather than a strict version for commands and a lenient one for prose — two rules means one of them is eventually applied to the wrong string.

**Generalizable rule.** When you start rendering attacker-influenced text that you previously only stored, re-enumerate the interpreters between the string and the screen — including the ones with no escape character, which are the ones a sanitizer written by pattern-matching on control bytes will always miss.

### A count that summed two groups asserted a fate the client could not know, and over-counted when the button was pressed twice

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round

**Evidence.** The third round taught deny-all to report `DenyAllScope.total = len(taken) + len(already_in_flight)`, because reporting only the cards this call cleared said "2 denied" while the gateway's `all: true` swept three. The docstring called `total` "the honest count for the operator: how many approvals the gateway just denied". It is false in exactly the case `already_in_flight` exists to cover: such an approval has its **own** `approval.respond` on the wire, and that respond may carry an affirmative. Reproduced with two calls outstanding simultaneously — `{'choice': 'once'}` and `{'choice': 'deny', 'all': True}` — and a transcript holding two contradictory claims about one command:

```
denied every waiting approval: 2 waiting, the gateway did not say how many it resolved
approval answered: once · command: rm -rf /
```

The same sum over-counted under repetition, because any approval arriving inside a deny-all round trip mounts a card whose only action is "deny all". Reproduced: three approvals, two presses, transcript reading `… 3 waiting` then `… 2 waiting` — **five denials reported for three approvals**.

**Mechanism.** One number was asked to answer two questions — "what did this call decide" and "what will the gateway's flag reach" — and the second question has no answer available to the client at all, because the ordering is resolved at the gateway. Summing them produced a number that was wrong for both. The fix reports them as two clauses and labels the second as undecided; the *first* clause counts only prompts this call removed from the registry, which is also what bounds repeated presses, since a press that removes none is refused.

**Generalizable rule.** A count is a claim. Before summing two groups into one number, check that the same verb is true of both — here "denied" was true of one group and unknowable for the other, and the sum asserted it of both. When a client cannot know an outcome, the honest report is a separate clause that says so, not a larger number.

### The approval card had no timeout, because the gateway announces one for every bridge except that one

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), fourth adversarial round

**Evidence.** Read at the pin (`git -C ~/.hermes/hermes-agent show 7f4d15515:tui_gateway/server.py`, lines 2981-2998): the gateway emits `<bridge>.expire` for exactly four bridges — `secret`, `sudo`, `clarify`, `terminal.read` — and not for approval, which does not use that bridge at all. `tools/approval.py` drops its own entry on timeout and on interrupt through `_drop_entry()` with no emit. Talaria's `_PROMPT_EVENTS` correctly has no `approval.expire`; nothing else aged an approval out either, and `PendingPrompt.opened_at` was recorded and never read.

The harm is not clutter. Reproduced: a stale approval plus a genuine later one both project as `answerable=False` with "more than one approval is waiting…", and `turn` pins at `waiting` — so the operator cannot allow the command they actually want to allow, and the only offered action denies it. **A phantom does not merely persist; it disables the rule that protects the real one.**

**Mechanism.** The registry was built to be event-driven, which is right, and four of five bridges supply the closing event. The fifth was covered by the same code with no closing event in existence, and nothing in the shape of the code says so — the absence of a key in `_EXPIRE_EVENTS` reads as "nothing to do" rather than "nobody will ever tell us".

**Generalizable rule.** When a lifecycle is closed by peer events, enumerate the states the peer never announces and give each one a local rule. And check what a stale entry does to the *rules that read it*, not only to the screen: a phantom in a safety predicate's input is a disabled safety predicate.

### The approval card showed the warning and never showed the command, because the gateway sends both and one of them is always populated

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), third adversarial round

**Evidence.** `approval.request` carries a `command` field and a `description` field, and at Hermes `7f4d15515` both are populated on every gateway approval: `tools/approval.py:3651-3660` builds `{"command": redact_sensitive_text(command), "description": redact_sensitive_text(combined_desc), …}` where `combined_desc` is `"; ".join(desc for _, desc, _ in warnings)` (`:3616`) — the *pattern warnings that triggered the prompt*, not the command. `tui_gateway/server.py:1655-1674` forwards both untouched apart from redaction.

Talaria's registry reduced the pair to one string, `description or command or "approval requested"`, and the card rendered that single string as the whole question line. Reproduced at 80x24 with a payload shaped as the pin builds it (`command: "rm -rf / --no-preserve-root"`, `description: "recursive delete outside the workspace"`): the card read `approval: recursive delete outside the workspace` above four buttons, `"rm -rf"` was **absent from `export_screenshot()`**, clicking `#choice-0` granted it, and the transcript recorded only the description — so the command was not in the audit trail either.

The shipping terminal UI does the opposite deliberately, with the reason in a source comment: `ui-tui/src/components/prompts.tsx:97-99` puts `description` in a one-line header and wraps `command` into the panel body, "the full command must be reviewable before approving", with a `… +N more lines` marker at ten rows (`CMD_PREVIEW_LINES`, `:16`).

**Mechanism.** The fallback chain reads as defensive — prefer the human-readable field, fall back to the raw one — and it is exactly wrong here, because the field it prefers is the one that is always present and never contains the thing being decided. A fallback only degrades safely when the preferred value is a *better* answer to the same question; these two fields answer different questions, so `or` silently picked the wrong one every single time rather than occasionally. The failure is invisible in review because the line it produces is fluent English about the right subject.

Two layout facts came out of the fix and both are the same shape as the earlier zero-height defect. Wrapping to `event.size.width` inside a `Resize` handler wraps to the widget's *outer* width, so Rich soft-wraps every row a second time a few cells from its end: a six-row cap rendered as fourteen shredded rows and pushed the truncation marker off the card. `content_size.width` is the renderable width. And once the card carries a wrapped body, two queued approvals can want more rows than the prompt region may take from the transcript — so the region is a `VerticalScroll`, because a plain container clips against its own edge with nothing on screen to say so, which is the same silent truncation the overflow marker exists to prevent, one level up.

**Generalizable rule.** When a protocol sends two fields and a client renders one, check what the *sender* puts in each at the revision you are reading — not what the names suggest. And never let `a or b` choose between two fields that answer different questions: if both are worth sending, both are worth rendering, and the one carrying the irreversible decision is the one that must not be the fallback.

### An answer in flight is still the gateway's, and the rule that counted approvals was counting the screen

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), third adversarial round

**Evidence.** `approval.respond` takes no discriminator: it pops the oldest entry in the session's queue (`tools/approval.py:2214-2222`). Talaria's safety rule is therefore "refuse to answer while more than one approval is outstanding, and offer deny-all instead", and both consumers — the refusal in `respond_to_prompt` and the unanswerable marking in `prompt_view` — read `SessionState.outstanding_approvals`, which iterated `self.prompts` alone.

`respond_to_prompt` moves a prompt out of `prompts` into `answering` **before** the call goes out, so for the length of one round trip the approval just answered was invisible to the rule that exists to stop a second one being answered. Reproduced end to end with a dispatcher that parks its first call: approval `rm -rf /data` answered; a second `approval.request` (`ls`) arrives inside the round trip and is marked `answerable=True` with full affirmative buttons; the operator answers it; **two `approval.respond` calls are in flight** against a FIFO resolver. When the first returns `not_sent`, `restore_prompt` puts `rm -rf /data` back and the operator's next press lands on the `ls` entry — or, mirrored, a command they denied has already been approved.

This is not a race that needs adverse scheduling. `_spawn_live` runs each respond as its own task precisely so the pump keeps rendering, so the window is one the operator is looking at a live interface in, and the interface invites the second press the moment the first is sent.

The same root cause under-counted deny-all: the gateway's `all: true` resolves **every** entry (`resolve_gateway_approval(..., resolve_all=True)` over `list(queue)`), so one in flight plus two on screen produced `denied every waiting approval: 2 on screen, None resolved` while the gateway denied three — one of which Talaria had separately recorded as `approval answered: once`.

**Mechanism.** "Outstanding" was quietly redefined by the data structure it was read from. The registry has two containers because the *client* needs to know which prompts have a control on screen; the safety rule is about the *gateway's* queue, and those two sets differ for exactly as long as a call takes. Merging them also has to sort — by frame `seq`, not by concatenation — because `answering` holds what was answered most recently, which is routinely older than what is still on screen, and the order is a claim about which command an answer would reach.

Deny-all needed the two questions separated rather than merged: the set it may restore or settle is what *this* call took out of the registry, and the number it reports is every approval the gateway will resolve. A single return value served both and was necessarily wrong for one of them.

**Generalizable rule.** When a safety predicate is named after a state ("outstanding", "pending", "active"), write down whose state it means — yours or the peer's — before choosing which collection to read. A client-side container that empties on send describes the client, and any rule about what the peer still holds must survive the round trip that empties it.

### The one action offered in the dangerous case was the one path that had never been hardened

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), third adversarial round

**Evidence.** The single-answer path had been taught, over two earlier rounds, to read three independent signals before writing a transcript line: the JSON-RPC envelope, U7's delivery table, and the reply *body* (a bridge that already expired answers `{"status": "expired"}`; an approval resolving nothing answers `{"resolved": 0}` — both JSON-RPC successes). Deny-all read none of them. Reproduced by clicking `#deny-all` with two queued approvals: a confirmed `{"status": "expired"}` produced "denied every waiting approval: 2 on screen, None resolved" — the gateway threw the denial away and the interface said it was applied; `NO_REPLY_IN_TIME` and `LOST_WITH_TRANSPORT` produced the same sentence as a confirmed reply, while the single-answer path on the identical outcome correctly said "delivery unconfirmed"; and a missing count reached the operator as Python's `None`, which reads in English as "none resolved" — the opposite of "the gateway did not say".

Deny-all is the **only** action the interface offers once two approvals queue. The design funnels the safety-critical case into the path that was hardened last, and nothing about the code said so: each path looked locally complete.

Fixed with one function, `read_answer`, returning a four-valued verdict both paths switch on, rather than a second correct copy. Twenty-two deliberate defects were injected one at a time into a disposable copy of the tree; every one produces a red test, including the five that classify an outcome differently on the two paths.

**Mechanism.** This is the rule already in this journal — *a sanitizer attached to one selection rule is not a boundary* — with different nouns. Two readings of one question do not stay equal; they drift, and they drift in whichever direction each caller's local logic makes convenient. Here both drifts pointed the same way: toward reporting that a denial had been applied.

The clip is worth recording too. `record_local_note` bounds an entry at `SYSTEM_LINE_CLIP` (120) and marks its cut, and `DELIVERY_NOTES["not_sent"]` is 121 characters on its own — so any headline prefixing it loses the tail. The deny-all line therefore drops the "how many were resolved" clause when delivery is unconfirmed: an unacknowledged call carries no count anyway, and the clause would only push the *reason* past the cut. Which half of a sentence survives a clip is a design decision, not a formatting one.

**Generalizable rule.** After hardening a path, ask which action the interface offers in the case the hardening was for — and check that action's path specifically. A rule enforced at one call site is a property of that call site; make it a function both call sites must go through, and prove the choke point by mutating it and watching both sides go red.

### A negative assertion about the screen is satisfied by a blank screen — and the screen was blank

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), from three adversarial reviews of the unit

**Evidence.** Every prompt control U8 shipped rendered at **zero content rows**. Driving the real `TalariaApp` under `run_test()` and reading `App.export_screenshot()`: for an approval offering `["once","session","deny"]` the question line was on screen and the three labels were each absent, with button content sizes `Size(width=8, height=0)`, `(9,0)`, `(8,0)`; an `Input` reported `(74,0)` and typed text never appeared. Identical at (80,24), (120,50) and (200,80).

The suite was green, and one test was green *because* of the defect. `test_a_hidden_bridge_masks_its_input_on_the_rendered_screen` asserted `CANARY not in export_screenshot()` — a claim an empty screen satisfies trivially. Mutating `password=self.row.kind in HIDDEN_KINDS` to `password=False` left the whole suite passing; so did emptying `HIDDEN_KINDS`, which failed only on `assert kind in HIDDEN_KINDS` — a parametrize literal compared against the constant it imports. Once the layout was fixed, a sudo password and an API secret would have echoed in plaintext with the suite still green.

Two more tests in the same family: `viewport_rows()` was compared against itself (`rows = app.viewport_rows()`, then every expectation moved with it — `return 1` survived, guard included, at `40 > 1`), and the approval click test reached into the DOM for the button and called `.press()` on it, which posts the message whether or not the widget occupies any rows. A real `pilot.click("#choice-0")` produced an empty dispatcher call list.

**Mechanism.** Two independent CSS-cascade faults, both the same misreading of Textual's specificity. Textual's `Button` declares its chrome as `border-top: tall` plus `border-bottom: tall` inside a `&.-style-default` block, and `Input` re-declares `border: tall` inside `&:focus`. Both selectors carry a class or pseudo-class, so they outrank a plain descendant selector: the `border: none` written in `PromptCard`'s own CSS lost the cascade, the two border rows survived, and `height: 1` left a content box of `1 - 2 = -1`, clamped to zero. The same fault in the composer made its editor three rows focused and one row blurred, so the entire stack above it jumped two rows the instant focus moved — which is why a real mouse click missed: traced directly, the buttons sat at `y=15` for the `MouseDown` and `y=17` for the `Click`. `compact=True` is the framework's own answer; its `-textual-compact` rules use `!important` and do win.

The testing mechanism is the more general one. A negative assertion about a screen — "the secret is not visible", "the value does not appear" — carries no information on its own, because the emptiest possible screen satisfies it. It only becomes evidence when something in the same test proves the screen was rendering at all. Twenty-two deliberate defects were injected one at a time into a disposable copy of the tree; before the rewrite the mask test caught none of the four that affect masking, and after it catches all four, because it now asserts one mask glyph per character of the value rather than the absence of the value.

**Generalizable rule.** Pair every "X is absent from the screen" with a "Y is present on the screen" in the same test, and make the positive assertion one the defect would break. The same rule with different nouns: assert against a *rendered* observation, never against the widget tree — `query_one(...).press()` proves a message was posted, `pilot.click(...)` proves the operator could have posted it. And when a value is derived from the layout, pin it to a literal at a named terminal size plus a second size, so no constant can satisfy it.

### Two of U8's own tests could not fail, and both compared a thing with itself

**Author.** v0.1 milestone-2, unit U8 (blocking prompts), from the unit's own mutation sweep

**Evidence.** Thirty-five deliberate defects were injected one at a time into a disposable copy of the tree, each one the exact fault a U8 test exists to catch. The first pass of nineteen left three survivors; two of them were defects in the *tests* rather than gaps in the sweep, and both are the shape this suite keeps producing:

1. **`test_each_bridge_answers_with_its_own_method_and_field` compared the mapping with itself.** It asserted `sent[0]["params"][RESPOND_VALUE_FIELDS[kind]] == answer` — reading the expected field name out of the same table the sender used to build the frame. Renaming `secret.respond`'s field from `value` to `answer` renames it on both sides: the gateway would receive a password in a key its handler never reads, `_respond` would store the empty string, and all four parametrizations stayed green. Fixed by transcribing the method and field as literals in the parametrize table, from the gateway's own handler registrations.
2. **`test_a_terminal_read_with_no_window_answers_the_visible_screen` compared the response with itself.** `answered["end"] == answered["total_lines"]` and `answered["start"] == max(0, answered["total_lines"] - rows)` are both computed from the answer, so a client that read "no arguments" as "from line 0" satisfies neither of the two clauses that matter. Fixed by computing the expectation from `transcript_view(app.state)` and adding the discriminating assertion — `start > 0`, which is false exactly when the whole scrollback was served.

The final sweep is thirty-five mutations against forty-two tests with no survivors, and every test that no mutation had reached was given one rather than assumed to hold — three were, and all three went red. A third survivor from the first pass was a genuine weakness of a different kind: `test_an_unavailable_projection_sends_nothing_and_says_so_locally` asserted that the failure line still contains "projection is unavailable" after `scrub_urls`, and the *constant Talaria prepends* already contained that phrase — so the scrub could eat the entire exception message and the assertion held. Worse, it was eating it: the combined line was 146 characters against `SYSTEM_LINE_CLIP`'s 120, so the exception's own words were being clipped away in the passing case too. Fixed by shortening the constant to a prefix that does not repeat the reason, and asserting on a phrase only the exception supplies.

**Mechanism.** All three have one root: the test took its expected value from something downstream of the code under test. A shared constant, the response object, and a prefix the production code adds are all "downstream" in the sense that matters — a defect propagates into them, so the comparison is satisfied by construction. The reason this is hard to see in review is that each looked like *avoiding duplication*, which is normally the right instinct; in a test the duplication is the assertion.

**Generalizable rule.** In a test, write the expected value out by hand from the authority — the spec, the source you are re-encoding, the protocol document — never by reading it from the code, from a shared table, or from the answer. If deleting the implementation would also delete your expectation, you have written a tautology. And when a test asserts that a message survives redaction, assert on words only the *original* message carries: a phrase the production code prepends is not evidence about anything.

### The in-flight guard was invisible until the renders were made consecutive, and the wire was never the thing it protected

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)

**Evidence.** Terminal-read is dispatched from the render pass, so an obvious defect is answering the same blocking question once per 50ms tick until the reply lands. `TalariaApp._answering` guards that, and the first test written for it — four renders with `await asyncio.sleep(0.01)` between them, against a stub holding its reply open — passed identically with the guard deleted. Measured directly instead of inferred: with a sleep between renders, **1 dispatch guarded and 1 unguarded**; with the renders consecutive, **1 guarded and 4 unguarded**. The sleep let the spawned answer run, which cleared the prompt from the registry, which removed the row from the next projection.

**Mechanism.** Two guards sit on this path and they protect different things. The registry is what protects the *wire*: `respond_to_prompt` clears the prompt before the call goes out, so a duplicate dispatch is refused before it reaches a socket — under any schedule, which is why "exactly one respond on the wire" is an invariant rather than a race outcome. What `_answering` adds is that the duplicate is never attempted, and its observable trace is `rejected_responses` plus the "that prompt is no longer waiting" notice a self-inflicted refusal puts on screen. The unguarded run ends with three such refusals and that sentence in the composer, for a race Talaria caused itself.

Consecutive renders are not a contrivance either. The 50ms tick is not the only caller — `drain` and the U5 gate's forced checkpoints call `render_snapshot` directly from another task, which is the same arrangement that produced U7's render-lock defect.

**Generalizable rule.** When two guards cover one path, find out which of them your test is actually exercising before you believe it covers either. The way to find out is to delete each in turn and *count* the difference, not to check whether the assertion still holds — an assertion satisfied by the other guard is indistinguishable from one satisfied by the guard you meant to test. And when a test's precondition is a race, the thing that removes the race is usually the innocuous-looking `sleep` somebody added to make it reliable.

### `rejected_responses` was computed and thrown away, and only the assertion written first noticed

**Author.** v0.1 milestone-2, unit U8 (blocking prompts)

**Evidence.** `respond_to_prompt` returns `(new_state, accepted)`, and the refusing branches increment `rejected_responses` on the state they return. `TalariaApp.respond_live` called it, read `accepted`, and on a refusal returned early — discarding the returned state. The refusal worked: nothing was sent, and the notice appeared. The counter never moved, from any path, ever.

**Mechanism.** The counter is the only externally visible trace that the guard fired at all — the observable behaviour of a correct refusal and of a guard that silently does nothing are the same screen. So the one signal that could distinguish them was being dropped by the caller, in a function whose early return looked obviously right. It was found because the test asserted `rejected_responses == 1` before the handler existed; a test written after the code would have asserted on the notice, which was correct.

**Generalizable rule.** A function returning `(value, flag)` has two results, and a caller that reads only the flag has silently decided the value does not matter. Where the value carries a counter, that decision retires the counter. Assert on the counter, not only on the visible effect — the visible effect is what the code was written to produce, and the counter is what tells you the code took the path it claims.

### The "flaky test" was an unserialized renderer, and three plausible reproductions passed before one worked

**Author.** v0.1 milestone-2, unit U7 (found while running the project check)

**Evidence.** `tests/ui/test_transcript_bounds.py::test_mounted_widgets_stay_under_the_cap_while_content_stays_reachable` failed on `pane.rendered_lines == view.lines[pane.condensed_count:]` with **one line of skew** — `'line 38.3' != 'line 38.4'` at index 30 — in 2 of 12 paired runs and 2 of 9 whole-suite runs. The file already described the symptom as a test-harness timing problem: the `_drain` helper's docstring says the accounting assertions "fail roughly one run in three, and only under whole-suite load", and `_drain` forces a flush to suppress it.

**Mechanism.** Not a test problem. `TalariaApp.render_snapshot` has two kinds of caller, and only one of them is serialized. The coalescing timer runs on Textual's message pump and never re-enters itself; forced flushes — `TalariaApp.drain`, and every checkpoint in the U5 gate — run on whatever task called them. `TranscriptPane.apply` is a read-modify-write over `_top` and `_stable` spanning several awaits, so a forced flush that starts while a timer-driven pass is inside `apply`, over a *different* projection, leaves the pane holding a window the projection does not have. One line of skew is exactly what a single interleaved mount produces. Fixed with an `asyncio.Lock` around `render_snapshot`, uncontended in the ordinary path. Rate after: 12 of 12 clean.

**Three reproductions passed against the unfixed code before one failed**, and each failure taught the same lesson in a different disguise:

1. `asyncio.gather(render_snapshot(), render_snapshot())` — the first render stores its snapshot *before* it awaits, so the second finds `changed` empty and returns without entering `apply`.
2. The same, with the domain state advanced between them — better, but the first render had no mounting work (the pane was already current after `_drain`), so `apply` returned without ever yielding and the first finished before the second could enter.
3. Only when **both** renders had real mounting work, and the second started while the first was demonstrably parked inside `apply`, did the overlap appear: measured depth 2 unfixed, 1 fixed.

**Generalizable rule.** When a test is called flaky and the harness already carries a workaround for it, treat the workaround as the report of a defect nobody finished diagnosing. And a concurrency reproduction that passes is not evidence of correctness until you have measured the interleave you were trying to cause — count the overlap, do not infer it from the assertion.

### A guard against identifier reuse is untestable if the identifiers can never repeat

**Author.** v0.1 milestone-2, unit U7 (live transport)

**Evidence.** KTD13 requires RPC replies to be correlated by `(connection epoch, request id)` rather than by request id alone, so a reply arriving late from a socket already declared dead cannot satisfy an identifier minted after reconnect. The obvious implementation pairs that key with a process-wide monotonic id counter — and with a monotonic counter the guard can never fire, because no id is ever reused. `talaria/transport/rpc.py` therefore restarts the id counter on every `open_epoch()`. `tests/transport/test_rpc.py::test_a_stale_epoch_reply_cannot_resolve_a_reused_identifier` asserts `second.id == first.id` before it asserts anything about the guard, and both epoch tests were confirmed to fail against a deliberately-broken correlator keyed on id alone.

**Mechanism.** The failure the epoch key prevents needs three things to coincide: a call in flight when the socket drops, a reconnect, and a *reused* identifier. A monotonic counter removes the third, so the code looks correct, the guard looks tested, and the guard is dead. Restarting per connection is also what a freshly dialled client would naturally do — one connection is in flight at a time — so the choice that makes the race reachable is the same one that matches the wire.

**Generalizable rule.** When a guard depends on a precondition your own code can make impossible, make the precondition *reachable on purpose* and pin it with an assertion inside the test — otherwise the guard is decoration and its test is a tautology.

### An RPC's "unknown" outcome has to reach the transcript as three different renderings, not one

**Author.** v0.1 milestone-2, unit U7 (live transport)

**Evidence.** AE8 says a call interrupted by a disconnect is marked unknown rather than successful. Applying that literally at the UI produced two defects, both caught by writing the transcript assertion before the handler.

1. **An unconfirmed submit written plainly is a lie; written not at all it is a different lie.** If Talaria writes the operator's line as though it were delivered, the operator believes it was; if it writes nothing, the agent may answer a question that is nowhere in the transcript. `record_submission(..., confirmed=False)` writes the line *and* a separate `system` entry saying delivery is unconfirmed, and the composer is cleared — because leaving the text in the composer as well invites a resend, and a resend of a message that did arrive makes the agent do the work twice.
2. **An unconfirmed interrupt must not apply the cancelled state.** `cancelled` is sticky in this domain (it suppresses later deltas until the next `message.start`), so optimistically cancelling on an unknown outcome would silently swallow the rest of a turn that never stopped. Verified: forcing `interrupt_live` to cancel unconditionally fails `tests/transport/test_reconnect.py::test_an_interrupt_only_cancels_the_turn_when_the_gateway_agrees` with `assert 'cancelled' == 'streaming'`.

**Mechanism.** "Unknown" is not a severity between success and failure; it is a statement about *what is knowable*, and each caller has a different safe action under it. Submit's safe action is "record it and mark it"; interrupt's safe action is "change nothing and say so". A shared `if not outcome.ok:` branch would have given both the same one.

**Generalizable rule.** Where a three-valued outcome meets user-visible state, write the assertion about what the screen may claim before writing the handler — the second and third values are where the design decisions are, and a two-branch handler will quietly assign one of them to the wrong side.

### A pseudo-terminal test for "does not echo" fails two ways before it works

**Author.** v0.1 milestone-2, unit U7 (live transport)

**Evidence.** R9 requires the interactive credential prompt not to echo. The weak version of this test asserts that the module references `getpass`; it passes against an implementation that references `getpass` and then calls `input()`. The real version drives a pseudo-terminal and searches the master side for the typed credential — and it took two corrections to become correct:

1. **`subprocess.Popen` with the pty slave as stdin hangs.** `getpass.getpass` opens `/dev/tty` first, and a child that is not a session leader resolves that to the *test runner's* controlling terminal, where the typed credential never arrives. `pty.fork()` makes the child a session leader with the pty as its controlling terminal, which is what makes `/dev/tty` the right device.
2. **Typing before the prompt appears proves nothing.** Until `getpass` clears `ECHO` through `termios`, the line discipline is still echoing, so an early write is reflected by the *terminal* and the assertion fires against the wrong cause. The probe waits for the prompt, then types.

Confirmed discriminating: replacing the prompt with `input()` makes `tests/transport/test_attach.py::test_the_interactive_prompt_does_not_echo` fail with the credential visible in the terminal transcript.

**Generalizable rule.** For a security property that a real device enforces, test against the real device — and when the test hangs or passes trivially, suspect the test harness's relationship to that device before suspecting the code.

### A rate measured over a 61-millisecond window is not a rate

**Author.** v0.1 milestone-2, unit U7 (live transport)

**Evidence.** AE16 asks the live path to meet KTD14's streaming thresholds on its own measurements. Firing 2,000 delta frames into the socket as fast as loopback allows produced **two** coalescing flushes in 0.0616 seconds, which divides to "32.5 flushes per second" against a ceiling of 25 — a failure reported by a measurement with no information in it. KTD14 specifies a fixed 60-second window for exactly this reason. The test now paces the frames over 1.5 seconds (shorter than KTD14's window, long enough that the 50ms coalescing boundary has fired ~30 times), asserts the elapsed window is at least that long before dividing, and separately asserts the coalescing property directly: 2,000 deltas produced fewer than 200 renders.

**Mechanism.** A ratio inherits the sampling error of its denominator. Below a few multiples of the timer period, the numerator is a small integer and one extra flush moves the result by tens of percent. Pacing the input also matches what the gateway actually does — Hermes coalesces per-token frames on a ~33ms timer of its own (`tui_gateway/ws.py`) — so the unpaced version was measuring loopback throughput, not the renderer.

**Generalizable rule.** Before asserting a threshold on a rate, assert that the measurement window is long enough for the rate to mean something; a threshold test that can be failed by scheduling jitter is a flake with a story.

### The diagnosis in the defect report was wrong, and both proposed fixes followed from it

**Author.** v0.1 milestone-1, closing the U5 gate failure

**Evidence.** QUEUED.md carried a P0 saying `TranscriptPane` desynchronizes because "a transient notice line appears mid-transcript and later disappears", and offered two remedies: reconcile the full window, or make notice lines non-transient. Replaying the stress corpus through the reducer and simulating the pane's index arithmetic in plain Python found 15 incidents where a line below the pane's stable floor changed. **All 15 were of one class, and it was neither of the two the report described: zero committed lines ever changed.** The first is frame 31, where the floor stood at line 1 while **zero** entries had been committed — the floor was entirely inside the streaming block.

**Mechanism.** The domain transcript is strictly append-only and entry text is immutable, so no line ever disappears. What moves is the **provisional streaming block**, which `transcript_view` places *after* the committed lines. Committing an entry while a turn is still streaming pushes every provisional line down by the length of that entry. The pane recorded its scan floor at the true divergence point, and two consecutive snapshots agree on a provisional line whenever the streaming text did not change between them — constant with multi-line streaming, since each delta only rewrites the last line. The floor advanced on that coincidence, onto lines that were about to move, and nothing looked at them again.

Neither proposed remedy would have worked. Reconciling the full window costs O(transcript) per 50ms tick, which is the cost KTD14 exists to bound. Making notice lines non-transient fixes nothing, because the notice lines were never transient — they are ordinary committed entries.

The fix is for the projection to publish the boundary rather than have the renderer guess it: `TranscriptView.committed_lines`, with `self._stable = min(stable, view.committed_lines)`. Truncation still uses the true divergence, so a streaming delta churns one widget rather than the whole block.

**Generalizable rule.** A defect report's *measurement* is evidence; its *mechanism* is a hypothesis, and the remedies it proposes inherit whatever is wrong with the hypothesis. Reproduce the mechanism before implementing the fix, especially when the report is your own.

### Correct reconciliation exposed two defects the incorrect one had been hiding

**Author.** v0.1 milestone-1, closing the U5 gate failure

**Evidence.** With the floor clamp in place the 600-frame test passed and content loss went to zero — and the full-scale gate still returned `fail`, on two checks that had been green before the fix:

| check | before the fix | after the floor clamp | threshold |
| --- | --- | --- | --- |
| peak mounted widgets, stress | 501 | **667** | ≤ 600 |
| content loss, stress | 0 of 11 | **1 of 11** | 0 |

At the failing checkpoint the pane reported **7,493 lines condensed out of a transcript that had only ever contained 4,454**.

**Mechanism.** Two independent defects, both masked by the first one.

1. **A window position was being inferred from an eviction tally.** `_condensed_count` incremented on every left-hand eviction and was *also* used as `_top_index`, the absolute index of the first mounted line. Those are the same number only if no line is ever evicted twice. Correct reconciliation evicts twice routinely: the provisional block is dropped from the right and re-derived, so lines cross the left edge again. The tally then exceeded the number of lines that had ever existed, the window sat at an index the projection does not have, and the pane rendered a wrong slice of a correct projection. The incorrect floor had hidden this by never re-deriving the block. Fixed by tracking `self._top` directly and deriving `condensed_count` from it — a position, which can fall when the window is re-derived further up, where a tally cannot.

2. **The cap was enforced after the mount, not before.** `apply` mounted the new batch and then trimmed, so the transient peak was `existing + batch`. With the floor wrong, batches were small; with it right, a tick that re-derives the whole provisional block mounts it in one go — 667 widgets against KTD14's ceiling of 600. Fixed by condensing from the top *before* mounting, so `len(current) - self._top <= mount_cap` holds at every instant. The pane's own test constant relaxed the bound to `2 * cap + 1` to accommodate the old order; it is now `cap + 1`, which is the claim actually being made.

**Generalizable rule.** When a fix makes previously-green checks go red, the first hypothesis should be that the checks were green *because* of the bug, not in spite of it. A defect that suppresses work also suppresses everything that work would have exercised.

### The fix for a credential leak corrupted the artifact, and the check that would have caught it could not reach the case

**Author.** v0.1 milestone-1 integration, from external review of the redaction boundary

**Evidence.** Closing a URL-userinfo leak by writing the redaction marker into the userinfo position produced `[redacted]@host`, which does not parse:

```
>>> urlsplit("http://[redacted]@cdp.example/")
ValueError: 'cdp.example' does not appear to be an IPv4 or IPv6 address
```

The frame-log header runs through `redact_url`, so a basic-auth endpoint corrupted the header of an entire recording. Separately, the equivalence corpus that is supposed to pin the KTD6 relation contained **zero** frames carrying a URL — 0 of 19 — and `compare_records` compared frame bodies with a flat equality that had no authorized-divergence path at all.

**Mechanism.** Two independent failures that arrived together.

The corruption: a bare `[` at the start of a `netloc` commits Python's parser to an IPv6 literal, and it then rejects the real hostname that follows. `urlsplit`/`urlunsplit` do not round-trip a `netloc` edited by hand — `netloc` has its own grammar, and the sentinel value `[redacted]` violates it. The output was inspected as a *string* (does the secret still appear? no) and never as a *URL* (does it still parse? no).

The unreachable check: the in-body URL redaction added earlier was a genuine KTD6 divergence, enumerated in the module docstring and nowhere the harness could see it. With no URL in any fixture frame, the harness could not exercise the divergence; with no frame-body allowance, it could not have permitted it. Its comment asserting that any frame-body redaction is "unexplained drift" had been false since that commit. The docstring's claim that the relation is "pinned by a test rather than drifting" was untrue for exactly the two entries most likely to matter.

**Fix.** Emit `%5Bredacted%5D`, which parses and is already the on-disk form of a redacted query value; three round-trip cases pinned, including IPv6-with-port. Fixture carries both divergent shapes; the comparator authorizes them by reason with its own independent expectation of a redacted URL, and eight attack cases pin the relaxation so the allowance cannot swallow an arbitrary frame difference.

**Generalizable rules.** Three.

1. *A security fix can be worse than the bug.* A credential in a header is a disclosure; a header nothing can parse is a destroyed recording, in an append-only artifact with no repair path. When hardening something that writes to durable storage, ask what the fix costs if it is wrong, not only what the leak costs.
2. *Anything that rewrites a structured value must be asserted to parse back*, not merely inspected for the absence of the secret. "The credential is gone" and "the result is still a URL" are different claims and the first does not imply the second.
3. *Reachability of a check is its own review target.* This is the fifth instance in one milestone of a check that looked like evidence and was not — the `peak_mounted` identity, `content_is_complete` comparing state to a function of itself, `mounted_count` reading its own bookkeeping, a credential fixture caught by the wrong mechanism, and a corpus with no URLs pinning URL redaction. Different causes, one shape: the check and the thing checked were not independent. Ask what input would make each check fail, then confirm that input is in the corpus.

**Worth recording about how it was found.** The bug surfaced while building a fixture to exercise a comparator change previously priced as too expensive to attempt. The cheap check that was nearly skipped found the expensive defect, and the pricing that justified skipping it was itself wrong. When a cost estimate is the only thing standing between you and a check, verify the estimate.

### A results doc argued against its own headline number, and its table came from a different run than its evidence

**Author.** v0.1 milestone-1 integration, from an external review of the gate

**Evidence.** `docs/analysis/2026-08-03-textual-validation-gate-results.md` published a memory slope of **0.33 MB per 1,000 frames** and extrapolated a million-frame session to "around 330 MB", as the stated input to whether transcript eviction becomes a milestone-3 requirement. Recomputed with the gate's own `_fit_slope` over the published series:

| fit | MB per 1,000 frames | one million frames |
| --- | --- | --- |
| all 12 samples | 0.337 | ~337 MB |
| excluding the final sample | 0.197 | ~197 MB |
| steady state | 0.109 | ~109 MB |

The final step alone contributes 21.31 MB over 2,636 frames — 59% of all growth in 5% of the frames.

Separately, the doc's RSS table read 90.02 → 125.62 while the evidence JSON it cites read 90.66 → 126.72. Two different gate runs, close enough to look like rounding.

**Mechanism.** Two failures that look nothing alike and are both about the relationship between a number and its basis.

The slope: the section *already contained* a qualification stating that the final jump is teardown rather than streaming and that the interesting figure is the earlier samples — then fitted across all twelve anyway. The prose and the arithmetic disagreed, and the prose was right. Nobody caught it because both halves were individually defensible; only reading them against each other exposes it.

The table: a number that cannot be traced to the artifact it cites survives every recomputation, because each side is internally consistent. Recomputing the slope from the doc's own table would have reproduced the doc's own answer and confirmed nothing.

**Fix.** Steady-state published as the headline with all three fits tabulated and the excluded sample named; extrapolation corrected to ~110 MB; table regenerated directly from the evidence JSON.

**Why the direction matters.** Over-reporting growth is conservative for the 300 MB *threshold* — it can only cause a false fail, never a false pass — so the verdict was never at risk, and the instinct is to file it as cosmetic. It is not conservative for the *decision* the number exists to feed: a 3.1x overstatement argues for eviction work the measurement does not support. A figure that is safe for the gate can still be wrong for the roadmap.

**Generalizable rule.** Two checks, both cheap. First, read a document's qualifications against its own headline: when a section explains why a number is misleading and then publishes it, the explanation is usually the correct half. Second, verify that a published figure and its cited evidence are the *same measurement*, not merely that each is individually correct — provenance drift is invisible to recomputation and outlives every review that only checks the arithmetic.

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

## 2026-08-03

### An assertion that cannot fail is the dominant defect shape in this suite, not a footnote

**Evidence.** U7's adversarial verification pass found five tests that pass for a reason other than the one they claim, each proved by breaking the code they nominally cover and watching them stay green:

| Test | What it claims | Why it cannot fail |
|---|---|---|
| `test_the_credential_never_reaches_argv` (`tests/transport/test_attach.py:220`) | the token never reaches a command line | reads the *pytest process's* `sys.argv`, which nothing under test writes. Adding a `dialed_url` field carrying the credentialed URL to `AttachSuccess` passed 99/99. |
| the module's "credential arrives as `?token=` and nowhere else" claim | the credential is in the query and nowhere else | `tests/transport/conftest.py` records only `request.path` and stores no handshake headers. Duplicating the token into an `X-Talaria-Token` header on every dial passed 99/99. |
| `test_reads_pause_at_the_byte_bound_as_well` (`tests/transport/test_reconnect.py:661`) | reads pause on the byte bound | its only assertion, `peak_queued_bytes >= 512`, is already implied by the loop it waited on, because `_enqueue` updates the high-water mark before incrementing `read_pauses`. Deleting `await self._reads_allowed.wait()` failed the *frame*-bound test and passed this one. |
| `test_each_transport_state_renders_a_distinct_line` (`tests/ui/test_live_wiring.py:201`) | four transport states render distinguishably | each expected fragment is a substring of its own state name, and `connecting` is a substring of `reconnecting`. Setting both notices to one identical string passed 99/99. |
| the two `runtime_checkable` protocol tests | a type satisfies `CredentialProvider` / `LiveDispatcher` | `isinstance` against a `runtime_checkable` Protocol is a `hasattr` check. `def acquire(self, a, b, c)` returning a string satisfies the first; `call = 42` satisfies the second. |

Milestone-1 remediated a sixth of the same shape: `test_the_key_name_net_catches_a_credential_nested_at_arbitrary_depth` (`tests/recorder/test_redact.py:192`) was renamed for what it actually pins, its docstring records why the old name misled, and the missing coverage was added beside it as `test_the_deny_set_survives_every_envelope_shape` (`:486`).

**The line numbers above describe the state at discovery.** All five were remediated in the same commit that records this entry, each replacement verified to fail against the defect its predecessor missed: the `sys.argv` sweep became `test_the_attach_outcome_carries_no_credential_anywhere` (traverses every string reachable from the outcome plus its repr; catches the `dialed_url` injection that passed 102/102 before); the query-only claim became `test_the_credential_arrives_in_the_query_and_in_no_header`, backed by a stub that now records every handshake header as a `(name, value)` list — repeats kept, because a dict would drop the duplicate `Cookie` a credential could hide in — and guarded against its own vacuous pass by asserting headers were captured at all; the backpressure test became `test_reads_pause_at_the_byte_bound_and_resume_at_half`, which now fails alongside the frame-bound test when the pause is deleted rather than passing beside it; and the distinctness test split into `test_the_four_transport_states_render_four_different_lines` (no notice empty, all four different, none contained *whole* inside another — containment is the property "connecting"/"reconnecting" violated) plus `test_each_transport_state_names_its_own_condition`.

The sixth, the two `runtime_checkable` protocol checks, is **not** remediated and is not a defect to fix: `isinstance` against a `runtime_checkable` Protocol is a `hasattr` check by language design and cannot verify signatures. The honest remedy is to stop reading those tests as evidence of conformance — they pin attribute presence, which is all they ever could.

**Mechanism.** All six share one structure: the assertion is a consequence of something the test already established, rather than an independent observation of the system. A canary sweep that reads its own process's state; a bound implied by the loop that waited for it; a fragment that is a substring of the name it came from; an equality re-derived from an equality just asserted. None of them is careless — each reads as a reasonable check, which is why they survive review. They fail only under the one question that distinguishes them: *what change to the production code would turn this red?*

**Why it is worse than a missing test.** A gap is visible in a coverage report and reads as work outstanding. An assertion that cannot fail occupies the slot where the real check would go and reports itself green, so nobody looks again. Five of these sat under a suite that a reviewer would describe as thorough — 99 tests over a transport whose headline claims were, in fact, unpinned.

**Generalizable rule.** A test is not finished when it passes; it is finished when it has been observed to fail against a deliberately broken version of the thing it covers. Where that is impractical, say so in the docstring rather than letting the green tick imply it. Name the test for the property it actually pins, not the property you wish it pinned — the U7 sweep was named for a claim three times broader than its one load-bearing assertion.

**The same shape reaches the harness, not just the assertion.** A check that imports the thing it checks must **fail** when the import fails, never skip. The out-of-band script written to verify the `describe_dial_error` fix resolves the function by name across three candidates and exits non-zero when none is found, because the fix was mid-rewrite and a rename was likely: had it died on `ImportError`, the run would have produced no failing case and read exactly like "no leak." A skipped check and a passing check are indistinguishable in a summary line. This is worth applying forward rather than only cataloguing backward — it was caught while the rewrite was in flight, not after a rename silently greened it.

**Three ways a claim presents as verified without being measured**, all three hit during this one review: a claim *adjacent* to verified things (the tree was measured clean, then reported clean minutes later while two agents wrote to it); a claim from a *reliable source* (a peer's finding taken as fact because their previous findings held); and a claim from *read source* — quoting the guard at `redact.py:263-266` and asserting how it behaves at runtime. The third is the most deceptive, because the citation makes it look measured: `urlsplit` turned out to be far more permissive than reading the guard suggested, parsing a trailing sentence into the query string rather than rejecting it. Reading a guard is not running it. A measurement also has an expiry — reporting one without saying when it was taken is the first variant in slow motion.

**And knowing a failure mode by name is not inoculation against it.** A fourth instance arrived inside a message describing the other three: an A/B comparison of wrapped versus unwrapped `git` output, run as two reads at different instants against a tree that two agents were actively writing to, producing a confident report of tool corruption (`1221` vs `1245` insertions, a line count off by one). The moving variable had been identified by name moments earlier, in the same message. Re-run back to back on a static tree, the two agree exactly. The rule that would have caught it is narrower and more useful than "verify your claims": **control the variable you already know is moving** — an A/B across a changing baseline measures the change, not the thing under test.

**A fifth variant, and the one that fooled two of us at once: an instrument that cannot see the variable.** Both the reviewer and the parent watched the tree settle by running `git diff --shortstat`, read a near-static number, and concluded the writing had stopped. `--shortstat` counts **tracked** files only, and the directory absorbing nearly all the work — `tests/transport/`, entirely new — was untracked. The gauge was structurally incapable of registering the change it was being used to rule out, and a blind gauge reads exactly like a stable one. It surfaced only when a test *count* moved (264 → 264 → 268) across runs the insertion count called identical. So: **confirm the instrument can observe the variable before trusting a null reading.** For working-tree state that means `git status --porcelain` including untracked entries, or file mtimes — never a tracked-only diffstat.

### A sanitizer attached to one selection rule is not a boundary

**Evidence.** `build_child_env` (`talaria/status/contract.py`) stripped the credential query from `TALARIA_GATEWAY_URL` inside the loop that forwards the known `TALARIA_*` variables, then ran the operator allowlist loop afterwards with no stripping at all. Because `is_suspicious_key("TALARIA_GATEWAY_URL")` is `False`, an operator who allowlisted that variable got the raw value written over the sanitized one, and the session token reached a spawned child process's environment:

```
allowlist=[]                        -> ws://127.0.0.1:9119/api/ws
allowlist=['TALARIA_GATEWAY_URL']   -> ws://127.0.0.1:9119/api/ws?token=<secret>
```

Reachable today through `talaria replay` with any status command configured. Fixed by moving the sanitizer out of the selection loop into `_sanitize(name, value)`, called from `_maybe_forward` — the one path every forwarded variable takes, whichever rule chose it. Pinned by `test_the_allowlist_cannot_re_forward_the_gateway_url_with_its_token` and a userinfo variant, so the test pins the sanitizer rather than one branch of it; moving the strip back inside the `TALARIA_*` loop fails exactly those two.

**Mechanism.** Two rules selected values for forwarding; only one of them was taught to clean. The cleaning was correct, well-tested, and simply not on the path the second rule took. This is the same shape as the `redact_url` findings in U7 and milestone-1: a redactor invoked where somebody remembered rather than a boundary everything crosses. It recurs because attaching the cleanup next to the code that motivated it is the locally obvious thing to do.

**Generalizable rule.** Put the sanitizer on the single choke point every value crosses, never on the branch that happens to have prompted it — and when auditing, enumerate the *egress surfaces* and ask which cross the boundary, rather than enumerating the sanitizer's existing call sites, which can only rediscover the places already thought about.

### A redaction record must be derived from bytes that changed, never from the decision to inspect them

**Evidence.** `redact_url` compared query keys against `URL_ONLY_DENIED_QUERY_KEYS` case-*sensitively* while `_redact_credential_url` lowercased when deciding whether to fire. So for `?Ticket=<secret>` the rule fired, `redact_url` returned the string unchanged, and `redact_frame` appended `Redaction(path='url', reason='url-credential')` anyway:

```
redact_frame({"url": "ws://h/api/ws?Ticket=CANARY"})
 -> frame unchanged, redactions=[Redaction(path='url', reason='url-credential')]
```

A recorded corpus asserted in its own redaction list that a value had been withheld while the credential sat in the frame body verbatim. `?Token=` was fine — only the two Python-only superset keys had the gap, and only against the shift key.

**Mechanism.** The record was produced by the *decision to redact*, not by the redaction. Those two agree right up until a rule fires and does nothing, which is exactly the case a case-sensitivity bug creates. Fixed on both sides: the key comparison lowercases like every other name rule in the file, `_redact_credential_url` returns `cleaned if cleaned != value else None`, and `redact_frame` will not append a record unless the bytes actually differ — belt and braces, because the corpus is append-only and a false attestation in it cannot be withdrawn later.

**Generalizable rule.** Derive the audit record from the observed change, not from the intent. A claim about what was withheld is worth less than nothing when it can be true of an unmodified value: the corpus's whole worth is that it is a faithful record, and an entry that lies about itself makes every other entry unfalsifiable too.

### A guard whose argument is derived from the value it checks can never fire

**Evidence.** `RpcCorrelator.resolve` exists to discard a reply that belongs to a dead connection, and its docstring names the failure precisely: *"`epoch` is the epoch of the connection the frame was **read from**, supplied by the reader loop. Passing the correlator's own epoch here would defeat the entire guard, which is why it is a required keyword."* The only production caller then did exactly that:

```python
self.correlator.resolve(frame, epoch=self.correlator.epoch)   # source.py, pre-fix
```

So `if epoch != self._epoch` was unsatisfiable for every frame the reader would ever ingest, and `stale_epoch_replies` was a counter production could not increment. Deleting the guard entirely left 85 tests passing; only a unit test that called `resolve()` by hand with a literal stale epoch ever exercised it. Fixed by storing the epoch at dial time (`_dial` keeps `self._connection_epoch`), capturing it beside the connection in `_read_loop`, and passing *that* through `_ingest`.

**Mechanism.** Making the parameter keyword-only was a real precaution against positional-argument confusion, and it worked — the caller supplies it by name, correctly spelled, and still defeats the check, because the value came from the object being guarded. A keyword requirement constrains *how* an argument is passed, never *where it came from*. The docstring's warning was accurate and was not enough, because prose cannot bind an argument.

**A second, related trap in the test.** The end-to-end reproduction could not be produced by hanging up the socket: `_ingest` is synchronous and the reader single-threaded, so nothing between `recv()` returning and correlation can open a new epoch — which is precisely why the defect was invisible. The test therefore performs, in order and by hand, the three correlator operations that `_handle_disconnect` and `call` perform, then has the stub deliver the epoch-1 reply. That is the *state* the guard exists for, reached over the real reader; the docstring says so plainly rather than implying a natural race was reproduced.

**Generalizable rule.** When a guard compares an argument against internal state, check where the caller gets that argument. If it can reach for the same field the guard compares against, the check is decorative — and it will read as covered, because the unit tests that supply the argument by hand all pass. Capture the value at the moment it is still independent (here: dial time) and carry it, rather than re-reading it at the point of use.

### The dialler is handed the only credentialed string in the system, so its exceptions are a credential surface

**Evidence.** `AttachTarget` is credential-free by construction — `from_url` strips the credential query, and `safe_url` additionally routes through `redact_url` so userinfo is withheld too. Exactly one string in the process carries the token: the return of `dial_url(credential)`, built for the call and never stored. It is handed to `websockets.connect`, and `InvalidURI.__str__` is `f"{self.uri} isn't a valid URI: {self.msg}"` — the URI it was handed. `describe_dial_error` interpolated `str(exc)`, so a single `http://` for `ws://` typo put the operator's session token into `LiveSource.last_failure`, the `on_connection` detail, and the composer notice on screen — and re-leaked it on every reconnect attempt, since `_dial` is reached from both the initial connect and the retry loop.

**Mechanism.** The docstring reasoned that the dialler "is never handed anything but the URL", which is true and backwards: that URL is the one credentialed value in the system. Everything the design does right — stripping at construction, redacting the property, never storing the dial URL — makes the exception path the *only* remaining surface, and therefore the one worth checking first rather than last.

**The obvious fix ships a worse bug.** Routing `str(exc)` through `redact_url` looks correct and passes a regression test written against the reproduction, because it does remove the token there. What it also does is delete the diagnostic: `urlsplit` is far more permissive than its scheme/netloc guard suggests, so when the message *starts* with the URL, `parse_qsl` swallows the trailing sentence into the value of `token` and redacting that key takes the explanation with it. Worse, the ordinary failures — `ConnectionRefusedError`, `TimeoutError`, DNS — carry no URL at all, fail the guard, and collapse whole to `[redacted]`. That is every routine "could not connect" message, and it is the common path. Behavior also depends on *where* the URL sits in the sentence, which disqualifies it as a boundary on its own.

The fix is a third primitive, `scrub_urls`, which redacts URL-shaped substrings in place and leaves surrounding prose intact, plus a literal pass over the credential just used. Redaction damaging the artifact it protects has now happened twice in this module — milestone-1's userinfo fix first emitted `[redacted]@host`, which `urlsplit` could not parse, corrupting append-only frame-log headers.

**Generalizable rule.** Test a redaction on both halves, always: the secret is gone **and** the message still says what went wrong. A single-half test certifies the regression. And when auditing, ask which values are credentialed *by construction* and follow those — here exactly one string was, and every surface it touched was a leak candidate.

## 2026-08-11

### A comment saying a claim was replaced is not evidence the code was removed

**Evidence.** An independent review of the unit B4 plan returned `BLOCKED` partly on the finding that
the replay gate's `interface_shows_everything` check "no longer exists", having been "replaced by the
two-part ownership proof (`content_is_complete` and `block_documents_are_owned`)", and cited
`tests/replay/test_gate.py:352-365` for it. That comment block says something narrower:
"`interface_shows_everything`'s original claim was one-line-one-widget, true only while every mounted
unit was a `TranscriptLine`... These tests prove the two-part ownership proof that replaced it." The
function is defined at `talaria/replay/gate.py:996` and called by the gate at `gate.py:1382`, two lines
after `content_is_complete`. Both checks run. A second agent, given only the mechanical job of
resolving every citation in the same document to a file and line, independently reported
`interface_shows_everything` present at `gate.py:996`, which is what settled it.

**Mechanism.** The comment is accurate and the misreading is a reasonable one: "the two-part ownership
proof that replaced it" leaves "it" to resolve to either *the claim* or *the function*, and the
sentence before it makes the claim the nearer antecedent only if read carefully. What made the error
expensive is the direction it pushed — the plan was already under-citing by naming one of the gate's
two settled-transcript checks, and the finding asked for the wrong one to be deleted rather than for
the missing one to be added. Acting on it would have removed a live check's name from the acceptance
evidence of a change that moves exactly what that check measures.

**A second, structural half.** The review ran on a reasoning-tier model and the citation check on a
cheaper one with no latitude to judge anything. The cheap mechanical pass is what caught the expensive
judgement pass's error, because resolving a symbol to a line has a right answer and interpreting a
comment does not. The pairing was chosen for cost, and its actual value turned out to be adversarial.

**Generalizable rule.** A finding that something was *removed* is a claim about a definition, so verify
it against the definition — `grep` for the `def`, and for its callers — never against prose describing
a change to it. Design comments narrate why something is the way it is and go stale in a direction that
reads as deletion. And when a document's factual claims matter, run a judgement reviewer and a
mechanical citation resolver over it separately: the one with no room to interpret is the one that can
falsify the other.

### A plan merged on `main` looks exactly like a plan waiting to be built

**Evidence.** The root session surveyed the release for an idle work slot, found
`docs/plans/2026-08-11-v0-3-unit-b4-unknown-event-flood-plan.md` present on `main` with no open pull
request against it, concluded unit B4 was planned-but-unimplemented, wrote a full implementation brief,
created a worktree and a branch, and launched an agent against it. Unit B4 had already shipped —
`unknown_event_repeats` is on `main` at `talaria/domain/state.py:172` and `platforms.changed` is in the
ambient list at `decode.py:120`, merged as pull request 60 roughly four hours earlier. The child
session was interrupted about ninety seconds in and its worktree and branch removed.

**Mechanism.** Two signals were read as one. *A plan file exists on the default branch* and *no pull
request is open for that unit* are both true of a unit that is finished, and both true of a unit that
is planned and idle — the states are indistinguishable from repository metadata alone. The signal that
does distinguish them is the code, and the signal that would have distinguished them for free is the
release's own child-session register, which already carried a `b4-implement` row naming pull request 60.
The register was read four minutes **after** the launch, while drafting a different edit, and that is
what surfaced the error. The child agent independently reached the same suspicion before it was stopped:
its last line was that `platforms.changed` was already in `_OBSERVED_ON_A_LIVE_GATEWAY` and it needed to
check whether that was on `main`.

**The shape this shares with three other failures in the same session.** A signal whose failure mode is
indistinguishable from success — a lifecycle wait that returns instantly, an empty check rollup that
reads as green, a pane blocked on a permission dialog that is perfectly still, and now an absent pull
request that reads as absent work. In each case the cheap proxy fails toward "proceed", and in each case
the fix was to require positive evidence of the thing itself rather than the absence of its
counter-evidence.

**Generalizable rule.** Before launching work against a unit, prove the unit is unbuilt by finding the
symbol its plan introduces missing from the default branch — not by observing that no pull request is
open. Absence of an artifact about the work is not absence of the work. Where a project keeps a ledger
of what shipped, read the ledger *before* dispatching, not while writing it up: a register that records
outcomes is only a control if it is consulted at the moment a decision is made.

## 2026-08-12

### Seven passing cases cannot tell a live guard from a dead one

**Evidence.** Reviewing a repair to `tests/replay/test_gate.py`, the root session claimed that the
replay gate's middle failure branch — `elif not content_is_complete(app.state, final_view)` at
`talaria/replay/gate.py:1380`, where `final_view` is `transcript_view(app.state)` built six lines
earlier — was dead code that could never fire, and instructed a child session to file it as a defect.
The claim rested on `content_is_complete`'s own docstring, which warns that
`view == transcript_view(state)` "would pass no matter what", plus an empirical check: the call
evaluated `True` across seven hand-built states including empty text, duplicate entries, mixed kinds
and in-flight streaming text.

The claim was wrong and was withdrawn before the filing landed. `content_is_complete` renders each
entry **in isolation** — `transcript_view(SessionState(transcript=(entry,)))` at `gate.py:1041` — and
then requires those lines to appear in order in the aggregate view. Mutating the aggregate projection
to drop its last entry while leaving per-entry rendering correct returned `False`. The branch guards a
real class of bug.

**Mechanism, and why the evidence looked strong.** Every one of the seven cases ran *correct* code.
A guard that is live and a guard that is dead behave identically when nothing is broken — both stay
quiet. Volume of passing cases feels like evidence and is not: seventy would have been no better than
seven. The only observation that separates the two is breaking the thing being guarded, and that
observation was one mutation away throughout.

The docstring compounded it. It warns against `view == transcript_view(state)` — an *equality*
comparison — which is not what the function does internally. Reading the warning as applying to any
call whose `view` argument is `transcript_view(state)` is a plausible over-read, and the surrounding
code invites it: `interface_shows_everything`'s docstring at `gate.py:997-1005` says the call sites
*were* effectively self-comparisons and that blanking the pane "produced a completely blank screen and
a `pass` verdict with zero content loss." Both things are true at once — the branch cannot see the
pane, and it can see an aggregate projection loss — and collapsing them into "dead code" loses the
distinction that matters.

**Generalizable rule.** To claim a check never fires, make it fire. A guard's liveness is only
observable by breaking what it guards, so mutation is not the strongest evidence available — it is the
*only* evidence, and passing cases are not weak evidence of deadness but zero evidence. The same
session had already demanded exactly this of three child reviewers before failing to apply it to its
own finding, which is the more useful half of the lesson: a verification standard held for others and
not for oneself is not a standard.
