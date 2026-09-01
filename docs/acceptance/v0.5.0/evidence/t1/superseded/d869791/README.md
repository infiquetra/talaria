# Talaria v0.5.0 acceptance evidence — talaria-t1

This directory records issue #110 acceptance against frozen candidate commit
`d86979127f871a479eb104fc10c886b5c5480a8c` and wheel Secure Hash Algorithm 256-bit
(SHA-256) digest `a165ad24bd2a4baa7d11aec5d5f434e1451fd688661fed1fe8919ca0c65a1afb`.
The acceptance harness installed that wheel at
`<scratch-root>`.
Its exact install receipt is committed as `install-receipt.json`; raw American National Standards
Institute (ANSI) pseudo-terminal captures remain under the scratch root.

Every item receipt in `receipts/` was written by `scripts.acceptance.v050_receipt`, not by hand, and
passed that validator with capture and screenshot hash checks enabled. The screenshots are
deterministic Portable Network Graphics renderings of reviewed raw pseudo-terminal bytes through a
Pyte terminal screen and Pillow image renderer. This is the same headless method used for the T2
evidence. No Computer Use or graphical user interface automation was used.

## Verdicts

| Item | Verdict | Judgment |
| ---: | :--- | :--- |
| 1 | passed | The install receipt proves the exact frozen wheel, version 0.5.0, installed entry point, bare launch, and complete 50,000-delta gate report. |
| 2 | blocked | The gateway rejects Talaria's `/model` dispatch as command class 4018, so the approved primary route cannot be established from this client. |
| 4 | passed | Refined Default is visibly applied across transcript, Composer, status bar, inspector, selection, focus, and diff surfaces. |
| 5 | passed | Dark Green Terminal previews, accepts, and retains readable content across the full wide layout. |
| 6 | passed | Neutral Dark previews, accepts, and retains the same geometry with its low-saturation palette. |
| 7 | passed | Accessible High Contrast applies to the complete layout and read-only diff; its runtime colors match the specified contrast-qualified built-in tokens. |
| 8 | passed | Two immediate previews cancel back to Refined Default, and the scratch config hash and modification time remain exact. |
| 9 | passed | Three launches prove user, repository, and session theme precedence; only the two explicit save actions mutate their intended files. |
| 10 | passed | An unknown saved theme produces the visible Refined Default fallback notice, while the partial import reports every warning and filled token. |
| 11 | passed | Two Visual Studio Code imports store identical bytes, and a fresh installed launch loads the imported theme without a fallback notice. |
| 22 | passed | Focus visits every required surface with correct caret-location cues and stable Composer, transcript, HelpBar, and BottomStatusBar geometry. |
| 23 | blocked | Reconnecting and authentication-failure states require coordinated control of the shared Hermes gateway; no unowned gateway mutation was made. |
| 24 | blocked | The capture proves all seven agent states and empty, waiting, and blocked queue forms, but shipped replay cannot create a genuine possibly-duplicate polled-feed state. |
| 25 | failed | All six transcript identities remain legible without color, but blank spacer rows appear after Reasoning and Talaria assistant entries. |
| 26 | passed | Restarting with reduced motion changes animated `working…` to static `[..] working` without losing transcript, agent, elapsed-time, scroll, theme, layout, or connection-state updates. |
| 27 | failed | A genuine middle reading position jumps back to the newest transcript after later appends and chrome changes. |
| 28 | passed | F5 follows new output, manual wheel input releases follow mode, and the last F5 returns predictably to the newest entry. |
| 31 | passed | The installed importer exits 2 on the malformed repository fixture and creates no stored theme. |
| 33 | blocked | A real dead or stale gateway credential requires gateway revocation or restart authority that this tester does not hold. |
| 35 | passed | A config edit leaves the running theme unchanged and takes effect only in the fresh process after restart. |

Item 34 was assigned to T2, not this tester. It remains blocked on the same gateway-owned live-session
control seam and was deliberately not retried here.

## Failed and blocked evidence

