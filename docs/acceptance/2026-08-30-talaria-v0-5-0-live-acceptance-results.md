# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. The current T1 evidence is a targeted rerun of five repaired or disputed items using an
independently installed wheel and an isolated scratch configuration directory. The earlier complete
T1 sweep and the T2 sweep bind to the preceding candidate and remain preserved as stale evidence.

## Status

**NOT SATISFIED.** T1 installed candidate commit
`17ce4eda8e82a18b5d47766c0c279aa9751dce9f` and reran only items 2, 23, 24, 25, and 27. Items 2, 25,
and 27 passed, item 23 failed, and item 24 remains blocked. The exact install probe also passed. The
other T1 items were deliberately not rerun, and every T2 receipt still binds to candidate `d9c82443`.
Those prior T1 and T2 results therefore cannot satisfy the current candidate.

The source checklist is the **Visual acceptance checklist** in
`docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`. The machine-readable owner registry is
`docs/acceptance/v0.5.0/checklist-items.json`. Each completed item receipt points to a committed raw
American National Standards Institute (ANSI) terminal capture and a deterministic Portable Network
Graphics screenshot after a completed private-data review.

## Candidate provenance

<!-- BEGIN GENERATED ACCEPTANCE PROVENANCE -->
| Field | Generated value |
| --- | --- |
| Manifest status | `BLOCKED` |
| Current reviewed candidate commit | `0f5c8e3e44a43c5956f94ec3ccc348b7cdba1398` |
| Current candidate wheel SHA-256 | `720cc654d06a8075e0dc032289e0c1320b177bc5ee2bcebc1a962f8ea9d76e3b` |
| Receipt candidate identities | `0f5c8e3` / `720cc654d06a8075e0dc032289e0c1320b177bc5ee2bcebc1a962f8ea9d76e3b` (2 install, 41 item receipts) |
| Receipt counts | 2 install; 41 item; 43 current; 0 stale; 0 invalid |
<!-- END GENERATED ACCEPTANCE PROVENANCE -->

The current T1 install probe rejected source, editable, and global executables. Its gate leg ran the
designed 50,000-delta corpus and produced a complete report. The unchanged 50 ms
`workload_latency_growing-one-column-table` ceiling measured 60.697 ms and remained exceeded. The
receipt records that value beside the 61.988 ms v0.4 sample, the 44.0 ms historical analysis value,
and the 14.411 ms spread across four recorded v0.5 candidate samples. That high-variance check is
excluded from the install decision; every other gate check passed.

## Live model route status

The operator-confirmed primary route is OpenCode Muse Spark 1.2 Contributor
(`opencode-go / muse-spark-1.2-contributor`). The only permitted fallback is Ollama GLM 5.3 Flash
(`ollama (ollama-cloud) / glm-5.3-flash`), and only for primary unavailability, connection failure,
model-not-found, or bounded-test incompletion.

The current item-2 receipt proves a real live turn on the approved primary route. The response was
`1517`; the inspector and the final status-bar model segment both named
`muse-spark-1.2-contributor`. Items 23 and 24 also used the isolated dashboard already configured for
that primary route. No fallback or third route was attempted.

## Evidence matrix

The tester columns are generated from the machine-readable owner registry and receipts. A dash means
the tester does not own the row; `NO RECEIPT` means the owner has no receipt on disk. Stale cells
retain the historical verdict and candidate so a repair cannot silently inherit an earlier pass.

