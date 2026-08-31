# Talaria v0.5.0 T2 acceptance evidence

This active evidence set was produced from the installed wheel for reviewed candidate commit
`122bd918e0056404e576ae5623ce9e97bfe1ad93`, wheel SHA-256
`a15b991fd16069a7a935548f949b5e287db86ba386799bbbebfc802f60f76126`. It contains 14 passing
receipts and one blocked receipt. The earlier `d869791` evidence is retained unchanged as
superseded evidence under `superseded/d869791/`; in particular, its item 17 receipt remains
SHA-256 `6aa763a44405d257719a97ac4bd31e13fe2e5b96d39a4fc4fdb21ae9e722c6cc`.

| Item | Verdict | Receipt | Raw pseudo-terminal capture | Screenshot |
| ---: | --- | --- | --- | --- |
| 3 | passed | [receipt](receipts/item-03-talaria-t2.json) | [raw](raw/item-03.ansi) | [screenshot](screenshots/item-03.png) |
| 12 | passed | [receipt](receipts/item-12-talaria-t2.json) | [raw](raw/item-12.ansi) | [screenshot](screenshots/item-12.png) |
| 13 | passed | [receipt](receipts/item-13-talaria-t2.json) | [raw](raw/item-13-rerun.ansi) | [screenshot](screenshots/item-13.png) |
| 14 | passed | [receipt](receipts/item-14-talaria-t2.json) | [raw](raw/item-14.ansi) | [screenshot](screenshots/item-14.png) |
| 15 | passed | [receipt](receipts/item-15-talaria-t2.json) | [invalid configuration](raw/item-15-invalid.ansi), [bounded command](raw/item-15-command.ansi) | [invalid configuration](screenshots/item-15-invalid.png), [bounded command](screenshots/item-15-command.png) |
| 16 | passed | [receipt](receipts/item-16-talaria-t2.json) | [raw](raw/item-16.ansi) | [screenshot](screenshots/item-16.png) |
| 17 | passed | [receipt](receipts/item-17-talaria-t2.json) | [empty state](raw/item-17-empty.ansi), [populated state](raw/item-17-populated.ansi) | [empty state](screenshots/item-17-empty.png), [populated state](screenshots/item-17-populated.png) |
| 18 | passed | [receipt](receipts/item-18-talaria-t2.json) | [responsive drive](raw/item-18.ansi), [119-column overlay](raw/item-18-overlay.ansi) | [responsive drive](screenshots/item-18.png), [119-column overlay](screenshots/item-18-overlay.png) |
| 19 | passed | [receipt](receipts/item-19-talaria-t2.json) | [raw](raw/item-19-screenshot.ansi) | [screenshot](screenshots/item-19.png) |
| 20 | passed | [receipt](receipts/item-20-talaria-t2.json) | [mode transitions](raw/item-20-screenshot-width.ansi), [111-column refusal](raw/item-20-refusal.ansi) | [mode transitions](screenshots/item-20.png), [111-column refusal](screenshots/item-20-refusal.png) |
| 21 | passed | [receipt](receipts/item-21-talaria-t2.json) | [navigation](raw/item-21-screenshot-width.ansi), [command palette](raw/item-21-palette.ansi) | [navigation](screenshots/item-21.png), [command palette](screenshots/item-21-palette.png) |
| 29 | passed | [receipt](receipts/item-29-talaria-t2.json) | [raw](raw/item-29.ansi) | [screenshot](screenshots/item-29.png) |
| 30 | passed | [receipt](receipts/item-30-talaria-t2.json) | [raw](raw/item-30-screenshot-width.ansi) | [screenshot](screenshots/item-30.png) |
| 32 | passed | [receipt](receipts/item-32-talaria-t2.json) | [session toggle](raw/item-32.ansi), [restart](raw/item-32-restart.ansi) | [session toggle](screenshots/item-32.png), [restart](screenshots/item-32-restart.png) |
| 34 | blocked | [receipt](receipts/item-34-talaria-t2.json) | [raw](raw/item-34-live-blocked-retry.ansi) | [screenshot](screenshots/item-34-retry.png) |

Item 17 was rerun first, followed by items 16 and 18 as inspector smoke checks. The populated item
17 drive renders held task, context, file, and operation state. The header-only drive retains all
four headings, renders `[none available from this session]` for each, and no longer invents a
`needs-you unavailable` task.

Item 34 remains blocked. The live gateway created a throwaway session on its unapproved
`openai-api/gpt-5.5` default and initialization failed for missing credentials before Talaria could
dispatch the corrected primary selection, `opencode-go / muse-spark-1.2-contributor`. No approved
model turn completed, so there was no eligible session to kill. No third route or gateway-side
configuration change was attempted.

The screenshots were rendered from the corresponding raw ANSI pseudo-terminal bytes with Pyte and
Pillow. The renderer preserves terminal colours and layout; it does not automate a graphical user
interface or simulate Talaria. Before publication, all raw captures were checked for credentials,
token-like values, and operator home paths. Receipt and pseudo-terminal-result paths are repository
relative wherever they identify committed files.
