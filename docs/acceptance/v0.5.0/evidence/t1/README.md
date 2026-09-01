# Talaria v0.5.0 cycle-three acceptance evidence — talaria-t1

This directory records the T1 half of acceptance against candidate
`4c2d8dbf0ddfb7f38ba1f228369ae2d929319758`, installed from the shared wheel whose SHA-256 digest is
`d5a41c67384e78d9b1048cb7c6524a668c20f8059031e0f5e0a93f0b289f7d88`. The installed artifact,
not the source checkout, produced every capture below. Every item receipt records harness commit
`4c2d8dbf0ddfb7f38ba1f228369ae2d929319758`.

| Item | Verdict | Receipt | Raw terminal evidence | Screenshot |
| ---: | --- | --- | --- | --- |
| 1 | passed | [install receipt](install-receipt.json) | install probe, version, bare launch, and replay gate are embedded in the receipt | not applicable |
| 2 | passed | [receipt](receipts/item-02-talaria-t1.json) | [live primary turn](raw/item-02-c3b.ansi) | [screenshot](screenshots/item-02-c3b.png) |
| 4 | passed | [receipt](receipts/item-04-talaria-t1.json) | [Refined Default](raw/item-04-c3.ansi) | [screenshot](screenshots/item-04-c3.png) |
| 5 | passed | [receipt](receipts/item-05-talaria-t1.json) | [Dark Green Terminal](raw/item-05-c3.ansi) | [screenshot](screenshots/item-05-c3.png) |
| 6 | passed | [receipt](receipts/item-06-talaria-t1.json) | [Neutral Dark](raw/item-06-c3.ansi) | [screenshot](screenshots/item-06-c3.png) |
| 7 | passed | [receipt](receipts/item-07-talaria-t1.json) | [Accessible High Contrast](raw/item-07-c3.ansi) | [screenshot](screenshots/item-07-c3.png) |
| 8 | passed | [receipt](receipts/item-08-talaria-t1.json) | [preview cancellation](raw/item-08-c3b.ansi) | [screenshot](screenshots/item-08-c3b.png) |
| 9 | passed | [receipt](receipts/item-09-talaria-t1.json) | [user save](raw/item-09-user-c3.ansi), [repository save](raw/item-09-repository-c3.ansi), [session preview](raw/item-09-session-c3.ansi) | [user](screenshots/item-09-user-c3.png), [repository](screenshots/item-09-repository-c3.png), [session](screenshots/item-09-session-c3.png) |
| 10 | passed | [receipt](receipts/item-10-talaria-t1.json) | [fixture import](raw/item-10-import-c3b.ansi), [fallback and selection](raw/item-10-c3.ansi) | [import](screenshots/item-10-import-c3b.png), [interface](screenshots/item-10-c3.png) |
| 11 | passed | [receipt](receipts/item-11-talaria-t1.json) | [first import](raw/item-11-import-1-c3.ansi), [second import](raw/item-11-import-2-c3.ansi), [installed-theme restart](raw/item-11-c3.ansi) | [first](screenshots/item-11-import-1-c3.png), [second](screenshots/item-11-import-2-c3.png), [interface](screenshots/item-11-c3.png) |
| 15 | passed | [receipt](receipts/item-15-talaria-t1.json) | [invalid settings](raw/item-15-invalid-c3.ansi), [bounded command](raw/item-15-command-c3.ansi) | [invalid](screenshots/item-15-invalid-c3.png), [command](screenshots/item-15-command-c3.png) |
| 22 | passed | [receipt](receipts/item-22-talaria-t1.json) | [focus and caret](raw/item-22-c3.ansi) | [screenshot](screenshots/item-22-c3.png) |
| 23 | passed | [receipt](receipts/item-23-talaria-t1.json) | [healthy and reconnect cycle](raw/item-23-c3.ansi); item 33 supplies authentication | [screenshot](screenshots/item-23-c3.png) |
| 24 | blocked | [receipt](receipts/item-24-talaria-t1.json) | [reachable agent and queue states](raw/item-24-c3.ansi) | [screenshot](screenshots/item-24-c3.png) |
| 25 | passed | [receipt](receipts/item-25-talaria-t1.json) | [monochrome identities](raw/item-25-c3.ansi) | [screenshot](screenshots/item-25-c3.png) |
| 26 | passed | [receipt](receipts/item-26-talaria-t1.json) | [standard motion](raw/item-26-standard-c3.ansi), [reduced motion](raw/item-26-reduced-c3.ansi) | [standard](screenshots/item-26-standard-c3.png), [reduced](screenshots/item-26-reduced-c3.png) |
| 27 | passed | [receipt](receipts/item-27-talaria-t1.json) | [unpinned scroll](raw/item-27-c3.ansi) | [screenshot](screenshots/item-27-c3.png) |
| 28 | passed | [receipt](receipts/item-28-talaria-t1.json) | [pinned scroll](raw/item-28-c3.ansi) | [screenshot](screenshots/item-28-c3.png) |
| 31 | passed | [receipt](receipts/item-31-talaria-t1.json) | [malformed import](raw/item-31-c3.ansi) | [screenshot](screenshots/item-31-c3.png) |
| 33 | passed | [receipt](receipts/item-33-talaria-t1.json) | [stale credential](raw/item-33-c3b.ansi) | [screenshot](screenshots/item-33-c3b.png) |
| 34 | passed | [receipt](receipts/item-34-talaria-t1.json) | [completed turn and killed session](raw/item-34-c3.ansi), [hashed close audit](audits/item-34-session-close-c3.json) | [screenshot](screenshots/item-34-c3.png) |
| 35 | passed | [receipt](receipts/item-35-talaria-t1.json) | [running process](raw/item-35-before-c3.ansi), [restart](raw/item-35-after-c3.ansi) | [before](screenshots/item-35-before-c3.png), [after](screenshots/item-35-after-c3.png) |
| 36 | passed | [receipt](receipts/item-36-talaria-t1.json) | [T1 evidence half](raw/item-36-c3.ansi) | [screenshot](screenshots/item-36-c3.png) |

