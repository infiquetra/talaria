# Talaria v0.5.0 cycle-two acceptance evidence — talaria-t1

This directory records the T1 half of acceptance against candidate
`788fc791fadd701cb74b7db8686c0a8bb444b8f8`, installed from the shared wheel whose SHA-256 digest is
`bc5406c8b201c08758b8c51db8ab54059fa291be93fa06766df662be9dea73be`. The installed artifact,
not the source checkout, produced every capture below.

| Item | Verdict | Receipt | Raw terminal evidence | Screenshot |
| ---: | --- | --- | --- | --- |
| 1 | passed | [install receipt](install-receipt.json) | install probe, version, bare launch and replay gate are embedded in the receipt | not applicable |
| 2 | passed | [receipt](receipts/item-02-talaria-t1.json) | [live primary turn](raw/item-02-c2.ansi) | [screenshot](screenshots/item-02-c2.png) |
| 4 | passed | [receipt](receipts/item-04-talaria-t1.json) | [Refined Default](raw/item-04-c2a.ansi) | [screenshot](screenshots/item-04-c2a.png) |
| 5 | passed | [receipt](receipts/item-05-talaria-t1.json) | [Dark Green Terminal](raw/item-05-c2a.ansi) | [screenshot](screenshots/item-05-c2a.png) |
| 6 | passed | [receipt](receipts/item-06-talaria-t1.json) | [Neutral Dark](raw/item-06-c2a.ansi) | [screenshot](screenshots/item-06-c2a.png) |
| 7 | passed | [receipt](receipts/item-07-talaria-t1.json) | [Accessible High Contrast](raw/item-07-c2a.ansi) | [screenshot](screenshots/item-07-c2a.png) |
| 8 | passed | [receipt](receipts/item-08-talaria-t1.json) | [preview cancellation](raw/item-08-c2a.ansi) | [screenshot](screenshots/item-08-c2a.png) |
| 9 | passed | [receipt](receipts/item-09-talaria-t1.json) | [user save](raw/item-09-user-c2a.ansi), [repository save](raw/item-09-repository-c2a.ansi), [unsaved session preview](raw/item-09-session-c2a.ansi) | [user](screenshots/item-09-user-c2a.png), [repository](screenshots/item-09-repository-c2a.png), [session](screenshots/item-09-session-c2a.png) |
| 10 | passed | [receipt](receipts/item-10-talaria-t1.json) | [fixture import](raw/item-10-import-c2a.ansi), [fallback notice and selection](raw/item-10-c2.ansi) | [import](screenshots/item-10-import-c2a.png), [interface](screenshots/item-10-c2.png) |
| 11 | passed | [receipt](receipts/item-11-talaria-t1.json) | [first import](raw/item-11-import-1-c2a.ansi), [second import](raw/item-11-import-2-c2a.ansi), [restart into imported theme](raw/item-11-c2.ansi) | [first import](screenshots/item-11-import-1-c2a.png), [second import](screenshots/item-11-import-2-c2a.png), [interface](screenshots/item-11-c2.png) |
| 15 | passed | [receipt](receipts/item-15-talaria-t1.json) | [invalid settings](raw/item-15-invalid-c2a.ansi), [bounded command](raw/item-15-command-c2.ansi) | [invalid settings](screenshots/item-15-invalid-c2a.png), [command](screenshots/item-15-command-c2.png) |
| 22 | passed | [receipt](receipts/item-22-talaria-t1.json) | [focus and caret](raw/item-22-c2a.ansi) | [screenshot](screenshots/item-22-c2a.png) |
| 23 | passed | [receipt](receipts/item-23-talaria-t1.json) | [healthy, reconnecting and disconnected cycle](raw/item-23-c2b.ansi); item 33 supplies the separate authentication state | [screenshot](screenshots/item-23-c2b.png) |
| 24 | blocked | [receipt](receipts/item-24-talaria-t1.json) | [all reachable monochrome agent and queue states](raw/item-24-c2.ansi) | [screenshot](screenshots/item-24-c2.png) |
| 25 | passed | [receipt](receipts/item-25-talaria-t1.json) | [monochrome transcript identities](raw/item-25-c2a.ansi) | [screenshot](screenshots/item-25-c2a.png) |
| 26 | passed | [receipt](receipts/item-26-talaria-t1.json) | [standard motion](raw/item-26-standard-c2.ansi), [reduced motion after restart](raw/item-26-reduced-c2.ansi) | [standard](screenshots/item-26-standard-c2.png), [reduced](screenshots/item-26-reduced-c2.png) |
| 27 | passed | [receipt](receipts/item-27-talaria-t1.json) | [unpinned resize and new output](raw/item-27-c2a.ansi) | [screenshot](screenshots/item-27-c2a.png) |
| 28 | passed | [receipt](receipts/item-28-talaria-t1.json) | [pinned resize and new output](raw/item-28-c2a.ansi) | [screenshot](screenshots/item-28-c2a.png) |
| 31 | passed | [receipt](receipts/item-31-talaria-t1.json) | [malformed import rejection](raw/item-31-c2b.ansi) | [screenshot](screenshots/item-31-c2b.png) |
| 33 | passed | [receipt](receipts/item-33-talaria-t1.json) | [stale credential](raw/item-33-c2.ansi) | [screenshot](screenshots/item-33-c2.png) |
| 34 | passed | [receipt](receipts/item-34-talaria-t1.json) | [completed turn followed by killed session](raw/item-34-c2b.ansi), [hashed close audit](audits/item-34-session-close-c2b.json) | [screenshot](screenshots/item-34-c2b.png) |
| 35 | passed | [receipt](receipts/item-35-talaria-t1.json) | [running process](raw/item-35-before-c2.ansi), [clean restart](raw/item-35-after-c2.ansi) | [before](screenshots/item-35-before-c2.png), [after](screenshots/item-35-after-c2.png) |
| 36 | passed | [receipt](receipts/item-36-talaria-t1.json) | [T1 evidence half](raw/item-36-c2.ansi) | [screenshot](screenshots/item-36-c2.png) |

