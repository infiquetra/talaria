# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. Both testers used independently installed wheels and isolated scratch configuration
directories. Earlier runs remain preserved under explicitly named `superseded/` directories and do
not contribute to the generated verdict.

<!-- BEGIN GENERATED ACCEPTANCE STATUS -->
## Status: **STALE**

This verdict is generated from `artifact-manifest.json`; it is not maintained by hand. The manifest records 23 current receipts, 20 stale receipts, and 0 invalid item receipts.
Regenerate it with `uv run python -m scripts.acceptance.v050_records refresh --current-candidate-commit 788fc791fadd701cb74b7db8686c0a8bb444b8f8`.

```gate
id: talaria-v0-5-0-live-acceptance
verdict: STALE
review-by: 2026-09-30
blocks-on: row-1 missing
blocks-on: row-2 missing
blocks-on: row-3 missing
blocks-on: row-12 missing
blocks-on: row-13 missing
blocks-on: row-14 missing
blocks-on: row-15 missing
blocks-on: row-16 missing
blocks-on: row-17 missing
blocks-on: row-18 missing
blocks-on: row-19 missing
blocks-on: row-20 missing
blocks-on: row-21 missing
blocks-on: row-24 blocked
blocks-on: row-29 missing
blocks-on: row-30 missing
blocks-on: row-32 missing
blocks-on: row-33 missing
blocks-on: row-34 missing
blocks-on: row-35 missing
blocks-on: row-36 missing
```

## Evidence table