The cycle-three binding audit found no T1 event JSON that needed editing. The theme flows use slash
commands rather than the changed function keys, item 22 types nothing while a no-text region holds
focus, and the scroll flows already use upward wheel input plus F5 rather than Down-at-bottom to
unpin. All eighteen committed event scripts parsed through
`scripts.acceptance.v050_pty_driver.parse_events` before execution.

The live legs requested and observed `opencode-go / muse-spark-1.2-contributor`, with no fallback.
Item 2 completed a real turn and its recording contains matching `session.info` provider and model
fields. Item 23 captured `[..] wait`, `[ok] up`, `[~] retry`, and `[x] down` during the controlled
restart of only the isolated dashboard on port 8790; item 33 separately captured `[!] auth` and HTTP
403 from the stale scratch credential. Item 34 closed only its acceptance-created session and then
showed gateway code 4001 while the dashboard remained available.

Item 24 remains blocked on current evidence. Hermes 0.21.0 assigns request identifiers to live
approval rows, so they do not enter the possibly-duplicate state. The shipped replay format cannot
encode the keyless admin-polled row that triggers that state without simulating acceptance. The
current replay nevertheless proves every reachable agent and queue form.

Screenshots are deterministic Portable Network Graphics renderings of the corresponding real raw
American National Standards Institute terminal bytes through Pyte and Pillow. This preserves the
pseudo-terminal geometry and colours without graphical user interface automation or a simulated
Talaria application. Captures were reviewed for the scratch credential, token queries,
authorization headers, bearer values, operator home paths, and email addresses before publication;
none were present. Publication replaced only the isolated scratch-root identifier and rebound the
published capture and pseudo-terminal result hashes.
