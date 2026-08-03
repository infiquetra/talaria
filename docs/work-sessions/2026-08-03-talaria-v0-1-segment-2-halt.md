# v0.1 segment 2 — milestone 1, and the halt

Date: 2026-08-03
Outcome: **halted at the U5 framework gate.** Milestone 1 is built and green; the gate verdict is `fail`.

## What was built

Four units, all complete and tested: the frame recorder and its redaction boundary (U2), the
domain core with the 38-rule Hermes reconciliation catalogue (U3), the KTD5 status-line runner
(U6), and the Textual pilot with the replay source and validation gate (U5). 371 tests, ruff
and mypy clean over 71 files, bandit clean, CI green on all five jobs.

## Why it halted

The gate reported `pass`. I verified that verdict independently — same corpora by sha256, same
numbers, exit 0. Then an adversarial audit asked whether the gate could *fail*, and injected
into each check the exact defect that check exists to detect. Four of seven could not fail. A
completely blank interface passed. Discarding 90% of inbound frames passed. A 4,455-widget leak
against a 600 ceiling reported 501.

The root cause in each case was the same shape: the measurement was taken from inside the thing
being measured. Six of seven numbers were counters the subject kept about itself; only resident
memory was observed from outside. The thresholds themselves were all honest and matched the plan
exactly — which is why this survived review.

The repaired gate then failed on a genuine defect: `TranscriptPane.reconcile` desynchronizes
from the projection when a transient notice line appears mid-transcript and later disappears,
leaving 274 lines rendered against 275 projected and one line of conversation rendered nowhere.

## Decisions taken during the run

Everything below was found by review or by CI and fixed within the milestone.

- **Status runner P0** — the 16 KiB output cap was applied when slicing what `communicate()`
  had already read, so it bounded the display and not the memory: a flooding command took
  resident memory to 3 GB in 2 seconds, every tick. Now read-bounded; +0 MB.
- **Status runner P1 ×3** — leaked worker processes (one per tick, reparented to init), an
  `aclose()` that production never called, and a 2-descriptor-per-tick leak. All fixed and
  measured at zero.
- **Redaction P1 ×3** — a direct probe put credentials on disk three ways: an oddly-cased key
  (`ApIkEy`), a space-separated key (`api key`), and a credential URL under an innocuous key.
  The last matters most: KTD11 puts the attach credential exactly there.
- **ADR-0002 guard P2** — its detection held against every attack; its *file walk* did not.
  A symlinked subpackage was importable and invisible.
- **KTD14 metric** — `peak_mounted` was sampled after the trim, so it could never exceed
  `cap + 1`. "501 against 600" was an identity, not a measurement.

## What the operator has to decide

1. **How to fix `reconcile`.** Either reconcile the full window rather than the suffix below the
   common prefix, or stop making notice lines transient so the append-only assumption becomes
   true. The second is probably cheaper and arguably the right model anyway — a line the
   operator has seen should not vanish. This is U5's design call.
2. **Whether milestone 1 merges before that fix.** The code is green and the fixes are real; the
   only blocker is that ADR-0005 cannot be accepted on a failing gate. Merging the milestone
   while leaving ADR-0005 `proposed` and the defect P0 is defensible, but it is the operator's
   call, not the executor's.
3. **Branch protection on `main`** — still absent, still deliberately not configured unattended.

## What is explicitly not claimed

That Textual is unsuitable. The defect is in Talaria's own code. The framework question is open
because the evidence that closed it was measuring itself, not because the framework failed.
