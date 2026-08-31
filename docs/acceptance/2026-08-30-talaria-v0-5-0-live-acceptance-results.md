# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. The current T1 evidence is a confirming sweep of all twenty assigned items using an
independently installed wheel and an isolated scratch configuration directory. The T2 sweep binds to
an earlier candidate and remains visible as stale evidence.

## Status

**NOT SATISFIED.** T1 installed final candidate commit
`0f5c8e3e44a43c5956f94ec3ccc348b7cdba1398` and reran all twenty assignments. Nineteen passed and
item 24 remains blocked by an accepted protocol limitation. Every T2 receipt still binds to candidate
`d9c82443f51932483ecb37c653d5c0cd8c342dac`, so the repository-wide record remains stale until that
tester reruns its assignments against the final candidate.

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
| Current reviewed candidate commit | `0f5c8e3e44a43c5956f94ec3ccc348b7cdba1398` |
| Current candidate wheel SHA-256 | `720cc654d06a8075e0dc032289e0c1320b177bc5ee2bcebc1a962f8ea9d76e3b` |
| Receipt candidate identities | `0f5c8e3` / `720cc654d06a8075e0dc032289e0c1320b177bc5ee2bcebc1a962f8ea9d76e3b` (1 install, 19 item receipts)<br>`d9c8244` / `839f7a26985267db5e0cc2fa52b46ac3924f7791cdfbc70fbadfdc0e7f6cdfda` (1 install, 15 item receipts) |
| Receipt counts | 2 install; 34 item; 20 current; 16 stale; 0 invalid |
<!-- END GENERATED ACCEPTANCE PROVENANCE -->

The current T1 install probe rejected source, editable, and global executables. Its gate leg ran the
designed 50,000-delta corpus and produced a complete report. The unchanged 50 ms
`workload_latency_growing-one-column-table` ceiling measured 63.839 ms and remained exceeded. The
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
`muse-spark-1.2-contributor`. Item 23 used the same route for a genuine reconnect cycle. Item 33
requested that route but deliberately stopped at the authentication boundary. No fallback or third
route was attempted.

## Evidence matrix

The tester columns are generated from the machine-readable owner registry and receipts. A dash means
the tester does not own the row; `NO RECEIPT` means the owner has no receipt on disk. Stale cells
retain the historical verdict and candidate so a repair cannot silently inherit an earlier pass.

