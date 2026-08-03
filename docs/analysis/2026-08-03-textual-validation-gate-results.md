# Textual validation gate: results and verdict

Status: `final`
Authority: `evidence`
Date: 2026-08-03

## Verdict

> **This document has been through three verdicts in one day, and the sequence is the most
> useful thing in it.** `pass` on a gate that was measuring itself → `fail` on the repaired
> gate, which found real defects → `pass` on the repaired gate after those defects were fixed.
> Everything below the history is the third run. Nothing from the first run's numbers survives.
>
> **First verdict: `pass`, and worthless.** An adversarial audit found that four of the gate's
> seven measurements were structurally incapable of failing, and proved it by injecting the
> exact defect each check exists to detect. The decisive one: the content-completeness check
> called `content_is_complete(app.state, transcript_view(app.state))` — the projection compared
> against itself. `content_is_complete`'s own docstring warns that this "would pass no matter
> what", and both call sites did precisely it. Making the transcript pane a no-op, so the
> interface rendered *nothing at all*, still produced a `pass` verdict with zero content loss.
> Alongside it: `mounted_widgets` counted the pane's private bookkeeping deque rather than the
> widget tree (a leak of 4,455 widgets, 7.4x the ceiling, reported as 501), and
> `render_ticks_per_second` was counted in a 50ms timer callback, so it was bounded by 20/s by
> construction and could never breach its own 25/s threshold — defeating coalescing entirely
> made the number go *down*.
>
> The checks were repaired to measure what they claim: mounted widgets from `len(children)`,
> renders counted where renders happen, content completeness compared against the pane's
> actually-rendered lines, plus new checks for frame accounting, minimum sample counts, and a
> missing corpus path becoming an error instead of a silent skip. That is 13 checks, not 10.
>
> **Second verdict: `fail`, on three real defects.** All three were in Talaria's own
> reconciliation code, and the third only became visible once the first was fixed:
>
> 1. **The pane's scan floor advanced into the provisional streaming block.** The projection
>    places in-flight streaming text *after* the committed lines, so committing an entry
>    mid-stream pushes every provisional line down. Two snapshots agree on a provisional line
>    whenever the stream did not move between them, and the pane read that agreement as
>    "settled". Measured: **274 lines rendered against 275 projected, misaligned from index
>    251, with one line of real content rendered nowhere at all.**
> 2. **The window's position was inferred from an eviction tally.** One counter served as both
>    "lines folded away" and "index of the first mounted line". Those agree only if no line is
>    ever evicted twice — and correct reconciliation evicts twice routinely. The tally reached
>    **7,493 for a transcript that only ever held 4,454 lines.**
> 3. **The mount cap was enforced after the mount, not before**, so a tick that re-derived the
>    whole provisional block transiently held **667 widgets against a ceiling of 600.**
>
> Defects 2 and 3 had been reported as passing checks before defect 1 was fixed, because
> defect 1 suppressed the work that would have exercised them.
>
> **Third verdict: `pass`, and this time the checks can fail.** All 13. The fixes are
> `TranscriptView.committed_lines` (the domain publishes the settled boundary rather than
> leaving the renderer to infer it), `TranscriptPane._top` as an absolute position, and
> condensing before mounting. Each is pinned by a test verified to fail against the
> implementation it replaced.
>
> Note what none of this says. At no point was there evidence that Textual is unsuitable —
> every defect was in Talaria's own code. What the second verdict said was that the framework
> question was still *open*, because the run that claimed to answer it was measuring itself.

**Pass.** All thirteen threshold checks passed. Textual 8.2.8 is validated as Talaria's
presentation layer for v0.1, and unit U5 of the
[v0.1 prototype plan](../plans/2026-08-02-talaria-v0-1-prototype-plan.md) is discharged.

