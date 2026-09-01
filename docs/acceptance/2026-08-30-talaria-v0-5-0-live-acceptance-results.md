# Talaria v0.5.0 real-terminal acceptance evidence

This Talaria repository document is the issue #110 evidence record for the exact installed v0.5.0
candidate. Both testers used independently installed wheels and isolated scratch configuration
directories. Earlier runs remain preserved under explicitly named `superseded/` directories and do
not contribute to the generated verdict.

<!-- BEGIN GENERATED ACCEPTANCE STATUS -->
## Status: **STALE**

This verdict is generated from `artifact-manifest.json`; it is not maintained by hand. The manifest records 21 current receipts, 23 stale receipts, and 0 invalid item receipts.
Regenerate it with `uv run python -m scripts.acceptance.v050_records refresh --current-candidate-commit 788fc791fadd701cb74b7db8686c0a8bb444b8f8`.

```gate
id: talaria-v0-5-0-live-acceptance
verdict: STALE
review-by: 2026-09-30
blocks-on: row-1 missing
blocks-on: row-2 missing
blocks-on: row-4 missing
blocks-on: row-5 missing
blocks-on: row-6 missing
blocks-on: row-7 missing
blocks-on: row-8 missing
blocks-on: row-9 missing
blocks-on: row-10 missing
blocks-on: row-11 missing
blocks-on: row-15 missing
blocks-on: row-22 missing
blocks-on: row-23 missing
blocks-on: row-24 missing
blocks-on: row-25 missing
blocks-on: row-26 missing
blocks-on: row-27 missing
blocks-on: row-28 missing
blocks-on: row-31 missing
blocks-on: row-33 missing
blocks-on: row-34 missing
blocks-on: row-35 missing
blocks-on: row-36 missing
```

## Evidence table