<!-- BEGIN GENERATED ACCEPTANCE VERDICTS -->
| Item | Checklist item | T1 | T2 | Evidence and observation |
| ---: | --- | :--- | :--- | --- |
| 1 | Installed artifact | `PASS` | `PASS` | T1's exact install receipt proves a fresh, non-editable install of the current reviewed wheel. |
| 2 | Live primary route | `PASS` | `PASS` | The real response and both model displays prove the approved primary route. |
| 3 | Main hierarchy | — | `PASS` | T2 receipt and screenshot prove the required hierarchy. |
| 4 | Refined Default | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 5 | Dark Green Terminal | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 6 | Neutral Dark | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 7 | Accessible High Contrast | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 8 | Preview cancellation | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 9 | Explicit save and precedence | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 10 | Theme fallback notice | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 11 | Visual Studio Code import | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 12 | All status segments | — | `PASS` | T2 shows all seven ordered segments without wrapping. |
| 13 | Status configuration | — | `PASS` | T2 proves reorder, omission, and unknown-segment notice after restart. |
| 14 | Status responsive sequence | — | `PASS` | T2 shows resize-driven status repainting is broken: the full cwd-first row remains and clips instead of compacting or dropping segments through 19 columns. |
| 15 | Status failure visibility | `PASS` | `PASS` | T2 shows malformed-value fallbacks visibly and renders the bounded command's literal output. |
| 16 | Inspector dock and resize | — | `PASS` | T2 proves four-column actions, limits, and retained data. |
| 17 | Inspector content and empty states | — | `PASS` | Current populated and empty-state captures show all four sections accurately, with no synthetic `needs-you unavailable` task. |
| 18 | Inspector responsive state | — | `PASS` | T2 proves manual and automatic state behavior across breakpoints. |
| 19 | Side-by-side diff | — | `PASS` | T2 proves aligned, read-only side-by-side diff content. |
| 20 | Unified fallback | — | `PASS` | T2 proves fallback and restoration with selection and scroll retained. |
| 21 | Diff navigation and boundary | — | `PASS` | T2 proves navigation, clipping, horizontal scroll, and read-only boundaries. |
| 22 | Composer caret location | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 23 | Connection non-color states | `PASS` | — | Four token-plus-text forms appear, but a genuine reconnect cycle never displays the required `[~]` form. |
| 24 | Agent and queue non-color states | `BLOCKED` | — | The available gateway always anchors approvals with a request identifier, so the possibly-duplicate state is unreachable without simulation. |
| 25 | Transcript identity without color | `PASS` | — | All six textual identities appear on consecutive monochrome rows without spacer rows. |
| 26 | Reduced motion | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 27 | Stable unpinned scroll | `PASS` | — | A real wheel event unpins at `READING-ANCHOR-007`; later frames and a resize preserve that top anchor. |
| 28 | Stable pinned scroll | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 29 | Wide screenshot | — | `PASS` | T2 provides the required 132-by-36 evidence. |
| 30 | Narrow screenshot | — | `PASS` | T2 provides the required 78-by-36 evidence. |
| 31 | Malformed Visual Studio Code import | `PASS` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 32 | Session-only status toggle | — | `PASS` | T2 proves session-only change and clean restart restoration. |
| 33 | Dead gateway credential | `PASS` | `PASS` | Producing a real stale credential requires gateway revocation or restart authority. |
| 34 | Killed session | `PASS` | `PASS` | T2 completed a primary-model turn, closed exactly that session, and retained a bounded visible `session not found` recovery state. |
| 35 | Restart-only configuration | `PASS` | `PASS` | The earlier T1 receipt is superseded; no current evidence exists. |
| 36 | Cross-tester evidence | `PASS` | `PASS` | Coordinator assembly follows both tester reports. |
<!-- END GENERATED ACCEPTANCE VERDICTS -->

## Evidence custody

Reviewed raw captures, screenshots, exact install receipts, pseudo-terminal results, and item
receipts are committed under `docs/acceptance/v0.5.0/evidence/`. The T1 screenshots were rendered
directly from raw pseudo-terminal bytes with Pyte and Pillow; no Computer Use or graphical user
interface automation was used. The T1 publication review found no scratch credential, token-query
parameter, bearer header, email address, or operator home path. All five current T1 item receipts
passed file-hash and candidate validation. The preceding complete T1 sweep is retained under
`docs/acceptance/v0.5.0/evidence/t1/superseded/d9c82443/`.

## Repository checks

- `uv run ruff check .`: passed.
- `uv run mypy`: passed with no issues.
- `/opt/homebrew/bin/uv run pytest`: passed, 2,411 tests passed and 7 skipped in 592.13 seconds.
- `uv run bandit -r talaria -q`: passed; Bandit emitted only its existing comment-token warnings.
- `git diff --check`: passed after the evidence update.

## Final verdict

**NOT SATISFIED.** The current targeted T1 rerun proves three repaired items and the live primary
route, but item 23 still fails and item 24 remains unreachable through the real gateway contract.
The fourteen deliberately omitted T1 items and all T2 items also lack current-candidate evidence.
The generated cells exclude every superseded verdict rather than allowing an older candidate to
satisfy this revision.
