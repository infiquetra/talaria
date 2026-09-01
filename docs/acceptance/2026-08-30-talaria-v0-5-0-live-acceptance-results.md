# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. Both testers used independently installed wheels and isolated scratch configuration
directories. Earlier runs remain preserved under explicitly named `superseded/` directories and do
not contribute to the generated verdict.

<!-- BEGIN GENERATED ACCEPTANCE STATUS -->
## Status: **BLOCKED**

This verdict is generated from `artifact-manifest.json`; it is not maintained by hand. The manifest records 44 current receipts, 0 stale receipts, and 0 invalid item receipts.
Regenerate it with `uv run python -m scripts.acceptance.v050_records refresh --current-candidate-commit 788fc791fadd701cb74b7db8686c0a8bb444b8f8`.

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
| Current reviewed candidate commit | `788fc791fadd701cb74b7db8686c0a8bb444b8f8` |
| Current candidate wheel SHA-256 | `bc5406c8b201c08758b8c51db8ab54059fa291be93fa06766df662be9dea73be` |
| Receipt candidate identities | `788fc79` / `bc5406c8b201c08758b8c51db8ab54059fa291be93fa06766df662be9dea73be` (2 install, 42 item receipts) |
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
| 1 | Installed artifact | `PASS` | `PASS` | Installed executable launched from the isolated wheel environment; hierarchy and /bar evidence were produced without a source or editable install. |
| 2 | Live primary route | `PASS` | `PASS` | A real prompt completed with TALARIA-T1-C2-PRIMARY-OK; the inspector and status bar named muse-spark-1.2-contributor on the isolated dashboard. Primary route opencode-go / muse-spark-1.2-contributor is exact in the full-width agent_model segment and recording; the bounded live reply completed. The bound raw capture starts at privacy-safe width 119, while the bound final screenshot comes from a fresh successful turn expanded to 144 with the inspector closed. No fallback was requested or used. |
| 3 | Main hierarchy | — | `PASS` | Transcript, StatusRegion, Composer, HelpBar, and the one-row BottomStatusBar retain the specified vertical hierarchy. |
| 4 | Refined Default | `PASS` | — | Refined Default rendered the populated transcript, composer, agent row, status surfaces, inspector and diff with the specified cool-neutral hierarchy. |
| 5 | Dark Green Terminal | `PASS` | — | Dark Green Terminal rendered every populated surface with its terminal-green palette while retaining readable hierarchy and state markers. |
| 6 | Neutral Dark | `PASS` | — | Neutral Dark rendered every populated surface with neutral dark surfaces and legible semantic contrast. |
| 7 | Accessible High Contrast | `PASS` | — | Accessible High Contrast rendered the populated application with high-contrast borders, text, semantic markers and focus treatment. |
| 8 | Preview cancellation | `PASS` | — | The picker previewed Neutral Dark, Escape cancelled it, and the final frame restored the Refined Default palette without saving the preview. |
| 9 | Explicit save and precedence | `PASS` | — | User save persisted Dark Green Terminal, repository save overrode it with Neutral Dark, and an unsaved session preview of Accessible High Contrast left both configuration hashes unchanged. |
| 10 | Theme fallback notice | `PASS` | — | Startup named unavailable theme not-installed, displayed the fallback notice, then listed and applied the imported partial-fallback theme. |
| 11 | Visual Studio Code import | `PASS` | — | Two imports from the committed Visual Studio Code fixture produced identical stored bytes, and the installed binary restarted into vscode-import-evidence without a fallback notice. |
| 12 | All status segments | — | `PASS` | At 144 columns all seven default segments appear in order with six separators on exactly one row. |
| 13 | Status configuration | — | `PASS` | The configured five recognized segments render in order, omitted segments stay absent, and the unknown segment produces a visible notice. Supplemental restart capture item-13-restart-c2.ansi proves the same configuration after a fresh process. |
| 14 | Status responsive sequence | — | `PASS` | The raw capture records all eighteen required widths with real unpinned terminal geometry; forms compact and segments drop by breakpoint while connection remains on one row. |
| 15 | Status failure visibility | `PASS` | `PASS` | Invalid status configuration produced bounded visible notices for interval and width fallbacks and disabled the empty command; a second drive rendered the bounded acceptance-status-t1 command result. Malformed command, invalid interval, and three invalid width caps each render a visible fallback notice. Supplemental item-15-command-c2.ansi shows literal bounded acceptance-status output. |
| 16 | Inspector dock and resize | — | `PASS` | Focused inspector width changes in four-column steps, clamps at 48 and 28, remains right-docked, and preserves held data. |
| 17 | Inspector content and empty states | — | `PASS` | All four empty headings remain and each shows the complete none-available sentence with cycle-2 wrapping across the exercised widths. Supplemental item-17-populated-c2.ansi proves accurate populated state. |
| 18 | Inspector responsive state | — | `PASS` | The primary raw capture proves open auto-restore, manual-close persistence, and narrow overlay behavior; the fresh-process screenshot restores the expanded 36-column dock. |
| 19 | Side-by-side diff | — | `PASS` | The held three-file, five-hunk diff renders aligned side-by-side panes, line numbers, hunk markers, syntax and intraline changes, read-only labels, and temporary inspector collapse. |
| 20 | Unified fallback | — | `PASS` | The raw drive preserves file, hunk, and anchor across 112-to-111-to-112 transitions and u/s preferences; the 111-column screenshot shows forced unified mode and the header-row refusal. |
| 21 | Diff navigation and boundary | — | `PASS` | Hunk and file navigation cycle deterministically, long-line horizontal movement remains clipped, and neither hints nor unbound mutation-like keys expose an edit, stage, revert, discard, or apply surface. |
| 22 | Composer caret location | `PASS` | — | Focus moved among transcript, inspector, and composer; the caret label and visible insertion point followed the focused surface and returned to composer. |
| 23 | Connection non-color states | `PASS` | — | A healthy connection painted [..] wait then [ok] up; stopping only the isolated dashboard produced [~] retry and then [x] down, while the separate item 33 capture proves [!] auth. |
| 24 | Agent and queue non-color states | `BLOCKED` | — | The current corpus rendered queued, running, completed, error, failed, interrupted and timeout agent forms plus blocked, approval and clarification rows in monochrome. Blocked: Hermes v0.21.0 assigns request identifiers to live approvals, while the shipped replay format cannot encode the keyless admin-polled shape that alone becomes possibly duplicate; creating it would simulate acceptance. |
| 25 | Transcript identity without color | `PASS` | — | Operator, reasoning, assistant, tool, session, and error entries remained distinguishable by fixed glyph and label identity with NO_COLOR enabled. |
| 26 | Reduced motion | `PASS` | — | A standard-motion drive showed the working ellipsis while ui.reduced_motion=true restarted into the static [..] working form before the same completion. |
| 27 | Stable unpinned scroll | `PASS` | — | Keyboard scrolling unpinned follow mode, a real 132-to-119-to-132 resize preserved the reading anchor, and new transcript output did not jump to the bottom. |
| 28 | Stable pinned scroll | `PASS` | — | With follow mode pinned, the same real resize cycle and new output kept the newest bottom entry visible. |
| 29 | Wide screenshot | — | `PASS` | The required 132-by-36 wide frame has the 36-column inspector dock, stable main hierarchy, and fixed one-row help and status bars. |
| 30 | Narrow screenshot | — | `PASS` | The required 78-by-36 narrow evidence keeps the inspector out of the dock, preserves fixed footer rows, and shows the held diff in unified mode. |
| 31 | Malformed Visual Studio Code import | `PASS` | — | The installed theme importer rejected the committed malformed Visual Studio Code fixture as non-strict JSON with exit 3 and did not create malformed-acceptance.json. |
| 32 | Session-only status toggle | — | `PASS` | The session command immediately hides cwd without changing config bytes or modification time; the fresh-process screenshot restores cwd. |
| 33 | Dead gateway credential | `PASS` | `PASS` | The authorized isolated dashboard restart changed the listener from PID 95480 to PID 19596 with the identical hermes -p default dashboard --host 127.0.0.1 --port 8790 --no-open command. The stale scratch credential produced a visible authentication failed HTTP 403 state and [!] auth without a hang, blank interface, token query, bearer header, or credential value in the reviewed capture. A deliberately invalid scratch credential produces visible HTTP 403 authentication failure with no hang, blank, or credential text. The approved model route was requested but authentication prevented model reach; no fallback was used. |
| 34 | Killed session | `PASS` | `PASS` | A real primary-route turn completed with TALARIA-T1-C2-KILLED-SESSION-LIVE-OK; session.active_list identified exactly that acceptance-created session and session.close returned closed true. The next prompt was visibly refused with gateway code 4001 session not found, while Talaria remained responsive and exited cleanly; the dashboard was not stopped. A primary-route session completed TALARIA-T2-LIVE-OK, exact recorded-session close returned true, and the next prompt was visibly refused with code 4001 session not found. No fallback was requested or used. |
| 35 | Restart-only configuration | `PASS` | `PASS` | Changing the scratch theme from Accessible High Contrast to Neutral Dark did not affect the running process; only the clean restart applied Neutral Dark. The running process kept connection,version after the scratch file changed; the restarted screenshot applies cwd,connection,version. |
| 36 | Cross-tester evidence | `PASS` | `PASS` | Talaria-t1 independently supplies current-candidate receipts for shared items 2, 15, 33, 34, 35, and this cross-tester half, plus every assigned-track item; each receipt names a real capture and screenshot. The T1 live receipts for items 2 and 34 both observed opencode-go / muse-spark-1.2-contributor with no fallback; full cross-tester completion is a generated-manifest property once the parallel T2 item 36 receipt is combined. Talaria-t2 independently produced current-candidate receipts for its thirteen owned and all seven shared items, including primary-route live evidence. |
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