| Item | Condition | Status |
| ---: | --- | --- |
| 1 | Missing checklist receipt | **missing** |
| 2 | Live primary route | **missing** |
| 3 | Main hierarchy | **missing** |
| 12 | All status segments | **missing** |
| 13 | Status configuration | **missing** |
| 14 | Status responsive sequence | **missing** |
| 15 | Status failure visibility | **missing** |
| 16 | Inspector dock and resize | **missing** |
| 17 | Inspector content and empty states | **missing** |
| 18 | Inspector responsive state | **missing** |
| 19 | Side-by-side diff | **missing** |
| 20 | Unified fallback | **missing** |
| 21 | Diff navigation and boundary | **missing** |
| 24 | Agent and queue non-color states | **blocked** |
| 29 | Wide screenshot | **missing** |
| 30 | Narrow screenshot | **missing** |
| 32 | Session-only status toggle | **missing** |
| 33 | Dead gateway credential | **missing** |
| 34 | Killed session | **missing** |
| 35 | Restart-only configuration | **missing** |
| 36 | Cross-tester evidence | **missing** |
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
| Manifest status | `STALE` |
| Current reviewed candidate commit | `788fc791fadd701cb74b7db8686c0a8bb444b8f8` |
| Current candidate wheel SHA-256 | `bc5406c8b201c08758b8c51db8ab54059fa291be93fa06766df662be9dea73be` |
| Receipt candidate identities | `0f5c8e3` / `720cc654d06a8075e0dc032289e0c1320b177bc5ee2bcebc1a962f8ea9d76e3b` (1 install, 19 item receipts)<br>`788fc79` / `bc5406c8b201c08758b8c51db8ab54059fa291be93fa06766df662be9dea73be` (1 install, 22 item receipts) |
| Receipt counts | 2 install; 41 item; 23 current; 20 stale; 0 invalid |
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
| 1 | Installed artifact | `PASS` | `STALE — prior PASS @ 0f5c8e3` | The install receipts prove fresh, non-editable installations of the current reviewed wheel. |
| 2 | Live primary route | `PASS` | `STALE — prior PASS @ 0f5c8e3` | A real prompt completed with TALARIA-T1-C2-PRIMARY-OK; the inspector and status bar named muse-spark-1.2-contributor on the isolated dashboard. A real Hermes-backed turn returned TALARIA-T2-PRIMARY-OK; the final agent_model segment displayed OpenCode Go / muse-spark-1.2-contributor and the inspector named muse-spark-1.2-contributor. The final status bar showed [ok] connected; the primary route completed and no fallback was requested or used. |
| 3 | Main hierarchy | — | `STALE — prior PASS @ 0f5c8e3` | Main body, composer, help bar, and bottom status bar preserve the specified hierarchy. |
| 4 | Refined Default | `PASS` | — | Refined Default rendered the populated transcript, composer, agent row, status surfaces, inspector and diff with the specified cool-neutral hierarchy. |
| 5 | Dark Green Terminal | `PASS` | — | Dark Green Terminal rendered every populated surface with its terminal-green palette while retaining readable hierarchy and state markers. |
| 6 | Neutral Dark | `PASS` | — | Neutral Dark rendered every populated surface with neutral dark surfaces and legible semantic contrast. |
| 7 | Accessible High Contrast | `PASS` | — | Accessible High Contrast rendered the populated application with high-contrast borders, text, semantic markers and focus treatment. |
| 8 | Preview cancellation | `PASS` | — | The picker previewed Neutral Dark, Escape cancelled it, and the final frame restored the Refined Default palette without saving the preview. |
| 9 | Explicit save and precedence | `PASS` | — | User save persisted Dark Green Terminal, repository save overrode it with Neutral Dark, and an unsaved session preview of Accessible High Contrast left both configuration hashes unchanged. |
| 10 | Theme fallback notice | `PASS` | — | Startup named unavailable theme not-installed, displayed the fallback notice, then listed and applied the imported partial-fallback theme. |
| 11 | Visual Studio Code import | `PASS` | — | Two imports from the committed Visual Studio Code fixture produced identical stored bytes, and the installed binary restarted into vscode-import-evidence without a fallback notice. |
| 12 | All status segments | — | `STALE — prior PASS @ 0f5c8e3` | At 144 columns all seven default status segments render in order on one row with six separators. |
| 13 | Status configuration | — | `STALE — prior PASS @ 0f5c8e3` | Configured order is honored, hidden segments stay absent after restart, and the unknown segment produces a visible notice. |
| 14 | Status responsive sequence | — | `STALE — prior PASS @ 0f5c8e3` | With COLUMNS and LINES unpinned, the real child reflowed at every breakpoint from 144 through 19 columns; the final 19-column frame contains only the connection [ok] form and exactly fifteen background cells. |
| 15 | Status failure visibility | `PASS` | `STALE — prior PASS @ 0f5c8e3` | Invalid status configuration produced bounded visible notices for interval and width fallbacks and disabled the empty command; a second drive rendered the bounded acceptance-status-t1 command result. Malformed status values produce visible fallback notices, and the separate command leg renders literal bounded output. |
| 16 | Inspector dock and resize | — | `STALE — prior PASS @ 0f5c8e3` | The inspector docks right, resizes in four-column steps, clamps at 28 and 48 columns, and retains its data. |
| 17 | Inspector content and empty states | — | `STALE — prior PASS @ 0f5c8e3` | Separate populated and header-only sessions show accurate inspector data and honest none-available states without a needs-you row. |
| 18 | Inspector responsive state | — | `STALE — prior PASS @ 0f5c8e3` | The corrected drive emitted both Inspector [docked 36] and Inspector [overlay] across real 120-to-119-column transitions, restored the manual preference at 120, and a supplemental 119-column capture visibly confirms overlay without transcript reflow. |
| 19 | Side-by-side diff | — | `STALE — prior PASS @ 0f5c8e3` | The three-file, five-hunk diff renders aligned side-by-side panes, line numbers, marks, intraline changes, and read-only status. |
| 20 | Unified fallback | — | `STALE — prior PASS @ 0f5c8e3` | At a real 111-column terminal width the viewer remained unified and visibly reported side-by-side needs 112 columns; the full drive also crossed 112 and 111 repeatedly without losing diff context. |
| 21 | Diff navigation and boundary | — | `STALE — prior PASS @ 0f5c8e3` | File, hunk, and horizontal navigation work; palette and viewer expose only read-only navigation and view controls. |
| 22 | Composer caret location | `PASS` | — | Focus moved among transcript, inspector, and composer; the caret label and visible insertion point followed the focused surface and returned to composer. |
| 23 | Connection non-color states | `PASS` | — | A healthy connection painted [..] wait then [ok] up; stopping only the isolated dashboard produced [~] retry and then [x] down, while the separate item 33 capture proves [!] auth. |
| 24 | Agent and queue non-color states | `BLOCKED` | — | The current corpus rendered queued, running, completed, error, failed, interrupted and timeout agent forms plus blocked, approval and clarification rows in monochrome. Blocked: Hermes v0.21.0 assigns request identifiers to live approvals, while the shipped replay format cannot encode the keyless admin-polled shape that alone becomes possibly duplicate; creating it would simulate acceptance. |
| 25 | Transcript identity without color | `PASS` | — | Operator, reasoning, assistant, tool, session, and error entries remained distinguishable by fixed glyph and label identity with NO_COLOR enabled. |
| 26 | Reduced motion | `PASS` | — | A standard-motion drive showed the working ellipsis while ui.reduced_motion=true restarted into the static [..] working form before the same completion. |
| 27 | Stable unpinned scroll | `PASS` | — | Keyboard scrolling unpinned follow mode, a real 132-to-119-to-132 resize preserved the reading anchor, and new transcript output did not jump to the bottom. |
| 28 | Stable pinned scroll | `PASS` | — | With follow mode pinned, the same real resize cycle and new output kept the newest bottom entry visible. |
| 29 | Wide screenshot | — | `STALE — prior PASS @ 0f5c8e3` | After a real 144-to-132-column resize, the wide capture retains the compact one-row footer and the 36-column docked inspector without clipping. |
| 30 | Narrow screenshot | — | `STALE — prior PASS @ 0f5c8e3` | After a real 132-to-78-column resize, the narrow capture uses unified diff, has no docked inspector, and keeps a single compact footer row. |
| 31 | Malformed Visual Studio Code import | `PASS` | — | The installed theme importer rejected the committed malformed Visual Studio Code fixture as non-strict JSON with exit 3 and did not create malformed-acceptance.json. |
| 32 | Session-only status toggle | — | `STALE — prior PASS @ 0f5c8e3` | The configured segment toggles immediately, the config file remains byte- and timestamp-identical, and restart restores it. |
| 33 | Dead gateway credential | `PASS` | `STALE — prior PASS @ 0f5c8e3` | The authorized isolated dashboard restart changed the listener from PID 95480 to PID 19596 with the identical hermes -p default dashboard --host 127.0.0.1 --port 8790 --no-open command. The stale scratch credential produced a visible authentication failed HTTP 403 state and [!] auth without a hang, blank interface, token query, bearer header, or credential value in the reviewed capture. A deliberately invalid scratch credential produced a visible authentication failed status and an HTTP 403 handshake rejection without a hang or silent blank; Talaria exited cleanly. The raw capture and rendered screenshot contain neither the invalid credential nor operator private identifiers; the dashboard was not restarted or modified. |
| 34 | Killed session | `PASS` | `STALE — prior PASS @ 0f5c8e3` | A real primary-route turn completed with TALARIA-T1-C2-KILLED-SESSION-LIVE-OK; session.active_list identified exactly that acceptance-created session and session.close returned closed true. The next prompt was visibly refused with gateway code 4001 session not found, while Talaria remained responsive and exited cleanly; the dashboard was not stopped. A real primary-route turn completed, the exact tester-created session was closed, and the next prompt visibly returned gateway code 4001 session not found without a hang or blank. |
| 35 | Restart-only configuration | `PASS` | `STALE — prior PASS @ 0f5c8e3` | Changing the scratch theme from Accessible High Contrast to Neutral Dark did not affect the running process; only the clean restart applied Neutral Dark. The running process reported connection, version both before and after config.toml changed from SHA-256 43bc09486811aa3c6f6dd134e4400890dea3a9d6b0576e16c5d42c39d90e640a to 3faca4f91f40cb7b41e0b1d5da1c252fb75d77d2f32a25d1f9c8935d51365acb. A separate fresh process loaded the changed file and reported cwd, connection, version; the supplemental restart capture is item-35-restart.ansi and its screenshot is item-35-restart.png. |
| 36 | Cross-tester evidence | `PASS` | `STALE — prior PASS @ 0f5c8e3` | Talaria-t1 independently supplies current-candidate receipts for shared items 2, 15, 33, 34, 35, and this cross-tester half, plus every assigned-track item; each receipt names a real capture and screenshot. The T1 live receipts for items 2 and 34 both observed opencode-go / muse-spark-1.2-contributor with no fallback; full cross-tester completion is a generated-manifest property once the parallel T2 item 36 receipt is combined. Talaria-t2 independently supplies current-candidate receipts for shared items 2, 15, 33, 34, 35, and this cross-tester half, plus every assigned-track item; each receipt names a real capture and screenshot. The T2 live receipts for items 2 and 34 both observed opencode-go / muse-spark-1.2-contributor with no fallback; full cross-tester completion remains a manifest property once the parallel T1 item 36 receipt is combined. |
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
