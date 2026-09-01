# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. Both testers used independently installed wheels and isolated scratch configuration
directories. Earlier runs remain preserved under explicitly named `superseded/` directories and do
not contribute to the generated verdict.

<!-- BEGIN GENERATED ACCEPTANCE STATUS -->
## Status: **BLOCKED**

This verdict is generated from `artifact-manifest.json`; it is not maintained by hand. **BLOCKED**: 43 of 43 expected checklist/tester slots are covered. The evidence set separately contains 44 current receipts (42 item and 2 install). The one-receipt overlap is checklist item 1 for talaria-t2, which has both an item receipt and an install receipt. Item verdicts are 41 pass, 1 blocked, and 0 fail.
The manifest also records 0 stale receipts and 0 invalid item receipts.
Regenerate it with `uv run python -m scripts.acceptance.v050_records refresh --current-candidate-commit d16357900f6f3b472ff6e0309e7bf7fe70ad9b06`.

```gate
id: talaria-v0-5-0-live-acceptance
verdict: BLOCKED
review-by: 2026-09-30
blocks-on: row-24 blocked
```

## Evidence table

| Item | Condition | Status |
| ---: | --- | --- |
| 24 | Agent and queue non-color states | **blocked** |
<!-- END GENERATED ACCEPTANCE STATUS -->

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
| Current reviewed candidate commit | `d16357900f6f3b472ff6e0309e7bf7fe70ad9b06` |
| Current candidate wheel SHA-256 | `7322029e90617c4ef91657f16c5cf0e1baad92ee4a02907b4705be8df8d08625` |
| Receipt candidate identities | `d163579` / `7322029e90617c4ef91657f16c5cf0e1baad92ee4a02907b4705be8df8d08625` (2 install, 42 item receipts) |
| Receipt counts | 2 install; 42 item; 44 current; 0 stale; 0 invalid |
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
| 1 | Installed artifact | `PASS` | `PASS` | The installed executable launched from the isolated frozen-wheel environment; visible v0.5.0 and hierarchy evidence were produced without a source or editable install. |
| 2 | Live primary route | `PASS` | `PASS` | A real prompt completed with TALARIA-T1-C4-PRIMARY-OK; the recorded live frames and status bar named provider opencode-go and model muse-spark-1.2-contributor. The approved primary route completed two bounded real turns; the 144-column screenshot names muse-spark-1.2-contributor and the primary capture records the live answer. No fallback was requested or used. The visible commands.catalog compatibility warning did not prevent completion. |
| 3 | Main hierarchy | — | `PASS` | The current installed replay shows the transcript, inspector, composer, help row, and one-row status bar in the specified main hierarchy. |
| 4 | Refined Default | `PASS` | — | Refined Default rendered the populated transcript, composer, agent row, status surfaces, inspector and diff with the specified cool-neutral hierarchy. |
| 5 | Dark Green Terminal | `PASS` | — | Dark Green Terminal rendered every populated surface with its terminal-green palette while retaining readable hierarchy and state markers. |
| 6 | Neutral Dark | `PASS` | — | Neutral Dark rendered every populated surface with neutral dark surfaces and legible semantic contrast. |
| 7 | Accessible High Contrast | `PASS` | — | Accessible High Contrast rendered the populated application with high-contrast borders, text, semantic markers, focus treatment and populated diff. |
| 8 | Preview cancellation | `PASS` | — | The picker previewed Dark Green Terminal and Neutral Dark, Escape restored Refined Default, and the scratch configuration SHA-256 remained 439427b14bd9b50d9aa8d0c99c8e1a9b51097630c90b94e81802150bb2ad5fcf. |
| 9 | Explicit save and precedence | `PASS` | — | User save persisted Dark Green Terminal, repository save overrode it with Neutral Dark, and an unsaved Accessible High Contrast session selection left both configuration hashes unchanged. |
| 10 | Theme fallback notice | `PASS` | — | The installed importer reported exactly 56 Refined Default fallbacks and 19 warnings; startup named unavailable theme not-installed, displayed the fallback notice, then listed and applied partial-fallback. |
| 11 | Visual Studio Code import | `PASS` | — | Two installed imports from the committed Visual Studio Code fixture produced identical stored bytes with SHA-256 2ab65e1fb8b3489d49fd25b5e59e70db56cb2dc549fc7b302c4daf496b46820d; restart loaded vscode-import-evidence with no fallback notice. |
| 12 | All status segments | — | `PASS` | At 144 columns the capture shows cwd, git branch, agent model, context, task progress, connection, and version in the required one-row status bar. |
| 13 | Status configuration | — | `PASS` | The configured process reports the requested visible order while ignoring an unknown segment with a visible notice; the supplemental fresh process restores the default order. |
| 14 | Status responsive sequence | — | `PASS` | The raw capture records all eighteen required widths with real terminal geometry; the 20-47 band keeps task progress and connection, connection survives below 20, and the bar remains one row. |
| 15 | Status failure visibility | `PASS` | `PASS` | Invalid command, interval, and all three width caps produced bounded visible fallback notices; a second drive rendered literal bounded acceptance-status-t1 output. Malformed command, invalid interval, and invalid width caps render visible fallback notices; the supplemental bounded-command capture shows literal status output. |
| 16 | Inspector dock and resize | — | `PASS` | The inspector docks at the breakpoint, resizes in four-column steps, clamps at 28 and 48 columns, and preserves its held task, context, changed-file, and operation content. |
| 17 | Inspector content and empty states | — | `PASS` | Separate populated and header-only drives show all four inspector headings; every empty section renders the complete none-available-from-this-session sentence with wrapping and no synthetic row. |
| 18 | Inspector responsive state | — | `PASS` | The inspector follows dock, overlay, manual-close, and restore behavior across the responsive breakpoint; a separate fresh process restores the default dock geometry. |
| 19 | Side-by-side diff | — | `PASS` | Held state with three files and five hunks renders side-by-side at 112 or more columns with aligned intraline changes and long scrollable lines. |
| 20 | Unified fallback | — | `PASS` | The same held diff falls back to unified at 111 columns, visibly refuses side-by-side below its boundary, and preserves state across the transition. |
| 21 | Diff navigation and boundary | — | `PASS` | Navigation crosses files and hunks and reaches boundaries; the modal and hints expose only read-only navigation and view commands, and edit-like keys reveal no mutation surface. |
| 22 | Composer caret location | `PASS` | — | Focus moved among composer, transcript, agent, prompt, and inspector surfaces; caret text and reserved focus cues changed while composer and footer geometry remained stable. |
| 23 | Connection non-color states | `PASS` | — | The healthy approved route painted [..] wait and [ok] up; stopping only isolated dashboard PID 48915 painted [~] retry and [x] down, the identical command restored it as PID 11988, and item 33 separately proves [!] auth. |
| 24 | Agent and queue non-color states | `BLOCKED` | — | The current candidate rendered all seven agent states plus empty, waiting, blocked, approval, and clarification queue forms in monochrome. Blocked on current evidence: Hermes 0.21.0 assigns request identifiers to live approvals, while shipped replay cannot encode the keyless admin-polled shape that alone becomes possibly duplicate; creating that shape would simulate acceptance. |
| 25 | Transcript identity without color | `PASS` | — | Operator, assistant, reasoning, tool activity, session, and fault entries remained distinct by fixed glyph and label in monochrome, with no trailing spacer rows. |
| 26 | Reduced motion | `PASS` | — | A standard-motion drive showed working ellipsis while ui.reduced_motion=true restarted into static [..] working; completion and state updates remained live in both drives. |
| 27 | Stable unpinned scroll | `PASS` | — | Upward wheel input unpinned follow mode; appends, status and agent updates, theme preview cancellation, inspector toggles, and the real 132-to-119-to-132 resize preserved the reading anchor. |
| 28 | Stable pinned scroll | `PASS` | — | F5 pinned follow mode through appends, status updates and a real 132-to-119-to-132 resize; manual wheel input then unpinned before F5 restored bottom following. |
| 29 | Wide screenshot | — | `PASS` | The 132-by-36 capture shows the required wide hierarchy with a docked 36-column inspector and one-row status bar. |
| 30 | Narrow screenshot | — | `PASS` | The 78-by-36 capture shows the required narrow hierarchy with overlay-only inspector behavior and unified read-only diff mode. |
| 31 | Malformed Visual Studio Code import | `PASS` | — | The installed importer rejected the committed malformed Visual Studio Code fixture with exit 3, reported the strict JSON error, and created no malformed-acceptance theme. |
| 32 | Session-only status toggle | — | `PASS` | The running process hides cwd only for that session; the configuration hash and nanosecond timestamp stay identical, and a fresh process restores cwd. |
| 33 | Dead gateway credential | `PASS` | `PASS` | After only isolated dashboard 8790 restarted, the stale scratch credential produced visible HTTP 403 authentication failure and [!] auth without a hang, blank screen, or credential disclosure. A deliberately invalid scratch credential produced a visible HTTP 403 authentication failure and failure status with no hang, blank, or credential text. Authentication prevented model reach; no fallback was used. |
| 34 | Killed session | `PASS` | `PASS` | A real primary-route turn completed with TALARIA-T1-C4-KILLED-SESSION-LIVE-OK; session.close returned closed true for only that acceptance-created session, and the next prompt visibly failed with gateway code 4001 session not found while Talaria remained responsive. The approved primary route completed TALARIA-T2-LIVE-OK; closing exactly that recorded session returned closed true, and the next prompt was visibly refused with code 4001 session not found while Talaria remained responsive and exited cleanly. No fallback was requested or used. |
| 35 | Restart-only configuration | `PASS` | `PASS` | Changing the scratch theme from Accessible High Contrast to Neutral Dark while Talaria ran had no live effect; only a clean restart applied Neutral Dark. The running process kept connection and version after the scratch file changed; only the restarted process applied cwd, connection, and version. |
| 36 | Cross-tester evidence | `PASS` | `PASS` | Talaria-t1 supplies current-candidate evidence for every assigned and shared item; all item results carry harness commit d16357900f6f3b472ff6e0309e7bf7fe70ad9b06 and the live receipts name the approved route without fallback. Talaria-t2 independently produced current-candidate evidence for its thirteen owned and all seven shared items, including primary-route live evidence. |
<!-- END GENERATED ACCEPTANCE VERDICTS -->

## Evidence custody

Reviewed raw captures, screenshots, exact install receipts, pseudo-terminal results, and item
receipts are committed under `docs/acceptance/v0.5.0/evidence/`. The T1 screenshots were rendered
directly from raw pseudo-terminal bytes with Pyte and Pillow; no Computer Use or graphical user
interface automation was used. The T1 publication review found no scratch credential, token-query
parameter, bearer header, email address, or operator home path. Every current item receipt passed
file-hash and candidate validation. A preceding T1 sweep is explicitly retained as superseded under
`docs/acceptance/v0.5.0/evidence/t1/superseded/d9c82443/`.

## Repository checks

- `uv run ruff check .`: passed.
- `uv run mypy`: passed with no issues.
- `/opt/homebrew/bin/uv run pytest`: passed, 2,444 tests passed and 7 skipped in 601.89 seconds.
- `uv run bandit -r talaria -q`: passed; Bandit emitted only its existing comment-token warnings.
- `git diff --check`: passed after the evidence update.
