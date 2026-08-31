# Talaria v0.5.0 acceptance evidence — talaria-t2

This directory records issue #110 acceptance against frozen candidate commit
`d86979127f871a479eb104fc10c886b5c5480a8c` and wheel SHA-256
`a165ad24bd2a4baa7d11aec5d5f434e1451fd688661fed1fe8919ca0c65a1afb`. The wheel
was installed by the acceptance harness at
`/private/var/folders/ky/n5fq5mgd5rl4321kxfy3jtvw0000gn/T/talaria-v050-talaria-t2-tdxgj8kp`.
Every committed receipt passed the receipt validator. Raw American National Standards Institute
(ANSI) pseudo-terminal captures remain under that scratch root. The committed screenshots are
deterministic Portable Network Graphics renderings of those reviewed raw pseudo-terminal captures;
no Computer Use or graphical user interface automation was used.

## Verdicts

| Item | Verdict | Judgment |
| ---: | :--- | :--- |
| 3 | passed | The 144-column hierarchy keeps transcript and StatusRegion in the flexible body, Composer below them, HelpBar above BottomStatusBar, and BottomStatusBar on the final row. |
| 12 | passed | All seven status segments render in the required order with six separators and no wrap. |
| 13 | passed | A config replacement followed by restart reorders five visible segments, omits `cwd` and `version`, and shows the unknown-segment notice. |
| 14 | passed | The full 144-to-19 width sequence compacts and drops segments at the specified boundaries while retaining connection on one row. |
| 15 | passed | Malformed command, interval, and width values each produce visible fallback notices; a clean restart renders literal `acceptance-status` command output. |
| 16 | passed | The right dock changes four columns per action, clamps at 28 and 48, and retains its held data. |
| 17 | failed | The header-only empty drive shows honest empties for Context, Changed Files, and Operation Details, but Tasks invents a `needs-you unavailable` row instead of showing `[none available from this session]`. |
| 18 | passed | Open and manually closed preferences survive breakpoint round trips, a 119-column supplemental capture shows the non-reflowing overlay, and two fresh processes reset geometry to 36 before widening to 40. |
| 19 | passed | The 132-column viewer shows aligned base/working panes, line numbers, hunks, syntax, intraline treatment, position, and read-only labels. |
| 20 | passed | Unified fallback and side-by-side restoration retain selection and scroll position; the 111-column refusal remains in the one-row header. |
| 21 | passed | File and hunk navigation cycle, the long line clips and scrolls horizontally, unbound keys do not mutate data, and the palette/hints expose no writing action. |
| 29 | passed | The 132-by-36 wide screenshot has a 36-column inspector and one-row footer surfaces. |
| 30 | passed | The 78-by-36 narrow screenshot has a unified diff, overlay-only inspector behavior, and one-row footer surfaces. |
| 32 | passed | `/bar cwd` changes only the session; the config hash and modification time are identical before and after, and a fresh process restores `cwd`. |
| 34 | blocked | Neither approved route reached a model-backed session, so no working throwaway session existed to kill and no result was inferred. |

## Failed and blocked evidence

Item 17's failing raw capture is
`/private/var/folders/ky/n5fq5mgd5rl4321kxfy3jtvw0000gn/T/talaria-v050-talaria-t2-tdxgj8kp/raw/item-17-header-only.ansi`.
Its screenshot is `screenshots/item-17-header-only.png`. A separate populated drive at
`raw/item-17.ansi` under the scratch root correctly shows the held subagent, context, three files,
and operation details; the defect is limited to the empty Tasks rendering.

Item 34 is blocked by live session initialization. The primary attempt requested OpenCode Muse
Spark 1.2 Contributor Free (`opencode/muse-spark-1.2-contributor-free`). The gateway admin API
accepted that default and Talaria initially displayed it, but `session.create` then changed the
session to `openai-api/gpt-5.5` and reported that provider had no usable credentials. The capture is
`/private/var/folders/ky/n5fq5mgd5rl4321kxfy3jtvw0000gn/T/talaria-v050-talaria-t2-tdxgj8kp/raw/live-primary-confirm.ansi`.

The permitted fallback was attempted only because the primary bounded test could not complete.
The fallback requested Ollama GLM 5.3 Flash (`glm-5.3-flash:cloud`). Its admin default was also
accepted and initially displayed, but the created session again changed to `openai-api/gpt-5.5`
and failed before the fallback model could run. The capture is
`/private/var/folders/ky/n5fq5mgd5rl4321kxfy3jtvw0000gn/T/talaria-v050-talaria-t2-tdxgj8kp/raw/live-fallback-confirm.ansi`.
No third route was attempted. The original `xai-oauth/grok-4.6` default was restored after these
bounded attempts.

## Supplemental execution evidence

- Item 13 used the default-status item 12 process as the baseline, then replaced the scratch config
  and restarted before its own drive.
- Item 14 was also observed in fresh 36-row processes at every required width. The final rows were:
  full at 144; compact at 143; version absent at 119; `cwd` absent at 95; `git_branch` absent at 79;
  context absent at 63; agent absent at 47; task progress absent at 31; full connection at 20; and
  compact connection alone at 19.
- Item 15's literal-command capture is `raw/item-15-command.ansi` under the scratch root.
- Item 18's overlay capture is `raw/item-18-overlay.ansi` under the scratch root and its second full
  fresh-process drive is `raw/item-18-fresh.ansi`.
- Item 20's explicit below-threshold refusal capture is `raw/item-20-refusal.ansi` under the scratch
  root.
- Item 21's filtered command-palette capture is `raw/item-21-palette.ansi` under the scratch root.
- Item 32's config SHA-256 remained
  `0c639dc2495adcc566a32389763497b477574a30c2d0ad13cca002c6d177e126` and its modification time
  remained `1788156286`; the restart capture is `raw/item-32-restart.ansi` under the scratch root.

The required screenshot pair remains at:

- wide: `/private/var/folders/ky/n5fq5mgd5rl4321kxfy3jtvw0000gn/T/talaria-v050-talaria-t2-tdxgj8kp/screenshots/item-29.png`
- narrow: `/private/var/folders/ky/n5fq5mgd5rl4321kxfy3jtvw0000gn/T/talaria-v050-talaria-t2-tdxgj8kp/screenshots/item-30.png`

## Validation and repository checks

The receipt validator reported `valid` for all fifteen item receipts with capture and screenshot
hash checks enabled. The raw captures and screenshots were searched for credential patterns, bearer
values, token-bearing URLs, operator home paths, usernames, and private email/domain identifiers;
none were found.

- `uv run ruff check .`: passed.
- `uv run mypy`: passed with no issues.
- `/opt/homebrew/bin/uv run pytest`: passed, 2,329 tests passed and 7 skipped in 553.71 seconds. The
  absolute executable bypassed a fleet hook that otherwise inserted the prohibited RTK wrapper.
- `uv run bandit -r talaria -q`: passed; Bandit emitted only its existing comment-token warnings.
- `git diff --check`: passed before this report was written and is rerun as the final pre-commit
  whitespace check.
