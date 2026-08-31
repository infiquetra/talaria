# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. Both tester runs used independently installed copies of the same frozen wheel and isolated
scratch configuration directories.

## Status

**NOT SATISFIED.** The current reviewed revision has no frozen wheel or acceptance receipt. The
committed receipts are historical evidence for an earlier candidate: most deterministic visual items
passed, including the repaired Visual Studio Code importer, but transcript spacing and unpinned
scroll stability failed. The inspector empty-state repair landed after that wheel was frozen, so the
generated record marks every earlier receipt stale. Gateway-controlled live items remain blocked
rather than being inferred or run on an unapproved model route.

The source checklist is the **Visual acceptance checklist** in
`docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`. The machine-readable owner registry is
`docs/acceptance/v0.5.0/checklist-items.json`. Each completed item receipt points to a raw American
National Standards Institute (ANSI) terminal capture in tester scratch and a deterministic Portable
Network Graphics screenshot of that capture, after a completed private-data review.

## Candidate provenance

<!-- BEGIN GENERATED ACCEPTANCE PROVENANCE -->
| Field | Generated value |
| --- | --- |
| Manifest status | `STALE` |
| Current reviewed candidate commit | `122bd918e0056404e576ae5623ce9e97bfe1ad93` |
| Current candidate wheel SHA-256 | `not frozen` |
| Receipt candidate identities | `d869791` / `a165ad24bd2a4baa7d11aec5d5f434e1451fd688661fed1fe8919ca0c65a1afb` (2 install, 31 item receipts) |
| Receipt counts | 2 install; 31 item; 0 current; 33 stale; 1 invalid |
<!-- END GENERATED ACCEPTANCE PROVENANCE -->

Both historical install probes rejected source, editable, and global executables. Their gate legs
ran the designed 50,000-delta corpus and produced complete reports. The unchanged 50 ms
`workload_latency_growing-one-column-table` ceiling remained exceeded and recorded, but was excluded
from the install decision because its measured 14.411 ms run-to-run spread on one unchanged candidate
is larger than the difference from the v0.4 sample. Every other gate check passed.

## Live model route status

The operator-confirmed primary route is OpenCode Muse Spark 1.2 Contributor
(`opencode-go / muse-spark-1.2-contributor`). The only permitted fallback is Ollama GLM 5.3 Flash
(`ollama (ollama-cloud) / glm-5.3-flash`), and only for primary unavailability, connection failure,
model-not-found, or bounded-test incompletion.

No passing live receipt exists. Against this gateway, Talaria composes `/model <name> --provider
<slug>` and the gateway refuses that entire command class with code 4018: `not a
quick/plugin/bundle/skill command: model`. Corrected identifiers therefore do not provide a client-side
route switch. No third route was substituted, and the gateway was not restarted, revoked, or
reconfigured without operator authority.

## Evidence matrix

The tester columns are generated from the machine-readable owner registry and receipts. A dash means
the tester does not own the row; `NO RECEIPT` means the owner has no receipt on disk. Stale cells
retain the historical verdict and candidate so a repair cannot silently inherit an earlier pass.