The verdict is arithmetic. Every check is a number compared against a threshold fixed in advance by
KTD14 (Key Technical Decision 14 — the coalescing-tick and bounded-mount decision) and PC7 (planning
closure item 7 — the pinned baseline, thresholds, corpus and platform matrix). No reviewer, panel,
or observer sign-off gated it, and subjective smoothness was not a pass condition. Reproduce it with:

```
uv run talaria gate --corpus <a frame-log v1 recording> --deltas 50000
```

The command exits `0` on pass and `1` on fail. The full machine-readable record of this run is
[`evidence/2026-08-03-textual-validation-gate.json`](evidence/2026-08-03-textual-validation-gate.json).

*(The plan names this file `2026-08-02-textual-validation-gate-results.md`. It carries the date the
gate actually ran, 2026-08-03, because a results document dated before its measurement is a small
lie about provenance.)*

## What was measured, and against what

The gate replays a corpus through the shipped interface — `talaria.ui.app.TalariaApp`, the same
build the operator runs — with counters attached. There is no separate harness: a gate that
measures a purpose-built rig proves the rig.

Two corpora, three passes.

| pass | corpus | replay speed | what it answers |
| ---- | ------ | ------------ | --------------- |
| stress (unbounded) | synthetic, 53,516 frames | no delay at all | can the reducer and the bounded mount survive a full 50,000-delta stream |
| stress (sustained) | the same synthetic corpus | scaled to a 60-second window | does the renderer keep up while a stream is genuinely still arriving |
| recorded session | a real Hermes session, 5,773 frames | no delay at all | does the interface drive from actual gateway traffic (R30) |

