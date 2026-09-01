# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. Both testers used independently installed wheels and isolated scratch configuration
directories. Earlier runs remain preserved under explicitly named `superseded/` directories and do
not contribute to the generated verdict.

<!-- BEGIN GENERATED ACCEPTANCE STATUS -->
## Status: **BLOCKED**

This verdict is generated from `artifact-manifest.json`; it is not maintained by hand. **BLOCKED**: 43 of 43 expected checklist/tester slots are covered. The evidence set separately contains 44 current receipts (42 item and 2 install). The one-receipt overlap is checklist item 1 for talaria-t2, which has both an item receipt and an install receipt. Item verdicts are 41 pass, 1 blocked, and 0 fail.
The manifest also records 0 stale receipts and 0 invalid item receipts.
Regenerate it with `uv run python -m scripts.acceptance.v050_records refresh --current-candidate-commit 4c2d8dbf0ddfb7f38ba1f228369ae2d929319758`.

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
| Current reviewed candidate commit | `4c2d8dbf0ddfb7f38ba1f228369ae2d929319758` |
| Current candidate wheel SHA-256 | `d5a41c67384e78d9b1048cb7c6524a668c20f8059031e0f5e0a93f0b289f7d88` |
| Receipt candidate identities | `4c2d8db` / `d5a41c67384e78d9b1048cb7c6524a668c20f8059031e0f5e0a93f0b289f7d88` (2 install, 42 item receipts) |
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
| 1 | Installed artifact | `PASS` | `PASS` | Installed executable launched from the isolated frozen-wheel environment; visible v0.5.0 and hierarchy evidence were produced without a source or editable install. |
| 2 | Live primary route | `PASS` | `PASS` | A real prompt completed with TALARIA-T1-C3-PRIMARY-OK; the recorded session.info frame named provider opencode-go and model muse-spark-1.2-contributor, and the status bar showed the same model identity. The approved primary route completed a bounded real turn twice; the 144-column screenshot names muse-spark-1.2-contributor and the primary capture records the live answer. No fallback was requested or used. Hermes 0.21.0 also produced Talaria's visible commands.catalog compatibility warning without preventing completion. |
| 3 | Main hierarchy | — | `PASS` | Transcript and StatusRegion occupy the flexible body; Composer, one-row HelpBar, and one-row BottomStatusBar retain the required vertical hierarchy. |
| 4 | Refined Default | `PASS` | — | Refined Default rendered the populated transcript, composer, agent row, status surfaces, inspector and diff with the specified cool-neutral hierarchy. |
| 5 | Dark Green Terminal | `PASS` | — | Dark Green Terminal rendered every populated surface with its terminal-green palette while retaining readable hierarchy and state markers. |
| 6 | Neutral Dark | `PASS` | — | Neutral Dark rendered every populated surface with neutral dark surfaces and legible semantic contrast. |
| 7 | Accessible High Contrast | `PASS` | — | Accessible High Contrast rendered the populated application with high-contrast borders, text, semantic markers and focus treatment. |
| 8 | Preview cancellation | `PASS` | — | The picker previewed Dark Green Terminal and Neutral Dark, Escape restored Refined Default, and the scratch configuration hash and modification time remained unchanged. |
| 9 | Explicit save and precedence | `PASS` | — | User save persisted Dark Green Terminal, repository save overrode it with Neutral Dark, and an unsaved session preview of Accessible High Contrast left both configuration hashes unchanged. |
| 10 | Theme fallback notice | `PASS` | — | The importer reported 56 Refined Default fallbacks and 19 warnings; startup named unavailable theme not-installed, displayed the fallback notice, then listed and applied partial-fallback. |
| 11 | Visual Studio Code import | `PASS` | — | Two imports from the committed Visual Studio Code fixture produced identical stored bytes with SHA-256 2ab65e1fb8b3489d49fd25b5e59e70db56cb2dc549fc7b302c4daf496b46820d, and the installed binary restarted into vscode-import-evidence without a fallback notice. |
| 12 | All status segments | — | `PASS` | At 144 columns all seven default status segments appear in order with six separators on exactly one row. |
| 13 | Status configuration | — | `PASS` | The configured five recognized segments render in the configured order, omitted segments stay absent, and the unknown segment produces a visible notice; item-13-restart-c3.ansi proves the same after a fresh process. |
| 14 | Status responsive sequence | — | `PASS` | The raw capture records all eighteen required widths with real terminal geometry; the 20-47 band keeps only task_progress and connection, connection survives below 20, and the bar remains one row. |
| 15 | Status failure visibility | `PASS` | `PASS` | Invalid command, interval, and three width caps each produced bounded visible fallback notices; a second drive rendered literal bounded acceptance-status-t1 output. Malformed command, invalid interval, and three invalid width caps each render a visible fallback notice; item-15-command-c3.ansi shows literal bounded acceptance-status output. |
| 16 | Inspector dock and resize | — | `PASS` | Focused inspector width changes in four-column steps, clamps at 48 and 28, remains right-docked, and preserves held data. |
| 17 | Inspector content and empty states | — | `PASS` | All four empty headings remain and each shows the full none-available sentence with current two-row wrapping at width 36; item-17-populated-c3.ansi proves accurate populated held state across widths 28, 36, and 48. |
| 18 | Inspector responsive state | — | `PASS` | The primary drive proves open auto-restore, manual-close persistence, and narrow overlay behavior; the fresh-process screenshot restores the expanded 36-column dock. |
| 19 | Side-by-side diff | — | `PASS` | The held three-file, five-hunk diff renders aligned side-by-side panes, base and working line numbers, hunk markers, syntax and intraline changes, read-only labels, and temporary inspector collapse. |
| 20 | Unified fallback | — | `PASS` | The drive preserves file, hunk, and anchor across 112-to-111-to-112 transitions and u/s preferences; the 111-column screenshot shows forced unified mode and the header-row refusal. |
| 21 | Diff navigation and boundary | — | `PASS` | Hunk and file navigation cycle deterministically, long-line horizontal movement remains clipped, and hints plus unbound mutation-like keys expose no edit, stage, revert, discard, apply, or other write surface. |
| 22 | Composer caret location | `PASS` | — | Focus moved among composer, transcript, agent, prompt, and inspector surfaces; caret text and reserved focus cues changed while the composer and footer geometry remained stable. |
| 23 | Connection non-color states | `PASS` | — | A healthy primary-route connection painted [..] wait and [ok] up; the controlled isolated-dashboard restart painted [~] retry and [x] down, while the separate item 33 capture proves [!] auth. All five remain literal in monochrome evidence. |
| 24 | Agent and queue non-color states | `BLOCKED` | — | The current candidate rendered all seven agent states plus empty, waiting, blocked, approval, and clarification queue forms in monochrome. Blocked on current evidence: Hermes 0.21.0 assigns request identifiers to live approvals, while shipped replay cannot encode the keyless admin-polled shape that alone becomes possibly duplicate; creating that shape would simulate acceptance. |
| 25 | Transcript identity without color | `PASS` | — | Operator, assistant, reasoning, tool activity, session, and fault entries remained distinct by fixed glyph and label in monochrome, with no trailing spacer rows. |
| 26 | Reduced motion | `PASS` | — | A standard-motion drive showed working ellipsis while ui.reduced_motion=true restarted into static [..] working; completion and state updates remained live in both drives. |
| 27 | Stable unpinned scroll | `PASS` | — | Upward wheel input unpinned follow mode; appends, status and agent updates, theme preview cancellation, inspector toggles, and the real 132-to-119-to-132 resize preserved the reading anchor. |
| 28 | Stable pinned scroll | `PASS` | — | F5 pinned follow mode through appends, status updates and a real 132-to-119-to-132 resize; manual wheel input then unpinned before F5 restored bottom following. |
| 29 | Wide screenshot | — | `PASS` | The required 132-by-36 wide frame has the 36-column inspector dock, stable main hierarchy, and fixed one-row help and status bars. |
| 30 | Narrow screenshot | — | `PASS` | The required 78-by-36 narrow evidence keeps the inspector out of the dock, preserves fixed footer rows, and shows the held diff in unified mode. |
| 31 | Malformed Visual Studio Code import | `PASS` | — | The installed importer rejected the committed malformed Visual Studio Code fixture with exit 3, reported the strict JSON error, and created no malformed-acceptance theme. |
| 32 | Session-only status toggle | — | `PASS` | The session command immediately hides cwd without changing configuration bytes or nanosecond modification time; the fresh-process screenshot restores cwd. |
| 33 | Dead gateway credential | `PASS` | `PASS` | Restarting only isolated dashboard 8790 changed its listener from PID 48271 to PID 48915; the now-stale scratch credential produced visible HTTP 403 authentication failure and [!] auth without a hang, blank screen, or credential disclosure. A deliberately invalid scratch credential produced a visible HTTP 403 authentication failure and [!] status with no hang, blank, or credential text. Authentication prevented model reach; no fallback was used. |
| 34 | Killed session | `PASS` | `PASS` | A real primary-route turn completed with TALARIA-T1-C3-KILLED-SESSION-LIVE-OK; session.close returned closed true for only that acceptance-created session, and the next prompt visibly failed with gateway code 4001 session not found while Talaria remained responsive. The approved primary route completed TALARIA-T2-LIVE-OK; closing exactly that recorded session returned closed true, and the next prompt was visibly refused with code 4001 session not found while Talaria remained responsive and exited cleanly. No fallback was requested or used. |
| 35 | Restart-only configuration | `PASS` | `PASS` | Changing the scratch theme from Accessible High Contrast to Neutral Dark while Talaria ran had no live effect; only a clean restart applied Neutral Dark. The running process kept connection,version after the scratch file changed; only the restarted process applied cwd,connection,version. |
| 36 | Cross-tester evidence | `PASS` | `PASS` | Talaria-t1 supplies current-candidate evidence for every assigned and shared item; all item results carry harness commit 4c2d8dbf0ddfb7f38ba1f228369ae2d929319758 and the live receipts name the approved route without fallback. Talaria-t2 independently produced current-candidate evidence for its thirteen owned and all seven shared items, including primary-route live evidence. |
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
