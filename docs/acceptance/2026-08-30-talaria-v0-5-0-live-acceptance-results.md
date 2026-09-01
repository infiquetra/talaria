# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. Both testers used independently installed wheels and isolated scratch configuration
directories. Earlier runs remain preserved under explicitly named `superseded/` directories and do
not contribute to the generated verdict.

<!-- BEGIN GENERATED ACCEPTANCE STATUS -->
## Status: **BLOCKED**

This verdict is generated from `artifact-manifest.json`; it is not maintained by hand. The manifest records 43 current receipts, 0 stale receipts, and 0 invalid item receipts.
Regenerate it with `uv run python -m scripts.acceptance.v050_records refresh --current-candidate-commit 0f5c8e3e44a43c5956f94ec3ccc348b7cdba1398`.

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
| 1 | Installed artifact | `PASS` | `PASS` | The install receipts prove fresh, non-editable installations of the current reviewed wheel. |
| 2 | Live primary route | `PASS` | `PASS` | A real Hermes-backed turn returned 1517; the inspector and final status-bar agent_model segment both named muse-spark-1.2-contributor. The status bar showed the healthy [ok] connection form; no fallback route was attempted. A real Hermes-backed turn returned TALARIA-T2-PRIMARY-OK; the final agent_model segment displayed OpenCode Go / muse-spark-1.2-contributor and the inspector named muse-spark-1.2-contributor. The final status bar showed [ok] connected; the primary route completed and no fallback was requested or used. |
| 3 | Main hierarchy | — | `PASS` | Main body, composer, help bar, and bottom status bar preserve the specified hierarchy. |
| 4 | Refined Default | `PASS` | — | Refined Default settled across transcript, composer, inspector, status region, help bar and bottom status bar with distinct user, assistant, reasoning, tool, divider, code and diff roles. |
| 5 | Dark Green Terminal | `PASS` | — | Dark Green Terminal settled on every visible surface; the terminal palette remained legible without collapsing semantic roles. |
| 6 | Neutral Dark | `PASS` | — | Neutral Dark settled on the full interface and preserved transcript, chrome, status and diff separation. |
| 7 | Accessible High Contrast | `PASS` | — | Accessible High Contrast settled on all required surfaces with the visual specification's high-contrast token relationships visible in the capture. |
| 8 | Preview cancellation | `PASS` | — | The picker preview changed twice, Escape restored Refined Default, and the prepared scratch config retained SHA-256 d52792f63d1007367c808ded991d822a3d3180f02cfbd0d4a5486486a100ef5 and mtime 1788206599. |
| 9 | Explicit save and precedence | `PASS` | — | Three installed-binary legs proved user save, repository override, and session-only preview precedence; the session leg left both user and repository files byte-identical. The final user config hash was 126e3d59fce2646f70d5aa5748cefe909e66538d507aeef35d25a78918c7df7f and the repository config hash was fabff0bbd0902a86aac6e7c9e1b7a20f6f309b8f0eec8659e34b0f116a2aa101. |
| 10 | Theme fallback notice | `PASS` | — | An unknown configured theme produced the visible fallback notice and Refined Default; the installed partial import remained selectable, and its import capture recorded complete fallback accounting. |
| 11 | Visual Studio Code import | `PASS` | — | Two imports of the shipped Visual Studio Code fixture produced identical saved bytes with SHA-256 2ab65e1fb8b3489d49fd25b5e59e70db56cb2dc549fc7b302c4daf496b46820d. The installed imported theme loaded on restart without a fallback notice and visibly styled transcript comments and diff content. |
| 12 | All status segments | — | `PASS` | At 144 columns all seven default status segments render in order on one row with six separators. |
| 13 | Status configuration | — | `PASS` | Configured order is honored, hidden segments stay absent after restart, and the unknown segment produces a visible notice. |
| 14 | Status responsive sequence | — | `PASS` | With COLUMNS and LINES unpinned, the real child reflowed at every breakpoint from 144 through 19 columns; the final 19-column frame contains only the connection [ok] form and exactly fifteen background cells. |
| 15 | Status failure visibility | `PASS` | `PASS` | Invalid status interval and width values produced explicit notices and documented defaults; a separate bounded replay capture rendered the configured literal command acceptance-status-t1. Malformed status values produce visible fallback notices, and the separate command leg renders literal bounded output. |
| 16 | Inspector dock and resize | — | `PASS` | The inspector docks right, resizes in four-column steps, clamps at 28 and 48 columns, and retains its data. |
| 17 | Inspector content and empty states | — | `PASS` | Separate populated and header-only sessions show accurate inspector data and honest none-available states without a needs-you row. |
| 18 | Inspector responsive state | — | `PASS` | The corrected drive emitted both Inspector [docked 36] and Inspector [overlay] across real 120-to-119-column transitions, restored the manual preference at 120, and a supplemental 119-column capture visibly confirms overlay without transcript reflow. |
| 19 | Side-by-side diff | — | `PASS` | The three-file, five-hunk diff renders aligned side-by-side panes, line numbers, marks, intraline changes, and read-only status. |
| 20 | Unified fallback | — | `PASS` | At a real 111-column terminal width the viewer remained unified and visibly reported side-by-side needs 112 columns; the full drive also crossed 112 and 111 repeatedly without losing diff context. |
| 21 | Diff navigation and boundary | — | `PASS` | File, hunk, and horizontal navigation work; palette and viewer expose only read-only navigation and view controls. |
| 22 | Composer caret location | `PASS` | — | The composer caret and focus cues stayed in the composer row while transcript, status and inspector remained stable through focus movement and resize. |
| 23 | Connection non-color states | `PASS` | — | Separate drives reached all five non-colour connection forms: [..] wait, [ok] up, [~] retry, [x] down, and [!] auth. The genuine reconnect cycle held and resumed the same authorized dashboard process; the raw capture contains both 'connection lost — reconnecting' and [~] retry before recovery. No fallback route was attempted. |
| 24 | Agent and queue non-color states | `BLOCKED` | — | The deterministic corpus displayed queued, running, completed, error, failed, interrupted and timeout agent states plus waiting approval and clarification rows in monochrome. Blocked: the gateway assigns request identifiers, so live approvals are anchored and never possibly duplicate; replay cannot encode the keyless admin-polled shape without simulating acceptance. |
| 25 | Transcript identity without color | `PASS` | — | User, assistant, reasoning, tool/subagent, session and error identities remained distinguishable without colour on consecutive rows, with no trailing Markdown spacer rows after assistant or reasoning entries. |
| 26 | Reduced motion | `PASS` | — | Separate restart legs exercised ui.reduced_motion=false and true with the same motion-producing corpus; the reduced leg removed focus animation while elapsed updates, transcript anchoring and replay progress continued. |
| 27 | Stable unpinned scroll | `PASS` | — | A real wheel event unpinned the long transcript at the reading anchor; later replay frames and actual 132-to-119-to-132-column resizes preserved the reading region rather than jumping to the newest output. |
| 28 | Stable pinned scroll | `PASS` | — | After returning to follow mode, the long transcript stayed pinned to NEWEST-BOTTOM-ENTRY across later output and actual 132-to-119-to-132-column resizes. |
| 29 | Wide screenshot | — | `PASS` | After a real 144-to-132-column resize, the wide capture retains the compact one-row footer and the 36-column docked inspector without clipping. |
| 30 | Narrow screenshot | — | `PASS` | After a real 132-to-78-column resize, the narrow capture uses unified diff, has no docked inspector, and keeps a single compact footer row. |
| 31 | Malformed Visual Studio Code import | `PASS` | — | The shipped malformed Visual Studio Code fixture exited 3 with a strict-JSON diagnostic, and no malformed-acceptance theme artifact was created. |
| 32 | Session-only status toggle | — | `PASS` | The configured segment toggles immediately, the config file remains byte- and timestamp-identical, and restart restores it. |
| 33 | Dead gateway credential | `PASS` | `PASS` | The authorized isolated dashboard restart changed the listener from PID 74947 to PID 19785 using the identical hermes -p default dashboard --host 127.0.0.1 --port 8790 --no-open command. Talaria reached a visible authentication-failed HTTP 403 state with [!] auth instead of hanging or showing a blank interface; the selected capture contains no credential value, token query, or bearer header. The replacement dashboard remains detached under parent PID 1, and the scratch credential was freshly re-minted after the failure leg. A deliberately invalid scratch credential produced a visible authentication failed status and an HTTP 403 handshake rejection without a hang or silent blank; Talaria exited cleanly. The raw capture and rendered screenshot contain neither the invalid credential nor operator private identifiers; the dashboard was not restarted or modified. |
| 34 | Killed session | `PASS` | `PASS` | A completed live turn returned TALARIA-T1-LIVE-OK on muse-spark-1.2-contributor; closing only that recorded session returned closed=true, and the next prompt produced an explicit code 4001 session not found response while dashboard process 19785 remained listening. A real primary-route turn completed, the exact tester-created session was closed, and the next prompt visibly returned gateway code 4001 session not found without a hang or blank. |
| 35 | Restart-only configuration | `PASS` | `PASS` | The running interface stayed Accessible High Contrast after the scratch config changed to Neutral Dark; only a clean restart applied Neutral Dark. The running process reported connection, version both before and after config.toml changed from SHA-256 43bc09486811aa3c6f6dd134e4400890dea3a9d6b0576e16c5d42c39d90e640a to 3faca4f91f40cb7b41e0b1d5da1c252fb75d77d2f32a25d1f9c8935d51365acb. A separate fresh process loaded the changed file and reported cwd, connection, version; the supplemental restart capture is item-35-restart.ansi and its screenshot is item-35-restart.png. |
| 36 | Cross-tester evidence | `PASS` | `PASS` | Talaria-t1 independently supplies current-candidate receipts for shared items 2, 15, 33, 34, 35, and this cross-tester half, plus every assigned-track item; each receipt names a real capture and screenshot. The T1 live receipts for items 2 and 34 both observed opencode-go / muse-spark-1.2-contributor with no fallback; full cross-tester completion is a generated-manifest property once the parallel T2 item 36 receipt is combined. Talaria-t2 independently supplies current-candidate receipts for shared items 2, 15, 33, 34, 35, and this cross-tester half, plus every assigned-track item; each receipt names a real capture and screenshot. The T2 live receipts for items 2 and 34 both observed opencode-go / muse-spark-1.2-contributor with no fallback; full cross-tester completion remains a manifest property once the parallel T1 item 36 receipt is combined. |
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