<!-- BEGIN GENERATED ACCEPTANCE VERDICTS -->
| Item | Checklist item | T1 | T2 | Evidence and observation |
| ---: | --- | :--- | :--- | --- |
| 1 | Installed artifact | `PASS` | `STALE — prior PASS @ d9c8244` | T1's exact install receipt proves a fresh, non-editable install of the current reviewed wheel. |
| 2 | Live primary route | `PASS` | `NO RECEIPT` | The real response and both model displays prove the approved primary route. |
| 3 | Main hierarchy | — | `STALE — prior PASS @ d9c8244` | T2 receipt and screenshot prove the required hierarchy. |
| 4 | Refined Default | `PASS` | — | The current capture shows Refined Default across all required interface surfaces and semantic roles. |
| 5 | Dark Green Terminal | `PASS` | — | The current capture shows the complete Dark Green Terminal interface with legible semantic separation. |
| 6 | Neutral Dark | `PASS` | — | The current capture shows Neutral Dark across transcript, chrome, status and diff surfaces. |
| 7 | Accessible High Contrast | `PASS` | — | The current capture shows Accessible High Contrast across all required surfaces and token relationships. |
| 8 | Preview cancellation | `PASS` | — | Escape restored Refined Default after two previews, and the scratch configuration's hash and modification time did not change. |
| 9 | Explicit save and precedence | `PASS` | — | Three current legs prove user save, repository override, and session-only preview without persistence. |
| 10 | Theme fallback notice | `PASS` | — | An unknown theme produces the visible fallback notice; a partial import remains selectable with fallback accounting. |
| 11 | Visual Studio Code import | `PASS` | — | Two imports produce identical bytes, and the installed imported theme survives restart without a fallback notice. |
| 12 | All status segments | — | `STALE — prior PASS @ d9c8244` | T2 shows all seven ordered segments without wrapping. |
| 13 | Status configuration | — | `STALE — prior PASS @ d9c8244` | T2 proves reorder, omission, and unknown-segment notice after restart. |
| 14 | Status responsive sequence | — | `STALE — prior PASS @ d9c8244` | T2 proves the specified 144-to-19-column compaction sequence. |
| 15 | Status failure visibility | `NO RECEIPT` | `STALE — prior PASS @ d9c8244` | T2 shows malformed-value fallbacks visibly and renders the bounded command's literal output. |
| 16 | Inspector dock and resize | — | `STALE — prior PASS @ d9c8244` | T2 proves four-column actions, limits, and retained data. |
| 17 | Inspector content and empty states | — | `STALE — prior PASS @ d9c8244` | Current populated and empty-state captures show all four sections accurately, with no synthetic `needs-you unavailable` task. |
| 18 | Inspector responsive state | — | `STALE — prior PASS @ d9c8244` | T2 proves manual and automatic state behavior across breakpoints. |
| 19 | Side-by-side diff | — | `STALE — prior PASS @ d9c8244` | T2 proves aligned, read-only side-by-side diff content. |
| 20 | Unified fallback | — | `STALE — prior PASS @ d9c8244` | T2 proves fallback and restoration with selection and scroll retained. |
| 21 | Diff navigation and boundary | — | `STALE — prior PASS @ d9c8244` | T2 proves navigation, clipping, horizontal scroll, and read-only boundaries. |
| 22 | Composer caret location | `PASS` | — | The current capture keeps the caret and focus cue in the composer row through focus movement and resize. |
| 23 | Connection non-color states | `PASS` | — | Separate current captures show `[..] wait`, `[ok] up`, `[~] retry`, `[x] down`, and `[!] auth`; the reconnect leg recovers. |
| 24 | Agent and queue non-color states | `BLOCKED` | — | The available gateway always anchors approvals with a request identifier, so the possibly-duplicate state is unreachable without simulation. |
| 25 | Transcript identity without color | `PASS` | — | All six textual identities appear on consecutive monochrome rows without spacer rows. |
| 26 | Reduced motion | `PASS` | — | Two restart legs show standard and reduced focus transitions while progress and elapsed updates continue. |
| 27 | Stable unpinned scroll | `PASS` | — | A real wheel event unpins the long transcript; later frames and a resize preserve the reading region instead of following the tail. |
| 28 | Stable pinned scroll | `PASS` | — | Follow mode keeps `NEWEST-BOTTOM-ENTRY` visible through later output and a mid-drive resize. |
| 29 | Wide screenshot | — | `STALE — prior PASS @ d9c8244` | T2 provides the required 132-by-36 evidence. |
| 30 | Narrow screenshot | — | `STALE — prior PASS @ d9c8244` | T2 provides the required 78-by-36 evidence. |
| 31 | Malformed Visual Studio Code import | `PASS` | — | The shipped malformed fixture exits 3 with a strict-JSON diagnostic and creates no theme artifact. |
| 32 | Session-only status toggle | — | `STALE — prior PASS @ d9c8244` | T2 proves session-only change and clean restart restoration. |
| 33 | Dead gateway credential | `PASS` | `NO RECEIPT` | The authorized isolated-dashboard restart produces a visible HTTP 403 and `[!] auth` without exposing the credential. |
| 34 | Killed session | `NO RECEIPT` | `STALE — prior PASS @ d9c8244` | T2 completed a primary-model turn, closed exactly that session, and retained a bounded visible `session not found` recovery state. |
| 35 | Restart-only configuration | `PASS` | `NO RECEIPT` | The running interface retains Accessible High Contrast; Neutral Dark appears only after a clean restart. |
| 36 | Cross-tester evidence | `NO RECEIPT` | `NO RECEIPT` | Coordinator assembly follows both tester reports. |
<!-- END GENERATED ACCEPTANCE VERDICTS -->

## Evidence custody

Reviewed raw captures, screenshots, exact install receipts, pseudo-terminal results, and item
receipts are committed under `docs/acceptance/v0.5.0/evidence/`. The T1 screenshots were rendered
directly from raw pseudo-terminal bytes with Pyte and Pillow; no Computer Use or graphical user
interface automation was used. The T1 publication review found no scratch credential, token-query
parameter, bearer header, email address, or operator home path. All nineteen current T1 item receipts
passed file-hash and candidate validation. The preceding targeted T1 rerun is retained under
`docs/acceptance/v0.5.0/evidence/t1/superseded/17ce4eda/`.

## Repository checks

- `uv run ruff check .`: passed.
- `uv run mypy`: passed with no issues.
- `/opt/homebrew/bin/uv run pytest`: passed, 2,412 tests passed and 7 skipped in 588.61 seconds.
- `uv run bandit -r talaria -q`: passed; Bandit emitted only its existing comment-token warnings.
- `git diff --check`: passed after the evidence update.

## Final verdict

**NOT SATISFIED.** The current T1 sweep passes nineteen of twenty assignments, including every
repaired condition and the live primary route. Item 24 remains unreachable through the real gateway
and replay contracts without simulation. All T2 receipts remain stale, and the generated cells
exclude those older verdicts rather than allowing a different artifact to satisfy this revision.
