# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. The current T2 run used an independently installed wheel and an isolated scratch
configuration directory; T1 has not rerun this reviewed candidate.

## Status

**NOT SATISFIED.** T2 installed and exercised reviewed candidate commit
`122bd918e0056404e576ae5623ce9e97bfe1ad93`: 14 assigned items passed and the live killed-session item
is blocked before an approved model turn. T1 has no receipt for this candidate, so its 16 assigned
items and shared responsibilities remain unevidenced. Earlier receipts are preserved under each
tester's `superseded/d869791/` directory and are excluded from the current generated record.

The source checklist is the **Visual acceptance checklist** in
`docs/design/2026-08-30-talaria-v0-5-0-visual-spec.md`. The machine-readable owner registry is
`docs/acceptance/v0.5.0/checklist-items.json`. Each completed item receipt points to a committed raw
American National Standards Institute (ANSI) terminal capture and a deterministic Portable Network
Graphics screenshot after a completed private-data review.

## Candidate provenance

<!-- BEGIN GENERATED ACCEPTANCE PROVENANCE -->
| Field | Generated value |
| --- | --- |
| Manifest status | `IN-PROGRESS` |
| Current reviewed candidate commit | `d9c82443f51932483ecb37c653d5c0cd8c342dac` |
| Current candidate wheel SHA-256 | `839f7a26985267db5e0cc2fa52b46ac3924f7791cdfbc70fbadfdc0e7f6cdfda` |
| Receipt candidate identities | `d9c8244` / `839f7a26985267db5e0cc2fa52b46ac3924f7791cdfbc70fbadfdc0e7f6cdfda` (1 install, 15 item receipts) |
| Receipt counts | 1 install; 15 item; 16 current; 0 stale; 0 invalid |
<!-- END GENERATED ACCEPTANCE PROVENANCE -->

The current T2 install probe rejected source, editable, and global executables. Its gate leg ran the
designed 50,000-delta corpus and produced a complete report. The unchanged 50 ms
`workload_latency_growing-one-column-table` ceiling measured 80.208 ms and remained exceeded. The
receipt records that value beside the 61.988 ms v0.4 sample, the 44.0 ms historical analysis value,
and the 25.758 ms spread across recorded v0.5 candidate samples. That high-variance check is excluded
from the install decision; every other gate check passed.

## Live model route status

The operator-confirmed primary route is OpenCode Muse Spark 1.2 Contributor
(`opencode-go / muse-spark-1.2-contributor`). The only permitted fallback is Ollama GLM 5.3 Flash
(`ollama (ollama-cloud) / glm-5.3-flash`), and only for primary unavailability, connection failure,
model-not-found, or bounded-test incompletion.

No passing live receipt exists. In the current T2 attempt, the gateway created a throwaway session on
its unapproved `openai-api/gpt-5.5` default and initialization failed for missing credentials before
the corrected primary model command could dispatch. The separately established gateway refusal of
Talaria's `/model <name> --provider <slug>` command remains an external blocker. No third route was
substituted, and the gateway was not restarted, revoked, or reconfigured without operator authority.

## Evidence matrix

The tester columns are generated from the machine-readable owner registry and receipts. A dash means
the tester does not own the row; `NO RECEIPT` means the owner has no receipt on disk. Stale cells
retain the historical verdict and candidate so a repair cannot silently inherit an earlier pass.

