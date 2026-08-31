# Talaria v0.5.0 T2 confirming acceptance evidence

This active evidence set was produced from the installed wheel for final candidate commit
`0f5c8e3e44a43c5956f94ec3ccc348b7cdba1398`, wheel SHA-256
`720cc654d06a8075e0dc032289e0c1320b177bc5ee2bcebc1a962f8ea9d76e3b`. All fifteen T2 receipts
pass. The immediately preceding `d9c82443` candidate evidence is retained
unchanged under `superseded/d9c82443/`; the still older evidence remains in its existing
candidate-specific directories.

| Item | Verdict | Receipt | Raw pseudo-terminal capture | Screenshot |
| ---: | --- | --- | --- | --- |
| 3 | passed | [receipt](receipts/item-03-talaria-t2.json) | [raw](raw/item-03.ansi) | [screenshot](screenshots/item-03.png) |
| 12 | passed | [receipt](receipts/item-12-talaria-t2.json) | [raw](raw/item-12.ansi) | [screenshot](screenshots/item-12.png) |
| 13 | passed | [receipt](receipts/item-13-talaria-t2.json) | [configured process](raw/item-13-rerun.ansi), [fresh process](raw/item-13-restart.ansi) | [configured process](screenshots/item-13-rerun.png), [fresh process](screenshots/item-13-restart.png) |
| 14 | passed | [receipt](receipts/item-14-talaria-t2.json) | [corrected full resize sequence](raw/item-14-resize-fix.ansi) | [19-column form](screenshots/item-14-resize-fix.png) |
| 15 | passed | [receipt](receipts/item-15-talaria-t2.json) | [invalid configuration](raw/item-15-invalid.ansi), [bounded command](raw/item-15-command.ansi) | [invalid configuration](screenshots/item-15-invalid.png), [bounded command](screenshots/item-15-command.png) |
| 16 | passed | [receipt](receipts/item-16-talaria-t2.json) | [raw](raw/item-16.ansi) | [screenshot](screenshots/item-16.png) |
| 17 | passed | [receipt](receipts/item-17-talaria-t2.json) | [empty state](raw/item-17-empty-rerun.ansi), [populated state](raw/item-17-populated-rerun.ansi) | [empty state](screenshots/item-17-empty-rerun.png), [populated state](screenshots/item-17-populated-rerun.png) |
| 18 | passed | [receipt](receipts/item-18-talaria-t2.json) | [corrected responsive drive](raw/item-18-resize-fix.ansi), [119-column overlay](raw/item-18-overlay-resize-fix.ansi) | [120-column restoration](screenshots/item-18-resize-fix.png), [119-column overlay](screenshots/item-18-overlay-resize-fix.png) |
| 19 | passed | [receipt](receipts/item-19-talaria-t2.json) | [raw](raw/item-19-screenshot.ansi) | [screenshot](screenshots/item-19-screenshot.png) |
| 20 | passed | [receipt](receipts/item-20-talaria-t2.json) | [corrected mode transitions](raw/item-20-resize-fix.ansi), [111-column refusal](raw/item-20-refusal-resize-fix.ansi) | [restored layout](screenshots/item-20-resize-fix.png), [refusal](screenshots/item-20-refusal-resize-fix.png) |
| 21 | passed | [receipt](receipts/item-21-talaria-t2.json) | [navigation and unbound keys](raw/item-21-screenshot.ansi), [command palette](raw/item-21-palette.ansi) | [navigation](screenshots/item-21-screenshot.png), [palette](screenshots/item-21-palette.png) |
| 29 | passed | [receipt](receipts/item-29-talaria-t2.json) | [corrected resize drive](raw/item-29-resize-fix.ansi) | [wide screenshot](screenshots/item-29-resize-fix.png) |
| 30 | passed | [receipt](receipts/item-30-talaria-t2.json) | [unified diff](raw/item-30-screenshot-resize-fix.ansi), [narrow status](raw/item-30-resize-fix.ansi) | [unified diff](screenshots/item-30-screenshot-resize-fix.png), [narrow status](screenshots/item-30-resize-fix.png) |
| 32 | passed | [receipt](receipts/item-32-talaria-t2.json) | [session toggle](raw/item-32.ansi), [fresh process](raw/item-32-restart.ansi) | [session toggle](screenshots/item-32.png), [fresh process](screenshots/item-32-restart.png) |
| 34 | passed | [receipt](receipts/item-34-talaria-t2.json) | [live killed session](raw/item-34-live-killed-final.ansi) | [screenshot](screenshots/item-34-final.png) |

The shared driver previously exported `COLUMNS=144` and `LINES=36`, so Textual ignored later
pseudo-terminal resizes even though the driver recorded the new kernel dimensions. The corrected
item 14 drive leaves those variables unset and traverses every specified breakpoint from 144 through
19 columns. The final frame contains only `[ok]` plus fifteen background cells, which is the required
minimum form. Items 18, 20, 29, and 30 were rerun for the same reason and now prove their layout at
the real child dimensions. The invalid pinned-dimension captures and receipts remain under
`superseded/driver-pinned-dimensions/` and are not active evidence.

Items 12, 13, and 32 otherwise passed. At 144 columns item 12 showed all seven default segments in
the required order on one row. Item 13 showed the configured five recognized segments in order,
omitted `cwd` and `version`, displayed the unknown-segment notice, and retained the same config after
restart. Its config remained SHA-256
`de189cd94d976240a6ba2f1748e82334cbbeb0938a1ce0321cc87310e98c0c1c`, 186 bytes, modification
time `1788193831`. Item 32 hid `cwd` immediately, then a fresh process restored it; its config
remained SHA-256 `1123f0cdd03ffbf3dd793248a822d92cce59376a71d18b06b06c7a0297aff697`, 74 bytes,
modification time `1788193831` before the toggle, after the toggle, and after restart.

Item 17 used separate populated and header-only replay processes. All four inspector headings remain;
the populated process shows held task, context, three changed files, and operation data. The empty
process shows the honest none-available state and contains no synthetic `needs-you unavailable` row.

Item 21's user-visible mutation boundary remains structural. The command palette offers only
`/diffs`; the modal labels itself `read only`; and the hints expose navigation and view-mode actions
only. The held state contains three files, five hunks, intraline replacements, and long clipped
lines. Edit-like unbound keys did not change the diff or expose edit, stage, revert, discard, or
apply controls.

Item 34 used the isolated dashboard at `ws://127.0.0.1:8790/api/ws` with profile `default`. The
observed primary route was `opencode-go / muse-spark-1.2-contributor`; no fallback was requested or
used. A real turn completed with `TALARIA-T2-LIVE-OK`, then a separately authenticated connection
closed exactly that newly recorded session and Hermes returned `closed: true` (session identifier
SHA-256 `1b9402c67ab06f574e1d4dc60c6adf89f01a1359e3567cb68e68534ca8556788`). The next prompt
visibly returned `prompt.submit was refused by the gateway (code 4001): session not found` while
Talaria retained its transcript, inspector context, status, and clean exit path. The dashboard
remained running.

All screenshots were rendered from the corresponding real raw American National Standards
Institute terminal bytes with Pyte and Pillow. This preserves pseudo-terminal geometry and colours
without graphical user interface automation or a simulated Talaria application. Before publication,
40 selected and breakpoint captures were checked for the scratch credential, operator home paths,
email addresses, authorization material, and unrelated private identifiers; none were present.
