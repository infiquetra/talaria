# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for exact installed v0.5.0
candidates. T1 used an independently installed wheel for the current candidate and an isolated
scratch configuration directory. T2's evidence remains bound to the preceding candidate.

## Status

**NOT SATISFIED.** T1 installed and exercised current candidate commit
`d9c82443f51932483ecb37c653d5c0cd8c342dac`: 15 assigned or shared items passed, four failed, and one
is blocked. T2's 15 item receipts and install receipt are stale because they bind to candidate
`122bd918e0056404e576ae5623ce9e97bfe1ad93`. Earlier receipts are preserved under each tester's
`superseded/` directory and are excluded from the current generated verdict count.

The source checklist is the **Visual acceptance checklist** in
`docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`. The machine-readable owner registry is
`docs/acceptance/v0.5.0/checklist-items.json`. Each completed item receipt points to a committed raw
American National Standards Institute (ANSI) terminal capture and a deterministic Portable Network
Graphics screenshot after a completed private-data review.

## Candidate provenance

<!-- BEGIN GENERATED ACCEPTANCE PROVENANCE -->
| Field | Generated value |
| --- | --- |
| Manifest status | `STALE` |
| Current reviewed candidate commit | `d9c82443f51932483ecb37c653d5c0cd8c342dac` |
| Current candidate wheel SHA-256 | `839f7a26985267db5e0cc2fa52b46ac3924f7791cdfbc70fbadfdc0e7f6cdfda` |
| Receipt candidate identities | `122bd91` / `a15b991fd16069a7a935548f949b5e287db86ba386799bbbebfc802f60f76126` (1 install, 15 item receipts)<br>`d9c8244` / `839f7a26985267db5e0cc2fa52b46ac3924f7791cdfbc70fbadfdc0e7f6cdfda` (1 install, 19 item receipts) |
| Receipt counts | 2 install; 34 item; 20 current; 16 stale; 0 invalid |
<!-- END GENERATED ACCEPTANCE PROVENANCE -->

The current T1 install probe rejected source, editable, and global executables. Its gate leg ran the
designed 50,000-delta corpus and produced a complete report. The unchanged 50 ms
`workload_latency_growing-one-column-table` ceiling measured 55.358 ms and remained exceeded. The
receipt records that value beside the 61.988 ms v0.4 sample, the 44.0 ms historical analysis value,
and the 14.411 ms spread across four recorded v0.5 candidate samples. That high-variance check is
excluded from the install decision; every other gate check passed.

## Live model route status

The operator-confirmed primary route is OpenCode Muse Spark 1.2 Contributor
(`opencode-go / muse-spark-1.2-contributor`). The only permitted fallback is Ollama GLM 5.3 Flash
(`ollama (ollama-cloud) / glm-5.3-flash`), and only for primary unavailability, connection failure,
model-not-found, or bounded-test incompletion.

A current live receipt proves a real bounded turn completed on the approved primary route and the
inspector named `muse-spark-1.2-contributor`. Item 2 nevertheless failed because the required bottom
status `agent_model` segment displayed `agent: ?`. The authorized port 8790 restart also produced a
visible HTTP 403 authentication failure without exposing the scratch credential. No fallback or
third route was used.

## Evidence matrix

The tester columns are generated from the machine-readable owner registry and receipts. A dash means
the tester does not own the row; `NO RECEIPT` means the owner has no receipt on disk. Stale cells
retain the historical verdict and candidate so a repair cannot silently inherit an earlier pass.