The two stress passes exist because one of them alone would have been misleading. At unbounded
speed the domain reducer drains 53,516 frames in **2.3 seconds** — faster than the 50ms coalescing
tick can fire more than a handful of times. That is a real and good result (it is what "coalescing
is engaged" looks like), but it means the unbounded pass measures the *reducer*, not the renderer.
The sustained pass replays on the corpus's own recorded cadence, scaled so the run occupies about a
minute, and the render loop then runs 2,907 times against a live stream. Reporting only the first
number would have claimed a renderer result on reducer evidence.

### Corpus identity

Corpora are never committed (R29) and never cited by local path — this repository is public. Each is
identified by an opaque label, a sha256, and a frame count.

**Recorded session.** `talaria-live-v1-5773f-88a3604c34b7`, sha256
`88a3604c34b75c40110cf6a7cff4e27aef078fbd3a668223ecfd5cd2b3cef751`, 5,773 frames, frame-log v1,
recorded 2026-08-03T03:50:01Z.

Captured for this gate under the plan's corpus-provenance step, because U2's own capture reached
only 45 frames — two orders of magnitude short of the ≥5,000 the plan requires, and enough to prove
the recorder worked but not enough to replay an interface against. Capture used the existing
TypeScript reference recorder (`src/record/`) attached to a dedicated loopback gateway launched on a
free port with an injected session token, driven through four long streaming turns in a scratch
session. No Python touched a socket, so the replay-first ordering the plan relies on is intact. The
scratch gateway was torn down afterwards and the operator's own profile gateways were untouched.
The log was swept before use: no credential-shaped key appears in it, and the injected token appears
nowhere — the header's `endpoint` carries `token=%5Bredacted%5D`.

Frame mix: 5,718 `message.delta`, 4 complete turns (`message.start` / `message.complete`), 14
`thinking.delta`, 4 `reasoning.delta`, 4 `reasoning.available`, 4 `session.info`, 1 `gateway.ready`,
1 `session.title`, 6 JSON-RPC responses, and 13 frames of type `sessions.changed` — a type Talaria
does not model, so the recorded corpus exercises R5's unknown-event path without anything being
staged.

**Synthetic stress corpus.** `talaria-stress-v1-50000d-seed20260802`, sha256
`34b52ddaba7b33f993ca621aff765f2337c4581d0aef9d2899267b50d3033c0c`, 53,516 frames, generated from
seed 20260802.

Generated rather than committed, for the same R29 reason, and reproducible from the seed alone —
`talaria.replay.stress.build_stress_corpus(deltas=50000, seed=20260802)` reproduces that digest
exactly, which is a stronger provenance claim than a checked-in blob nothing verifies. Contents:
50,000 message deltas across 625 completed turns, one unparseable frame and one unknown event type
per turn, a well-formed-but-not-an-object frame every eighth turn, a three-child sub-agent fan-out
with terminal statuses every fourth turn, and delta text drawn from a fragment set that includes
wide East Asian characters, combining accents, a right-to-left run, an astral-plane symbol and an
embedded tab.

## The measurements

| check | measured | threshold | verdict |
| ----- | -------- | --------- | ------- |
| frames applied vs frames in corpus | 53,516 | = 53,516 | pass |
| mounted line widgets, stress corpus | 501 | ≤ 600 | pass |
| mounted line widgets, sustained pass | 501 | ≤ 600 | pass |
| resident-set growth, full stress replay | 44.3 MB | < 300 MB | pass |
| render ticks per second, maximum speed | 3.50 | ≤ 25 | pass |
| render ticks per second, sustained streaming | 15.28 | ≤ 25 | pass |
| content loss, stress corpus | 0 of 11 checkpoints | 0 | pass |
| content loss, sustained pass | 0 of 11 checkpoints | 0 | pass |
| content loss, recorded session | 0 of 2 checkpoints | 0 | pass |
| memory samples taken | 12 | ≥ 6 | pass |
| content checkpoints taken | 11 | ≥ 5 | pass |
| replay determinism (AE11) | identical | identical | pass |
| mutation controls inert (AE11) | `interrupt`, `submit` refused | both refused | pass |

The last four rows are the checks added when the gate was repaired. Two of them —
"memory samples taken" and "content checkpoints taken" — are floors on *how much was
sampled*, because a check that runs zero times reports zero failures. They are the reason
`run_gate(deltas=600)` in the unit test cannot reach a `pass` verdict at any implementation
quality: a 600-delta corpus at unbounded replay speed drains between two 20ms polls, so the
samples are never taken. The unit test asserts that these two, and only these two, are what
fails at reduced scale.

### Mounted widgets

501 is the designed steady state and, since this run, also the peak: a 500-line cap plus the single
condensed block that stands in for everything older. Across the stress corpus 3,954 lines were
condensed and 4,454 lines existed in total, so the collapse ran continuously rather than once at the
end.

The number is a high-water mark read from the pane's own counter, not a sample. That distinction is
the whole point of the check: an implementation that mounted a whole backlog and then removed the
surplus would satisfy a steady-state cap while briefly holding many times it, and a sample taken
afterwards could not see the difference.
`tests/ui/test_transcript_bounds.py::test_a_backlog_larger_than_the_cap_is_never_mounted_in_full`
pins the distinction by forcing an entire corpus through one render tick.

**Correction 1, 2026-08-03.** The high-water counter did not originally do what the paragraph above
claims. It was updated at the end of `apply()`, *after* the loop that trims back to the cap — so it
could never report a value above `mount_cap + 1` no matter what the pane did in between. The check
read like a measurement with a safety margin (501 against a ceiling of 600) when it was closer to an
identity (500 + 1). The bug was found from the outside: a mid-stream assertion in the resize-storm
test failed on a loaded CI runner at 49 mounted widgets against a test cap of 40, which is a state
the counter said was impossible. Instrumenting `mount_all` directly confirmed a real transient of up
to 51 against that same cap of 40. The counter is now sampled immediately after the mount.

**Correction 2, 2026-08-03, later.** With the peak sampled honestly *and* the reconciliation defect
fixed, the stress pass measured **667** widgets — over the ceiling. Fixing the pane's scan floor
made it re-derive the whole provisional block whenever an entry commits mid-stream, and `apply`
mounted that block before trimming, so the transient was `existing + batch`. The earlier 501 and 507
were low because the pane was skipping work it should have been doing.

The order is now condense-then-mount: the pane drops from the top until
`len(current) - self._top <= mount_cap`, and only then mounts. The count cannot exceed the cap at
any instant, transient or settled, so **there is one bound and it is `mount_cap + 1`** — 501, on
both passes, against the unchanged ceiling of 600. `TRANSIENT_CAP` in
`tests/ui/test_transcript_bounds.py` was `2 * SMALL_CAP + 1` to accommodate the old order; it is now
`SMALL_CAP + 1`, and it is asserted against `peak_mounted`, which is sampled at the moment of
maximum mount. Passing it is therefore a claim about the pane rather than about where the sample sits.

**The accounting is also asserted now, and it was wrong before.** Every line must be either mounted
or condensed, exactly once: `condensed_count + len(rendered_lines) == total_lines`, with
`rendered_lines == lines[condensed_count:]`. It did not hold. `condensed_count` was a cumulative
eviction tally doubling as the window's start index, and on this corpus it reached **7,493 against a
transcript of 4,454 lines** — a window positioned at an index the projection does not have. It is
now derived from an explicitly tracked position, and on this run it is 3,954, which with 500 mounted
lines accounts for all 4,454 exactly.

### Memory

Sampled with `resource.getrusage(RUSAGE_SELF).ru_maxrss` every 5,000 frames, as KTD14 specifies.
The full series for the unbounded stress pass, in megabytes:

| frames | RSS (MB) |
| ------ | -------- |
| 0 | 90.17 |
| 5,504 | 104.06 |
| 11,712 | 106.73 |
| 15,424 | 109.27 |
| 20,160 | 111.83 |
| 25,088 | 112.31 |
| 30,784 | 113.67 |
| 35,008 | 114.12 |
| 40,384 | 114.39 |
| 45,440 | 114.77 |
| 50,304 | 115.11 |
| 53,516 | 134.52 |

> **Corrected twice on 2026-08-03.** First, after external review: this section had published
> the all-points slope of 0.33 MB per 1,000 frames as the headline and extrapolated a
> million-frame session to "around 330 MB", a figure dominated by a single final sample that
> qualification 2 below *already told the reader to exclude*. The section argued against its own
> headline number and then used it anyway. Second, after the reconciliation fix: the numbers
> below are from the third gate run, and they are **higher** than the corrected figures from the
> second. The steady-state slope went from 0.109 to 0.232 MB per 1,000 frames. That is not
> noise. The pane now re-derives the provisional block whenever an entry commits mid-stream —
> real work it was previously skipping because its scan floor was wrong — so it allocates and
> discards more `Static` widgets per tick. The earlier, more flattering number was measured
> against an interface that was losing a line of the conversation.

**Steady-state slope: 0.23 MB per 1,000 frames.** All-points slope, including teardown:
0.48 MB per 1,000 frames. Growth across the replay: 44.34 MB against a 300 MB ceiling.

The gap between those two numbers is the whole of this section. Recomputed with the gate's own
`_fit_slope` over the published series:

| fit | MB per 1,000 frames | one million frames |
| --- | --- | --- |
| all 12 samples | 0.479 | ~479 MB |
| excluding the final sample | 0.372 | ~372 MB |
| steady state (samples 2..11) | **0.232** | **~232 MB** |

The final step alone adds 19.41 MB over 3,212 frames — **44% of all growth in 6% of the frames.**

Three honest qualifications, because this is the measurement most easily over-read.

1. **`ru_maxrss` is a maximum, not a current.** It is monotonic by construction and can never show
   memory being returned. Every number above is therefore an upper bound on what was held, not a
   description of what is held now.
2. **The final jump is teardown, not streaming.** The last step is the final render plus the
   pause-and-measure sequence, not the stream. The interesting figure is the shape of the earlier
   samples, which is close to flat after the first 10,000 frames. This is why the steady-state fit
   is the headline and the all-points fit is shown beside it rather than instead of it.
3. **The sustained pass ran in the same process**, so its series starts at the previous pass's
   high-water mark and its growth is not independent evidence. The same is true of the recorded
   pass, which starts at the sustained pass's mark.

What the slope is *for*: KTD14 bounds mounted widgets, not the domain transcript, and the domain
transcript accumulates without eviction. Extrapolating the steady-state 0.23 MB per 1,000 frames, a
session of one million frames would sit around **232 MB** — inside the 300 MB ceiling, but no longer
comfortably. That is the input to the deferred QUEUED.md item "Bound the domain transcript, not just
the mounted widget count" — the gate's job was to produce the number, not to spend the decision, and
this number argues for that work considerably more strongly than the previous one did.

**Why the direction of each correction matters.** Over-reporting growth is conservative for the
300 MB *threshold* — it can only cause a false fail, never a false pass, so the gate's verdict was
never at risk from the first correction. It is not conservative for the *decision*: a 3.1x
overstatement argues for eviction work that the measurement did not support. The second correction
runs the other way and is the more interesting one: a defect that suppressed real work also
suppressed the memory cost of that work, so the interface looked cheaper than it is. Both are
reminders that a measurement inherits every defect of the thing it measures.

### Render cadence

Counted with a monotonic counter incremented in the coalescing flush callback. At maximum replay
speed: 8 flushes over 2.286 seconds, 3.50/s. Under sustained streaming: 2,907 flushes over 190.2
seconds, 15.28/s — below the 20/s ceiling the 50ms coalescing boundary imposes by construction, and
well under the 25/s threshold.

Read this as a *check that coalescing is engaged* rather than as a performance score. A regression
that rendered once per frame would appear here as hundreds per second; the threshold catches that
and nothing subtler.

### Content completeness

Two claims, checked separately, because they fail separately. **The domain kept the conversation:**
every line of every committed transcript entry appears, in order, in the projection — the check
walks the domain's own entries rather than comparing the projection to itself, and matches whole
lines rather than substrings. **The interface is showing it:** at the settled checkpoint, the pane's
rendered lines must equal the projection's window at the pane's own top index, and mounted plus
condensed must equal the whole transcript. The second is the one that caught the reconciliation
defect, and the one the original gate had no equivalent of at all.

Zero failures across 24 checkpoints in three passes.

This is the measurement that would catch the worst available outcome — a fast, bounded, smooth
interface quietly losing the conversation — which is why it is checked repeatedly during the stream
rather than once at the end.

### Determinism and inert controls (AE11)

The recorded session was replayed three ways: at 64x, at 64x with a pause and resume halfway
through, and unbounded. All three ended in an identical `SessionState`. The comparison is over the
whole frozen dataclass, not a summary — comparing transcript text alone would pass for an
implementation that silently dropped a sub-agent row or miscounted ignored events.

Mid-replay, the sub-agent interrupt control and the composer's Enter-submit were both invoked. Both
refused, both said so on screen, and the composed text was retained. Nothing was echoed into the
transcript: a locally echoed message would be indistinguishable on screen from one that was
actually delivered, which is precisely the confusion the inert-control rule exists to prevent.

## Exercised platform matrix (PC7)

| | |
| --- | --- |
| OS | Darwin 25.5.0 (macOS), arm64 |
| Python | CPython 3.12.11 |
| Textual | 8.2.8 |
| Terminal | none — the gate runs headless through Textual's `run_test()` driver |

**What was not exercised, stated plainly.** PC7's initial matrix names CPython 3.12–3.13, tmux 3.x,
and the operator's terminal host. This gate run measured 3.12 on macOS arm64 only, headless. The
Pilot suites (not the threshold measurements) run on 3.12 and 3.13 on macOS in CI, and
informationally on Linux. tmux and a real terminal host were **not** exercised by this run, and no
claim is made about them; that is a U10 acceptance concern, not a threshold this gate measured.

## What the gate does not prove

* **Not bounded memory as history grows.** It bounds rendering cost. The distinction is KTD14's own
  and is restated here because "bounded" is the word most likely to be quoted without it.
* **Not real-terminal behaviour.** Headless `run_test()` drives Textual's own driver. Bracketed
  paste, wide-character measurement, and IME input are exercised as synthetic events with the real
  widget; they are not exercised through a terminal emulator. Ctrl+J in particular is asserted as a
  synthetic key event — the argument that a real terminal delivers it as a plain line feed (KTD4)
  rests on the protocol, not on this measurement.
* **Not live traffic.** Every pass is a replay. R31 and AE16 — that replay and live sources produce
  identical domain transitions — belong to U7 and are not evidence this gate produced.
* **Not subjective quality.** By design.

## Two framework findings worth carrying forward

Both cost real debugging time and neither is caught by ruff, mypy, or any behavioural test.

**Textual's `App` subclass namespace is a minefield, and a collision does not look like one.** Two
names in this unit collided with the framework: `_flush` (which `App` uses to flush captured stdout)
and `_closing` (an instance attribute `MessagePump` sets in its constructor). Assigning `_closing`
during Talaria's own teardown told Textual its shutdown was already in progress, after which the app
never finished closing — *every* Pilot test hung at the end of its `run_test` block, with a
traceback that named only the asyncio event loop and nothing in this repository. The guard is now
structural: `tests/ui/test_app_shadowing.py` reads the class body and every `self.<name> =`
assignment out of the source with `ast` and fails the build on any collision not listed as a
deliberate override. Source-reading, not `vars()`, because `_closing` is never declared at class
level and a class-dictionary comparison would have missed exactly the name that bit.

**A `Paste` event posted to a widget inserts twice.** Posting `events.Paste` directly to a
`TextArea` in Textual 8.2.8 inserts the text once in the widget's handler and again after the event
bubbles to the `App`, which forwards it back to the focused widget. A real bracketed paste is
delivered to the `App`, so tests must post it there. A test that posts to the widget silently
measures double insertion and would have hidden a real paste defect.

## Traceability

| requirement | evidence |
| ----------- | -------- |
| R3 (streaming) | recorded-session replay; `tests/domain/test_transcript_state.py` |
| R6 (content completeness) | content-loss checks, 0 of 24 checkpoints |
| R10 (bordered composer visible while streaming) | `tests/ui/test_composer.py::test_the_border_is_present_while_the_transcript_streams` |
| R11 (paste, wide and combining characters) | `tests/ui/test_composer.py` AE4 sweep |
| R12 (multi-line entry, discoverable bindings) | `tests/ui/test_composer.py` |
| R14, R16 (sub-agent rows and count) | `tests/ui/test_agent_rows.py` |
| R15 (interrupt inert in replay) | inert-control check; `tests/replay/test_controls.py` |
| R22 (literal rendering, ANSI not interpreted) | `tests/ui/test_status_region.py` |
| R30 (full interface from a frame log, no socket) | `tests/replay/test_source.py::test_the_whole_interface_runs_from_a_file_with_no_socket_opened` |
| R38 (bounded mount, anchors, thresholds) | mounted-widget and memory checks; `tests/ui/test_transcript_bounds.py` |
| R40 (pause, resume, speed; status under replay) | `tests/replay/test_controls.py` |
| AE4 | composer sweep, above |
| AE5 | `tests/ui/test_transcript_bounds.py::test_a_resize_storm_preserves_reflow_anchors_and_content` |
| AE11 | determinism and inert-control checks, above |
| PC1 | KTD4 configuration asserted in `tests/ui/test_composer.py` |
| PC3 | five-field row asserted in `tests/ui/test_agent_rows.py` |
| PC7 | corpus identity, thresholds and matrix, above |
| PC8 | [Python fallback presentation layer](2026-08-02-python-fallback-presentation-layer.md), dated before this verdict |