The theme drives exercised the current slash-command surface. None of the committed T1 event JSON
files used the superseded function keys, so no event sequence changed for cycle two. The event-script
README was corrected to expect exit code 3 from the current malformed-import command. Item 11's two
imports of the committed `unsupported-dark.json` fixture produced identical stored bytes with digest
`2ab65e1fb8b3489d49fd25b5e59e70db56cb2dc549fc7b302c4daf496b46820d`; the installed application then
loaded the imported theme without a fallback notice.

The live legs used profile `default` on the isolated dashboard at port 8790 and requested and
observed `opencode-go / muse-spark-1.2-contributor`, except item 33, where authentication failed before
a route could be observed. No fallback was requested. Item 23 separately captured the healthy,
connecting, reconnecting and disconnected forms; item 33 supplies the authentication form. Item 34
completed a real turn, closed only the acceptance-created session through `session.close`, and then
showed the next prompt fail visibly with gateway code 4001 while the dashboard remained running.

Item 24 remains blocked on current evidence. Hermes v0.21.0 assigns request identifiers to live
approval rows, so they are anchored and never enter the possibly-duplicate state. The shipped replay
format cannot encode the keyless admin-polled row that triggers that state without simulating the
acceptance condition. Item 36 records the independently complete T1 half: all assigned and shared T1
receipts bind to candidate `788fc791f`, and both T1 live-session receipts name the approved primary
route with no fallback. The generated manifest reports 0 stale receipts, 0 missing current receipts,
and 42 invalid item receipts. Both testers' evidence is merged and covers every expected slot, but
the new required harness provenance deliberately invalidates the old item receipts until the full
acceptance re-drive replaces them.

All screenshots were rendered from the corresponding real raw American National Standards
Institute terminal bytes with Pyte and Pillow. This preserves pseudo-terminal geometry and colours
without graphical user interface automation or a simulated Talaria application. Before publication,
60 selected raw captures and screenshots were checked for the scratch credential, token queries,
authorization headers, bearer values, operator home paths and email addresses; none were present.