<!-- BEGIN GENERATED ACCEPTANCE VERDICTS -->
| Item | Checklist item | T1 | T2 | Evidence and observation |
| ---: | --- | :--- | :--- | --- |
| 1 | Installed artifact | `PASS` | `STALE — prior PASS @ 122bd91` | T1's exact install receipt proves a fresh install of the current wheel; T2's install receipt is stale. |
| 2 | Live primary route | `FAIL` | `NO RECEIPT` | A real primary-model turn completed, but the required bottom-status model segment displayed `agent: ?`. |
| 3 | Main hierarchy | — | `STALE — prior PASS @ 122bd91` | T2 receipt and screenshot prove the required hierarchy. |
| 4 | Refined Default | `PASS` | — | The current T1 capture shows the refined default across every required surface. |
| 5 | Dark Green Terminal | `PASS` | — | The current T1 capture shows the dark green theme across every required surface. |
| 6 | Neutral Dark | `PASS` | — | The current T1 capture shows the neutral dark theme across every required surface. |
| 7 | Accessible High Contrast | `PASS` | — | The current T1 capture shows the high-contrast theme across every required surface. |
| 8 | Preview cancellation | `PASS` | — | Escape restored the saved theme while config bytes and mtime stayed unchanged. |
| 9 | Explicit save and precedence | `PASS` | — | Three current T1 legs prove user save, repository override, and session-only preview precedence. |
| 10 | Theme fallback notice | `PASS` | — | The unknown theme produced a notice and default fallback; the partial imported theme remained selectable. |
| 11 | Visual Studio Code import | `PASS` | — | Two imports produced identical bytes and the imported theme loaded without a fallback notice. |
| 12 | All status segments | — | `STALE — prior PASS @ 122bd91` | T2 shows all seven ordered segments without wrapping. |
| 13 | Status configuration | — | `STALE — prior PASS @ 122bd91` | T2 proves reorder, omission, and unknown-segment notice after restart. |
| 14 | Status responsive sequence | — | `STALE — prior PASS @ 122bd91` | T2 proves the specified 144-to-19-column compaction sequence. |
| 15 | Status failure visibility | `NO RECEIPT` | `STALE — prior PASS @ 122bd91` | T2 shows malformed values and bounded command failures visibly. |
| 16 | Inspector dock and resize | — | `STALE — prior PASS @ 122bd91` | T2 proves four-column actions, limits, and retained data. |
| 17 | Inspector content and empty states | — | `STALE — prior PASS @ 122bd91` | Current populated and empty-state captures show all four sections accurately, with no synthetic `needs-you unavailable` task. |
| 18 | Inspector responsive state | — | `STALE — prior PASS @ 122bd91` | T2 proves manual and automatic state behavior across breakpoints. |
| 19 | Side-by-side diff | — | `STALE — prior PASS @ 122bd91` | T2 proves aligned, read-only side-by-side diff content. |
| 20 | Unified fallback | — | `STALE — prior PASS @ 122bd91` | T2 proves fallback and restoration with selection and scroll retained. |
| 21 | Diff navigation and boundary | — | `STALE — prior PASS @ 122bd91` | T2 proves navigation, clipping, horizontal scroll, and read-only boundaries. |
| 22 | Composer caret location | `PASS` | — | The current T1 focus and resize drive kept the caret in the composer row. |
| 23 | Connection non-color states | `FAIL` | — | The controlled monochrome restart did not retain all five required token-plus-text forms. |
| 24 | Agent and queue non-color states | `BLOCKED` | — | Replay cannot provide the admin-polled `possibly duplicate` state, so the required row is absent. |
| 25 | Transcript identity without color | `FAIL` | — | All six identities are visible, but blank spacer rows remain after reasoning and assistant entries. |
| 26 | Reduced motion | `PASS` | — | Restarted standard and reduced drives prove static reduced forms while information continues updating. |
| 27 | Stable unpinned scroll | `FAIL` | — | The unpinned visible range jumped after later interface updates. |
| 28 | Stable pinned scroll | `PASS` | — | Follow mode retained a predictable newest-content anchor across resize. |
| 29 | Wide screenshot | — | `STALE — prior PASS @ 122bd91` | T2 provides the required 132-by-36 evidence. |
| 30 | Narrow screenshot | — | `STALE — prior PASS @ 122bd91` | T2 provides the required 78-by-36 evidence. |
| 31 | Malformed Visual Studio Code import | `PASS` | — | The malformed shipped fixture exited 3 with a strict-JSON error and created no theme. |
| 32 | Session-only status toggle | — | `STALE — prior PASS @ 122bd91` | T2 proves session-only change and clean restart restoration. |
| 33 | Dead gateway credential | `PASS` | `NO RECEIPT` | The authorized isolated restart produced visible HTTP 403 and `[!] auth` states without exposing the credential. |
| 34 | Killed session | `NO RECEIPT` | `STALE — prior BLOCKED @ 122bd91` | T2 could not establish an approved model-backed session to kill. |
| 35 | Restart-only configuration | `PASS` | `NO RECEIPT` | The running theme stayed unchanged after the config edit and changed only after restart. |
| 36 | Cross-tester evidence | `NO RECEIPT` | `NO RECEIPT` | Coordinator assembly follows both tester reports. |
<!-- END GENERATED ACCEPTANCE VERDICTS -->

## Evidence custody

Reviewed raw captures, screenshots, exact install receipts, pseudo-terminal results, and item
receipts are committed under each tester's directory in `docs/acceptance/v0.5.0/evidence/`. The
screenshots were rendered directly from raw pseudo-terminal bytes with Pyte and Pillow; no Computer
Use or graphical user interface automation was used. The T1 publication review found no credential,
token query, bearer header, email address, or operator home path. All 19 current T1 item receipts
passed file-hash validation against the current installed candidate. T2's evidence is retained and
visibly marked stale.

## Repository checks

- `uv run ruff check .`: passed.
- `uv run mypy`: passed with no issues.
- `/opt/homebrew/bin/uv run pytest`: passed, 2,407 tests passed and 7 skipped in 582.93 seconds.
- `uv run bandit -r talaria -q`: passed; Bandit emitted only its existing comment-token warnings.
  The workflow's broader `talaria scripts` scan also reports no medium or high findings.
- `git diff --check`: passed after the evidence update.

## Final verdict

**NOT SATISFIED.** T1 proves 15 assigned or shared items on the current wheel, records four failures,
and blocks one item whose required state is unavailable through replay. T2 must rerun its evidence on
the current candidate before its prior 14 passes and one block can count. The generated cells keep
those prior verdicts visible as stale rather than allowing an older candidate to satisfy this
revision.
