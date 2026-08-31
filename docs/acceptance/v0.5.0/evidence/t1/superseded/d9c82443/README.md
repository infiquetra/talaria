# Talaria v0.5.0 T1 acceptance evidence

This active evidence set was produced from the installed wheel for candidate commit
`d9c82443f51932483ecb37c653d5c0cd8c342dac`, wheel SHA-256
`839f7a26985267db5e0cc2fa52b46ac3924f7791cdfbc70fbadfdc0e7f6cdfda`. The T1 sweep recorded 15
passes, four failures, and one blocked item. The earlier evidence remains unchanged under
`superseded/` and is excluded from the active manifest.

| Item | Verdict | Receipt | Raw pseudo-terminal capture | Screenshot |
| ---: | --- | --- | --- | --- |
| 1 | passed | [install receipt](install-receipt.json) | recorded by install probe | not applicable |
| 2 | failed | [receipt](receipts/item-02-talaria-t1.json) | [raw](raw/item-02-live-primary-final.ansi) | [screenshot](screenshots/item-02-live-primary-final.png) |
| 4 | passed | [receipt](receipts/item-04-talaria-t1.json) | [raw](raw/item-04.ansi) | [screenshot](screenshots/item-04.png) |
| 5 | passed | [receipt](receipts/item-05-talaria-t1.json) | [raw](raw/item-05.ansi) | [screenshot](screenshots/item-05.png) |
| 6 | passed | [receipt](receipts/item-06-talaria-t1.json) | [raw](raw/item-06.ansi) | [screenshot](screenshots/item-06.png) |
| 7 | passed | [receipt](receipts/item-07-talaria-t1.json) | [raw](raw/item-07.ansi) | [screenshot](screenshots/item-07.png) |
| 8 | passed | [receipt](receipts/item-08-talaria-t1.json) | [raw](raw/item-08-final.ansi) | [screenshot](screenshots/item-08-final.png) |
| 9 | passed | [receipt](receipts/item-09-talaria-t1.json) | [user](raw/item-09-user.ansi), [repository](raw/item-09-repository.ansi), [session](raw/item-09-session.ansi) | [user](screenshots/item-09-user.png), [repository](screenshots/item-09-repository.png), [session](screenshots/item-09-session.png) |
| 10 | passed | [receipt](receipts/item-10-talaria-t1.json) | [import](raw/item-10-import.ansi), [interface](raw/item-10.ansi) | [import](screenshots/item-10-import.png), [interface](screenshots/item-10.png) |
| 11 | passed | [receipt](receipts/item-11-talaria-t1.json) | [first import](raw/item-11-import-1.ansi), [second import](raw/item-11-import-2.ansi), [interface](raw/item-11.ansi) | [screenshot](screenshots/item-11.png) |
| 22 | passed | [receipt](receipts/item-22-talaria-t1.json) | [raw](raw/item-22.ansi) | [screenshot](screenshots/item-22.png) |
| 23 | failed | [receipt](receipts/item-23-talaria-t1.json) | [raw](raw/item-23-live-states-final.ansi) | [screenshot](screenshots/item-23-live-states-final.png) |
| 24 | blocked | [receipt](receipts/item-24-talaria-t1.json) | [raw](raw/item-24.ansi) | [screenshot](screenshots/item-24.png) |
| 25 | failed | [receipt](receipts/item-25-talaria-t1.json) | [raw](raw/item-25.ansi) | [screenshot](screenshots/item-25.png) |
| 26 | passed | [receipt](receipts/item-26-talaria-t1.json) | [standard](raw/item-26-standard.ansi), [reduced](raw/item-26-reduced.ansi) | [standard](screenshots/item-26-standard.png), [reduced](screenshots/item-26-reduced.png) |
| 27 | failed | [receipt](receipts/item-27-talaria-t1.json) | [raw](raw/item-27.ansi) | [screenshot](screenshots/item-27.png) |
| 28 | passed | [receipt](receipts/item-28-talaria-t1.json) | [raw](raw/item-28.ansi) | [screenshot](screenshots/item-28.png) |
| 31 | passed | [receipt](receipts/item-31-talaria-t1.json) | [raw](raw/item-31-final2.ansi) | [screenshot](screenshots/item-31-final2.png) |
| 33 | passed | [receipt](receipts/item-33-talaria-t1.json) | [raw](raw/item-23-and-33-live-restart-final.ansi) | [screenshot](screenshots/item-23-and-33-live-restart-final.png) |
| 35 | passed | [receipt](receipts/item-35-talaria-t1.json) | [before restart](raw/item-35-before-restart.ansi), [after restart](raw/item-35-after-restart.ansi) | [before restart](screenshots/item-35-before-restart.png), [after restart](screenshots/item-35-after-restart.png) |

Item 2 completed a real Hermes-backed turn on the approved primary route,
`opencode-go / muse-spark-1.2-contributor`, and the inspector named that model. It failed because the
required bottom-status `agent_model` segment still displayed `agent: ?`. Item 23 failed because its
monochrome restart capture did not retain all five required token-plus-text connection forms. Item
25 failed because blank spacer rows remained after reasoning and assistant entries. Item 27 failed
because the unpinned transcript jumped after later interface updates. Item 24 is blocked because the
replay boundary cannot provide the admin-polled `possibly duplicate` agent state; the receipt does
not infer that absent row.

Item 33 used only the authorized isolated dashboard on port 8790. The deliberate restart changed
the listener from process 99486 to process 6547 with the identical command. A later evidence retry
left the same dashboard command running as process 18366 with parent process 1 and refreshed only the
scratch credential. Talaria displayed an HTTP 403 authentication failure with `[!] auth`, exited
cleanly, and exposed no credential in the reviewed capture. No fallback model was used in any live
leg.

The screenshots were rendered from the corresponding raw ANSI pseudo-terminal bytes with Pyte and
Pillow. The renderer preserves terminal colours and layout; it does not automate a graphical user
interface or simulate Talaria. Before publication, all selected raw captures were checked for
credential values, token-query parameters, bearer headers, email addresses, and operator home
paths. Receipt and pseudo-terminal-result paths are repository relative wherever they identify
committed files.