<!-- BEGIN GENERATED ACCEPTANCE VERDICTS -->
| Item | Checklist item | T1 | T2 | Evidence and observation |
| ---: | --- | :--- | :--- | --- |
| 1 | Installed artifact | `NO RECEIPT` | `PASS` | T2's exact install receipt proves a fresh install of the reviewed wheel; T1 has no current install receipt. |
| 2 | Live primary route | `NO RECEIPT` | `NO RECEIPT` | Gateway command class 4018 prevents the approved client-side model switch. |
| 3 | Main hierarchy | — | `PASS` | T2 receipt and screenshot prove the required hierarchy. |
| 4 | Refined Default | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 5 | Dark Green Terminal | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 6 | Neutral Dark | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 7 | Accessible High Contrast | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 8 | Preview cancellation | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 9 | Explicit save and precedence | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 10 | Theme fallback notice | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 11 | Visual Studio Code import | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 12 | All status segments | — | `PASS` | T2 shows all seven ordered segments without wrapping. |
| 13 | Status configuration | — | `PASS` | T2 proves reorder, omission, and unknown-segment notice after restart. |
| 14 | Status responsive sequence | — | `PASS` | T2 proves the specified 144-to-19-column compaction sequence. |
| 15 | Status failure visibility | `NO RECEIPT` | `PASS` | T2 shows malformed-value fallbacks visibly and renders the bounded command's literal output. |
| 16 | Inspector dock and resize | — | `PASS` | T2 proves four-column actions, limits, and retained data. |
| 17 | Inspector content and empty states | — | `PASS` | Current populated and empty-state captures show all four sections accurately, with no synthetic `needs-you unavailable` task. |
| 18 | Inspector responsive state | — | `PASS` | T2 proves manual and automatic state behavior across breakpoints. |
| 19 | Side-by-side diff | — | `PASS` | T2 proves aligned, read-only side-by-side diff content. |
| 20 | Unified fallback | — | `PASS` | T2 proves fallback and restoration with selection and scroll retained. |
| 21 | Diff navigation and boundary | — | `PASS` | T2 proves navigation, clipping, horizontal scroll, and read-only boundaries. |
| 22 | Composer caret location | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 23 | Connection non-color states | `NO RECEIPT` | — | Reconnect and authentication-failure plateaus require shared-gateway control. |
| 24 | Agent and queue non-color states | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 25 | Transcript identity without color | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 26 | Reduced motion | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 27 | Stable unpinned scroll | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 28 | Stable pinned scroll | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 29 | Wide screenshot | — | `PASS` | T2 provides the required 132-by-36 evidence. |
| 30 | Narrow screenshot | — | `PASS` | T2 provides the required 78-by-36 evidence. |
| 31 | Malformed Visual Studio Code import | `NO RECEIPT` | — | The earlier T1 receipt is superseded; no current evidence exists. |
| 32 | Session-only status toggle | — | `PASS` | T2 proves session-only change and clean restart restoration. |
| 33 | Dead gateway credential | `NO RECEIPT` | `NO RECEIPT` | Producing a real stale credential requires gateway revocation or restart authority. |
| 34 | Killed session | `NO RECEIPT` | `PASS` | T2 completed a primary-model turn, closed exactly that session, and retained a bounded visible `session not found` recovery state. |
| 35 | Restart-only configuration | `NO RECEIPT` | `NO RECEIPT` | The earlier T1 receipt is superseded; no current evidence exists. |
| 36 | Cross-tester evidence | `NO RECEIPT` | `NO RECEIPT` | Coordinator assembly follows both tester reports. |
<!-- END GENERATED ACCEPTANCE VERDICTS -->

## Evidence custody

Reviewed raw captures, screenshots, exact install receipts, pseudo-terminal results, and item
receipts are committed under `docs/acceptance/v0.5.0/evidence/t2/`. The screenshots were rendered
directly from raw pseudo-terminal bytes with Pyte and Pillow; no Computer Use or graphical user
interface automation was used. The T2 publication review found no credential, token-like value, or
operator home path. Every current T2 receipt passed file-hash validation against the reviewed
candidate. T1 has no current evidence; its earlier artifacts are retained under
`docs/acceptance/v0.5.0/evidence/t1/superseded/d869791/`.

## Repository checks

- `uv run ruff check .`: passed.
- `uv run mypy`: passed with no issues.
- `/opt/homebrew/bin/uv run pytest`: passed, 2,367 tests passed and 7 skipped in 576.93 seconds.
- `uv run bandit -r talaria -q`: passed; Bandit emitted only its existing comment-token warnings.
  The workflow's broader `talaria scripts` scan also reports no medium or high findings.
- `git diff --check`: passed after the evidence update.

## Final verdict

**NOT SATISFIED.** T2 proves 14 assigned items on the current reviewed wheel and records item 34 as
blocked before an approved live model turn. T1's 16 assigned items and its shared responsibilities
have no current receipts. The generated cells exclude every superseded verdict rather than allowing
an older candidate to satisfy this revision.