Item 25's raw capture is
`<scratch-root>/raw/item-25-monochrome.ansi`.
Its screenshot is `screenshots/item-25.png`. The monochrome frame shows `> You`, `. Reasoning`, `A
Talaria`, `$ Tool/Subagent`, `- Session`, and `! Error`, but visible blank rows follow the Reasoning
and assistant entries.

Item 27's raw capture is
`<scratch-root>/raw/item-27-wheels.ansi`.
Its screenshot is `screenshots/item-27.png`. Forty-five real terminal wheel-up inputs reached the
middle anchors around 66–89; later transcript appends, theme and inspector operations, and resizes
returned the viewport to anchors 98–120 at the bottom.

Item 24's blocking capture is `raw/item-24-monochrome.ansi` under the scratch root, with screenshot
`screenshots/item-24.png`. It shows the seven fixed agent glyph-and-word forms, waiting and blocked
queue rows, and narrow `/needs` detail. No `[?] possibly duplicate` state was produced, so prompt text
containing those words was not treated as evidence of the state.

Items 2, 23, and 33 remain blocked on gateway-owned controls. The operator-confirmed route names are
OpenCode Muse Spark 1.2 Contributor (`opencode-go / muse-spark-1.2-contributor`) and the permitted
fallback Ollama GLM 5.3 Flash (`ollama (ollama-cloud) / glm-5.3-flash`). Talaria's `/models` path
composes `/model <name> --provider <slug>`, but this gateway rejects that entire dispatch with code
4018: `not a quick/plugin/bundle/skill command: model`. No approved route reached a model turn, no
third route was accepted, and no live receipt was fabricated.

## Supplemental evidence

- Item 8 kept the config SHA-256 at
  `c0556af7ec91823950e3428c60c7cdd73dfc88e93c3fa436580699c3225547b6` and modification time at
  `1788163146` across preview cancellation.
- Item 9's user config changed only in its save leg from
  `c0556af7ec91823950e3428c60c7cdd73dfc88e93c3fa436580699c3225547b6` to
  `ee71172786c771bb5b47199be11901fbf2805f690091aea5911d316e432a9ce2`. The repository config became
  `bc74886467ba259b3a2f4e2966a4d2ebb153ec2c5395e91cdfe79f40ef044bb3`; the session leg changed
  neither. Screenshots for all three launches are committed.
- Item 10's installed import capture is `raw/item-10-import-final2.ansi` under the scratch root. It
  reports 2 source tokens, 56 Refined Default fallback tokens, and 19 warnings. The truecolor
  supplemental screenshot is `screenshots/item-10-color.png`.
- Item 11's installed import captures are `raw/item-11-import-1-final.ansi` and
  `raw/item-11-import-2-final.ansi` under the scratch root. Both stored files have SHA-256
  `4b96379b79089cce8bde457f2ac8b7a8cd86c9e44114eaad883c4823bafcf6ed`.
- Item 26 includes standard and reduced replay screenshots plus standard and reduced isolated
  dead-endpoint screenshots. The latter prove connection-state updates without touching the shared
  gateway or invoking a model.
- Item 35's paired screenshots are `screenshots/item-35-before-restart.png` and
  `screenshots/item-35-after-restart.png`.

## Review and validation

The 49 selected raw captures and text-rendered screenshots were searched for authorization headers,
bearer values, token- or credential-bearing URLs, operator home paths, usernames, and private
email/domain identifiers; no match was found. All 16 T1 item receipts validate against their raw
capture and screenshot hashes. The event scripts parse through
`scripts.acceptance.v050_pty_driver.parse_events`, and all six committed corpora parse through the
shipped frame-log reader.

- `uv run ruff check .`: passed.
- `uv run mypy`: passed with no issues.
- `/opt/homebrew/bin/uv run pytest`: passed, 2,329 tests passed and 7 skipped in 564.30 seconds. An
  initial whole-suite run had one intermittent status-process timing failure after 2,328 passes; the
  exact test then passed five consecutive focused runs, and the second complete run passed.
- `uv run bandit -r talaria -q`: passed; Bandit emitted only its existing comment-token warnings.
- `git diff --check`: passed after the evidence update.