<!-- BEGIN GENERATED ACCEPTANCE VERDICTS -->
| Item | Checklist item | T1 | T2 | Evidence and observation |
| ---: | --- | :--- | :--- | --- |
| 1 | Installed artifact | `STALE — prior PASS @ d869791` | `STALE — prior PASS @ d869791` | Both exact install receipts prove independent fresh installs of the frozen wheel. |
| 2 | Live primary route | `NO RECEIPT` | `NO RECEIPT` | Gateway command class 4018 prevents the approved client-side model switch. |
| 3 | Main hierarchy | — | `STALE — prior PASS @ d869791` | T2 receipt and screenshot prove the required hierarchy. |
| 4 | Refined Default | `STALE — prior PASS @ d869791` | — | T1 receipt and screenshot show the complete light palette. |
| 5 | Dark Green Terminal | `STALE — prior PASS @ d869791` | — | T1 picker drive previews and accepts the dark-green palette. |
| 6 | Neutral Dark | `STALE — prior PASS @ d869791` | — | T1 picker drive previews and accepts the neutral palette without geometry movement. |
| 7 | Accessible High Contrast | `STALE — prior PASS @ d869791` | — | T1 screenshot covers the full layout and real read-only diff with the specified tokens. |
| 8 | Preview cancellation | `STALE — prior PASS @ d869791` | — | T1 previews two themes, cancels, restores the original frame, and preserves exact config bytes and time. |
| 9 | Explicit save and precedence | `STALE — prior PASS @ d869791` | — | Three T1 launches prove user, repository, and session precedence with scoped file changes. |
| 10 | Theme fallback notice | `STALE — prior PASS @ d869791` | — | T1 shows the unknown-theme notice and reports all 56 fallback tokens and 19 warnings. |
| 11 | Visual Studio Code import | `STALE — prior PASS @ d869791` | — | Two installed imports save identical bytes; fresh startup loads the imported theme without fallback. |
| 12 | All status segments | — | `STALE — prior PASS @ d869791` | T2 shows all seven ordered segments without wrapping. |
| 13 | Status configuration | — | `STALE — prior PASS @ d869791` | T2 proves reorder, omission, and unknown-segment notice after restart. |
| 14 | Status responsive sequence | — | `STALE — prior PASS @ d869791` | T2 proves the specified 144-to-19-column compaction sequence. |
| 15 | Status failure visibility | `NO RECEIPT` | `STALE — prior PASS @ d869791` | T2 shows malformed values and bounded command failures visibly. |
| 16 | Inspector dock and resize | — | `STALE — prior PASS @ d869791` | T2 proves four-column actions, limits, and retained data. |
| 17 | Inspector content and empty states | — | `STALE — prior FAIL @ d869791` | The frozen candidate invents `needs-you unavailable` in an empty Tasks surface. |
| 18 | Inspector responsive state | — | `STALE — prior PASS @ d869791` | T2 proves manual and automatic state behavior across breakpoints. |
| 19 | Side-by-side diff | — | `STALE — prior PASS @ d869791` | T2 proves aligned, read-only side-by-side diff content. |
| 20 | Unified fallback | — | `STALE — prior PASS @ d869791` | T2 proves fallback and restoration with selection and scroll retained. |
| 21 | Diff navigation and boundary | — | `STALE — prior PASS @ d869791` | T2 proves navigation, clipping, horizontal scroll, and read-only boundaries. |
| 22 | Composer caret location | `STALE — prior PASS @ d869791` | — | T1 visits every focus surface with stable geometry and correct caret cues. |
| 23 | Connection non-color states | `NO RECEIPT` | — | Reconnect and authentication-failure plateaus require shared-gateway control. |
| 24 | Agent and queue non-color states | `STALE — prior BLOCKED @ d869791` | — | T1 proves the agent states and three queue forms; shipped replay cannot create the genuine duplicate-feed state. |
| 25 | Transcript identity without color | `STALE — prior FAIL @ d869791` | — | All six identities are visible, but blank spacer rows appear after reasoning and assistant entries. |
| 26 | Reduced motion | `STALE — prior PASS @ d869791` | — | Restarted T1 drives prove the motion-policy difference without losing state updates. |
| 27 | Stable unpinned scroll | `STALE — prior FAIL @ d869791` | — | T1's real wheel-established middle anchor jumps back to the bottom after later updates. |
| 28 | Stable pinned scroll | `STALE — prior PASS @ d869791` | — | T1 proves F5 follow, manual release, stable unpinned reading, and final return to newest output. |
| 29 | Wide screenshot | — | `STALE — prior PASS @ d869791` | T2 provides the required 132-by-36 evidence. |
| 30 | Narrow screenshot | — | `STALE — prior PASS @ d869791` | T2 provides the required 78-by-36 evidence. |
| 31 | Malformed Visual Studio Code import | `STALE — prior PASS @ d869791` | — | Installed importer exits 2 and stores no artifact. |
| 32 | Session-only status toggle | — | `STALE — prior PASS @ d869791` | T2 proves session-only change and clean restart restoration. |
| 33 | Dead gateway credential | `NO RECEIPT` | `NO RECEIPT` | Producing a real stale credential requires gateway revocation or restart authority. |
| 34 | Killed session | `NO RECEIPT` | `STALE/INVALID — prior BLOCKED @ d869791` | T2 could not establish an approved model-backed session to kill. |
| 35 | Restart-only configuration | `STALE — prior PASS @ d869791` | `NO RECEIPT` | T1 proves the running process does not reload the config and the fresh process does. |
| 36 | Cross-tester evidence | `NO RECEIPT` | `NO RECEIPT` | Coordinator assembly follows both tester reports. |
<!-- END GENERATED ACCEPTANCE VERDICTS -->

## Evidence custody

Raw captures remain in the tester scratch roots recorded by each install receipt. Reviewed screenshots,
exact install receipts, and item receipts are committed under `docs/acceptance/v0.5.0/evidence/t1/`
and `docs/acceptance/v0.5.0/evidence/t2/`. The screenshots were rendered directly from pseudo-terminal
bytes with Pyte and Pillow; no Computer Use or graphical user interface automation was used.

T1's selected 49 raw capture and text-render files contained no credential, token-bearing URL,
operator path, username, or private email/domain match. All 16 generated T1 item receipts passed the
receipt validator with file-hash checks. T2's corresponding review and validation are recorded in
its evidence README.

## Repository checks

- `uv run ruff check .`: passed.
- `uv run mypy`: passed with no issues.
- `/opt/homebrew/bin/uv run pytest`: passed, 2,329 tests passed and 7 skipped in 564.30 seconds. The
  first full run had one intermittent status-process timing failure after 2,328 passes; that exact
  test passed five consecutive focused reruns, and the second complete run passed.
- `uv run bandit -r talaria -q`: passed; Bandit emitted only its existing comment-token warnings.
- `git diff --check`: passed after the evidence update.

## Final verdict

**NOT SATISFIED.** No receipt proves the current reviewed revision. The earlier installed candidate
and Visual Studio Code importer fix have historical evidence, but those verdicts cannot survive the
later product repair. The generated cells preserve prior failures and blocks without presenting any
stale verdict as current.