| Item | Condition | Status |
| ---: | --- | --- |
| 1 | Installed artifact | **missing** |
| 2 | Live primary route | **missing** |
| 4 | Refined Default | **missing** |
| 5 | Dark Green Terminal | **missing** |
| 6 | Neutral Dark | **missing** |
| 7 | Accessible High Contrast | **missing** |
| 8 | Preview cancellation | **missing** |
| 9 | Explicit save and precedence | **missing** |
| 10 | Theme fallback notice | **missing** |
| 11 | Visual Studio Code import | **missing** |
| 15 | Status failure visibility | **missing** |
| 22 | Composer caret location | **missing** |
| 23 | Connection non-color states | **missing** |
| 24 | Agent and queue non-color states | **missing** |
| 25 | Transcript identity without color | **missing** |
| 26 | Reduced motion | **missing** |
| 27 | Stable unpinned scroll | **missing** |
| 28 | Stable pinned scroll | **missing** |
| 31 | Malformed Visual Studio Code import | **missing** |
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
| Receipt candidate identities | `0f5c8e3` / `720cc654d06a8075e0dc032289e0c1320b177bc5ee2bcebc1a962f8ea9d76e3b` (1 install, 22 item receipts)<br>`788fc79` / `bc5406c8b201c08758b8c51db8ab54059fa291be93fa06766df662be9dea73be` (1 install, 20 item receipts) |
| Receipt counts | 2 install; 42 item; 21 current; 23 stale; 0 invalid |
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
| 1 | Installed artifact | `STALE — prior PASS @ 0f5c8e3` | `PASS` | Installed executable launched from the isolated wheel environment; hierarchy and /bar evidence were produced without a source or editable install. |
| 2 | Live primary route | `STALE — prior PASS @ 0f5c8e3` | `PASS` | A real Hermes-backed turn returned 1517; the inspector and final status-bar agent_model segment both named muse-spark-1.2-contributor. The status bar showed the healthy [ok] connection form; no fallback route was attempted. Primary route opencode-go / muse-spark-1.2-contributor is exact in the full-width agent_model segment and recording; the bounded live reply completed. The bound raw capture starts at privacy-safe width 119, while the bound final screenshot comes from a fresh successful turn expanded to 144 with the inspector closed. No fallback was requested or used. |
| 3 | Main hierarchy | — | `PASS` | Transcript, StatusRegion, Composer, HelpBar, and the one-row BottomStatusBar retain the specified vertical hierarchy. |
| 4 | Refined Default | `STALE — prior PASS @ 0f5c8e3` | — | Refined Default settled across transcript, composer, inspector, status region, help bar and bottom status bar with distinct user, assistant, reasoning, tool, divider, code and diff roles. |
| 5 | Dark Green Terminal | `STALE — prior PASS @ 0f5c8e3` | — | Dark Green Terminal settled on every visible surface; the terminal palette remained legible without collapsing semantic roles. |
| 6 | Neutral Dark | `STALE — prior PASS @ 0f5c8e3` | — | Neutral Dark settled on the full interface and preserved transcript, chrome, status and diff separation. |
| 7 | Accessible High Contrast | `STALE — prior PASS @ 0f5c8e3` | — | Accessible High Contrast settled on all required surfaces with the visual specification's high-contrast token relationships visible in the capture. |
| 8 | Preview cancellation | `STALE — prior PASS @ 0f5c8e3` | — | The picker preview changed twice, Escape restored Refined Default, and the prepared scratch config retained SHA-256 d52792f63d1007367c808ded991d822a3d3180f02cfbd0d4a5486486a100ef5 and mtime 1788206599. |
| 9 | Explicit save and precedence | `STALE — prior PASS @ 0f5c8e3` | — | Three installed-binary legs proved user save, repository override, and session-only preview precedence; the session leg left both user and repository files byte-identical. The final user config hash was 126e3d59fce2646f70d5aa5748cefe909e66538d507aeef35d25a78918c7df7f and the repository config hash was fabff0bbd0902a86aac6e7c9e1b7a20f6f309b8f0eec8659e34b0f116a2aa101. |
| 10 | Theme fallback notice | `STALE — prior PASS @ 0f5c8e3` | — | An unknown configured theme produced the visible fallback notice and Refined Default; the installed partial import remained selectable, and its import capture recorded complete fallback accounting. |
| 11 | Visual Studio Code import | `STALE — prior PASS @ 0f5c8e3` | — | Two imports of the shipped Visual Studio Code fixture produced identical saved bytes with SHA-256 2ab65e1fb8b3489d49fd25b5e59e70db56cb2dc549fc7b302c4daf496b46820d. The installed imported theme loaded on restart without a fallback notice and visibly styled transcript comments and diff content. |
| 12 | All status segments | — | `PASS` | At 144 columns all seven default segments appear in order with six separators on exactly one row. |
| 13 | Status configuration | — | `PASS` | The configured five recognized segments render in order, omitted segments stay absent, and the unknown segment produces a visible notice. Supplemental restart capture item-13-restart-c2.ansi proves the same configuration after a fresh process. |
| 14 | Status responsive sequence | — | `PASS` | The raw capture records all eighteen required widths with real unpinned terminal geometry; forms compact and segments drop by breakpoint while connection remains on one row. |
| 15 | Status failure visibility | `STALE — prior PASS @ 0f5c8e3` | `PASS` | Invalid status interval and width values produced explicit notices and documented defaults; a separate bounded replay capture rendered the configured literal command acceptance-status-t1. Malformed command, invalid interval, and three invalid width caps each render a visible fallback notice. Supplemental item-15-command-c2.ansi shows literal bounded acceptance-status output. |
| 16 | Inspector dock and resize | — | `PASS` | Focused inspector width changes in four-column steps, clamps at 48 and 28, remains right-docked, and preserves held data. |
| 17 | Inspector content and empty states | — | `PASS` | All four empty headings remain and each shows the complete none-available sentence with cycle-2 wrapping across the exercised widths. Supplemental item-17-populated-c2.ansi proves accurate populated state. |
| 18 | Inspector responsive state | — | `PASS` | The primary raw capture proves open auto-restore, manual-close persistence, and narrow overlay behavior; the fresh-process screenshot restores the expanded 36-column dock. |
| 19 | Side-by-side diff | — | `PASS` | The held three-file, five-hunk diff renders aligned side-by-side panes, line numbers, hunk markers, syntax and intraline changes, read-only labels, and temporary inspector collapse. |
| 20 | Unified fallback | — | `PASS` | The raw drive preserves file, hunk, and anchor across 112-to-111-to-112 transitions and u/s preferences; the 111-column screenshot shows forced unified mode and the header-row refusal. |
| 21 | Diff navigation and boundary | — | `PASS` | Hunk and file navigation cycle deterministically, long-line horizontal movement remains clipped, and neither hints nor unbound mutation-like keys expose an edit, stage, revert, discard, or apply surface. |
| 22 | Composer caret location | `STALE — prior PASS @ 0f5c8e3` | — | The composer caret and focus cues stayed in the composer row while transcript, status and inspector remained stable through focus movement and resize. |
| 23 | Connection non-color states | `STALE — prior PASS @ 0f5c8e3` | — | Separate drives reached all five non-colour connection forms: [..] wait, [ok] up, [~] retry, [x] down, and [!] auth. The genuine reconnect cycle held and resumed the same authorized dashboard process; the raw capture contains both 'connection lost — reconnecting' and [~] retry before recovery. No fallback route was attempted. |
| 24 | Agent and queue non-color states | `STALE — prior BLOCKED @ 0f5c8e3` | — | The deterministic corpus displayed queued, running, completed, error, failed, interrupted and timeout agent states plus waiting approval and clarification rows in monochrome. Blocked: the gateway assigns request identifiers, so live approvals are anchored and never possibly duplicate; replay cannot encode the keyless admin-polled shape without simulating acceptance. |
| 25 | Transcript identity without color | `STALE — prior PASS @ 0f5c8e3` | — | User, assistant, reasoning, tool/subagent, session and error identities remained distinguishable without colour on consecutive rows, with no trailing Markdown spacer rows after assistant or reasoning entries. |
| 26 | Reduced motion | `STALE — prior PASS @ 0f5c8e3` | — | Separate restart legs exercised ui.reduced_motion=false and true with the same motion-producing corpus; the reduced leg removed focus animation while elapsed updates, transcript anchoring and replay progress continued. |
| 27 | Stable unpinned scroll | `STALE — prior PASS @ 0f5c8e3` | — | A real wheel event unpinned the long transcript at the reading anchor; later replay frames and actual 132-to-119-to-132-column resizes preserved the reading region rather than jumping to the newest output. |
| 28 | Stable pinned scroll | `STALE — prior PASS @ 0f5c8e3` | — | After returning to follow mode, the long transcript stayed pinned to NEWEST-BOTTOM-ENTRY across later output and actual 132-to-119-to-132-column resizes. |
| 29 | Wide screenshot | — | `PASS` | The required 132-by-36 wide frame has the 36-column inspector dock, stable main hierarchy, and fixed one-row help and status bars. |
| 30 | Narrow screenshot | — | `PASS` | The required 78-by-36 narrow evidence keeps the inspector out of the dock, preserves fixed footer rows, and shows the held diff in unified mode. |
| 31 | Malformed Visual Studio Code import | `STALE — prior PASS @ 0f5c8e3` | — | The shipped malformed Visual Studio Code fixture exited 3 with a strict-JSON diagnostic, and no malformed-acceptance theme artifact was created. |
| 32 | Session-only status toggle | — | `PASS` | The session command immediately hides cwd without changing config bytes or modification time; the fresh-process screenshot restores cwd. |
| 33 | Dead gateway credential | `STALE — prior PASS @ 0f5c8e3` | `PASS` | The authorized isolated dashboard restart changed the listener from PID 74947 to PID 19785 using the identical hermes -p default dashboard --host 127.0.0.1 --port 8790 --no-open command. Talaria reached a visible authentication-failed HTTP 403 state with [!] auth instead of hanging or showing a blank interface; the selected capture contains no credential value, token query, or bearer header. The replacement dashboard remains detached under parent PID 1, and the scratch credential was freshly re-minted after the failure leg. A deliberately invalid scratch credential produces visible HTTP 403 authentication failure with no hang, blank, or credential text. The approved model route was requested but authentication prevented model reach; no fallback was used. |
| 34 | Killed session | `STALE — prior PASS @ 0f5c8e3` | `PASS` | A completed live turn returned TALARIA-T1-LIVE-OK on muse-spark-1.2-contributor; closing only that recorded session returned closed=true, and the next prompt produced an explicit code 4001 session not found response while dashboard process 19785 remained listening. A primary-route session completed TALARIA-T2-LIVE-OK, exact recorded-session close returned true, and the next prompt was visibly refused with code 4001 session not found. No fallback was requested or used. |
| 35 | Restart-only configuration | `STALE — prior PASS @ 0f5c8e3` | `PASS` | The running interface stayed Accessible High Contrast after the scratch config changed to Neutral Dark; only a clean restart applied Neutral Dark. The running process kept connection,version after the scratch file changed; the restarted screenshot applies cwd,connection,version. |
| 36 | Cross-tester evidence | `STALE — prior PASS @ 0f5c8e3` | `PASS` | Talaria-t1 independently supplies current-candidate receipts for shared items 2, 15, 33, 34, 35, and this cross-tester half, plus every assigned-track item; each receipt names a real capture and screenshot. The T1 live receipts for items 2 and 34 both observed opencode-go / muse-spark-1.2-contributor with no fallback; full cross-tester completion is a generated-manifest property once the parallel T2 item 36 receipt is combined. Talaria-t2 independently produced current-candidate receipts for its thirteen owned and all seven shared items, including primary-route live evidence. |
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
